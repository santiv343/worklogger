use std::future::Future;
use std::pin::Pin;
use std::time::Duration as StandardDuration;

use hours_core::{DateRange, IssueKey, WeeklyTarget};
use jira_adapter::{IssueDto, JiraClient, JiraSiteUrl, PageLimits, WeeklyReport};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime, UtcOffset, macros::format_description};

use crate::tool_failure::{ERROR_INVALID_PERIOD, RESPONSE_SCHEMA_VERSION, SOURCE_JIRA};
use crate::{JiraConfiguration, ToolFailure};

const DATE_FORMAT: &[time::format_description::FormatItem<'static>] =
    format_description!("[year]-[month]-[day]");
const MINUTES_PER_HOUR: u32 = 60;

pub type OwnHoursFuture<'backend> =
    Pin<Box<dyn Future<Output = Result<WeeklyReport, OwnHoursBackendError>> + Send + 'backend>>;
pub type UnloggedIssuesFuture<'backend> = Pin<
    Box<dyn Future<Output = Result<Vec<UnloggedIssue>, OwnHoursBackendError>> + Send + 'backend>,
>;

pub trait OwnHoursBackend: Send + Sync {
    fn load_own_hours(&self, period: DateRange) -> OwnHoursFuture<'_>;
    fn load_unlogged_issues(&self, period: DateRange) -> UnloggedIssuesFuture<'_>;
}

#[derive(Debug, thiserror::Error)]
pub enum OwnHoursBackendError {
    #[error("the Jira configuration is invalid")]
    InvalidConfiguration,
    #[error("the Jira session is invalid")]
    AuthenticationRequired,
    #[error("the authenticated account cannot view its hours")]
    Forbidden,
    #[error("the requested Jira resource does not exist")]
    NotFound,
    #[error("Jira returned invalid data or inconsistent pagination")]
    InvalidProviderResponse,
    #[error("Jira rejected or could not complete the hours query")]
    Provider { retryable: bool },
}

pub struct JiraOwnHoursBackend {
    client: JiraClient,
    site: JiraSiteUrl,
    board_id: u64,
    target: WeeklyTarget,
    offset: UtcOffset,
    limits: PageLimits,
    maximum_concurrent_requests: usize,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnHoursRequest {
    /// Inclusive start date (YYYY-MM-DD). Omit both dates for the current week through today.
    pub date_from: Option<String>,
    /// Inclusive end date (YYYY-MM-DD). It cannot be later than today.
    pub date_to: Option<String>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnHoursToolResponse {
    pub schema_version: u16,
    pub success: bool,
    pub data: Option<OwnHoursReportData>,
    pub error: Option<ToolFailure>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnloggedIssuesToolResponse {
    pub schema_version: u16,
    pub success: bool,
    pub data: Option<UnloggedIssuesData>,
    pub error: Option<ToolFailure>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnloggedIssuesData {
    pub generated_at: String,
    pub source: String,
    pub board_id: u64,
    pub period: ReportPeriod,
    pub issues: Vec<UnloggedIssue>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnloggedIssue {
    pub issue_key: String,
    pub summary: String,
    pub issue_url: String,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnHoursReportData {
    pub generated_at: String,
    pub source: String,
    pub identity: ReportIdentity,
    pub period: ReportPeriod,
    pub totals: ReportTotals,
    pub days: Vec<ReportDay>,
    pub tasks: Vec<ReportTask>,
    pub entries: Vec<ReportEntry>,
    pub warnings: Vec<ReportWarning>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportIdentity {
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportPeriod {
    pub date_from: String,
    pub date_to: String,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportTotals {
    pub loaded_seconds: u32,
    pub target_seconds: u32,
    pub missing_seconds: u32,
    pub progress_percent: u32,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportDay {
    pub date: String,
    pub duration_seconds: u32,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportTask {
    pub issue_key: String,
    pub summary: String,
    pub issue_url: String,
    pub duration_seconds: u32,
    pub entries: usize,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportEntry {
    pub worklog_id: String,
    pub issue_key: String,
    pub issue_summary: String,
    pub issue_url: String,
    pub started_at: String,
    pub duration_seconds: u32,
    pub comment: String,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportWarning {
    pub issue_key: String,
    pub message: String,
}

impl JiraOwnHoursBackend {
    /// Builds the Jira-backed port without exposing the token in configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when connection limits or credentials are invalid.
    pub fn new(
        configuration: &JiraConfiguration,
        token: String,
    ) -> Result<Self, OwnHoursBackendError> {
        let site = JiraSiteUrl::parse(&configuration.base_url)
            .map_err(|_| OwnHoursBackendError::InvalidConfiguration)?;
        let timeout = StandardDuration::from_secs(configuration.request_timeout_seconds);
        let client = JiraClient::new(site.clone(), configuration.email.clone(), token, timeout)
            .map_err(|_| OwnHoursBackendError::InvalidConfiguration)?;
        Self::from_client(configuration, site, client)
    }

    fn from_client(
        configuration: &JiraConfiguration,
        site: JiraSiteUrl,
        client: JiraClient,
    ) -> Result<Self, OwnHoursBackendError> {
        let hours = configuration
            .hours
            .as_ref()
            .ok_or(OwnHoursBackendError::InvalidConfiguration)?;
        Ok(Self {
            client,
            site,
            board_id: configuration.board_id,
            target: weekly_target(hours.weekly_target_hours)?,
            offset: utc_offset(hours.utc_offset_minutes)?,
            limits: page_limits(configuration)?,
            maximum_concurrent_requests: hours.maximum_concurrent_worklog_requests,
        })
    }
}

impl OwnHoursBackend for JiraOwnHoursBackend {
    fn load_own_hours(&self, period: DateRange) -> OwnHoursFuture<'_> {
        Box::pin(async move {
            self.client
                .load_weekly_report(
                    self.board_id,
                    period,
                    self.target,
                    self.offset,
                    self.limits,
                    self.maximum_concurrent_requests,
                )
                .await
                .map_err(|error| own_hours_error(&error))
        })
    }

    fn load_unlogged_issues(&self, period: DateRange) -> UnloggedIssuesFuture<'_> {
        Box::pin(async move {
            let issues = self
                .client
                .list_unlogged_assigned_sprint_issues(
                    self.board_id,
                    period,
                    self.offset,
                    self.limits,
                    self.maximum_concurrent_requests,
                )
                .await
                .map_err(|error| own_hours_error(&error))?;
            map_unlogged_issues(&self.site, issues)
        })
    }
}

fn own_hours_error(error: &jira_adapter::JiraError) -> OwnHoursBackendError {
    match error {
        jira_adapter::JiraError::AuthenticationRequired => {
            OwnHoursBackendError::AuthenticationRequired
        }
        jira_adapter::JiraError::Forbidden => OwnHoursBackendError::Forbidden,
        jira_adapter::JiraError::NotFound => OwnHoursBackendError::NotFound,
        jira_adapter::JiraError::InvalidPagination
        | jira_adapter::JiraError::CollectionLimitReached
        | jira_adapter::JiraError::InvalidResponse(_)
        | jira_adapter::JiraError::InvalidWorklog => OwnHoursBackendError::InvalidProviderResponse,
        jira_adapter::JiraError::RateLimited { .. }
        | jira_adapter::JiraError::ServerUnavailable
        | jira_adapter::JiraError::Transport(_) => {
            OwnHoursBackendError::Provider { retryable: true }
        }
        _ => OwnHoursBackendError::Provider { retryable: false },
    }
}

/// Resolves and bounds an agent-supplied period against the configured local day.
///
/// # Errors
///
/// Returns a stable error code and message for invalid or future ranges.
pub fn resolve_period(
    request: &OwnHoursRequest,
    utc_offset_minutes: i16,
    maximum_report_period_days: u16,
    now: OffsetDateTime,
) -> Result<DateRange, ToolFailure> {
    let today = local_today(now, utc_offset_minutes)?;
    match (&request.date_from, &request.date_to) {
        (None, None) => current_partial_week(today),
        (Some(date_from), Some(date_to)) => {
            explicit_period(date_from, date_to, today, maximum_report_period_days)
        }
        _ => Err(invalid_period("dateFrom y dateTo deben enviarse juntos")),
    }
}

impl OwnHoursToolResponse {
    #[must_use]
    pub fn success(report: &WeeklyReport, generated_at: OffsetDateTime) -> Self {
        Self {
            schema_version: RESPONSE_SCHEMA_VERSION,
            success: true,
            data: Some(OwnHoursReportData::from_report(report, generated_at)),
            error: None,
        }
    }

    #[must_use]
    pub const fn failure(error: ToolFailure) -> Self {
        Self {
            schema_version: RESPONSE_SCHEMA_VERSION,
            success: false,
            data: None,
            error: Some(error),
        }
    }
}

impl UnloggedIssuesToolResponse {
    #[must_use]
    pub fn success(
        issues: Vec<UnloggedIssue>,
        board_id: u64,
        period: DateRange,
        generated_at: OffsetDateTime,
    ) -> Self {
        Self {
            schema_version: RESPONSE_SCHEMA_VERSION,
            success: true,
            data: Some(UnloggedIssuesData {
                generated_at: generated_at.to_string(),
                source: SOURCE_JIRA.to_owned(),
                board_id,
                period: period_data(period),
                issues,
            }),
            error: None,
        }
    }

    #[must_use]
    pub const fn failure(error: ToolFailure) -> Self {
        Self {
            schema_version: RESPONSE_SCHEMA_VERSION,
            success: false,
            data: None,
            error: Some(error),
        }
    }
}

impl OwnHoursReportData {
    fn from_report(report: &WeeklyReport, generated_at: OffsetDateTime) -> Self {
        Self {
            generated_at: generated_at.to_string(),
            source: SOURCE_JIRA.to_owned(),
            identity: identity(report),
            period: report_period(report),
            totals: report_totals(report),
            days: report_days(report),
            tasks: report_tasks(report),
            entries: report_entries(report),
            warnings: report_warnings(report),
        }
    }
}

fn weekly_target(hours: u16) -> Result<WeeklyTarget, OwnHoursBackendError> {
    let minutes = u32::from(hours).saturating_mul(MINUTES_PER_HOUR);
    WeeklyTarget::from_minutes(minutes).map_err(|_| OwnHoursBackendError::InvalidConfiguration)
}

fn utc_offset(minutes: i16) -> Result<UtcOffset, OwnHoursBackendError> {
    UtcOffset::from_whole_seconds(i32::from(minutes) * 60)
        .map_err(|_| OwnHoursBackendError::InvalidConfiguration)
}

fn page_limits(configuration: &JiraConfiguration) -> Result<PageLimits, OwnHoursBackendError> {
    PageLimits::new(
        configuration.page_size,
        configuration.maximum_collection_items,
    )
    .map_err(|_| OwnHoursBackendError::InvalidConfiguration)
}

fn local_today(now: OffsetDateTime, minutes: i16) -> Result<Date, ToolFailure> {
    let offset = UtcOffset::from_whole_seconds(i32::from(minutes) * 60)
        .map_err(|_| invalid_period("utcOffsetMinutes is invalid"))?;
    Ok(now.to_offset(offset).date())
}

fn current_partial_week(today: Date) -> Result<DateRange, ToolFailure> {
    let week = DateRange::week_containing(today);
    DateRange::new(week.start(), today).map_err(|_| invalid_period("the period is invalid"))
}

fn explicit_period(
    date_from: &str,
    date_to: &str,
    today: Date,
    maximum_report_period_days: u16,
) -> Result<DateRange, ToolFailure> {
    let start = parse_date(date_from)?;
    let end = parse_date(date_to)?;
    if end > today {
        return Err(invalid_period("dateTo no puede ser posterior a hoy"));
    }
    let period =
        DateRange::new(start, end).map_err(|_| invalid_period("dateFrom supera dateTo"))?;
    if period.day_count() > u64::from(maximum_report_period_days) {
        return Err(invalid_period(format!(
            "the period cannot exceed {maximum_report_period_days} days"
        )));
    }
    Ok(period)
}

fn parse_date(value: &str) -> Result<Date, ToolFailure> {
    Date::parse(value, DATE_FORMAT).map_err(|_| invalid_period("las fechas deben usar YYYY-MM-DD"))
}

fn invalid_period(message: impl Into<String>) -> ToolFailure {
    ToolFailure {
        code: ERROR_INVALID_PERIOD.to_owned(),
        message: message.into(),
        retryable: false,
    }
}

fn identity(report: &WeeklyReport) -> ReportIdentity {
    ReportIdentity {
        display_name: report.identity.display_name.clone(),
    }
}

fn report_period(report: &WeeklyReport) -> ReportPeriod {
    period_data(report.summary.period)
}

fn period_data(period: DateRange) -> ReportPeriod {
    ReportPeriod {
        date_from: period.start().to_string(),
        date_to: period.end().to_string(),
    }
}

fn map_unlogged_issues(
    site: &JiraSiteUrl,
    issues: Vec<IssueDto>,
) -> Result<Vec<UnloggedIssue>, OwnHoursBackendError> {
    issues
        .into_iter()
        .map(|issue| map_unlogged_issue(site, issue))
        .collect()
}

fn map_unlogged_issue(
    site: &JiraSiteUrl,
    issue: IssueDto,
) -> Result<UnloggedIssue, OwnHoursBackendError> {
    let key =
        IssueKey::new(&issue.key).map_err(|_| OwnHoursBackendError::InvalidProviderResponse)?;
    Ok(UnloggedIssue {
        issue_key: issue.key,
        summary: issue.fields.summary,
        issue_url: site.issue_browser_url(&key),
    })
}

const fn report_totals(report: &WeeklyReport) -> ReportTotals {
    ReportTotals {
        loaded_seconds: report.summary.loaded_seconds,
        target_seconds: report.summary.target.duration().seconds(),
        missing_seconds: report.summary.missing_seconds,
        progress_percent: report.summary.progress_percent,
    }
}

fn report_days(report: &WeeklyReport) -> Vec<ReportDay> {
    report
        .summary
        .days
        .iter()
        .map(|day| ReportDay {
            date: day.date.to_string(),
            duration_seconds: day.duration_seconds,
        })
        .collect()
}

fn report_tasks(report: &WeeklyReport) -> Vec<ReportTask> {
    report
        .summary
        .tasks
        .iter()
        .map(|task| ReportTask {
            issue_key: task.issue_key.as_str().to_owned(),
            summary: task.summary.clone(),
            issue_url: task.issue_url.clone(),
            duration_seconds: task.duration_seconds,
            entries: task.entries,
        })
        .collect()
}

fn report_entries(report: &WeeklyReport) -> Vec<ReportEntry> {
    report
        .worklogs
        .iter()
        .map(|entry| ReportEntry {
            worklog_id: entry.id.clone(),
            issue_key: entry.issue_key.as_str().to_owned(),
            issue_summary: entry.issue_summary.clone(),
            issue_url: entry.issue_url.clone(),
            started_at: entry.started.to_string(),
            duration_seconds: entry.duration.seconds(),
            comment: entry.comment.clone(),
        })
        .collect()
}

fn report_warnings(report: &WeeklyReport) -> Vec<ReportWarning> {
    report
        .warnings
        .iter()
        .map(|warning| ReportWarning {
            issue_key: warning.issue_key.clone(),
            message: warning.message.clone(),
        })
        .collect()
}
