#[cfg(not(windows))]
use std::sync::{Mutex, MutexGuard};
use std::time::Duration as RequestTimeout;

use hours_core::{DateRange, WeeklyTarget};
#[cfg(feature = "reports")]
use jira_adapter::TeamReport;
use jira_adapter::{JiraClient, JiraError, JiraSiteUrl, PageLimits, WorklogInput};
use time::{Date, OffsetDateTime, UtcOffset, format_description};

use crate::connection_model::{
    AccessibleBoard, AccessibleIssue, BoardDiscovery, BoardDiscoveryRequest, ConfigurationUpdate,
    ConnectedSession, ConnectionConfiguration, ConnectionRequest, CreateWorklogCommand,
    DeleteWorklogCommand, HoursConfiguration, JiraConfiguration, UpdateWorklogCommand,
    WorklogMutationOutcome,
};
use crate::copy::text;
#[cfg(windows)]
use crate::credentials::{
    CredentialError, CredentialPurpose, CredentialStore, CredentialTransactionGuard,
};
use crate::defaults::{ensure_defaults_valid, product_defaults};
#[cfg(not(windows))]
use crate::development_config;
#[cfg(windows)]
use crate::settings::{AppSettings, HoursSettings, JiraSettings, SettingsError, SettingsStore};

const SECONDS_PER_MINUTE: u32 = 60;
const SECONDS_PER_MINUTE_SIGNED: i32 = 60;
#[cfg(not(windows))]
static DEVELOPMENT_REQUEST: Mutex<Option<ConnectionRequest>> = Mutex::new(None);

pub(crate) async fn discover_boards(
    request: BoardDiscoveryRequest,
) -> Result<BoardDiscovery, String> {
    validate_organization_site(&request.site)?;
    let site = JiraSiteUrl::parse(&request.site).map_err(display_jira_error)?;
    let client = discovery_client(&request, site)?;
    let limits =
        PageLimits::new(request.page_size, request.maximum_items).map_err(display_jira_error)?;
    let identity = client.current_user().await.map_err(display_jira_error)?;
    let boards = client
        .list_boards(limits)
        .await
        .map_err(display_jira_error)?
        .into_iter()
        .filter(|board| organization_allows_board(&request.site, board.id))
        .map(map_board)
        .collect();
    Ok(BoardDiscovery {
        identity: identity.display_name,
        boards,
    })
}

fn organization_allows_board(site_url: &str, board_id: u64) -> bool {
    product_defaults().allows_jira_board(site_url, board_id)
}

fn validate_organization_site(site_url: &str) -> Result<(), String> {
    ensure_defaults_valid()?;
    let allowed = product_defaults()
        .modules
        .jira
        .as_ref()
        .is_some_and(|jira| jira.allows_site(site_url));
    allowed
        .then_some(())
        .ok_or_else(|| text("settings.error.boardOutsideScope").to_owned())
}

fn validate_organization_request(request: &ConnectionRequest) -> Result<(), String> {
    ensure_defaults_valid()?;
    organization_allows_board(&request.site, request.board_id)
        .then_some(())
        .ok_or_else(|| text("settings.error.boardOutsideScope").to_owned())
}

pub(crate) async fn discover_configured_boards() -> Result<Vec<AccessibleBoard>, String> {
    let request = required_saved_request()?;
    let discovery = discover_boards(BoardDiscoveryRequest {
        site: request.site,
        email: request.email,
        token: request.token,
        request_timeout_seconds: request.request_timeout_seconds,
        page_size: request.page_size,
        maximum_items: request.maximum_collection_items,
    })
    .await?;
    Ok(discovery.boards)
}

fn discovery_client(
    request: &BoardDiscoveryRequest,
    site: JiraSiteUrl,
) -> Result<JiraClient, String> {
    JiraClient::new(
        site,
        request.email.clone(),
        request.token.clone(),
        RequestTimeout::from_secs(request.request_timeout_seconds),
    )
    .map_err(display_jira_error)
}

