//! Shared, secret-free user preferences for the Desktop and MCP frontends.
//! Provider authorization and organization policy remain consumer concerns.

mod storage;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
pub use storage::{SettingsError, SettingsStore};
use worklogger_profile::{Capability, IntegrationModuleId};

pub const SCHEMA_VERSION: u16 = 1;
pub const DEFAULT_MAXIMUM_REPORT_PERIOD_DAYS: u16 = 7;
pub const MAXIMUM_REPORT_PERIOD_DAYS: u16 = 31;
const MAXIMUM_WEEKLY_HOURS: u16 = 168;
const MAXIMUM_OFFSET_MINUTES: i16 = 14 * 60;
const MAXIMUM_DAILY_HOURS: u8 = 24;
const MINUTES_PER_HOUR: u8 = 60;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsDocument {
    pub schema_version: u16,
    pub revision: u64,
    pub language: Option<Language>,
    pub jira: Option<JiraSettings>,
    pub hours: Option<HoursSettings>,
    pub reports: Option<ReportsSettings>,
    pub bitbucket: Option<BitbucketSettings>,
    pub mcp: Option<McpSettings>,
}

impl Default for SettingsDocument {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            revision: 0,
            language: None,
            jira: None,
            hours: None,
            reports: None,
            bitbucket: None,
            mcp: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    #[default]
    English,
    Spanish,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JiraSettings {
    pub base_url: Option<String>,
    pub email: Option<String>,
    pub board_id: Option<u64>,
    pub request_timeout_seconds: Option<u64>,
    pub page_size: Option<u16>,
    #[serde(alias = "maximumReportItems")]
    pub maximum_collection_items: Option<usize>,
    pub maximum_issue_search_results: Option<usize>,
    pub maximum_concurrent_worklog_requests: Option<usize>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HoursSettings {
    pub weekly_target_hours: Option<u16>,
    pub utc_offset_minutes: Option<i16>,
    pub maximum_daily_hours: Option<u8>,
    pub maximum_report_period_days: Option<u16>,
    pub default_worklog_start_hour: Option<u8>,
    pub default_worklog_start_minute: Option<u8>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportsSettings {
    pub enable_team_reports: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BitbucketSettings {
    pub email: Option<String>,
    pub workspaces: Option<BTreeMap<String, BTreeSet<String>>>,
    pub request_timeout_seconds: Option<u64>,
    pub page_size: Option<u16>,
    pub maximum_collection_items: Option<usize>,
    pub pull_request_defaults: Option<PullRequestDefaults>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PullRequestDefaults {
    pub reviewer_account_ids: BTreeSet<String>,
    pub close_source_branch: bool,
}

/// Explicit MCP consent. Shared Desktop preferences never enable these grants.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpSettings {
    pub modules: BTreeMap<IntegrationModuleId, ModuleSettings>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleSettings {
    pub enabled: bool,
    pub capabilities: BTreeSet<Capability>,
}

impl SettingsDocument {
    /// Checks intrinsic bounds while allowing incomplete, secret-free drafts.
    ///
    /// # Errors
    /// Rejects unsupported schemas and intrinsically invalid supplied values.
    pub fn validate(&self) -> Result<(), SettingsError> {
        require(self.schema_version == SCHEMA_VERSION, "schemaVersion")?;
        if let Some(jira) = &self.jira {
            validate_jira(jira)?;
        }
        if let Some(hours) = &self.hours {
            validate_hours(hours)?;
        }
        if let Some(bitbucket) = &self.bitbucket {
            validate_bitbucket(bitbucket)?;
        }
        validate_consent(self.mcp.as_ref())
    }
}

fn validate_jira(jira: &JiraSettings) -> Result<(), SettingsError> {
    require(jira.board_id != Some(0), "jira.boardId")?;
    require(
        jira.request_timeout_seconds != Some(0),
        "jira.requestTimeoutSeconds",
    )?;
    require(jira.page_size != Some(0), "jira.pageSize")?;
    require(
        jira.maximum_collection_items != Some(0),
        "jira.maximumCollectionItems",
    )?;
    require(
        jira.maximum_issue_search_results != Some(0),
        "jira.maximumIssueSearchResults",
    )?;
    require(
        jira.maximum_concurrent_worklog_requests != Some(0),
        "jira.maximumConcurrentWorklogRequests",
    )?;
    validate_email(jira.email.as_deref(), "jira.email")?;
    require(
        jira.base_url
            .as_ref()
            .is_none_or(|url| !url.trim().is_empty()),
        "jira.baseUrl",
    )
}

fn validate_hours(hours: &HoursSettings) -> Result<(), SettingsError> {
    require(
        hours
            .weekly_target_hours
            .is_none_or(|hours| (1..=MAXIMUM_WEEKLY_HOURS).contains(&hours)),
        "hours.weeklyTargetHours",
    )?;
    require(
        hours
            .utc_offset_minutes
            .is_none_or(|offset| offset.unsigned_abs() <= MAXIMUM_OFFSET_MINUTES.unsigned_abs()),
        "hours.utcOffsetMinutes",
    )?;
    require(
        hours
            .maximum_daily_hours
            .is_none_or(|hours| (1..=MAXIMUM_DAILY_HOURS).contains(&hours)),
        "hours.maximumDailyHours",
    )?;
    require(
        hours
            .maximum_report_period_days
            .is_none_or(|days| (1..=MAXIMUM_REPORT_PERIOD_DAYS).contains(&days)),
        "hours.maximumReportPeriodDays",
    )?;
    require(
        hours
            .default_worklog_start_hour
            .is_none_or(|hour| hour < MAXIMUM_DAILY_HOURS),
        "hours.defaultWorklogStartHour",
    )?;
    require(
        hours
            .default_worklog_start_minute
            .is_none_or(|minute| minute < MINUTES_PER_HOUR),
        "hours.defaultWorklogStartMinute",
    )
}

fn validate_bitbucket(settings: &BitbucketSettings) -> Result<(), SettingsError> {
    validate_email(settings.email.as_deref(), "bitbucket.email")?;
    require(
        settings.request_timeout_seconds != Some(0),
        "bitbucket.requestTimeoutSeconds",
    )?;
    require(settings.page_size != Some(0), "bitbucket.pageSize")?;
    require(
        settings.maximum_collection_items != Some(0),
        "bitbucket.maximumCollectionItems",
    )
}

fn validate_email(email: Option<&str>, field: &'static str) -> Result<(), SettingsError> {
    require(
        email.is_none_or(|email| email.contains('@') && !email.chars().any(char::is_whitespace)),
        field,
    )
}

fn validate_consent(settings: Option<&McpSettings>) -> Result<(), SettingsError> {
    let Some(settings) = settings else {
        return Ok(());
    };
    for (module_id, module) in &settings.modules {
        for capability in &module.capabilities {
            require(
                capability.module() == *module_id,
                "mcp.modules.capabilities",
            )?;
            require(
                capability
                    .required_read()
                    .is_none_or(|read| module.capabilities.contains(&read)),
                "mcp.modules.requiredRead",
            )?;
        }
    }
    Ok(())
}

fn require(valid: bool, field: &'static str) -> Result<(), SettingsError> {
    if valid {
        Ok(())
    } else {
        Err(SettingsError::Invalid(field))
    }
}
