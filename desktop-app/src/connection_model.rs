use jira_adapter::{ProjectPermissions, WeeklyReport};

#[derive(Clone, PartialEq)]
pub(crate) struct ConnectionRequest {
    pub site: String,
    pub email: String,
    pub token: String,
    pub board_id: u64,
    pub weekly_target_hours: u32,
    pub utc_offset_minutes: i16,
    pub request_timeout_seconds: u64,
    pub page_size: u16,
    pub maximum_collection_items: usize,
    pub maximum_issue_search_results: usize,
    pub maximum_concurrent_worklog_requests: usize,
    pub maximum_daily_hours: u8,
    pub default_worklog_start_hour: u8,
    pub default_worklog_start_minute: u8,
    pub enable_team_reports: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConnectionConfiguration {
    pub jira: JiraConfiguration,
    pub hours: HoursConfiguration,
    pub reports: ReportsConfiguration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct JiraConfiguration {
    pub site: String,
    pub email: String,
    pub board_id: u64,
    pub request_timeout_seconds: u64,
    pub page_size: u16,
    pub maximum_collection_items: usize,
    pub maximum_issue_search_results: usize,
    pub maximum_concurrent_worklog_requests: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HoursConfiguration {
    pub weekly_target_hours: u16,
    pub utc_offset_minutes: i16,
    pub maximum_daily_hours: u8,
    pub default_worklog_start_hour: u8,
    pub default_worklog_start_minute: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReportsConfiguration {
    pub enable_team_reports: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConfigurationUpdate {
    pub replacement_token: Option<String>,
    pub jira: JiraConfigurationUpdate,
    pub hours: HoursConfiguration,
    pub reports: ReportsConfiguration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct JiraConfigurationUpdate {
    pub board_id: u64,
    pub request_timeout_seconds: u64,
    pub page_size: u16,
    pub maximum_collection_items: usize,
    pub maximum_issue_search_results: usize,
    pub maximum_concurrent_worklog_requests: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CreateWorklogCommand {
    pub issue_key: String,
    pub date: String,
    pub minutes: u32,
    pub comment: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DeleteWorklogCommand {
    pub issue_key: String,
    pub worklog_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UpdateWorklogCommand {
    pub issue_key: String,
    pub worklog_id: String,
    pub date: String,
    pub minutes: u32,
    pub comment: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ConnectedSession {
    pub report: WeeklyReport,
    pub configuration: ConnectionConfiguration,
    pub can_view_team_report: bool,
    pub project_permissions: Option<ProjectPermissions>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum WorklogMutationOutcome {
    Refreshed(Box<ConnectedSession>),
    CommittedWithoutRefresh(String),
}

#[derive(Clone, PartialEq)]
pub(crate) struct BoardDiscoveryRequest {
    pub site: String,
    pub email: String,
    pub token: String,
    pub request_timeout_seconds: u64,
    pub page_size: u16,
    pub maximum_items: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AccessibleBoard {
    pub id: u64,
    pub name: String,
    pub board_type: String,
    pub project_key: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BoardDiscovery {
    pub identity: String,
    pub boards: Vec<AccessibleBoard>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AccessibleIssue {
    pub key: String,
    pub summary: String,
}