fn map_board(board: jira_adapter::BoardDto) -> AccessibleBoard {
    let project_key = board.location.and_then(|location| location.project_key);
    AccessibleBoard {
        id: board.id,
        name: board.name,
        board_type: board.board_type,
        project_key,
    }
}

pub(crate) async fn connect_and_save(
    request: ConnectionRequest,
) -> Result<ConnectedSession, String> {
    let session = connect(&request).await?;
    persist(&request)?;
    Ok(session)
}

pub(crate) async fn restore() -> Result<Option<ConnectedSession>, String> {
    let Some(request) = saved_request()? else {
        return Ok(None);
    };
    connect(&request).await.map(Some)
}

pub(crate) async fn load_period(period: DateRange) -> Result<ConnectedSession, String> {
    let request = required_saved_request()?;
    validate_period(period, &request)?;
    load_report(&request, period).await
}

pub(crate) async fn update_configuration(
    update: ConfigurationUpdate,
) -> Result<ConnectedSession, String> {
    let mut request = required_saved_request()?;
    apply_configuration(&mut request, update);
    let session = connect(&request).await?;
    persist(&request)?;
    Ok(session)
}

fn apply_configuration(request: &mut ConnectionRequest, update: ConfigurationUpdate) {
    if let Some(token) = update.replacement_token {
        request.token = token;
    }
    request.board_id = update.jira.board_id;
    request.weekly_target_hours = u32::from(update.hours.weekly_target_hours);
    request.utc_offset_minutes = update.hours.utc_offset_minutes;
    request.request_timeout_seconds = update.jira.request_timeout_seconds;
    request.page_size = update.jira.page_size;
    request.maximum_collection_items = update.jira.maximum_collection_items;
    request.maximum_issue_search_results = update.jira.maximum_issue_search_results;
    request.maximum_concurrent_worklog_requests = update.jira.maximum_concurrent_worklog_requests;
    request.maximum_daily_hours = update.hours.maximum_daily_hours;
    request.default_worklog_start_hour = update.hours.default_worklog_start_hour;
    request.default_worklog_start_minute = update.hours.default_worklog_start_minute;
    request.enable_team_reports = update.reports.enable_team_reports;
}

pub(crate) async fn search_issues(query: String) -> Result<Vec<AccessibleIssue>, String> {
    let request = required_saved_request()?;
    let client = jira_client(&request)?;
    let limits = PageLimits::new(request.page_size, request.maximum_collection_items)
        .map_err(display_jira_error)?;
    client
        .search_board_issues(
            request.board_id,
            &query,
            limits,
            request.maximum_issue_search_results,
        )
        .await
        .map_err(display_jira_error)
        .map(|issues| issues.into_iter().map(map_issue).collect())
}

pub(crate) async fn unlogged_assigned_sprint_issues() -> Result<Vec<AccessibleIssue>, String> {
    let request = required_saved_request()?;
    let client = jira_client(&request)?;
    let limits = PageLimits::new(request.page_size, request.maximum_collection_items)
        .map_err(display_jira_error)?;
    let offset = utc_offset(request.utc_offset_minutes)?;
    client
        .list_unlogged_assigned_sprint_issues(
            request.board_id,
            current_week(offset),
            offset,
            limits,
            request.maximum_concurrent_worklog_requests,
        )
        .await
        .map_err(display_jira_error)
        .map(|issues| issues.into_iter().map(map_issue).collect())
}

#[cfg(feature = "reports")]
pub(crate) async fn load_team_period(period: DateRange) -> Result<TeamReport, String> {
    let request = required_saved_request()?;
    if !request.enable_team_reports {
        return Err(display_jira_error(JiraError::Forbidden));
    }
    let client = jira_client(&request)?;
    let permissions = client
        .project_permissions(request.board_id)
        .await
        .map_err(display_jira_error)?;
    if !team_report_permission_granted(permissions) {
        return Err(display_jira_error(JiraError::Forbidden));
    }
    let limits = PageLimits::new(request.page_size, request.maximum_collection_items)
        .map_err(display_jira_error)?;
    let offset = utc_offset(request.utc_offset_minutes)?;
    client
        .load_team_report(
            request.board_id,
            period,
            offset,
            limits,
            request.maximum_concurrent_worklog_requests,
        )
        .await
        .map_err(display_jira_error)
}

