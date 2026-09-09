use std::cell::RefCell;
#[cfg(test)]
use std::env;
use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

use atomic_write_file::AtomicWriteFile;
use jira_adapter::JiraSiteUrl;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::copy::text;
use crate::defaults::product_defaults;

#[path = "shared_settings.rs"]
mod shared;

#[cfg(test)]
const CONFIG_FILE: &str = "config.json";
const HOURS_PER_DAY: u16 = 24;
const DAYS_PER_WEEK: u16 = 7;
const MINIMUM_POSITIVE_HOURS: u16 = 1;
const MINUTES_PER_HOUR: i16 = 60;
const MAX_EMAIL_LENGTH: usize = 254;
const MAX_UTC_OFFSET_MINUTES: i16 = 14 * MINUTES_PER_HOUR;
const MAX_WEEKLY_HOURS: u16 = HOURS_PER_DAY * DAYS_PER_WEEK;
pub(crate) const SETTINGS_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppSettings {
    pub schema_version: u16,
    pub jira: JiraSettings,
    pub hours: HoursSettings,
    #[serde(default)]
    pub reports: ReportsSettings,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JiraSettings {
    pub base_url: String,
    pub email: String,
    pub board_id: u64,
    pub request_timeout_seconds: u64,
    pub page_size: u16,
    #[serde(alias = "maximumReportItems")]
    pub maximum_collection_items: usize,
    #[serde(default = "default_maximum_issue_search_results")]
    pub maximum_issue_search_results: usize,
    #[serde(default = "default_maximum_concurrent_worklog_requests")]
    pub maximum_concurrent_worklog_requests: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HoursSettings {
    pub weekly_target: u16,
    pub utc_offset_minutes: i16,
    pub maximum_daily_hours: u8,
    pub default_worklog_start_hour: u8,
    pub default_worklog_start_minute: u8,
    #[serde(default, rename = "enableTeamReports", skip_serializing)]
    pub(crate) legacy_enable_team_reports: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReportsSettings {
    #[serde(default)]
    pub enable_team_reports: bool,
}

#[derive(Debug, Error)]
pub(crate) enum SettingsError {
    #[error(transparent)]
    Shared(#[from] worklogger_settings::SettingsError),
    #[error("the configuration is invalid: {0}")]
    Invalid(String),
    #[error("could not read the configuration at {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the configuration JSON at {path} is invalid: {source}")]
    Decode {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("could not save the configuration at {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not serialize the configuration: {0}")]
    Encode(#[source] serde_json::Error),
}

#[derive(Clone, Debug)]
pub(crate) struct SettingsStore {
    path: PathBuf,
    shared: Option<worklogger_settings::SettingsStore>,
    observed: RefCell<ObservedSettings>,
}

#[derive(Clone, Debug, Default)]
struct ObservedSettings {
    loaded: bool,
    document: Option<worklogger_settings::SettingsDocument>,
}

impl AppSettings {
    pub(crate) fn validate(&self) -> Result<(), SettingsError> {
        validate_schema(self.schema_version)?;
        validate_jira(&self.jira)?;
        validate_hours(&self.hours)
    }
}

impl SettingsStore {
    #[cfg(any(windows, feature = "dev-desktop"))]
    pub(crate) fn for_current_user() -> Result<Self, SettingsError> {
        let store = worklogger_settings::SettingsStore::for_current_user()?;
        Ok(Self::shared(store))
    }

    #[cfg(test)]
    pub(crate) fn at(path: PathBuf) -> Self {
        Self {
            path,
            shared: None,
            observed: RefCell::default(),
        }
    }

    fn shared(store: worklogger_settings::SettingsStore) -> Self {
        Self {
            path: store.path().to_path_buf(),
            shared: Some(store),
            observed: RefCell::default(),
        }
    }

    pub(crate) fn load(&self) -> Result<Option<AppSettings>, SettingsError> {
        if let Some(store) = &self.shared {
            let document = store.load()?;
            let settings = document
                .as_ref()
                .map(AppSettings::from_shared)
                .transpose()?
                .flatten();
            *self.observed.borrow_mut() = ObservedSettings {
                loaded: true,
                document,
            };
            return Ok(settings);
        }
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(read_error(&self.path, source)),
        };
        decode_settings(BufReader::new(file), &self.path).map(Some)
    }

    pub(crate) fn save(&self, settings: &AppSettings) -> Result<(), SettingsError> {
        settings.validate()?;
        if let Some(store) = &self.shared {
            let observed = self.observed.borrow().clone();
            let mut document = if observed.loaded {
                observed.document.unwrap_or_default()
            } else {
                store.load()?.unwrap_or_default()
            };
            settings.update_shared(&mut document)?;
            let committed = store.save(&document, document.revision)?;
            *self.observed.borrow_mut() = ObservedSettings {
                loaded: true,
                document: Some(committed),
            };
            return Ok(());
        }
        create_parent(&self.path)?;
        let mut bytes = serde_json::to_vec_pretty(settings).map_err(SettingsError::Encode)?;
        bytes.push(b'\n');
        write_atomically(&self.path, &bytes)
    }

    pub(crate) fn clear(&self) -> Result<(), SettingsError> {
        // Disconnect removes Desktop credentials; shared preferences belong to both frontends.
        if self.shared.is_some() {
            return Ok(());
        }
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(write_error(&self.path, source)),
        }
    }
}

fn decode_settings(reader: impl std::io::Read, path: &Path) -> Result<AppSettings, SettingsError> {
    let mut settings: AppSettings =
        serde_json::from_reader(reader).map_err(|source| SettingsError::Decode {
            path: path.to_path_buf(),
            source,
        })?;
    migrate_legacy_reports(&mut settings);
    settings.validate()?;
    Ok(settings)
}

fn migrate_legacy_reports(settings: &mut AppSettings) {
    if let Some(enabled) = settings.hours.legacy_enable_team_reports.take() {
        settings.reports.enable_team_reports = enabled;
    }
}

fn create_parent(path: &Path) -> Result<(), SettingsError> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("la ruta no tiene directorio padre"))?;
    fs::create_dir_all(parent).map_err(|source| write_error(path, source))
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), SettingsError> {
    let mut file = AtomicWriteFile::options()
        .open(path)
        .map_err(|source| write_error(path, source))?;
    file.write_all(bytes)
        .map_err(|source| write_error(path, source))?;
    file.commit().map_err(|source| write_error(path, source))
}

fn validate_schema(version: u16) -> Result<(), SettingsError> {
    if version != SETTINGS_SCHEMA_VERSION {
        return Err(invalid("schemaVersion no es compatible"));
    }
    Ok(())
}

fn validate_jira(settings: &JiraSettings) -> Result<(), SettingsError> {
    JiraSiteUrl::parse(&settings.base_url)
        .map_err(|_| invalid("jira.baseUrl debe ser un sitio HTTPS de atlassian.net"))?;
    validate_email(&settings.email)?;
    if settings.board_id == 0 {
        return Err(invalid("jira.boardId debe ser mayor que cero"));
    }
    if !product_defaults().allows_jira_board(&settings.base_url, settings.board_id) {
        return Err(invalid(text("settings.error.boardOutsideScope")));
    }
    if settings.request_timeout_seconds == 0
        || settings.page_size == 0
        || settings.maximum_collection_items == 0
        || settings.maximum_issue_search_results == 0
        || settings.maximum_concurrent_worklog_requests == 0
    {
        return Err(invalid("Jira limits must be greater than zero"));
    }
    validate_jira_maxima(settings)?;
    Ok(())
}

fn validate_jira_maxima(settings: &JiraSettings) -> Result<(), SettingsError> {
    let defaults = product_defaults();
    let limits = defaults.jira();
    let valid = settings.request_timeout_seconds <= limits.maximum_allowed_request_timeout_seconds
        && settings.page_size <= limits.maximum_allowed_page_size
        && settings.maximum_collection_items <= limits.maximum_allowed_collection_items
        && settings.maximum_issue_search_results <= limits.maximum_allowed_issue_search_results
        && settings.maximum_concurrent_worklog_requests
            <= limits.maximum_allowed_concurrent_worklog_requests;
    if !valid {
        return Err(invalid("Jira limits exceed the safe maximums"));
    }
    Ok(())
}

fn default_maximum_issue_search_results() -> usize {
    product_defaults().jira().maximum_issue_search_results
}

fn default_maximum_concurrent_worklog_requests() -> usize {
    product_defaults()
        .jira()
        .maximum_concurrent_worklog_requests
}

fn validate_email(email: &str) -> Result<(), SettingsError> {
    let value = email.trim();
    let valid_length = !value.is_empty() && value.len() <= MAX_EMAIL_LENGTH;
    if !valid_length || value.chars().any(char::is_whitespace) || !value.contains('@') {
        return Err(invalid("jira.email has an invalid format"));
    }
    Ok(())
}

fn validate_hours(settings: &HoursSettings) -> Result<(), SettingsError> {
    if settings.weekly_target == 0 || settings.weekly_target > MAX_WEEKLY_HOURS {
        return Err(invalid(format!(
            "hours.weeklyTarget debe estar entre {MINIMUM_POSITIVE_HOURS} y {MAX_WEEKLY_HOURS}"
        )));
    }
    if settings.maximum_daily_hours == 0 || u16::from(settings.maximum_daily_hours) > HOURS_PER_DAY
    {
        return Err(invalid(format!(
            "hours.maximumDailyHours debe estar entre {MINIMUM_POSITIVE_HOURS} y {HOURS_PER_DAY}"
        )));
    }
    if u16::from(settings.default_worklog_start_hour) >= HOURS_PER_DAY
        || i16::from(settings.default_worklog_start_minute) >= MINUTES_PER_HOUR
    {
        return Err(invalid("the default start hour is invalid"));
    }
    let offset = settings.utc_offset_minutes;
    if offset.unsigned_abs() > MAX_UTC_OFFSET_MINUTES.unsigned_abs() {
        return Err(invalid(
            "hours.utcOffsetMinutes must be within -14:00 and +14:00",
        ));
    }
    validate_hours_profile(settings)
}

fn validate_hours_profile(settings: &HoursSettings) -> Result<(), SettingsError> {
    let profile = product_defaults();
    let hours = profile.hours();
    let valid = hours.allows_weekly_target(u32::from(settings.weekly_target))
        && settings.maximum_daily_hours <= hours.maximum_daily_hours
        && hours.allows_utc_offset(settings.utc_offset_minutes);
    if !valid {
        return Err(invalid(text("settings.error.hoursOutsideProfile")));
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> SettingsError {
    SettingsError::Invalid(message.into())
}

fn read_error(path: &Path, source: std::io::Error) -> SettingsError {
    SettingsError::Read {
        path: path.to_path_buf(),
        source,
    }
}

fn write_error(path: &Path, source: std::io::Error) -> SettingsError {
    SettingsError::Write {
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEMP_SEQUENCE: AtomicUsize = AtomicUsize::new(0);
    const TEST_JIRA_SITE: &str = "https://example.atlassian.net";
    const TEST_JIRA_BOARD_ID: u64 = 123;

    #[test]
    fn saves_and_replaces_valid_settings_without_a_token() {
        let fixture = TestDirectory::new();
        let store = SettingsStore::at(fixture.path.join(CONFIG_FILE));
        let mut settings = valid_settings();
        store.save(&settings).expect("first save succeeds");
        settings.hours.weekly_target = 35;
        store.save(&settings).expect("replacement succeeds");
        assert_eq!(store.load().expect("load succeeds"), Some(settings));
        let json = fs::read_to_string(&store.path).expect("config is readable");
        assert!(!json.to_ascii_lowercase().contains("token"));
    }

    #[test]
    fn desktop_reads_mcp_preferences_and_preserves_its_scope_when_saving() {
        let fixture = TestDirectory::new();
        let shared = worklogger_settings::SettingsStore::at(fixture.path.join("settings.json"));
        let mut document = worklogger_settings::SettingsDocument::default();
        valid_settings().update_shared(&mut document).unwrap();
        document.bitbucket = Some(worklogger_settings::BitbucketSettings::default());
        document.mcp = Some(worklogger_settings::McpSettings {
            modules: [(
                worklogger_profile::IntegrationModuleId::Jira,
                worklogger_settings::ModuleSettings {
                    enabled: true,
                    capabilities: [worklogger_profile::Capability::ReadJiraIssues]
                        .into_iter()
                        .collect(),
                },
            )]
            .into_iter()
            .collect(),
        });
        let committed = shared.save(&document, 0).unwrap();
        let store = SettingsStore::shared(shared.clone());
        let mut settings = store.load().unwrap().unwrap();
        settings.hours.weekly_target = 35;
        store.save(&settings).unwrap();
        let saved = shared.load().unwrap().unwrap();
        assert_eq!(saved.mcp, committed.mcp);
        assert_eq!(saved.bitbucket, committed.bitbucket);
        assert_eq!(saved.hours.as_ref().unwrap().weekly_target_hours, Some(35));
        store.clear().unwrap();
        assert_eq!(shared.load().unwrap(), Some(saved));
    }

    #[test]
    fn desktop_rejects_a_save_after_another_frontend_changed_settings() {
        let fixture = TestDirectory::new();
        let shared = worklogger_settings::SettingsStore::at(fixture.path.join("settings.json"));
        let store = SettingsStore::shared(shared.clone());
        store.save(&valid_settings()).unwrap();
        let settings = store.load().unwrap().unwrap();
        let mut other = shared.load().unwrap().unwrap();
        other.jira.as_mut().unwrap().maximum_issue_search_results = Some(1);
        let committed = shared.save(&other, other.revision).unwrap();
        assert!(matches!(
            store.save(&settings),
            Err(SettingsError::Shared(
                worklogger_settings::SettingsError::Conflict { .. }
            ))
        ));
        assert_eq!(shared.load().unwrap(), Some(committed));
    }

    #[test]
    fn shared_draft_does_not_invent_a_desktop_board() {
        let document = worklogger_settings::SettingsDocument {
            jira: Some(worklogger_settings::JiraSettings {
                base_url: Some(TEST_JIRA_SITE.into()),
                ..worklogger_settings::JiraSettings::default()
            }),
            ..worklogger_settings::SettingsDocument::default()
        };
        assert_eq!(AppSettings::from_shared(&document).unwrap(), None);
    }

    #[test]
    fn first_desktop_setup_cannot_overwrite_concurrent_mcp_setup() {
        let fixture = TestDirectory::new();
        let shared = worklogger_settings::SettingsStore::at(fixture.path.join("settings.json"));
        let store = SettingsStore::shared(shared.clone());
        assert_eq!(store.load().unwrap(), None);
        let committed = shared
            .save(&worklogger_settings::SettingsDocument::default(), 0)
            .unwrap();
        assert!(matches!(
            store.save(&valid_settings()),
            Err(SettingsError::Shared(
                worklogger_settings::SettingsError::Conflict { .. }
            ))
        ));
        assert_eq!(shared.load().unwrap(), Some(committed));
    }

    #[test]
    fn accepts_unknown_fields_for_forward_compatibility() {
        let json = serde_json::to_string(&valid_settings()).expect("fixture serializes");
        let extended = json.replacen('{', "{\"futureField\":true,", 1);
        let loaded = decode_settings(extended.as_bytes(), Path::new(CONFIG_FILE));
        assert!(loaded.is_ok());
    }

    #[test]
    fn migrates_the_previous_collection_limit_name() {
        let defaults = product_defaults();
        let mut value = serde_json::to_value(valid_settings()).expect("fixture serializes");
        let jira = value
            .get_mut("jira")
            .and_then(serde_json::Value::as_object_mut)
            .expect("jira object");
        jira.remove("maximumIssueSearchResults");
        jira.remove("maximumConcurrentWorklogRequests");
        let limit = jira
            .remove("maximumCollectionItems")
            .expect("current limit");
        jira.insert("maximumReportItems".to_owned(), limit);
        let bytes = serde_json::to_vec(&value).expect("legacy fixture serializes");
        let loaded = decode_settings(bytes.as_slice(), Path::new(CONFIG_FILE)).expect("migration");
        assert_eq!(
            loaded.jira.maximum_issue_search_results,
            defaults.jira().maximum_issue_search_results
        );
        assert_eq!(
            loaded.jira.maximum_concurrent_worklog_requests,
            defaults.jira().maximum_concurrent_worklog_requests
        );
    }

    #[test]
    fn migrates_team_reports_out_of_hours() {
        let mut value = serde_json::to_value(valid_settings()).expect("fixture serializes");
        value
            .as_object_mut()
            .expect("settings object")
            .remove("reports");
        value
            .get_mut("hours")
            .and_then(serde_json::Value::as_object_mut)
            .expect("hours object")
            .insert("enableTeamReports".to_owned(), true.into());
        let bytes = serde_json::to_vec(&value).expect("legacy fixture serializes");
        let loaded = decode_settings(bytes.as_slice(), Path::new(CONFIG_FILE)).expect("migration");
        assert!(loaded.reports.enable_team_reports);
        assert_eq!(loaded.hours.legacy_enable_team_reports, None);
    }

    #[test]
    fn clear_removes_saved_settings_and_is_idempotent() {
        let fixture = TestDirectory::new();
        let store = SettingsStore::at(fixture.path.join(CONFIG_FILE));
        store.save(&valid_settings()).expect("settings are saved");
        store.clear().expect("settings are removed");
        store.clear().expect("a second clear is harmless");
        assert_eq!(store.load().expect("load succeeds"), None);
    }

    #[test]
    fn rejects_invalid_schema_and_fields_before_writing() {
        let fixture = TestDirectory::new();
        let store = SettingsStore::at(fixture.path.join(CONFIG_FILE));
        let mut settings = valid_settings();
        settings.schema_version = 2;
        assert!(matches!(
            store.save(&settings),
            Err(SettingsError::Invalid(_))
        ));
        assert!(!store.path.exists());
    }

    #[test]
    fn rejects_jira_limits_above_the_safe_maxima() {
        let mut settings = valid_settings();
        settings.jira.maximum_collection_items = product_defaults()
            .jira()
            .maximum_allowed_collection_items
            .saturating_add(1);

        assert!(matches!(
            settings.validate(),
            Err(SettingsError::Invalid(_))
        ));
    }

    #[test]
    fn rejects_hours_outside_the_organization_profile() {
        let mut settings = valid_settings();
        settings.hours.utc_offset_minutes = 15;

        assert!(matches!(
            settings.validate(),
            Err(SettingsError::Invalid(_))
        ));
    }

    #[test]
    fn extreme_time_offset_is_rejected_without_overflowing() {
        let mut settings = valid_settings();
        settings.hours.utc_offset_minutes = i16::MIN;
        assert!(settings.validate().is_err());
    }

    fn valid_settings() -> AppSettings {
        let defaults = product_defaults();
        let (base_url, board_id) = valid_jira_scope();
        AppSettings {
            schema_version: SETTINGS_SCHEMA_VERSION,
            jira: JiraSettings {
                base_url,
                email: "person@example.com".to_owned(),
                board_id,
                request_timeout_seconds: defaults.jira().request_timeout_seconds,
                page_size: defaults.jira().page_size,
                maximum_collection_items: defaults.jira().maximum_collection_items,
                maximum_issue_search_results: defaults.jira().maximum_issue_search_results,
                maximum_concurrent_worklog_requests: defaults
                    .jira()
                    .maximum_concurrent_worklog_requests,
            },
            hours: HoursSettings {
                weekly_target: u16::try_from(defaults.hours().suggested_weekly_target_hours)
                    .expect("el objetivo predeterminado entra en u16"),
                utc_offset_minutes: defaults.hours().suggested_utc_offset_minutes,
                maximum_daily_hours: defaults.hours().maximum_daily_hours,
                default_worklog_start_hour: defaults.hours().default_worklog_start_hour,
                default_worklog_start_minute: defaults.hours().default_worklog_start_minute,
                legacy_enable_team_reports: None,
            },
            reports: ReportsSettings {
                enable_team_reports: false,
            },
        }
    }

    fn valid_jira_scope() -> (String, u64) {
        let defaults = product_defaults();
        let Some(site) = defaults.jira().sites.first() else {
            return (TEST_JIRA_SITE.to_owned(), TEST_JIRA_BOARD_ID);
        };
        let board_id = site
            .allowed_board_ids
            .as_ref()
            .and_then(|board_ids| board_ids.first().copied())
            .unwrap_or(TEST_JIRA_BOARD_ID);
        (site.url.clone(), board_id)
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let name = format!("worklogger-settings-{}-{sequence}", std::process::id());
            let path = env::temp_dir().join(name);
            fs::create_dir_all(&path).expect("temporary directory is created");
            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _result = fs::remove_dir_all(&self.path);
        }
    }
}
