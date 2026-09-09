use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::connection_model::ConnectionRequest;
use crate::copy::text;
use crate::defaults::{ProductDefaultsGuard, product_defaults};

const CONFIGURATION_DIRECTORY_ENVIRONMENT_VARIABLE: &str =
    "WORKLOGGER_DEVELOPMENT_CONFIG_DIRECTORY";
const CREDENTIALS_FILE_NAME: &str = "credentials.json";
const TOOLKIT_CONFIG_FILE_NAME: &str = "config.json";
const PRIVATE_PERMISSION_MASK: u32 = 0o077;
const MILLISECONDS_PER_SECOND: u64 = 1_000;

#[derive(Deserialize)]
struct CredentialsDocument {
    jira: JiraCredentials,
}

#[derive(Deserialize)]
struct JiraCredentials {
    url: String,
    email: String,
    token: String,
}

#[derive(Deserialize)]
struct ToolkitConfigDocument {
    hours: HoursConfig,
    reports: Option<ReportsConfig>,
    http: Option<HttpConfig>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HoursConfig {
    weekly_target_hours: u32,
    jira_board_id: u64,
    jira_time_zone_offset: String,
    #[serde(default, rename = "enableTeamReports")]
    legacy_enable_team_reports: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReportsConfig {
    #[serde(default)]
    enable_team_reports: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpConfig {
    timeout_ms: u64,
}

pub(crate) fn load_request() -> Result<Option<ConnectionRequest>, String> {
    let Some(directory) = configuration_directory() else {
        return Ok(None);
    };
    let credentials_path = directory.join(CREDENTIALS_FILE_NAME);
    if !credentials_path.exists() {
        return Ok(None);
    }
    ensure_private(&credentials_path)?;
    let credentials = read_credentials(&credentials_path)?;
    let config = read_config(&directory.join(TOOLKIT_CONFIG_FILE_NAME))?;
    build_request(credentials, &config).map(Some)
}

fn configuration_directory() -> Option<PathBuf> {
    env::var_os(CONFIGURATION_DIRECTORY_ENVIRONMENT_VARIABLE)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn ensure_private(path: &Path) -> Result<(), String> {
    let permissions = fs::metadata(path)
        .map_err(|_| copy("developmentConfig.credentialsUnreadable"))?
        .permissions()
        .mode();
    if permissions & PRIVATE_PERMISSION_MASK != 0 {
        return Err(copy("developmentConfig.credentialsPermissions"));
    }
    Ok(())
}

fn read_credentials(path: &Path) -> Result<CredentialsDocument, String> {
    let contents =
        fs::read_to_string(path).map_err(|_| copy("developmentConfig.credentialsUnreadable"))?;
    serde_json::from_str(&contents).map_err(|_| copy("developmentConfig.credentialsInvalid"))
}

fn read_config(path: &Path) -> Result<ToolkitConfigDocument, String> {
    let contents =
        fs::read_to_string(path).map_err(|_| copy("developmentConfig.configurationUnreadable"))?;
    serde_json::from_str(&contents).map_err(|_| copy("developmentConfig.configurationInvalid"))
}

fn build_request(
    credentials: CredentialsDocument,
    config: &ToolkitConfigDocument,
) -> Result<ConnectionRequest, String> {
    let defaults = product_defaults();
    let utc_offset_minutes = parse_utc_offset(&config.hours.jira_time_zone_offset)?;
    Ok(request_from(
        credentials.jira,
        config,
        &defaults,
        utc_offset_minutes,
    ))
}

fn request_from(
    jira: JiraCredentials,
    config: &ToolkitConfigDocument,
    defaults: &ProductDefaultsGuard,
    utc_offset_minutes: i16,
) -> ConnectionRequest {
    ConnectionRequest {
        site: jira.url,
        email: jira.email,
        token: jira.token,
        board_id: config.hours.jira_board_id,
        weekly_target_hours: config.hours.weekly_target_hours,
        utc_offset_minutes,
        request_timeout_seconds: request_timeout(config, defaults),
        page_size: defaults.jira().page_size,
        maximum_collection_items: defaults.jira().maximum_collection_items,
        maximum_issue_search_results: defaults.jira().maximum_issue_search_results,
        maximum_concurrent_worklog_requests: defaults.jira().maximum_concurrent_worklog_requests,
        maximum_daily_hours: defaults.hours().maximum_daily_hours,
        default_worklog_start_hour: defaults.hours().default_worklog_start_hour,
        default_worklog_start_minute: defaults.hours().default_worklog_start_minute,
        enable_team_reports: team_reports_enabled(config),
    }
}

fn team_reports_enabled(config: &ToolkitConfigDocument) -> bool {
    config.reports.as_ref().map_or_else(
        || config.hours.legacy_enable_team_reports.unwrap_or_default(),
        |reports| reports.enable_team_reports,
    )
}

fn request_timeout(config: &ToolkitConfigDocument, defaults: &ProductDefaultsGuard) -> u64 {
    config
        .http
        .as_ref()
        .map_or(defaults.jira().request_timeout_seconds, |http| {
            http.timeout_ms.div_ceil(MILLISECONDS_PER_SECOND)
        })
}

fn parse_utc_offset(value: &str) -> Result<i16, String> {
    let (sign, digits) = value
        .split_at_checked(1)
        .ok_or_else(invalid_offset_message)?;
    let multiplier = parse_sign(sign)?;
    let (hours, minutes) = digits
        .split_at_checked(2)
        .ok_or_else(invalid_offset_message)?;
    combine_offset(parse_part(hours)?, parse_part(minutes)?, multiplier)
}

fn parse_sign(value: &str) -> Result<i16, String> {
    match value {
        "+" => Ok(1),
        "-" => Ok(-1),
        _ => Err(invalid_offset_message()),
    }
}

fn parse_part(value: &str) -> Result<i16, String> {
    value.parse().map_err(|_| invalid_offset_message())
}

fn combine_offset(hours: i16, minutes: i16, multiplier: i16) -> Result<i16, String> {
    hours
        .checked_mul(60)
        .and_then(|value| value.checked_add(minutes))
        .and_then(|value| value.checked_mul(multiplier))
        .ok_or_else(invalid_offset_message)
}

fn invalid_offset_message() -> String {
    copy("developmentConfig.timeZoneInvalid")
}

fn copy(key: &str) -> String {
    text(key).to_owned()
}

#[cfg(test)]
mod tests {
    use super::{ToolkitConfigDocument, parse_utc_offset, team_reports_enabled};

    #[test]
    fn parses_toolkit_time_zone_offset() {
        assert_eq!(parse_utc_offset("-0300"), Ok(-180));
        assert_eq!(parse_utc_offset("+0530"), Ok(330));
        assert!(parse_utc_offset("UTC-3").is_err());
    }

    #[test]
    fn reads_reports_section_and_migrates_legacy_hours_flag() {
        let legacy = parse_config(
            r#"{"hours":{"weeklyTargetHours":30,"jiraBoardId":768,"jiraTimeZoneOffset":"-0300","enableTeamReports":true}}"#,
        );
        assert!(team_reports_enabled(&legacy));

        let current = parse_config(
            r#"{"hours":{"weeklyTargetHours":30,"jiraBoardId":768,"jiraTimeZoneOffset":"-0300","enableTeamReports":true},"reports":{"enableTeamReports":false}}"#,
        );
        assert!(!team_reports_enabled(&current));
    }

    fn parse_config(json: &str) -> ToolkitConfigDocument {
        serde_json::from_str(json).expect("valid development config")
    }
}