pub(crate) fn team_report_permission_granted(
    permissions: jira_adapter::ProjectPermissions,
) -> bool {
    let defaults = product_defaults();
    defaults.modules.reports.as_ref().is_some_and(|reports| {
        reports
            .team_report_access_policy
            .allows(permissions.browse_projects, permissions.administer_projects)
    })
}

fn map_issue(issue: jira_adapter::IssueDto) -> AccessibleIssue {
    AccessibleIssue {
        key: issue.key,
        summary: issue.fields.summary,
    }
}

#[cfg(windows)]
pub(crate) fn disconnect() -> Result<(), String> {
    let _transaction_guard =
        CredentialTransactionGuard::acquire().map_err(display_credential_error)?;
    let store = SettingsStore::for_current_user().map_err(display_settings_error)?;
    let Some(settings) = store.load().map_err(display_settings_error)? else {
        return store.clear().map_err(display_settings_error);
    };
    disconnect_configured(&store, &settings)
}

#[cfg(windows)]
fn disconnect_configured(store: &SettingsStore, settings: &AppSettings) -> Result<(), String> {
    let credentials = CredentialStore::for_purpose(CredentialPurpose::Desktop)
        .map_err(display_credential_error)?;
    let token = credentials
        .load_api_token(&settings.jira.base_url, &settings.jira.email)
        .map_err(display_credential_error)?;
    store.clear().map_err(display_settings_error)?;
    let deletion = credentials
        .delete_api_token(&settings.jira.base_url, &settings.jira.email)
        .map_err(display_credential_error);
    if let Err(error) = deletion {
        return Err(disconnect_error(
            error,
            rollback_disconnect(store, credentials, settings, token),
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn rollback_disconnect(
    store: &SettingsStore,
    credentials: CredentialStore,
    settings: &AppSettings,
    token: Option<String>,
) -> Result<(), String> {
    let token_result = restore_stored_token(
        &credentials,
        &settings.jira.base_url,
        &settings.jira.email,
        token,
    );
    let settings_result = store.save(settings).map_err(display_settings_error);
    combine_disconnect_results(token_result, settings_result)
}

#[cfg(windows)]
fn combine_disconnect_results(
    token: Result<(), String>,
    settings: Result<(), String>,
) -> Result<(), String> {
    match (token, settings) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(token_error), Err(settings_error)) => Err(format!("{token_error}; {settings_error}")),
    }
}

#[cfg(windows)]
fn disconnect_error(error: String, rollback: Result<(), String>) -> String {
    match rollback {
        Ok(()) => error,
        Err(rollback_error) => format!("{error}; {rollback_error}"),
    }
}

#[cfg(not(windows))]
pub(crate) fn disconnect() -> Result<(), String> {
    *development_request()? = None;
    Ok(())
}

pub(crate) async fn create_worklog(
    command: CreateWorklogCommand,
) -> Result<WorklogMutationOutcome, String> {
    let request =
        saved_request()?.ok_or_else(|| text("connection.requiredForCreate").to_owned())?;
    let client = jira_client(&request)?;
    let issue_key = hours_core::IssueKey::new(&command.issue_key).map_err(display_error)?;
    ensure_issue_in_board(&client, &request, &issue_key).await?;
    let input = worklog_input(&command, &request)?;
    client
        .create_own_worklog(&issue_key, &input)
        .await
        .map_err(display_jira_error)?;
    Ok(committed_refresh_result(connect(&request).await))
}

pub(crate) async fn delete_worklog(
    command: DeleteWorklogCommand,
) -> Result<WorklogMutationOutcome, String> {
    let request = required_saved_request()?;
    let client = jira_client(&request)?;
    let issue_key = hours_core::IssueKey::new(command.issue_key).map_err(display_error)?;
    ensure_issue_in_board(&client, &request, &issue_key).await?;
    client
        .delete_own_worklog(&issue_key, &command.worklog_id)
        .await
        .map_err(display_jira_error)?;
    Ok(committed_refresh_result(connect(&request).await))
}

pub(crate) async fn update_worklog(
    command: UpdateWorklogCommand,
) -> Result<WorklogMutationOutcome, String> {
    let request = required_saved_request()?;
    let client = jira_client(&request)?;
    let issue_key = hours_core::IssueKey::new(&command.issue_key).map_err(display_error)?;
    ensure_issue_in_board(&client, &request, &issue_key).await?;
    let input = update_input(&command, &request)?;
    client
        .update_own_worklog(&issue_key, &command.worklog_id, &input)
        .await
        .map_err(display_jira_error)?;
    Ok(committed_refresh_result(connect(&request).await))
}

async fn ensure_issue_in_board(
    client: &JiraClient,
    request: &ConnectionRequest,
    issue_key: &hours_core::IssueKey,
) -> Result<(), String> {
    let limits = PageLimits::new(request.page_size, request.maximum_collection_items)
        .map_err(display_jira_error)?;
    let allowed = client
        .board_contains_issue(request.board_id, issue_key, limits)
        .await
        .map_err(display_jira_error)?;
    validate_issue_membership(allowed)
}

fn validate_issue_membership(allowed: bool) -> Result<(), String> {
    allowed
        .then_some(())
        .ok_or_else(|| text("settings.error.boardOutsideScope").to_owned())
}

fn committed_refresh_result(refresh: Result<ConnectedSession, String>) -> WorklogMutationOutcome {
    match refresh {
        Ok(session) => WorklogMutationOutcome::Refreshed(Box::new(session)),
        Err(error) => WorklogMutationOutcome::CommittedWithoutRefresh(format!(
            "{} {error}",
            text("connection.committedRefreshFailed")
        )),
    }
}

fn required_saved_request() -> Result<ConnectionRequest, String> {
    saved_request()?.ok_or_else(|| text("connection.requiredForChange").to_owned())
}

#[cfg(windows)]
fn saved_request() -> Result<Option<ConnectionRequest>, String> {
    let store = SettingsStore::for_current_user().map_err(display_settings_error)?;
    let Some(settings) = store.load().map_err(display_settings_error)? else {
        return Ok(None);
    };
    let credentials = CredentialStore::for_purpose(CredentialPurpose::Desktop)
        .map_err(display_credential_error)?;
    let Some(token) = credentials
        .load_api_token(&settings.jira.base_url, &settings.jira.email)
        .map_err(display_credential_error)?
    else {
        return Ok(None);
    };
    Ok(Some(request_from(settings, token)))
}

#[cfg(not(windows))]
fn saved_request() -> Result<Option<ConnectionRequest>, String> {
    let request = development_request()?.clone();
    match request {
        Some(request) => Ok(Some(request)),
        None => development_config::load_request(),
    }
}

#[cfg(not(windows))]
fn development_request() -> Result<MutexGuard<'static, Option<ConnectionRequest>>, String> {
    DEVELOPMENT_REQUEST
        .lock()
        .map_err(|_| text("connection.sessionUnavailable").to_owned())
}

async fn connect(request: &ConnectionRequest) -> Result<ConnectedSession, String> {
    let offset = utc_offset(request.utc_offset_minutes)?;
    let period = current_week(offset);
    load_report(request, period).await
}

async fn load_report(
    request: &ConnectionRequest,
    period: DateRange,
) -> Result<ConnectedSession, String> {
    validate_organization_request(request)?;
    let site = JiraSiteUrl::parse(&request.site).map_err(display_jira_error)?;
    let target = weekly_target(request.weekly_target_hours)?;
    let offset = utc_offset(request.utc_offset_minutes)?;
    let limits = PageLimits::new(request.page_size, request.maximum_collection_items)
        .map_err(display_jira_error)?;
    let client = jira_client_with_site(request, site)?;
    let report = client
        .load_weekly_report(
            request.board_id,
            period,
            target,
            offset,
            limits,
            request.maximum_concurrent_worklog_requests,
        )
        .await
        .map_err(display_jira_error)?;
    let project_permissions = client.project_permissions(request.board_id).await.ok();
    Ok(ConnectedSession {
        report,
        configuration: configuration_from(request)?,
        can_view_team_report: request.enable_team_reports
            && project_permissions.is_some_and(team_report_permission_granted),
        project_permissions,
    })
}

fn configuration_from(request: &ConnectionRequest) -> Result<ConnectionConfiguration, String> {
    Ok(ConnectionConfiguration {
        jira: JiraConfiguration {
            site: request.site.clone(),
            email: request.email.clone(),
            board_id: request.board_id,
            request_timeout_seconds: request.request_timeout_seconds,
            page_size: request.page_size,
            maximum_collection_items: request.maximum_collection_items,
            maximum_issue_search_results: request.maximum_issue_search_results,
            maximum_concurrent_worklog_requests: request.maximum_concurrent_worklog_requests,
        },
        hours: hours_configuration(request)?,
        reports: crate::connection_model::ReportsConfiguration {
            enable_team_reports: request.enable_team_reports,
        },
    })
}

fn hours_configuration(request: &ConnectionRequest) -> Result<HoursConfiguration, String> {
    Ok(HoursConfiguration {
        weekly_target_hours: request
            .weekly_target_hours
            .try_into()
            .map_err(|_| text("connection.targetDoesNotFit").to_owned())?,
        utc_offset_minutes: request.utc_offset_minutes,
        maximum_daily_hours: request.maximum_daily_hours,
        default_worklog_start_hour: request.default_worklog_start_hour,
        default_worklog_start_minute: request.default_worklog_start_minute,
    })
}

fn jira_client(request: &ConnectionRequest) -> Result<JiraClient, String> {
    validate_organization_request(request)?;
    let site = JiraSiteUrl::parse(&request.site).map_err(display_jira_error)?;
    jira_client_with_site(request, site)
}

fn jira_client_with_site(
    request: &ConnectionRequest,
    site: JiraSiteUrl,
) -> Result<JiraClient, String> {
    JiraClient::new(
        site,
        request.email.clone(),
        request.token.clone(),
        RequestTimeout::from_secs(request.request_timeout_seconds),
    )
    .map_err(display_jira_error)
}

fn worklog_input(
    command: &CreateWorklogCommand,
    request: &ConnectionRequest,
) -> Result<WorklogInput, String> {
    let date = parse_date(&command.date)?;
    validate_date(date, request)?;
    let local_time = date
        .with_hms(
            request.default_worklog_start_hour,
            request.default_worklog_start_minute,
            0,
        )
        .map_err(display_error)?;
    let started = local_time.assume_offset(utc_offset(request.utc_offset_minutes)?);
    validate_daily_duration(command.minutes, request.maximum_daily_hours)?;
    let seconds = command
        .minutes
        .checked_mul(SECONDS_PER_MINUTE)
        .ok_or_else(|| text("connection.durationTooLarge").to_owned())?;
    WorklogInput::new(started, seconds, command.comment.clone()).map_err(display_jira_error)
}

fn update_input(
    command: &UpdateWorklogCommand,
    request: &ConnectionRequest,
) -> Result<WorklogInput, String> {
    worklog_input(
        &CreateWorklogCommand {
            issue_key: command.issue_key.clone(),
            date: command.date.clone(),
            minutes: command.minutes,
            comment: command.comment.clone(),
        },
        request,
    )
}

fn validate_daily_duration(minutes: u32, maximum_hours: u8) -> Result<(), String> {
    let maximum_minutes = u32::from(maximum_hours)
        .checked_mul(SECONDS_PER_MINUTE)
        .ok_or_else(|| text("connection.invalidDailyMaximum").to_owned())?;
    if minutes > maximum_minutes {
        return Err(text("connection.exceedsDailyMaximum").to_owned());
    }
    Ok(())
}

fn parse_date(value: &str) -> Result<Date, String> {
    let format = format_description::parse("[year]-[month]-[day]").map_err(display_error)?;
    Date::parse(value, &format).map_err(|_| text("connection.invalidDate").to_owned())
}

#[cfg(windows)]
struct DesktopPersistSnapshot {
    store: SettingsStore,
    credentials: CredentialStore,
    #[cfg(feature = "mcp-management")]
    settings: Option<AppSettings>,
    token: Option<String>,
}

#[cfg(windows)]
fn desktop_persist_snapshot(request: &ConnectionRequest) -> Result<DesktopPersistSnapshot, String> {
    let store = SettingsStore::for_current_user().map_err(display_settings_error)?;
    let credentials = CredentialStore::for_purpose(CredentialPurpose::Desktop)
        .map_err(display_credential_error)?;
    #[cfg(feature = "mcp-management")]
    let settings = store.load().map_err(display_settings_error)?;
    let token = credentials
        .load_api_token(&request.site, &request.email)
        .map_err(display_credential_error)?;
    Ok(DesktopPersistSnapshot {
        store,
        credentials,
        #[cfg(feature = "mcp-management")]
        settings,
        token,
    })
}

#[cfg(windows)]
fn persist(request: &ConnectionRequest) -> Result<(), String> {
    let _transaction_guard =
        CredentialTransactionGuard::acquire().map_err(display_credential_error)?;
    let settings = settings_from(request)?;
    let snapshot = desktop_persist_snapshot(request)?;
    snapshot
        .credentials
        .save_api_token(&request.site, &request.email, &request.token)
        .map_err(display_credential_error)?;
    if let Err(error) = snapshot.store.save(&settings) {
        restore_api_token(&snapshot.credentials, request, snapshot.token)?;
        return Err(display_settings_error(error));
    }
    #[cfg(feature = "mcp-management")]
    if let Err(error) = synchronize_mcp(request) {
        rollback_desktop_persist(snapshot, request)?;
        return Err(error);
    }
    Ok(())
}

#[cfg(all(windows, feature = "mcp-management"))]
fn rollback_desktop_persist(
    snapshot: DesktopPersistSnapshot,
    request: &ConnectionRequest,
) -> Result<(), String> {
    let settings_result = restore_settings(&snapshot.store, snapshot.settings);
    let token_result = restore_api_token(&snapshot.credentials, request, snapshot.token);
    match (settings_result, token_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(settings_error), Err(token_error)) => Err(format!("{settings_error}; {token_error}")),
    }
}

#[cfg(all(windows, feature = "mcp-management"))]
fn restore_settings(store: &SettingsStore, previous: Option<AppSettings>) -> Result<(), String> {
    match previous {
        Some(settings) => store.save(&settings),
        None => store.clear(),
    }
    .map_err(display_settings_error)
}

#[cfg(windows)]
fn restore_api_token(
    credentials: &CredentialStore,
    request: &ConnectionRequest,
    previous_token: Option<String>,
) -> Result<(), String> {
    restore_stored_token(credentials, &request.site, &request.email, previous_token)
}

#[cfg(windows)]
fn restore_stored_token(
    credentials: &CredentialStore,
    site: &str,
    email: &str,
    previous_token: Option<String>,
) -> Result<(), String> {
    let result = match previous_token {
        Some(token) => credentials.save_api_token(site, email, &token),
        None => credentials.delete_api_token(site, email),
    };
    result.map_err(display_credential_error)
}

#[cfg(not(windows))]
fn persist(request: &ConnectionRequest) -> Result<(), String> {
    let previous = {
        let mut state = development_request()?;
        state.replace(request.clone())
    };
    #[cfg(feature = "mcp-management")]
    if let Err(error) = synchronize_mcp(request) {
        *development_request()? = previous;
        return Err(error);
    }
    Ok(())
}

#[cfg(feature = "mcp-management")]
fn synchronize_mcp(request: &ConnectionRequest) -> Result<(), String> {
    crate::mcp_management::synchronize(request)
}

#[cfg(windows)]
fn settings_from(request: &ConnectionRequest) -> Result<AppSettings, String> {
    let weekly_target = u16::try_from(request.weekly_target_hours)
        .map_err(|_| text("connection.targetDoesNotFit").to_owned())?;
    Ok(AppSettings {
        schema_version: crate::settings::SETTINGS_SCHEMA_VERSION,
        jira: JiraSettings {
            base_url: request.site.clone(),
            email: request.email.clone(),
            board_id: request.board_id,
            request_timeout_seconds: request.request_timeout_seconds,
            page_size: request.page_size,
            maximum_collection_items: request.maximum_collection_items,
            maximum_issue_search_results: request.maximum_issue_search_results,
            maximum_concurrent_worklog_requests: request.maximum_concurrent_worklog_requests,
        },
        hours: HoursSettings {
            weekly_target,
            utc_offset_minutes: request.utc_offset_minutes,
            maximum_daily_hours: request.maximum_daily_hours,
            default_worklog_start_hour: request.default_worklog_start_hour,
            default_worklog_start_minute: request.default_worklog_start_minute,
            legacy_enable_team_reports: None,
        },
        reports: crate::settings::ReportsSettings {
            enable_team_reports: request.enable_team_reports,
        },
    })
}

#[cfg(windows)]
fn request_from(settings: AppSettings, token: String) -> ConnectionRequest {
    ConnectionRequest {
        site: settings.jira.base_url,
        email: settings.jira.email,
        token,
        board_id: settings.jira.board_id,
        request_timeout_seconds: settings.jira.request_timeout_seconds,
        page_size: settings.jira.page_size,
        maximum_collection_items: settings.jira.maximum_collection_items,
        maximum_issue_search_results: settings.jira.maximum_issue_search_results,
        maximum_concurrent_worklog_requests: settings.jira.maximum_concurrent_worklog_requests,
        weekly_target_hours: u32::from(settings.hours.weekly_target),
        utc_offset_minutes: settings.hours.utc_offset_minutes,
        maximum_daily_hours: settings.hours.maximum_daily_hours,
        default_worklog_start_hour: settings.hours.default_worklog_start_hour,
        default_worklog_start_minute: settings.hours.default_worklog_start_minute,
        enable_team_reports: settings.reports.enable_team_reports,
    }
}

fn weekly_target(hours: u32) -> Result<WeeklyTarget, String> {
    let minutes = hours
        .checked_mul(SECONDS_PER_MINUTE)
        .ok_or_else(|| text("connection.targetTooLarge").to_owned())?;
    WeeklyTarget::from_minutes(minutes).map_err(display_error)
}

fn utc_offset(minutes: i16) -> Result<UtcOffset, String> {
    let seconds = i32::from(minutes)
        .checked_mul(SECONDS_PER_MINUTE_SIGNED)
        .ok_or_else(|| text("connection.invalidTimeZone").to_owned())?;
    UtcOffset::from_whole_seconds(seconds)
        .map_err(|_| text("connection.invalidTimeZone").to_owned())
}

fn current_week(offset: UtcOffset) -> DateRange {
    let local_date = OffsetDateTime::now_utc().to_offset(offset).date();
    let week = DateRange::week_containing(local_date);
    DateRange::new(week.start(), local_date).expect("current date belongs to its week")
}

fn validate_period(period: DateRange, request: &ConnectionRequest) -> Result<(), String> {
    validate_date(period.end(), request)
}

fn validate_date(date: Date, request: &ConnectionRequest) -> Result<(), String> {
    let offset = utc_offset(request.utc_offset_minutes)?;
    let today = OffsetDateTime::now_utc().to_offset(offset).date();
    ensure_not_future(date, today)
}

fn ensure_not_future(date: Date, today: Date) -> Result<(), String> {
    if date > today {
        return Err(text("connection.futureDate").to_owned());
    }
    Ok(())
}

fn display_error(_error: impl std::fmt::Display) -> String {
    text("connection.invalidInput").to_owned()
}

#[cfg(windows)]
fn display_settings_error(error: SettingsError) -> String {
    let key = match &error {
        SettingsError::Invalid(_) => "settings.error.invalid",
        SettingsError::Decode { .. } => "settings.error.corrupt",
        _ => "settings.error.storage",
    };
    drop(error);
    copy(key)
}

#[cfg(windows)]
fn display_credential_error(error: CredentialError) -> String {
    match error {
        CredentialError::InvalidSite
        | CredentialError::InvalidEmail
        | CredentialError::InvalidToken => copy("credentials.error.invalid"),
        CredentialError::Unavailable => copy("credentials.error.unavailable"),
        #[cfg(windows)]
        _ => copy("credentials.error.storage"),
    }
}

fn display_jira_error(error: JiraError) -> String {
    match error {
        JiraError::InvalidSiteUrl => copy("jira.error.site"),
        JiraError::MissingCredentials | JiraError::AuthenticationRequired => {
            copy("jira.error.authentication")
        }
        JiraError::Forbidden => copy("jira.error.forbidden"),
        JiraError::NotFound => copy("jira.error.notFound"),
        JiraError::RateLimited {
            retry_after_seconds,
        } => rate_limit_message(retry_after_seconds),
        JiraError::ServerUnavailable => copy("jira.error.unavailable"),
        JiraError::Transport(source) if source.is_timeout() => copy("jira.error.timeout"),
        JiraError::Transport(_) => copy("jira.error.offline"),
        JiraError::WorklogOwnershipMismatch => copy("jira.error.ownership"),
        JiraError::InvalidResponse(_) => copy("jira.error.response"),
        _ => copy("jira.error.invalid"),
    }
}

fn rate_limit_message(retry_after_seconds: Option<u64>) -> String {
    let Some(seconds) = retry_after_seconds else {
        return copy("jira.error.rateLimited");
    };
    format!(
        "{}{seconds}{}",
        text("jira.error.rateLimitedPrefix"),
        text("jira.error.rateLimitedSuffix")
    )
}

fn copy(key: &str) -> String {
    text(key).to_owned()
}

#[cfg(test)]
mod tests {
    use time::{Date, Month};

    use super::{committed_refresh_result, ensure_not_future, validate_issue_membership};
    use crate::{connection_model::WorklogMutationOutcome, copy::text};

    #[test]
    fn future_dates_are_rejected_before_accessing_jira() {
        let today = date(2);
        assert!(ensure_not_future(today, today).is_ok());
        assert!(ensure_not_future(date(3), today).is_err());
    }

    #[test]
    fn committed_mutation_is_not_reported_as_failed_when_refresh_fails() {
        let refresh_error = "refresh unavailable".to_owned();
        let outcome = committed_refresh_result(Err(refresh_error));
        let WorklogMutationOutcome::CommittedWithoutRefresh(message) = outcome else {
            panic!("refresh failure must preserve committed mutation status");
        };
        assert!(message.starts_with(text("connection.committedRefreshFailed")));
    }

    #[test]
    fn worklog_mutations_fail_closed_outside_the_board() {
        assert!(validate_issue_membership(true).is_ok());
        assert_eq!(
            validate_issue_membership(false),
            Err(text("settings.error.boardOutsideScope").to_owned())
        );
    }

    fn date(day: u8) -> Date {
        Date::from_calendar_date(2026, Month::September, day).expect("test date is valid")
    }
}
