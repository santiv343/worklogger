use std::collections::{BTreeMap, HashSet};
use std::future::Future;
use std::time::Duration as StdDuration;

use futures_util::stream::{self, StreamExt};
use hours_core::{
    AccountId, ConnectionId, DateRange, Duration, ExternalResourceRef, IssueKey,
    LoadOwnTimeEntries, OwnTimeEntryBatch, OwnTimeEntryReader, ProviderSubject, SourceWarning,
    TimeEntry, WeeklySummary, WeeklyTarget, Worklog,
};
use reqwest::{Client, Method, RequestBuilder, Response, StatusCode, Url, redirect::Policy};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use time::{Duration as TimeDuration, OffsetDateTime, UtcOffset, format_description};

use crate::dto::{
    AdfDocumentDto, BoardPageDto, IssuePageDto, JiraCommentRequestDto, JiraIssueSearchPageDto,
    JiraIssueSearchRequestDto, JiraIssueUpdateRequestDto, JiraTransitionIdDto,
    JiraTransitionRequestDto, JiraTransitionsDto, MyPermissionsDto, WorklogPageDto,
    WorklogRequestDto,
};
use crate::{
    BoardDto, IssueDto, JiraCommentDto, JiraEditMetadataDto, JiraError, JiraIssueDocumentDto,
    JiraSiteUrl, JiraTransitionDto, JiraUserDto, WorklogDto,
};

const API_SEGMENT: &str = "rest";
const PLATFORM_SEGMENT: &str = "api";
const PLATFORM_VERSION: &str = "3";
const SOFTWARE_SEGMENT: &str = "software";
const SOFTWARE_VERSION: &str = "1.0";
const BOARD_SEGMENT: &str = "board";
const BROWSE_SEGMENT: &str = "browse";
const ISSUE_SEGMENT: &str = "issue";
const SEARCH_SEGMENT: &str = "search";
const JQL_SEGMENT: &str = "jql";
const EDIT_METADATA_SEGMENT: &str = "editmeta";
const TRANSITIONS_SEGMENT: &str = "transitions";
const COMMENT_SEGMENT: &str = "comment";
const WORKLOG_SEGMENT: &str = "worklog";
const DESCRIPTION_FIELD_ID: &str = "description";
const MAX_PROVIDER_ERROR_BODY_BYTES: usize = 64 * 1_024;
const MAX_PROVIDER_ERROR_DETAIL_CHARACTERS: usize = 1_000;
const MYSELF_SEGMENT: &str = "myself";
const MY_PERMISSIONS_SEGMENT: &str = "mypermissions";
const BROWSE_PROJECTS_PERMISSION: &str = "BROWSE_PROJECTS";
const WORK_ON_ISSUES_PERMISSION: &str = "WORK_ON_ISSUES";
const EDIT_OWN_WORKLOGS_PERMISSION: &str = "EDIT_OWN_WORKLOGS";
const DELETE_OWN_WORKLOGS_PERMISSION: &str = "DELETE_OWN_WORKLOGS";
const ADMINISTER_PROJECTS_PERMISSION: &str = "ADMINISTER_PROJECTS";
const PROJECT_PERMISSION_KEYS: &str =
    "BROWSE_PROJECTS,WORK_ON_ISSUES,EDIT_OWN_WORKLOGS,DELETE_OWN_WORKLOGS,ADMINISTER_PROJECTS";
const ISSUE_FIELDS: &str = "summary,assignee,issuetype,status";
const LEAVE_ESTIMATE: &str = "leave";
const UNKNOWN_ACCOUNT_ID: &str = "unknown";
const WORKLOG_MAP_WARNING: &str = "Jira devolvió una carga inválida para el issue";
const WORKLOG_LOAD_WARNING: &str = "No se pudieron leer las cargas del issue";
const HTTP_CLIENT_ERROR_START: u16 = 400;
const HTTP_CLIENT_ERROR_END: u16 = 500;
const RETRY_AFTER_HEADER: &str = "retry-after";
const ASSIGNED_OPEN_SPRINT_JQL: &str =
    "assignee = currentUser() AND sprint in openSprints() ORDER BY updated DESC";
const RECENT_ISSUES_JQL: &str = "ORDER BY updated DESC";
const TEAM_ROSTER_JQL: &str =
    "assignee is not EMPTY AND sprint in openSprints() ORDER BY updated DESC";
const ISSUE_KEY_FIELD: &str = "key";
const ISSUE_SUMMARY_FIELD: &str = "summary";

pub struct JiraClient {
    site: JiraSiteUrl,
    email: String,
    api_token: String,
    http: Client,
}

struct BoardIssueLookup<'lookup> {
    board_id: u64,
    issue_key: &'lookup IssueKey,
    jql: &'lookup str,
    fields: &'lookup [String],
    limits: PageLimits,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageLimits {
    page_size: u16,
    max_items: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct JiraIssueSearchResult {
    pub issues: Vec<JiraIssueDocumentDto>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorklogInput {
    pub started: OffsetDateTime,
    pub time_spent_seconds: u32,
    pub comment: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeeklyReport {
    pub identity: JiraUserDto,
    pub summary: WeeklySummary,
    pub worklogs: Vec<Worklog>,
    pub warnings: Vec<WeeklyReportWarning>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TeamReport {
    pub period: DateRange,
    pub members: Vec<TeamMember>,
    pub worklogs: Vec<TeamWorklog>,
    pub warnings: Vec<WeeklyReportWarning>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TeamMember {
    pub account_id: String,
    pub display_name: String,
    pub active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TeamWorklog {
    pub worklog: Worklog,
    pub author_display_name: String,
    pub issue_type: Option<String>,
    pub issue_status: Option<String>,
    pub assignee_display_name: Option<String>,
    pub created: Option<String>,
    pub updated: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectPermissions {
    pub browse_projects: bool,
    pub administer_projects: bool,
    pub own_worklogs: OwnWorklogPermissions,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OwnWorklogPermissions {
    pub create: bool,
    pub edit: bool,
    pub delete: bool,
}

#[derive(Default)]
struct TeamReportCollection {
    worklogs: Vec<TeamWorklog>,
    warnings: Vec<WeeklyReportWarning>,
}

#[derive(Default)]
struct OwnTimeEntryCollection {
    entries: Vec<TimeEntry>,
    warnings: Vec<SourceWarning>,
}

struct JiraOwnTimeEntryReader<'client> {
    client: &'client JiraClient,
    board_id: u64,
    offset: UtcOffset,
    limits: PageLimits,
    maximum_concurrent_requests: usize,
    subject: ProviderSubject,
}

impl TeamReportCollection {
    fn append(&mut self, mut other: Self) {
        self.worklogs.append(&mut other.worklogs);
        self.warnings.append(&mut other.warnings);
    }
}

impl OwnTimeEntryCollection {
    fn append(&mut self, mut other: Self) {
        self.entries.append(&mut other.entries);
        self.warnings.append(&mut other.warnings);
    }
}

impl OwnTimeEntryReader for JiraOwnTimeEntryReader<'_> {
    type Error = JiraError;

    fn read_own_time_entries(
        &self,
        period: DateRange,
    ) -> impl Future<Output = Result<OwnTimeEntryBatch, Self::Error>> + Send {
        self.load(period)
    }
}

impl JiraOwnTimeEntryReader<'_> {
    async fn load(&self, period: DateRange) -> Result<OwnTimeEntryBatch, JiraError> {
        let issues = self
            .client
            .list_board_issues(self.board_id, period, self.limits)
            .await?;
        let collection = self.load_issues(issues, period).await?;
        Ok(OwnTimeEntryBatch {
            subject: self.subject.clone(),
            entries: collection.entries,
            warnings: collection.warnings,
        })
    }

    async fn load_issues(
        &self,
        issues: Vec<IssueDto>,
        period: DateRange,
    ) -> Result<OwnTimeEntryCollection, JiraError> {
        let loads = issues.into_iter().map(|issue| async move {
            let result = self.load_issue(&issue, period).await;
            (issue, result)
        });
        let loaded = stream::iter(loads)
            .buffered(self.maximum_concurrent_requests)
            .collect::<Vec<_>>()
            .await;
        collect_own_time_entry_results(self.client, loaded)
    }

    async fn load_issue(
        &self,
        issue: &IssueDto,
        period: DateRange,
    ) -> Result<OwnTimeEntryCollection, JiraError> {
        self.client
            .load_issue_own_time_entries(&self.subject, issue, period, self.offset, self.limits)
            .await
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeeklyReportWarning {
    pub issue_key: String,
    pub message: String,
}

impl PageLimits {
    /// Creates explicit pagination bounds.
    ///
    /// # Errors
    ///
    /// Returns [`JiraError::InvalidPageLimits`] for a zero limit.
    pub fn new(page_size: u16, max_items: usize) -> Result<Self, JiraError> {
        if page_size == 0 || max_items == 0 {
            return Err(JiraError::InvalidPageLimits);
        }
        Ok(Self {
            page_size,
            max_items,
        })
    }
}

impl WorklogInput {
    /// Creates validated input for a Jira worklog mutation.
    ///
    /// # Errors
    ///
    /// Returns [`JiraError::InvalidWorklogInput`] for a zero duration.
    pub fn new(
        started: OffsetDateTime,
        time_spent_seconds: u32,
        comment: impl Into<String>,
    ) -> Result<Self, JiraError> {
        let comment = comment.into().trim().to_owned();
        if time_spent_seconds == 0 {
            return Err(JiraError::InvalidWorklogInput);
        }
        Ok(Self {
            started,
            time_spent_seconds,
            comment,
        })
    }
}

fn validate_report_concurrency(value: usize) -> Result<(), JiraError> {
    if value == 0 {
        return Err(JiraError::InvalidPageLimits);
    }
    Ok(())
}

impl JiraClient {
    /// Creates a Jira Cloud client with a bounded request timeout.
    ///
    /// # Errors
    ///
    /// Returns an error for missing credentials or an invalid HTTP client.
    pub fn new(
        site: JiraSiteUrl,
        email: impl Into<String>,
        api_token: impl Into<String>,
        timeout: StdDuration,
    ) -> Result<Self, JiraError> {
        let email = email.into();
        let api_token = api_token.into();
        validate_credentials(&email, &api_token)?;
        let http = build_http_client(timeout)?;
        Ok(Self {
            site,
            email,
            api_token,
            http,
        })
    }

    /// Fetches and validates the authenticated Jira identity.
    ///
    /// # Errors
    ///
    /// Returns an error when Jira is unavailable or the identity is invalid.
    pub async fn current_user(&self) -> Result<JiraUserDto, JiraError> {
        let url = self.platform_url(&[MYSELF_SEGMENT])?;
        let user = self.get_json(url).await?;
        validate_identity(&user)?;
        Ok(user)
    }

    /// Loads selected fields from one visible Jira issue.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid fields or provider failure.
    pub async fn get_issue_detail(
        &self,
        issue_key: &IssueKey,
        fields: &[String],
    ) -> Result<JiraIssueDocumentDto, JiraError> {
        validate_issue_fields(fields)?;
        let mut url = self.issue_url(issue_key, &[])?;
        append_text_query(&mut url, "fields", &fields.join(","));
        self.get_json(url).await
    }

    /// Executes a bounded JQL search without truncating a provider page silently.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid input, pagination or provider failure.
    pub async fn search_issues(
        &self,
        jql: &str,
        fields: &[String],
        limits: PageLimits,
    ) -> Result<Vec<JiraIssueDocumentDto>, JiraError> {
        validate_issue_search(jql, fields)?;
        self.get_all_searched_issues(jql, fields, limits).await
    }

    /// Executes a bounded JQL search restricted to one configured board.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid input, pagination or provider failure.
    pub async fn search_board_issue_documents(
        &self,
        board_id: u64,
        jql: &str,
        fields: &[String],
        limits: PageLimits,
    ) -> Result<Vec<JiraIssueDocumentDto>, JiraError> {
        validate_issue_search(jql, fields)?;
        self.get_all_board_issue_documents(board_id, jql, fields, limits)
            .await
    }

    /// Executes a board-scoped search and reports whether the configured result cap truncated it.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid input, pagination or provider failure.
    pub async fn search_board_issue_documents_limited(
        &self,
        board_id: u64,
        jql: &str,
        fields: &[String],
        limits: PageLimits,
    ) -> Result<JiraIssueSearchResult, JiraError> {
        validate_issue_search(jql, fields)?;
        self.get_limited_board_issue_documents(board_id, jql, fields, limits)
            .await
    }

    /// Checks whether one issue belongs to a board and stops on the exact match.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid pagination or provider failure.
    pub async fn board_contains_issue(
        &self,
        board_id: u64,
        issue_key: &IssueKey,
        limits: PageLimits,
    ) -> Result<bool, JiraError> {
        let fields = [ISSUE_SUMMARY_FIELD.to_owned()];
        let jql = format!("{ISSUE_KEY_FIELD} = {}", issue_key.as_str());
        let lookup = BoardIssueLookup {
            board_id,
            issue_key,
            jql: &jql,
            fields: &fields,
            limits,
        };
        self.find_board_issue(lookup).await
    }

    /// Returns the fields Jira currently allows the authenticated account to edit.
    ///
    /// # Errors
    ///
    /// Returns an error when Jira rejects or cannot return the metadata.
    pub async fn get_issue_edit_metadata(
        &self,
        issue_key: &IssueKey,
    ) -> Result<JiraEditMetadataDto, JiraError> {
        let url = self.issue_url(issue_key, &[EDIT_METADATA_SEGMENT])?;
        self.get_json(url).await
    }

    /// Returns transitions currently available for the issue and authenticated account.
    ///
    /// # Errors
    ///
    /// Returns an error when Jira rejects or cannot return the transitions.
    pub async fn get_issue_transitions(
        &self,
        issue_key: &IssueKey,
    ) -> Result<Vec<JiraTransitionDto>, JiraError> {
        let url = self.issue_url(issue_key, &[TRANSITIONS_SEGMENT])?;
        let response: JiraTransitionsDto = self.get_json(url).await?;
        Ok(response.transitions)
    }

    /// Updates an explicit non-empty Jira fields object.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty fields object or provider rejection.
    pub async fn update_issue(
        &self,
        issue_key: &IssueKey,
        fields: &BTreeMap<String, Value>,
    ) -> Result<(), JiraError> {
        let fields = normalized_issue_update_fields(fields)?;
        let url = self.issue_url(issue_key, &[])?;
        let body = JiraIssueUpdateRequestDto { fields: &fields };
        self.send_empty(self.auth(Method::PUT, url).json(&body))
            .await
    }

    /// Adds one plain-text comment as the authenticated account.
    ///
    /// # Errors
    ///
    /// Returns an error for empty text or provider rejection.
    pub async fn add_issue_comment(
        &self,
        issue_key: &IssueKey,
        text: &str,
    ) -> Result<JiraCommentDto, JiraError> {
        let body = jira_comment_request(text)?;
        let url = self.issue_url(issue_key, &[COMMENT_SEGMENT])?;
        self.send_json(self.auth(Method::POST, url).json(&body))
            .await
    }

    /// Applies one transition ID returned by Jira for the issue.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid transition ID or provider rejection.
    pub async fn transition_issue(
        &self,
        issue_key: &IssueKey,
        transition_id: &str,
    ) -> Result<(), JiraError> {
        validate_transition_id(transition_id)?;
        let url = self.issue_url(issue_key, &[TRANSITIONS_SEGMENT])?;
        let body = JiraTransitionRequestDto {
            transition: JiraTransitionIdDto { id: transition_id },
        };
        self.send_empty(self.auth(Method::POST, url).json(&body))
            .await
    }

    /// Lists every accessible board within the supplied limits.
    ///
    /// # Errors
    ///
    /// Returns an error for HTTP, response, or pagination failures.
    pub async fn list_boards(&self, limits: PageLimits) -> Result<Vec<BoardDto>, JiraError> {
        let mut boards = Vec::new();
        let mut start_at = 0;
        loop {
            let page = self.get_board_page(start_at, limits.page_size).await?;
            let page_size = page.values.len();
            ensure_total_within_limit(page.total, limits.max_items)?;
            append_with_limit(&mut boards, page.values, limits.max_items)?;
            if offset_page_finished(page.start_at, page.total, page_size, &boards, limits)? {
                return Ok(boards);
            }
            start_at = next_offset(page.start_at, page_size)?;
        }
    }

    /// Returns the effective Jira permissions this application uses for the board project.
    ///
    /// # Errors
    ///
    /// Returns an error when the board has no project context or Jira cannot evaluate access.
    pub async fn project_permissions(
        &self,
        board_id: u64,
    ) -> Result<ProjectPermissions, JiraError> {
        let board = self.get_board(board_id).await?;
        let project_id = board
            .location
            .and_then(|location| location.project_id)
            .ok_or(JiraError::BoardProjectUnavailable)?;
        let permissions = self.my_project_permissions(project_id).await?;
        Ok(ProjectPermissions {
            browse_projects: has_permission(&permissions, BROWSE_PROJECTS_PERMISSION),
            administer_projects: has_permission(&permissions, ADMINISTER_PROJECTS_PERMISSION),
            own_worklogs: OwnWorklogPermissions {
                create: has_permission(&permissions, WORK_ON_ISSUES_PERMISSION),
                edit: has_permission(&permissions, EDIT_OWN_WORKLOGS_PERMISSION),
                delete: has_permission(&permissions, DELETE_OWN_WORKLOGS_PERMISSION),
            },
        })
    }

    /// Lists board issues that have worklogs in the period.
    ///
    /// # Errors
    ///
    /// Returns an error for HTTP, response, or pagination failures.
    pub async fn list_board_issues(
        &self,
        board_id: u64,
        period: DateRange,
        limits: PageLimits,
    ) -> Result<Vec<IssueDto>, JiraError> {
        let jql = worklog_period_jql(period);
        self.get_all_board_issues(board_id, &jql, limits).await
    }

    /// Searches accessible board issues locally without interpolating user input into JQL.
    ///
    /// # Errors
    ///
    /// Returns an error when Jira cannot list the board issues.
    pub async fn search_board_issues(
        &self,
        board_id: u64,
        query: &str,
        limits: PageLimits,
        maximum_results: usize,
    ) -> Result<Vec<IssueDto>, JiraError> {
        if query.trim().is_empty() || maximum_results == 0 {
            return Ok(Vec::new());
        }
        let issues = self
            .get_all_board_issues(board_id, RECENT_ISSUES_JQL, limits)
            .await?;
        Ok(filter_issues(issues, query, maximum_results))
    }

    /// Lists issues assigned to the authenticated user in an active sprint.
    ///
    /// # Errors
    ///
    /// Returns an error when Jira cannot list the board issues.
    pub async fn list_assigned_open_sprint_issues(
        &self,
        board_id: u64,
        limits: PageLimits,
    ) -> Result<Vec<IssueDto>, JiraError> {
        self.get_all_board_issues(board_id, ASSIGNED_OPEN_SPRINT_JQL, limits)
            .await
    }

    /// Lists current-sprint issues where the authenticated user has no worklog in the period.
    ///
    /// Jira's `updated DESC` order is preserved in the returned suggestions.
    ///
    /// # Errors
    ///
    /// Returns an error when identity, issue, worklog, or pagination requests fail.
    pub async fn list_unlogged_assigned_sprint_issues(
        &self,
        board_id: u64,
        period: DateRange,
        offset: UtcOffset,
        limits: PageLimits,
        maximum_concurrent_requests: usize,
    ) -> Result<Vec<IssueDto>, JiraError> {
        validate_report_concurrency(maximum_concurrent_requests)?;
        let identity = self.current_user().await?;
        let issues = self
            .list_assigned_open_sprint_issues(board_id, limits)
            .await?;
        let checks = issues.into_iter().map(|issue| async {
            let has_hours = self
                .has_own_worklog(&identity.account_id, &issue, period, offset, limits)
                .await?;
            Ok((issue, has_hours))
        });
        let results = stream::iter(checks)
            .buffered(maximum_concurrent_requests)
            .collect::<Vec<_>>()
            .await;
        collect_unlogged_issues(results)
    }

    /// Lists worklogs whose start lies in the local date range.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid dates, HTTP, response, or pagination.
    pub async fn list_worklogs(
        &self,
        issue_key: &IssueKey,
        period: DateRange,
        offset: UtcOffset,
        limits: PageLimits,
    ) -> Result<Vec<WorklogDto>, JiraError> {
        let bounds = worklog_bounds(period, offset)?;
        self.get_all_worklogs(issue_key, bounds, limits).await
    }

    /// Loads the authenticated user's weekly summary and preserves per-issue failures.
    ///
    /// # Errors
    ///
    /// Returns an error when identity, board discovery, or a non-recoverable request fails.
    pub async fn load_weekly_report(
        &self,
        board_id: u64,
        period: DateRange,
        target: WeeklyTarget,
        offset: UtcOffset,
        limits: PageLimits,
        maximum_concurrent_worklog_requests: usize,
    ) -> Result<WeeklyReport, JiraError> {
        validate_report_concurrency(maximum_concurrent_worklog_requests)?;
        let identity = self.current_user().await?;
        let reader = self.own_time_entry_reader(
            board_id,
            offset,
            limits,
            maximum_concurrent_worklog_requests,
            self.provider_subject(&identity)?,
        );
        let batch = LoadOwnTimeEntries::execute(&reader, period).await?;
        build_report_from_entries(identity, period, target, &batch)
    }

    /// Loads every worklog the authenticated Jira account may read on the board.
    ///
    /// # Errors
    ///
    /// Jira remains the authority for project and worklog visibility.
    pub async fn load_team_report(
        &self,
        board_id: u64,
        period: DateRange,
        offset: UtcOffset,
        limits: PageLimits,
        maximum_concurrent_worklog_requests: usize,
    ) -> Result<TeamReport, JiraError> {
        validate_report_concurrency(maximum_concurrent_worklog_requests)?;
        let issues = self.list_board_issues(board_id, period, limits).await?;
        let roster_issues = self
            .get_all_board_issues(board_id, TEAM_ROSTER_JQL, limits)
            .await?;
        let loads = issues.iter().map(|issue| async move {
            let result = self
                .load_issue_team_worklogs(issue, period, offset, limits)
                .await;
            (issue, result)
        });
        let loaded = stream::iter(loads)
            .buffered(maximum_concurrent_worklog_requests)
            .collect::<Vec<_>>()
            .await;
        let mut collections = Vec::new();
        for (issue, result) in loaded {
            match result {
                Ok(collection) => collections.push(collection),
                Err(error) if recoverable_issue_error(&error) => {
                    collections.push(team_warning_collection(load_warning(issue, &error)));
                }
                Err(error) => return Err(error),
            }
        }
        Ok(build_team_report(period, collections, &roster_issues))
    }

    /// Fetches one worklog by issue and identifier.
    ///
    /// # Errors
    ///
    /// Returns an error when the identifier or Jira response is invalid.
    pub async fn get_worklog(
        &self,
        issue_key: &IssueKey,
        worklog_id: &str,
    ) -> Result<WorklogDto, JiraError> {
        validate_worklog_id(worklog_id)?;
        let url = self.worklog_url(issue_key, Some(worklog_id))?;
        let worklog = self.get_json(url).await?;
        validate_worklog(&worklog)?;
        Ok(worklog)
    }

    /// Creates a worklog and validates Jira attributed it to this identity.
    ///
    /// # Errors
    ///
    /// Returns an error on failed mutation or an ownership mismatch.
    pub async fn create_own_worklog(
        &self,
        issue_key: &IssueKey,
        input: &WorklogInput,
    ) -> Result<WorklogDto, JiraError> {
        let actor = self.current_user().await?;
        let url = self.mutation_url(issue_key, None)?;
        let request = worklog_request(input)?;
        let created = self
            .send_json(self.auth(Method::POST, url).json(&request))
            .await?;
        ensure_owned_by(&created, &actor.account_id)?;
        Ok(created)
    }

    /// Updates a worklog only after revalidating its author.
    ///
    /// # Errors
    ///
    /// Returns an error on failed mutation or an ownership mismatch.
    pub async fn update_own_worklog(
        &self,
        issue_key: &IssueKey,
        worklog_id: &str,
        input: &WorklogInput,
    ) -> Result<WorklogDto, JiraError> {
        let actor = self.current_user().await?;
        let current = self.get_worklog(issue_key, worklog_id).await?;
        ensure_owned_by(&current, &actor.account_id)?;
        let url = self.mutation_url(issue_key, Some(worklog_id))?;
        let updated = self.put_worklog(url, input).await?;
        ensure_owned_by(&updated, &actor.account_id)?;
        Ok(updated)
    }

    /// Deletes a worklog only after revalidating its author.
    ///
    /// # Errors
    ///
    /// Returns an error on failed mutation or an ownership mismatch.
    pub async fn delete_own_worklog(
        &self,
        issue_key: &IssueKey,
        worklog_id: &str,
    ) -> Result<(), JiraError> {
        let actor = self.current_user().await?;
        let current = self.get_worklog(issue_key, worklog_id).await?;
        ensure_owned_by(&current, &actor.account_id)?;
        let url = self.mutation_url(issue_key, Some(worklog_id))?;
        self.send_empty(self.auth(Method::DELETE, url)).await
    }

    async fn get_all_board_issues(
        &self,
        board_id: u64,
        jql: &str,
        limits: PageLimits,
    ) -> Result<Vec<IssueDto>, JiraError> {
        let mut issues = Vec::new();
        let mut token = None;
        let mut seen_tokens = HashSet::new();
        let mut page_count = 0;
        loop {
            advance_page_count(&mut page_count, limits.max_items)?;
            let page = self
                .get_issue_page(board_id, jql, token.as_deref(), limits.page_size)
                .await?;
            append_with_limit(&mut issues, page.issues, limits.max_items)?;
            if page.is_last {
                return Ok(issues);
            }
            if issues.len() == limits.max_items {
                return Err(JiraError::CollectionLimitReached);
            }
            token = next_token(page.next_page_token, &mut seen_tokens)?;
        }
    }

    async fn load_issue_own_time_entries(
        &self,
        subject: &ProviderSubject,
        issue: &IssueDto,
        period: DateRange,
        offset: UtcOffset,
        limits: PageLimits,
    ) -> Result<OwnTimeEntryCollection, JiraError> {
        let resource = self.external_resource(issue)?;
        let Ok(issue_key) = IssueKey::new(&issue.key) else {
            return Ok(own_map_warning(resource));
        };
        let values = self
            .list_worklogs(&issue_key, period, offset, limits)
            .await?;
        Ok(map_own_time_entries(
            issue, &resource, subject, values, offset,
        ))
    }

    async fn has_own_worklog(
        &self,
        authenticated_account_id: &str,
        issue: &IssueDto,
        period: DateRange,
        offset: UtcOffset,
        limits: PageLimits,
    ) -> Result<bool, JiraError> {
        let issue_key = IssueKey::new(&issue.key).map_err(|_| JiraError::InvalidWorklog)?;
        let values = self
            .list_worklogs(&issue_key, period, offset, limits)
            .await?;
        own_worklog_presence(&values, authenticated_account_id)
    }

    async fn load_issue_team_worklogs(
        &self,
        issue: &IssueDto,
        period: DateRange,
        offset: UtcOffset,
        limits: PageLimits,
    ) -> Result<TeamReportCollection, JiraError> {
        let mut collection = TeamReportCollection::default();
        let Ok(issue_key) = IssueKey::new(&issue.key) else {
            collection.warnings.push(map_warning(issue));
            return Ok(collection);
        };
        let values = self
            .list_worklogs(&issue_key, period, offset, limits)
            .await?;
        self.map_team_worklogs(issue, &issue_key, values, offset, &mut collection);
        Ok(collection)
    }

    fn map_team_worklogs(
        &self,
        issue: &IssueDto,
        issue_key: &IssueKey,
        values: Vec<WorklogDto>,
        offset: UtcOffset,
        collection: &mut TeamReportCollection,
    ) {
        let issue_url = self.issue_browser_url(issue_key);
        for value in values {
            match map_team_worklog(issue, issue_key, &issue_url, value, offset) {
                Ok(worklog) => collection.worklogs.push(worklog),
                Err(()) => collection.warnings.push(map_warning(issue)),
            }
        }
    }

    async fn get_all_worklogs(
        &self,
        issue_key: &IssueKey,
        bounds: WorklogBounds,
        limits: PageLimits,
    ) -> Result<Vec<WorklogDto>, JiraError> {
        let mut worklogs = Vec::new();
        let mut start_at = 0;
        loop {
            let page = self
                .get_worklog_page(issue_key, start_at, bounds, limits.page_size)
                .await?;
            let page_size = page.worklogs.len();
            validate_worklogs(&page.worklogs)?;
            ensure_total_within_limit(page.total, limits.max_items)?;
            append_with_limit(&mut worklogs, page.worklogs, limits.max_items)?;
            if offset_page_finished(page.start_at, page.total, page_size, &worklogs, limits)? {
                return Ok(worklogs);
            }
            start_at = next_offset(page.start_at, page_size)?;
        }
    }

    async fn get_board_page(
        &self,
        start_at: u32,
        page_size: u16,
    ) -> Result<BoardPageDto, JiraError> {
        let mut url = self.agile_url(&[BOARD_SEGMENT])?;
        append_query(&mut url, "startAt", &start_at);
        append_query(&mut url, "maxResults", &page_size);
        self.get_json(url).await
    }

    async fn get_issue_page(
        &self,
        board_id: u64,
        jql: &str,
        token: Option<&str>,
        page_size: u16,
    ) -> Result<IssuePageDto, JiraError> {
        let board_id = board_id.to_string();
        let mut url = self.software_url(&[BOARD_SEGMENT, &board_id, ISSUE_SEGMENT])?;
        append_text_query(&mut url, "jql", jql);
        append_text_query(&mut url, "fields", ISSUE_FIELDS);
        append_query(&mut url, "maxResults", &page_size);
        if let Some(value) = token {
            append_text_query(&mut url, "nextPageToken", value);
        }
        self.get_json(url).await
    }

    async fn get_all_searched_issues(
        &self,
        jql: &str,
        fields: &[String],
        limits: PageLimits,
    ) -> Result<Vec<JiraIssueDocumentDto>, JiraError> {
        let mut issues = Vec::new();
        let mut token = None;
        let mut seen_tokens = HashSet::new();
        let mut page_count = 0;
        loop {
            advance_page_count(&mut page_count, limits.max_items)?;
            let page = self
                .get_searched_issue_page(jql, fields, token.as_deref(), limits.page_size)
                .await?;
            append_with_limit(&mut issues, page.issues, limits.max_items)?;
            if page.is_last {
                return Ok(issues);
            }
            token = next_token(page.next_page_token, &mut seen_tokens)?;
        }
    }

    async fn get_all_board_issue_documents(
        &self,
        board_id: u64,
        jql: &str,
        fields: &[String],
        limits: PageLimits,
    ) -> Result<Vec<JiraIssueDocumentDto>, JiraError> {
        let mut issues = Vec::new();
        let mut token = None;
        let mut seen_tokens = HashSet::new();
        let mut page_count = 0;
        loop {
            advance_page_count(&mut page_count, limits.max_items)?;
            let page = self
                .get_next_board_issue_document_page(board_id, jql, fields, token.as_deref(), limits)
                .await?;
            append_with_limit(&mut issues, page.issues, limits.max_items)?;
            if page.is_last {
                return Ok(issues);
            }
            token = next_token(page.next_page_token, &mut seen_tokens)?;
        }
    }

    async fn get_limited_board_issue_documents(
        &self,
        board_id: u64,
        jql: &str,
        fields: &[String],
        limits: PageLimits,
    ) -> Result<JiraIssueSearchResult, JiraError> {
        let mut issues = Vec::new();
        let mut token = None;
        let mut seen_tokens = HashSet::new();
        loop {
            let page_size = remaining_page_size(&issues, limits)?;
            let page = self
                .get_board_issue_document_page(board_id, jql, fields, token.as_deref(), page_size)
                .await?;
            append_with_limit(&mut issues, page.issues, limits.max_items)?;
            if page.is_last || issues.len() == limits.max_items {
                return Ok(JiraIssueSearchResult {
                    issues,
                    has_more: !page.is_last,
                });
            }
            token = next_token(page.next_page_token, &mut seen_tokens)?;
        }
    }

    async fn find_board_issue(&self, lookup: BoardIssueLookup<'_>) -> Result<bool, JiraError> {
        let mut issues = Vec::new();
        let mut token = None;
        let mut seen_tokens = HashSet::new();
        for _ in 0..lookup.limits.max_items {
            let page = self.lookup_page(&lookup, token.as_deref()).await?;
            append_with_limit(&mut issues, page.issues, lookup.limits.max_items)?;
            if issues
                .iter()
                .any(|issue| issue.key == lookup.issue_key.as_str())
            {
                return Ok(true);
            }
            if page.is_last {
                return Ok(false);
            }
            token = next_token(page.next_page_token, &mut seen_tokens)?;
        }
        Err(JiraError::InvalidPagination)
    }

    async fn lookup_page(
        &self,
        lookup: &BoardIssueLookup<'_>,
        token: Option<&str>,
    ) -> Result<JiraIssueSearchPageDto, JiraError> {
        self.get_next_board_issue_document_page(
            lookup.board_id,
            lookup.jql,
            lookup.fields,
            token,
            lookup.limits,
        )
        .await
    }

    async fn get_next_board_issue_document_page(
        &self,
        board_id: u64,
        jql: &str,
        fields: &[String],
        token: Option<&str>,
        limits: PageLimits,
    ) -> Result<JiraIssueSearchPageDto, JiraError> {
        self.get_board_issue_document_page(board_id, jql, fields, token, limits.page_size)
            .await
    }

    async fn get_board_issue_document_page(
        &self,
        board_id: u64,
        jql: &str,
        fields: &[String],
        token: Option<&str>,
        page_size: u16,
    ) -> Result<JiraIssueSearchPageDto, JiraError> {
        let board_id = board_id.to_string();
        let mut url = self.software_url(&[BOARD_SEGMENT, &board_id, ISSUE_SEGMENT])?;
        append_text_query(&mut url, "jql", jql);
        append_text_query(&mut url, "fields", &fields.join(","));
        append_query(&mut url, "maxResults", &page_size);
        if let Some(value) = token {
            append_text_query(&mut url, "nextPageToken", value);
        }
        self.get_json(url).await
    }

    async fn get_searched_issue_page(
        &self,
        jql: &str,
        fields: &[String],
        token: Option<&str>,
        page_size: u16,
    ) -> Result<JiraIssueSearchPageDto, JiraError> {
        let url = self.platform_url(&[SEARCH_SEGMENT, JQL_SEGMENT])?;
        let body = JiraIssueSearchRequestDto {
            jql,
            fields,
            max_results: page_size,
            next_page_token: token,
        };
        self.send_json(self.auth(Method::POST, url).json(&body))
            .await
    }

    async fn get_worklog_page(
        &self,
        issue_key: &IssueKey,
        start_at: u32,
        bounds: WorklogBounds,
        page_size: u16,
    ) -> Result<WorklogPageDto, JiraError> {
        let mut url = self.worklog_url(issue_key, None)?;
        append_query(&mut url, "startAt", &start_at);
        append_query(&mut url, "maxResults", &page_size);
        append_query(&mut url, "startedAfter", &bounds.started_after);
        append_query(&mut url, "startedBefore", &bounds.started_before);
        self.get_json(url).await
    }

    async fn put_worklog(&self, url: Url, input: &WorklogInput) -> Result<WorklogDto, JiraError> {
        let request = worklog_request(input)?;
        self.send_json(self.auth(Method::PUT, url).json(&request))
            .await
    }

    async fn get_json<T: DeserializeOwned>(&self, url: Url) -> Result<T, JiraError> {
        self.send_json(self.auth(Method::GET, url)).await
    }

    async fn get_board(&self, board_id: u64) -> Result<BoardDto, JiraError> {
        let board_id = board_id.to_string();
        let url = self.agile_url(&[BOARD_SEGMENT, &board_id])?;
        self.get_json(url).await
    }

    async fn my_project_permissions(&self, project_id: u64) -> Result<MyPermissionsDto, JiraError> {
        let mut url = self.platform_url(&[MY_PERMISSIONS_SEGMENT])?;
        append_query(&mut url, "projectId", &project_id);
        append_text_query(&mut url, "permissions", PROJECT_PERMISSION_KEYS);
        self.get_json(url).await
    }

    async fn send_json<T: DeserializeOwned>(
        &self,
        request: RequestBuilder,
    ) -> Result<T, JiraError> {
        let response = request.send().await.map_err(JiraError::Transport)?;
        let response = ensure_success(response).await?;
        response.json().await.map_err(JiraError::InvalidResponse)
    }

    async fn send_empty(&self, request: RequestBuilder) -> Result<(), JiraError> {
        let response = request.send().await.map_err(JiraError::Transport)?;
        ensure_success(response).await?;
        Ok(())
    }

    fn auth(&self, method: Method, url: Url) -> RequestBuilder {
        self.http
            .request(method, url)
            .basic_auth(&self.email, Some(&self.api_token))
    }

    fn own_time_entry_reader(
        &self,
        board_id: u64,
        offset: UtcOffset,
        limits: PageLimits,
        maximum_concurrent_requests: usize,
        subject: ProviderSubject,
    ) -> JiraOwnTimeEntryReader<'_> {
        JiraOwnTimeEntryReader {
            client: self,
            board_id,
            offset,
            limits,
            maximum_concurrent_requests,
            subject,
        }
    }

    fn provider_subject(&self, identity: &JiraUserDto) -> Result<ProviderSubject, JiraError> {
        ProviderSubject::new(
            self.connection_id()?,
            identity.account_id.clone(),
            identity.display_name.clone(),
        )
        .map_err(|_| JiraError::InvalidIdentity)
    }

    fn external_resource(&self, issue: &IssueDto) -> Result<ExternalResourceRef, JiraError> {
        let resource =
            ExternalResourceRef::new(self.connection_id()?, issue.id.clone(), issue.key.clone())
                .map_err(|_| JiraError::InvalidWorklog)?;
        resource
            .with_web_url(self.issue_browser_url_for(&issue.key)?)
            .map_err(|_| JiraError::InvalidWorklog)
    }

    fn connection_id(&self) -> Result<ConnectionId, JiraError> {
        ConnectionId::new(self.site.as_url().as_str()).map_err(|_| JiraError::InvalidSiteUrl)
    }

    fn issue_browser_url_for(&self, display_id: &str) -> Result<String, JiraError> {
        self.url(&[], &[BROWSE_SEGMENT, display_id])
            .map(|url| url.to_string())
    }

    fn platform_url(&self, tail: &[&str]) -> Result<Url, JiraError> {
        self.url(&[API_SEGMENT, PLATFORM_SEGMENT, PLATFORM_VERSION], tail)
    }

    fn agile_url(&self, tail: &[&str]) -> Result<Url, JiraError> {
        self.url(&[API_SEGMENT, "agile", SOFTWARE_VERSION], tail)
    }

    fn software_url(&self, tail: &[&str]) -> Result<Url, JiraError> {
        self.url(&[API_SEGMENT, SOFTWARE_SEGMENT, SOFTWARE_VERSION], tail)
    }

    fn worklog_url(&self, issue_key: &IssueKey, id: Option<&str>) -> Result<Url, JiraError> {
        let mut tail = vec![ISSUE_SEGMENT, issue_key.as_str(), WORKLOG_SEGMENT];
        if let Some(value) = id {
            tail.push(value);
        }
        self.platform_url(&tail)
    }

    fn issue_url(&self, issue_key: &IssueKey, tail: &[&str]) -> Result<Url, JiraError> {
        let mut segments = vec![ISSUE_SEGMENT, issue_key.as_str()];
        segments.extend_from_slice(tail);
        self.platform_url(&segments)
    }

    fn mutation_url(&self, issue_key: &IssueKey, id: Option<&str>) -> Result<Url, JiraError> {
        if let Some(value) = id {
            validate_worklog_id(value)?;
        }
        let mut url = self.worklog_url(issue_key, id)?;
        append_text_query(&mut url, "adjustEstimate", LEAVE_ESTIMATE);
        Ok(url)
    }

    fn issue_browser_url(&self, issue_key: &IssueKey) -> String {
        self.site.issue_browser_url(issue_key)
    }

    fn url(&self, prefix: &[&str], tail: &[&str]) -> Result<Url, JiraError> {
        let mut url = self.site.as_url().clone();
        let mut segments = url
            .path_segments_mut()
            .map_err(|()| JiraError::InvalidRequestUrl)?;
        segments.clear();
        segments.extend(prefix.iter().chain(tail.iter()).copied());
        drop(segments);
        Ok(url)
    }
}

#[derive(Clone, Copy)]
struct WorklogBounds {
    started_after: i64,
    started_before: i64,
}

fn build_http_client(timeout: StdDuration) -> Result<Client, JiraError> {
    Client::builder()
        .redirect(Policy::none())
        .timeout(timeout)
        .build()
        .map_err(JiraError::Transport)
}

fn validate_credentials(email: &str, api_token: &str) -> Result<(), JiraError> {
    if email.trim().is_empty() || api_token.trim().is_empty() {
        return Err(JiraError::MissingCredentials);
    }
    Ok(())
}

fn validate_issue_fields(fields: &[String]) -> Result<(), JiraError> {
    let valid = !fields.is_empty()
        && fields
            .iter()
            .all(|field| !field.trim().is_empty() && !field.contains(','));
    if valid {
        return Ok(());
    }
    Err(JiraError::InvalidIssueInput)
}

fn validate_issue_search(jql: &str, fields: &[String]) -> Result<(), JiraError> {
    if jql.trim().is_empty() {
        return Err(JiraError::InvalidIssueInput);
    }
    validate_issue_fields(fields)
}

fn validate_issue_update(fields: &BTreeMap<String, Value>) -> Result<(), JiraError> {
    if !fields.is_empty() {
        return Ok(());
    }
    Err(JiraError::InvalidIssueInput)
}

fn normalized_issue_update_fields(
    fields: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, JiraError> {
    validate_issue_update(fields)?;
    let mut normalized = fields.clone();
    let Some(Value::String(description)) = fields.get(DESCRIPTION_FIELD_ID) else {
        return Ok(normalized);
    };
    if description.trim().is_empty() {
        normalized.insert(DESCRIPTION_FIELD_ID.to_owned(), Value::Null);
        return Ok(normalized);
    }
    let document = serde_json::to_value(AdfDocumentDto::plain_text(description.clone()))
        .map_err(|_| JiraError::InvalidIssueInput)?;
    normalized.insert(DESCRIPTION_FIELD_ID.to_owned(), document);
    Ok(normalized)
}

fn validate_transition_id(transition_id: &str) -> Result<(), JiraError> {
    let valid = !transition_id.is_empty()
        && transition_id
            .chars()
            .all(|character| character.is_ascii_digit());
    if valid {
        return Ok(());
    }
    Err(JiraError::InvalidIssueInput)
}

fn jira_comment_request(text: &str) -> Result<JiraCommentRequestDto, JiraError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(JiraError::InvalidIssueInput);
    }
    Ok(JiraCommentRequestDto {
        body: AdfDocumentDto::plain_text(text.to_owned()),
    })
}

fn validate_identity(user: &JiraUserDto) -> Result<(), JiraError> {
    let invalid_account =
        user.account_id.trim().is_empty() || user.account_id == UNKNOWN_ACCOUNT_ID;
    if invalid_account || user.display_name.trim().is_empty() || !user.active {
        return Err(JiraError::InvalidIdentity);
    }
    Ok(())
}

fn ensure_owned_by(worklog: &WorklogDto, account_id: &str) -> Result<(), JiraError> {
    validate_worklog(worklog)?;
    if worklog.author.account_id != account_id {
        return Err(JiraError::WorklogOwnershipMismatch);
    }
    Ok(())
}

fn validate_worklog(worklog: &WorklogDto) -> Result<(), JiraError> {
    let invalid = worklog.id.is_empty()
        || worklog.author.account_id.is_empty()
        || worklog.time_spent_seconds == 0;
    if invalid {
        return Err(JiraError::InvalidWorklog);
    }
    Ok(())
}

fn validate_worklogs(worklogs: &[WorklogDto]) -> Result<(), JiraError> {
    worklogs.iter().try_for_each(validate_worklog)
}

fn own_worklog_presence(
    worklogs: &[WorklogDto],
    authenticated_account_id: &str,
) -> Result<bool, JiraError> {
    let mut invalid_own_worklog = false;
    for worklog in worklogs {
        if worklog.author.account_id != authenticated_account_id {
            continue;
        }
        if validate_worklog(worklog).is_ok() && parse_jira_timestamp(&worklog.started).is_some() {
            return Ok(true);
        }
        invalid_own_worklog = true;
    }
    if invalid_own_worklog {
        return Err(JiraError::InvalidWorklog);
    }
    Ok(false)
}

fn validate_worklog_id(value: &str) -> Result<(), JiraError> {
    if value.is_empty() || !value.chars().all(|character| character.is_ascii_digit()) {
        return Err(JiraError::InvalidWorklogId);
    }
    Ok(())
}

async fn ensure_success(response: Response) -> Result<Response, JiraError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    Err(response_error(response).await)
}

async fn response_error(response: Response) -> JiraError {
    let status = response.status();
    let status_error = status_error(status, retry_after_seconds(&response));
    if !matches!(status_error, JiraError::HttpStatus(_)) {
        return status_error;
    }
    let detail = jira_rejection_detail(response)
        .await
        .unwrap_or_else(|| status.to_string());
    JiraError::ProviderRejected {
        status: status.as_u16(),
        detail,
    }
}

fn status_error(status: StatusCode, retry_after_seconds: Option<u64>) -> JiraError {
    match status {
        StatusCode::UNAUTHORIZED => JiraError::AuthenticationRequired,
        StatusCode::FORBIDDEN => JiraError::Forbidden,
        StatusCode::NOT_FOUND => JiraError::NotFound,
        StatusCode::TOO_MANY_REQUESTS => JiraError::RateLimited {
            retry_after_seconds,
        },
        status if status.is_server_error() => JiraError::ServerUnavailable,
        status => JiraError::HttpStatus(status.as_u16()),
    }
}

fn retry_after_seconds(response: &Response) -> Option<u64> {
    response
        .headers()
        .get(RETRY_AFTER_HEADER)?
        .to_str()
        .ok()?
        .parse()
        .ok()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JiraErrorResponse {
    #[serde(default)]
    error_messages: Vec<String>,
    #[serde(default)]
    errors: BTreeMap<String, String>,
}

async fn jira_rejection_detail(response: Response) -> Option<String> {
    let body = bounded_error_body(response).await?;
    let response = serde_json::from_slice::<JiraErrorResponse>(&body).ok()?;
    let field_errors = response
        .errors
        .into_iter()
        .map(|(field, message)| format!("{field}: {message}"));
    limited_error_detail(response.error_messages.into_iter().chain(field_errors))
}

async fn bounded_error_body(mut response: Response) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        let size = body.len().checked_add(chunk.len())?;
        if size > MAX_PROVIDER_ERROR_BODY_BYTES {
            return None;
        }
        body.extend_from_slice(&chunk);
    }
    Some(body)
}

fn limited_error_detail(messages: impl Iterator<Item = String>) -> Option<String> {
    let detail = messages
        .filter(|message| !message.trim().is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    if detail.is_empty() {
        return None;
    }
    Some(
        detail
            .chars()
            .take(MAX_PROVIDER_ERROR_DETAIL_CHARACTERS)
            .collect(),
    )
}

fn append_query(url: &mut Url, name: &str, value: &(impl ToString + ?Sized)) {
    append_text_query(url, name, &value.to_string());
}

fn append_text_query(url: &mut Url, name: &str, value: &str) {
    url.query_pairs_mut().append_pair(name, value);
}

fn append_with_limit<T>(
    target: &mut Vec<T>,
    values: Vec<T>,
    maximum: usize,
) -> Result<(), JiraError> {
    let remaining = maximum.saturating_sub(target.len());
    if values.len() > remaining {
        return Err(JiraError::CollectionLimitReached);
    }
    target.extend(values);
    Ok(())
}

fn remaining_page_size<T>(values: &[T], limits: PageLimits) -> Result<u16, JiraError> {
    let remaining = limits.max_items.saturating_sub(values.len());
    let page_size = remaining.min(usize::from(limits.page_size));
    u16::try_from(page_size)
        .ok()
        .filter(|value| *value > 0)
        .ok_or(JiraError::InvalidPagination)
}

fn ensure_total_within_limit(total: u32, maximum: usize) -> Result<(), JiraError> {
    let total = usize::try_from(total).map_err(|_| JiraError::InvalidPagination)?;
    if total > maximum {
        return Err(JiraError::CollectionLimitReached);
    }
    Ok(())
}

fn offset_page_finished<T>(
    start_at: u32,
    total: u32,
    page_size: usize,
    collected: &[T],
    limits: PageLimits,
) -> Result<bool, JiraError> {
    if page_size == 0 {
        return Ok(true);
    }
    let next = next_offset(start_at, page_size)?;
    Ok(next >= total || collected.len() == limits.max_items)
}

fn next_offset(start_at: u32, page_size: usize) -> Result<u32, JiraError> {
    let increment = u32::try_from(page_size).map_err(|_| JiraError::InvalidPagination)?;
    start_at
        .checked_add(increment)
        .ok_or(JiraError::InvalidPagination)
}

fn advance_page_count(page_count: &mut usize, maximum_pages: usize) -> Result<(), JiraError> {
    if *page_count >= maximum_pages {
        return Err(JiraError::InvalidPagination);
    }
    *page_count += 1;
    Ok(())
}

fn next_token(
    token: Option<String>,
    seen_tokens: &mut HashSet<String>,
) -> Result<Option<String>, JiraError> {
    let value = token
        .filter(|candidate| !candidate.is_empty())
        .ok_or(JiraError::InvalidPagination)?;
    if !seen_tokens.insert(value.clone()) {
        return Err(JiraError::InvalidPagination);
    }
    Ok(Some(value))
}

fn worklog_period_jql(period: DateRange) -> String {
    format!(
        "worklogDate >= \"{}\" AND worklogDate <= \"{}\"",
        period.start(),
        period.end()
    )
}

fn collect_unlogged_issues(
    results: Vec<Result<(IssueDto, bool), JiraError>>,
) -> Result<Vec<IssueDto>, JiraError> {
    let mut issues = Vec::new();
    for result in results {
        let (issue, has_hours) = result?;
        if !has_hours {
            issues.push(issue);
        }
    }
    Ok(issues)
}

fn filter_issues(issues: Vec<IssueDto>, query: &str, maximum: usize) -> Vec<IssueDto> {
    let normalized_query = query.trim().to_lowercase();
    issues
        .into_iter()
        .filter(|issue| issue_matches(issue, &normalized_query))
        .take(maximum)
        .collect()
}

fn issue_matches(issue: &IssueDto, normalized_query: &str) -> bool {
    issue.key.to_lowercase().contains(normalized_query)
        || issue
            .fields
            .summary
            .to_lowercase()
            .contains(normalized_query)
}

fn worklog_bounds(period: DateRange, offset: UtcOffset) -> Result<WorklogBounds, JiraError> {
    let end = period
        .end()
        .checked_add(TimeDuration::DAY)
        .ok_or(JiraError::InvalidDate)?;
    Ok(WorklogBounds {
        started_after: midnight_millis(period.start(), offset)?,
        started_before: midnight_millis(end, offset)?,
    })
}

fn midnight_millis(date: time::Date, offset: UtcOffset) -> Result<i64, JiraError> {
    let local = date.with_hms(0, 0, 0).map_err(|_| JiraError::InvalidDate)?;
    local
        .assume_offset(offset)
        .unix_timestamp()
        .checked_mul(1_000)
        .ok_or(JiraError::InvalidDate)
}

fn worklog_request(input: &WorklogInput) -> Result<WorklogRequestDto, JiraError> {
    let format = format_description::parse(
        "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3][offset_hour sign:mandatory][offset_minute]",
    )
    .map_err(|_| JiraError::InvalidDate)?;
    let started = input
        .started
        .format(&format)
        .map_err(|_| JiraError::InvalidDate)?;
    Ok(WorklogRequestDto {
        started,
        time_spent_seconds: input.time_spent_seconds,
        comment: worklog_comment(input),
    })
}

fn build_report(
    identity: JiraUserDto,
    period: DateRange,
    target: WeeklyTarget,
    worklogs: &[Worklog],
    warnings: Vec<WeeklyReportWarning>,
) -> Result<WeeklyReport, JiraError> {
    let account =
        AccountId::new(identity.account_id.clone()).map_err(|_| JiraError::InvalidIdentity)?;
    let summary = WeeklySummary::calculate(&account, period, target, worklogs);
    let own_worklogs = own_worklogs(&account, period, worklogs);
    Ok(WeeklyReport {
        identity,
        summary,
        worklogs: own_worklogs,
        warnings,
    })
}

fn build_report_from_entries(
    identity: JiraUserDto,
    period: DateRange,
    target: WeeklyTarget,
    batch: &OwnTimeEntryBatch,
) -> Result<WeeklyReport, JiraError> {
    let worklogs = legacy_worklogs(&batch.entries)?;
    let warnings = legacy_warnings(&batch.warnings)?;
    build_report(identity, period, target, &worklogs, warnings)
}

fn legacy_worklogs(entries: &[TimeEntry]) -> Result<Vec<Worklog>, JiraError> {
    entries.iter().map(legacy_worklog).collect()
}

fn legacy_worklog(entry: &TimeEntry) -> Result<Worklog, JiraError> {
    let issue_key =
        IssueKey::new(entry.destination.display_id()).map_err(|_| JiraError::InvalidWorklog)?;
    let author = AccountId::new(entry.author.remote_id()).map_err(|_| JiraError::InvalidWorklog)?;
    let issue_url = entry
        .destination
        .web_url()
        .ok_or(JiraError::InvalidWorklog)?;
    Ok(Worklog {
        id: entry.id.clone(),
        issue_key,
        issue_summary: entry.destination_title.clone(),
        author,
        started: entry.started,
        duration: entry.duration,
        comment: entry.comment.clone(),
        issue_url: issue_url.to_owned(),
    })
}

fn legacy_warnings(warnings: &[SourceWarning]) -> Result<Vec<WeeklyReportWarning>, JiraError> {
    warnings.iter().map(legacy_warning).collect()
}

fn legacy_warning(warning: &SourceWarning) -> Result<WeeklyReportWarning, JiraError> {
    let resource = warning.resource().ok_or(JiraError::InvalidWorklog)?;
    Ok(WeeklyReportWarning {
        issue_key: resource.display_id().to_owned(),
        message: warning.message().to_owned(),
    })
}

fn build_team_report(
    period: DateRange,
    collections: Vec<TeamReportCollection>,
    roster_issues: &[IssueDto],
) -> TeamReport {
    let mut combined = TeamReportCollection::default();
    for collection in collections {
        combined.append(collection);
    }
    TeamReport {
        period,
        members: team_members(roster_issues, &combined.worklogs),
        worklogs: combined.worklogs,
        warnings: combined.warnings,
    }
}

fn team_members(roster_issues: &[IssueDto], worklogs: &[TeamWorklog]) -> Vec<TeamMember> {
    let mut members = BTreeMap::<String, TeamMember>::new();
    for assignee in roster_issues
        .iter()
        .filter_map(|issue| issue.fields.assignee.as_ref())
    {
        insert_team_member(&mut members, assignee);
    }
    for entry in worklogs {
        members
            .entry(entry.worklog.author.as_str().to_owned())
            .or_insert_with(|| TeamMember {
                account_id: entry.worklog.author.as_str().to_owned(),
                display_name: entry.author_display_name.clone(),
                active: true,
            });
    }
    let mut values = members.into_values().collect::<Vec<_>>();
    values.sort_by(|left, right| left.display_name.cmp(&right.display_name));
    values
}

fn insert_team_member(members: &mut BTreeMap<String, TeamMember>, user: &JiraUserDto) {
    members
        .entry(user.account_id.clone())
        .or_insert_with(|| TeamMember {
            account_id: user.account_id.clone(),
            display_name: user.display_name.clone(),
            active: user.active,
        });
}

fn own_worklogs(account: &AccountId, period: DateRange, worklogs: &[Worklog]) -> Vec<Worklog> {
    worklogs
        .iter()
        .filter(|worklog| worklog.author == *account && period.contains(worklog.started.date()))
        .cloned()
        .collect()
}

fn map_worklog(
    issue: &IssueDto,
    issue_key: &IssueKey,
    issue_url: &str,
    dto: WorklogDto,
    offset: UtcOffset,
) -> Result<Worklog, ()> {
    let author = AccountId::new(dto.author.account_id).map_err(|_| ())?;
    let duration = Duration::from_seconds(dto.time_spent_seconds).map_err(|_| ())?;
    let started = parse_jira_timestamp(&dto.started)
        .ok_or(())?
        .to_offset(offset);
    Ok(Worklog {
        id: dto.id,
        issue_key: issue_key.clone(),
        issue_summary: issue.fields.summary.clone(),
        author,
        started,
        duration,
        comment: adf_text(dto.comment.as_ref()),
        issue_url: issue_url.to_owned(),
    })
}

fn map_own_time_entries(
    issue: &IssueDto,
    resource: &ExternalResourceRef,
    subject: &ProviderSubject,
    values: Vec<WorklogDto>,
    offset: UtcOffset,
) -> OwnTimeEntryCollection {
    let mut collection = OwnTimeEntryCollection::default();
    for value in values {
        if value.author.account_id != subject.remote_id() {
            continue;
        }
        match map_own_time_entry(issue, resource.clone(), subject.clone(), value, offset) {
            Ok(entry) => collection.entries.push(entry),
            Err(()) => collection
                .warnings
                .push(source_map_warning(resource.clone())),
        }
    }
    collection
}

fn map_own_time_entry(
    issue: &IssueDto,
    destination: ExternalResourceRef,
    author: ProviderSubject,
    dto: WorklogDto,
    offset: UtcOffset,
) -> Result<TimeEntry, ()> {
    if dto.author.account_id != author.remote_id() {
        return Err(());
    }
    let duration = Duration::from_seconds(dto.time_spent_seconds).map_err(|_| ())?;
    let started = parse_jira_timestamp(&dto.started)
        .ok_or(())?
        .to_offset(offset);
    Ok(TimeEntry {
        id: dto.id,
        destination,
        destination_title: issue.fields.summary.clone(),
        author,
        started,
        duration,
        comment: adf_text(dto.comment.as_ref()),
    })
}

fn map_team_worklog(
    issue: &IssueDto,
    issue_key: &IssueKey,
    issue_url: &str,
    dto: WorklogDto,
    offset: UtcOffset,
) -> Result<TeamWorklog, ()> {
    let display_name = dto.author.display_name.trim().to_owned();
    if display_name.is_empty() {
        return Err(());
    }
    let created = dto.created.clone();
    let updated = dto.updated.clone();
    let worklog = map_worklog(issue, issue_key, issue_url, dto, offset)?;
    Ok(TeamWorklog {
        worklog,
        author_display_name: display_name,
        issue_type: issue
            .fields
            .issue_type
            .as_ref()
            .map(|field| field.name.clone()),
        issue_status: issue.fields.status.as_ref().map(|field| field.name.clone()),
        assignee_display_name: issue
            .fields
            .assignee
            .as_ref()
            .map(|user| user.display_name.clone()),
        created,
        updated,
    })
}

fn has_permission(permissions: &MyPermissionsDto, key: &str) -> bool {
    permissions
        .permissions
        .get(key)
        .is_some_and(|permission| permission.have_permission)
}

fn parse_jira_timestamp(value: &str) -> Option<OffsetDateTime> {
    let compact = format_description::parse(
        "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond][offset_hour sign:mandatory][offset_minute]",
    )
    .ok()?;
    OffsetDateTime::parse(value, &compact)
        .ok()
        .or_else(|| parse_colon_timestamp(value))
}

fn parse_colon_timestamp(value: &str) -> Option<OffsetDateTime> {
    let format = format_description::parse(
        "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond][offset_hour sign:mandatory]:[offset_minute]",
    )
    .ok()?;
    OffsetDateTime::parse(value, &format).ok()
}

fn adf_text(comment: Option<&serde_json::Value>) -> String {
    let mut fragments = Vec::new();
    if let Some(value) = comment {
        collect_adf_text(value, &mut fragments);
    }
    fragments.join(" ")
}

fn collect_adf_text<'value>(value: &'value serde_json::Value, fragments: &mut Vec<&'value str>) {
    if let Some(text) = value.get("text").and_then(serde_json::Value::as_str) {
        fragments.push(text);
    }
    if let Some(content) = value.get("content").and_then(serde_json::Value::as_array) {
        for child in content {
            collect_adf_text(child, fragments);
        }
    }
}

fn map_warning(issue: &IssueDto) -> WeeklyReportWarning {
    WeeklyReportWarning {
        issue_key: issue.key.clone(),
        message: WORKLOG_MAP_WARNING.to_owned(),
    }
}

fn collect_own_time_entry_results(
    client: &JiraClient,
    loaded: Vec<(IssueDto, Result<OwnTimeEntryCollection, JiraError>)>,
) -> Result<OwnTimeEntryCollection, JiraError> {
    let mut collection = OwnTimeEntryCollection::default();
    for (issue, result) in loaded {
        match result {
            Ok(issue_collection) => collection.append(issue_collection),
            Err(error) if recoverable_issue_error(&error) => collection
                .warnings
                .push(source_load_warning(client, &issue, &error)?),
            Err(error) => return Err(error),
        }
    }
    Ok(collection)
}

fn own_map_warning(resource: ExternalResourceRef) -> OwnTimeEntryCollection {
    OwnTimeEntryCollection {
        entries: Vec::new(),
        warnings: vec![source_map_warning(resource)],
    }
}

fn source_map_warning(resource: ExternalResourceRef) -> SourceWarning {
    SourceWarning::for_resource(resource, WORKLOG_MAP_WARNING)
}

fn source_load_warning(
    client: &JiraClient,
    issue: &IssueDto,
    error: &JiraError,
) -> Result<SourceWarning, JiraError> {
    let resource = client.external_resource(issue)?;
    let message = format!("{WORKLOG_LOAD_WARNING}: {error}");
    Ok(SourceWarning::for_resource(resource, message))
}

fn load_warning(issue: &IssueDto, error: &JiraError) -> WeeklyReportWarning {
    WeeklyReportWarning {
        issue_key: issue.key.clone(),
        message: format!("{WORKLOG_LOAD_WARNING}: {error}"),
    }
}

fn team_warning_collection(warning: WeeklyReportWarning) -> TeamReportCollection {
    TeamReportCollection {
        worklogs: Vec::new(),
        warnings: vec![warning],
    }
}

fn recoverable_issue_error(error: &JiraError) -> bool {
    match error {
        JiraError::Forbidden
        | JiraError::NotFound
        | JiraError::InvalidWorklog
        | JiraError::InvalidResponse(_) => true,
        JiraError::HttpStatus(status) | JiraError::ProviderRejected { status, .. } => {
            (HTTP_CLIENT_ERROR_START..HTTP_CLIENT_ERROR_END).contains(status)
        }
        _ => false,
    }
}

fn worklog_comment(input: &WorklogInput) -> Option<AdfDocumentDto> {
    if input.comment.is_empty() {
        return None;
    }
    Some(AdfDocumentDto::plain_text(input.comment.clone()))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};
    use std::env;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{SocketAddr, TcpListener, TcpStream};
    use std::thread::{self, JoinHandle};
    use std::time::Duration as StdDuration;

    use hours_core::{AccountId, DateRange, IssueKey, WeeklyTarget};
    use serde_json::json;
    use time::{Date, Month, Time, UtcOffset};

    use super::{
        JiraClient, MAX_PROVIDER_ERROR_BODY_BYTES, MAX_PROVIDER_ERROR_DETAIL_CHARACTERS,
        PageLimits, StatusCode, WorklogInput, append_with_limit, build_report, ensure_owned_by,
        filter_issues, limited_error_detail, map_own_time_entry, map_worklog, next_token,
        normalized_issue_update_fields, own_worklog_presence, parse_jira_timestamp,
        recoverable_issue_error, status_error, validate_report_concurrency, worklog_bounds,
    };
    use crate::dto::{IssueFieldsDto, WorklogAuthorDto};
    use crate::{IssueDto, JiraError, JiraSiteUrl, JiraUserDto, WorklogDto};

    const TEST_REQUEST_TIMEOUT: StdDuration = StdDuration::from_secs(2);
    const LIVE_TEST_REQUEST_TIMEOUT: StdDuration = StdDuration::from_secs(30);
    const LIVE_TEST_PAGE_SIZE: u16 = 100;
    const LIVE_TEST_MAXIMUM_ITEMS: usize = 2_000;
    const IDENTITY_RESPONSE: &str =
        r#"{"accountId":"mine","displayName":"Persona Demo","active":true}"#;
    const IDENTITY_PATH: &str = "/rest/api/3/myself";
    const BOARD_LIST_PATH: &str = "/rest/agile/1.0/board?startAt=0&maxResults=50";
    const BOARD_LIST_BODY: &str = r#"{"startAt":0,"total":1,"values":[{"id":42,"name":"Tablero de prueba","type":"scrum","self":"http://local/board/42"}]}"#;
    const BOARD_SEARCH_PATH: &str = "/rest/software/1.0/board/42/issue?jql=ORDER+BY+updated+DESC&fields=summary%2Cassignee%2Cissuetype%2Cstatus&maxResults=50";
    const BOARD_SEARCH_BODY: &str = r#"{"isLast":true,"issues":[{"id":"10001","key":"DEMO-1","self":"http://local/issue/10001","fields":{"summary":"Conciliación semanal"}}]}"#;
    const WORKLOG_RESPONSE: &str = r#"{"id":"123","author":{"accountId":"mine","displayName":"Persona Demo","active":true},"started":"2026-09-01T12:00:00.000+0000","timeSpentSeconds":3600}"#;
    const ISSUE_PATH: &str = "/rest/api/3/issue/DEMO-1";

    #[tokio::test]
    #[ignore = "requires explicit read-only Jira test environment variables"]
    async fn live_board_membership_contract_accepts_a_real_issue() {
        let site = required_environment("WORKLOGGER_TEST_JIRA_URL");
        let email = required_environment("WORKLOGGER_TEST_JIRA_EMAIL");
        let token = required_environment("WORKLOGGER_TEST_JIRA_TOKEN");
        let board_id = required_environment("WORKLOGGER_TEST_JIRA_BOARD_ID")
            .parse::<u64>()
            .expect("test board ID is numeric");
        let key = IssueKey::new(required_environment("WORKLOGGER_TEST_JIRA_ISSUE_KEY"))
            .expect("test issue key is valid");
        let site = JiraSiteUrl::parse(&site).expect("test Jira URL is valid");
        let client = JiraClient::new(site, email, token, LIVE_TEST_REQUEST_TIMEOUT)
            .expect("test client is valid");

        let result = client
            .board_contains_issue(
                board_id,
                &key,
                PageLimits::new(LIVE_TEST_PAGE_SIZE, LIVE_TEST_MAXIMUM_ITEMS).expect("limits"),
            )
            .await;
        let fields = [
            "summary",
            "description",
            "status",
            "priority",
            "assignee",
            "issuetype",
            "labels",
        ]
        .map(str::to_owned);
        let issue = client.get_issue_detail(&key, &fields).await;

        assert!(result.expect("provider response is compatible"));
        assert_eq!(issue.expect("issue detail is compatible").key, key.as_str());
    }

    fn required_environment(name: &str) -> String {
        env::var(name).unwrap_or_else(|_| panic!("missing {name}"))
    }
    const ISSUE_DETAIL_PATH: &str = "/rest/api/3/issue/DEMO-1?fields=summary%2Cstatus";
    const ISSUE_SEARCH_PATH: &str = "/rest/api/3/search/jql";
    const BOARD_ISSUE_PATH: &str = "/rest/software/1.0/board/42/issue?jql=key+%3D+DEMO-1&fields=summary%2Cstatus&maxResults=50";
    const ISSUE_EDIT_PATH: &str = "/rest/api/3/issue/DEMO-1/editmeta";
    const ISSUE_COMMENT_PATH: &str = "/rest/api/3/issue/DEMO-1/comment";
    const ISSUE_TRANSITIONS_PATH: &str = "/rest/api/3/issue/DEMO-1/transitions";
    const ISSUE_DETAIL_BODY: &str =
        r#"{"id":"10001","key":"DEMO-1","fields":{"summary":"Demo","status":{"name":"Open"}}}"#;
    const ISSUE_SEARCH_BODY: &str =
        r#"{"isLast":true,"issues":[{"id":"10001","key":"DEMO-1","fields":{"summary":"Demo"}}]}"#;
    const INCOMPLETE_SCOPE_BODY: &str =
        r#"{"isLast":false,"issues":[{"id":"10001","key":"DEMO-1","fields":{"summary":"Demo"}}]}"#;
    const ISSUE_EDIT_BODY: &str = r#"{"fields":{"summary":{"required":true,"name":"Summary"}}}"#;
    const TRANSITIONS_BODY: &str =
        r#"{"transitions":[{"id":"31","name":"Start","to":{"id":"3","name":"In progress"}}]}"#;
    const COMMENT_BODY: &str = r#"{"id":"9001"}"#;
    const UPDATE_REQUEST_BODY: &str = r#"{"fields":{"summary":"Updated"}}"#;
    const DESCRIPTION_UPDATE_REQUEST_BODY: &str = r#"{"fields":{"description":{"type":"doc","version":1,"content":[{"type":"paragraph","content":[{"type":"text","text":"Pending work"}]}]}}}"#;
    const COMMENT_REQUEST_BODY: &str = r#"{"body":{"type":"doc","version":1,"content":[{"type":"paragraph","content":[{"type":"text","text":"Useful context"}]}]}}"#;
    const TRANSITION_REQUEST_BODY: &str = r#"{"transition":{"id":"31"}}"#;

    struct TestResponse {
        method: &'static str,
        path: &'static str,
        status: &'static str,
        response_body: &'static str,
        forbidden_request_text: Option<&'static str>,
        expected_request_body: Option<&'static str>,
    }

    struct TestRequest {
        method: String,
        path: String,
        body: String,
    }

    #[test]
    fn client_builds_expected_paths() {
        let client = client();
        let key = IssueKey::new("DEMO-101").expect("valid key");
        let url = client.worklog_url(&key, Some("123")).expect("valid url");
        assert_eq!(
            url.as_str(),
            "https://example.atlassian.net/rest/api/3/issue/DEMO-101/worklog/123"
        );
    }

    #[tokio::test]
    async fn reads_identity_boards_and_issues_from_an_isolated_http_server() {
        let (address, server) = spawn_test_server(identity_board_issue_responses());
        let site =
            JiraSiteUrl::loopback_for_test(&format!("http://{address}")).expect("loopback origin");
        let client = JiraClient::new(
            site,
            "person@example.com",
            "test-token",
            TEST_REQUEST_TIMEOUT,
        )
        .expect("test client");

        let identity = client.current_user().await.expect("identity response");
        let boards = load_test_boards(&client).await;
        let issues = load_test_issues(&client).await;
        server.join().expect("server completed");
        assert_eq!(identity.account_id, "mine");
        assert_eq!(boards[0].id, 42);
        assert_eq!(boards[0].name, "Tablero de prueba");
        assert_eq!(issues[0].key, "DEMO-1");
    }

    fn identity_board_issue_responses() -> Vec<TestResponse> {
        vec![
            response("GET", IDENTITY_PATH, IDENTITY_RESPONSE, None),
            response("GET", BOARD_LIST_PATH, BOARD_LIST_BODY, None),
            response("GET", BOARD_SEARCH_PATH, BOARD_SEARCH_BODY, None),
        ]
    }

    async fn load_test_boards(client: &JiraClient) -> Vec<crate::BoardDto> {
        client
            .list_boards(PageLimits::new(50, 100).expect("limits"))
            .await
            .expect("boards response")
    }

    async fn load_test_issues(client: &JiraClient) -> Vec<IssueDto> {
        client
            .search_board_issues(
                42,
                "conciliación",
                PageLimits::new(50, 100).expect("limits"),
                10,
            )
            .await
            .expect("issue response")
    }

    #[tokio::test]
    async fn reads_and_mutates_issues_through_generic_jira_contracts() {
        let (address, server) = spawn_test_server(generic_issue_responses());
        let client = loopback_client(address);
        let key = IssueKey::new("DEMO-1").expect("issue key");
        let fields = ["summary".to_owned(), "status".to_owned()];
        assert_generic_issue_reads(&client, &key, &fields).await;
        assert_generic_issue_metadata(&client, &key).await;
        assert_generic_issue_mutations(&client, &key).await;
        server.join().expect("server completed");
    }

    #[tokio::test]
    async fn converts_plain_text_description_to_adf() {
        let response = mutation_response("PUT", ISSUE_PATH, "", DESCRIPTION_UPDATE_REQUEST_BODY);
        let (address, server) = spawn_test_server(vec![response]);
        let fields = BTreeMap::from([("description".to_owned(), json!("Pending work"))]);
        loopback_client(address)
            .update_issue(&IssueKey::new("DEMO-1").expect("key"), &fields)
            .await
            .expect("description update");
        server.join().expect("server completed");
    }

    #[test]
    fn converts_blank_description_to_null() {
        let fields = BTreeMap::from([("description".to_owned(), json!("  "))]);
        let normalized = normalized_issue_update_fields(&fields).expect("valid clear");
        assert_eq!(normalized["description"], serde_json::Value::Null);
    }

    #[tokio::test]
    async fn preserves_safe_jira_rejection_detail() {
        let body =
            r#"{"errorMessages":[],"errors":{"description":"must use Atlassian Document Format"}}"#;
        let response = rejected_response("PUT", ISSUE_PATH, body);
        let (address, server) = spawn_test_server(vec![response]);
        let fields = BTreeMap::from([("description".to_owned(), json!("Pending work"))]);
        let error = loopback_client(address)
            .update_issue(&IssueKey::new("DEMO-1").expect("key"), &fields)
            .await
            .expect_err("provider rejects update");
        server.join().expect("server completed");
        assert!(
            error
                .to_string()
                .contains("description: must use Atlassian Document Format")
        );
    }

    #[test]
    fn limits_provider_rejection_detail() {
        let oversized = "x".repeat(MAX_PROVIDER_ERROR_DETAIL_CHARACTERS + 1);
        let detail = limited_error_detail(std::iter::once(oversized)).expect("detail");
        assert_eq!(detail.chars().count(), MAX_PROVIDER_ERROR_DETAIL_CHARACTERS);
    }

    #[tokio::test]
    async fn discards_an_oversized_provider_rejection_body() {
        let message = "x".repeat(MAX_PROVIDER_ERROR_BODY_BYTES);
        let body = format!(r#"{{"errorMessages":["{message}"],"errors":{{}}}}"#);
        let response = rejected_response("PUT", ISSUE_PATH, Box::leak(body.into_boxed_str()));
        let (address, server) = spawn_test_server(vec![response]);
        let fields = BTreeMap::from([("summary".to_owned(), json!("Pending work"))]);
        let error = loopback_client(address)
            .update_issue(&IssueKey::new("DEMO-1").expect("key"), &fields)
            .await
            .expect_err("provider rejects update");
        server.join().expect("server completed");
        assert_eq!(
            error.to_string(),
            "Jira rechazó la solicitud (HTTP 400): 400 Bad Request"
        );
    }

    #[tokio::test]
    async fn board_membership_stops_after_an_exact_match() {
        let path =
            "/rest/software/1.0/board/42/issue?jql=key+%3D+DEMO-1&fields=summary&maxResults=1";
        let (address, server) =
            spawn_test_server(vec![response("GET", path, INCOMPLETE_SCOPE_BODY, None)]);
        let key = IssueKey::new("DEMO-1").expect("issue key");
        let found = loopback_client(address)
            .board_contains_issue(42, &key, PageLimits::new(1, 1).expect("limits"))
            .await
            .expect("exact first-page match");

        server.join().expect("server completed");
        assert!(found);
    }

    fn generic_issue_responses() -> Vec<TestResponse> {
        vec![
            response("GET", ISSUE_DETAIL_PATH, ISSUE_DETAIL_BODY, None),
            response("POST", ISSUE_SEARCH_PATH, ISSUE_SEARCH_BODY, None),
            response("GET", BOARD_ISSUE_PATH, ISSUE_SEARCH_BODY, None),
            response("GET", ISSUE_EDIT_PATH, ISSUE_EDIT_BODY, None),
            response("GET", ISSUE_TRANSITIONS_PATH, TRANSITIONS_BODY, None),
            mutation_response("PUT", ISSUE_PATH, "", UPDATE_REQUEST_BODY),
            mutation_response(
                "POST",
                ISSUE_COMMENT_PATH,
                COMMENT_BODY,
                COMMENT_REQUEST_BODY,
            ),
            mutation_response("POST", ISSUE_TRANSITIONS_PATH, "", TRANSITION_REQUEST_BODY),
        ]
    }

    async fn assert_generic_issue_reads(client: &JiraClient, key: &IssueKey, fields: &[String]) {
        let issue = client
            .get_issue_detail(key, fields)
            .await
            .expect("issue detail");
        let limits = PageLimits::new(50, 100).expect("limits");
        let search = client
            .search_issues("project = DEMO", fields, limits)
            .await
            .expect("search");
        let board_search = client
            .search_board_issue_documents(42, "key = DEMO-1", fields, limits)
            .await
            .expect("board-scoped search");
        assert_eq!(issue.key, "DEMO-1");
        assert_eq!(search[0].key, "DEMO-1");
        assert_eq!(board_search[0].key, "DEMO-1");
    }

    #[tokio::test]
    async fn bounded_board_search_returns_results_and_reports_more_pages() {
        let path = "/rest/software/1.0/board/42/issue?jql=project+%3D+DEMO&fields=summary%2Cstatus&maxResults=1";
        let body = r#"{"isLast":false,"nextPageToken":"next","issues":[{"id":"10001","key":"DEMO-1","fields":{"summary":"Demo"}}]}"#;
        let (address, server) = spawn_test_server(vec![response("GET", path, body, None)]);
        let fields = ["summary".to_owned(), "status".to_owned()];
        let result = loopback_client(address)
            .search_board_issue_documents_limited(
                42,
                "project = DEMO",
                &fields,
                PageLimits::new(1, 1).expect("limits"),
            )
            .await
            .expect("bounded search");

        server.join().expect("server completed");
        assert_eq!(result.issues.len(), 1);
        assert!(result.has_more);
    }

    async fn assert_generic_issue_metadata(client: &JiraClient, key: &IssueKey) {
        let metadata = client
            .get_issue_edit_metadata(key)
            .await
            .expect("edit metadata");
        let transitions = client
            .get_issue_transitions(key)
            .await
            .expect("transitions");
        assert!(metadata.fields.contains_key("summary"));
        assert_eq!(transitions[0].id, "31");
    }

    async fn assert_generic_issue_mutations(client: &JiraClient, key: &IssueKey) {
        client
            .update_issue(
                key,
                &BTreeMap::from([("summary".to_owned(), json!("Updated"))]),
            )
            .await
            .expect("update");
        let comment = client
            .add_issue_comment(key, "Useful context")
            .await
            .expect("comment");
        client
            .transition_issue(key, "31")
            .await
            .expect("transition");
        assert_eq!(comment.id, "9001");
    }

    #[test]
    fn pagination_rejects_a_repeated_token() {
        let mut seen_tokens = HashSet::new();
        let first = next_token(Some("same".to_owned()), &mut seen_tokens);
        let repeated = next_token(Some("same".to_owned()), &mut seen_tokens);

        assert_eq!(first.expect("first token"), Some("same".to_owned()));
        assert!(matches!(repeated, Err(JiraError::InvalidPagination)));
    }

    #[tokio::test]
    async fn suggests_recent_sprint_issues_without_own_hours_in_order() {
        let responses = unlogged_issue_responses();
        let (address, server) = spawn_test_server(responses);
        let issues = loopback_client(address)
            .list_unlogged_assigned_sprint_issues(
                42,
                current_partial_week(),
                UtcOffset::UTC,
                PageLimits::new(50, 100).expect("limits"),
                1,
            )
            .await
            .expect("unlogged issues");

        server.join().expect("server completed");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].key, "DEMO-2");
    }

    #[tokio::test]
    async fn loads_weekly_report_through_the_provider_neutral_reader() {
        let (address, server) = spawn_test_server(weekly_report_responses());
        let target = WeeklyTarget::from_minutes(1_800).expect("valid target");
        let report = loopback_client(address)
            .load_weekly_report(
                42,
                current_partial_week(),
                target,
                UtcOffset::UTC,
                PageLimits::new(50, 100).expect("limits"),
                1,
            )
            .await
            .expect("weekly report");

        server.join().expect("server completed");
        assert_eq!(report.identity.account_id, "mine");
        assert_eq!(report.summary.loaded_seconds, 3_600);
        assert_eq!(report.worklogs.len(), 1);
        assert_eq!(report.worklogs[0].issue_key.as_str(), "DEMO-1");
    }

    #[tokio::test]
    async fn board_discovery_fails_instead_of_returning_a_truncated_list() {
        let responses = vec![response(
            "GET",
            "/rest/agile/1.0/board?startAt=0&maxResults=1",
            r#"{"startAt":0,"total":2,"values":[{"id":42,"name":"Primer tablero","type":"scrum","self":"http://local/board/42"}]}"#,
            None,
        )];
        let (address, server) = spawn_test_server(responses);
        let result = loopback_client(address)
            .list_boards(PageLimits::new(1, 1).expect("limits"))
            .await;

        server.join().expect("server completed");
        assert!(matches!(result, Err(JiraError::CollectionLimitReached)));
    }

    #[test]
    fn refuses_to_truncate_collections_silently() {
        let mut values = vec![1_u8];
        let result = append_with_limit(&mut values, vec![2, 3], 2);
        assert!(matches!(result, Err(JiraError::CollectionLimitReached)));
        assert_eq!(values, vec![1]);
    }

    #[tokio::test]
    async fn creates_updates_and_deletes_only_the_authenticated_users_worklog() {
        let (address, server) = spawn_test_server(mutation_responses());
        let client = loopback_client(address);
        let issue_key = IssueKey::new("DEMO-1").expect("issue key");
        let input = test_worklog_input();

        let created = client
            .create_own_worklog(&issue_key, &input)
            .await
            .expect("create");
        let updated = client
            .update_own_worklog(&issue_key, "123", &input)
            .await
            .expect("update");
        client
            .delete_own_worklog(&issue_key, "123")
            .await
            .expect("delete");

        server.join().expect("server completed");
        assert_eq!(created.author.account_id, "mine");
        assert_eq!(updated.author.account_id, "mine");
    }

    #[test]
    fn computes_a_half_open_local_date_range() {
        let start = Date::from_calendar_date(2026, Month::August, 24).expect("valid date");
        let end = Date::from_calendar_date(2026, Month::August, 30).expect("valid date");
        let period = DateRange::new(start, end).expect("valid period");
        let offset = UtcOffset::from_hms(-3, 0, 0).expect("valid offset");
        let bounds = worklog_bounds(period, offset).expect("valid bounds");
        assert_eq!(bounds.started_before - bounds.started_after, 604_800_000);
    }

    #[test]
    fn ownership_check_fails_closed() {
        let worklog = worklog("other-account");
        let account = AccountId::new("current-account").expect("valid account");
        let result = ensure_owned_by(&worklog, account.as_str());
        assert!(matches!(result, Err(JiraError::WorklogOwnershipMismatch)));
    }

    #[test]
    fn rejects_invalid_limits() {
        assert!(PageLimits::new(0, 100).is_err());
        assert!(PageLimits::new(50, 0).is_err());
        assert!(validate_report_concurrency(0).is_err());
    }

    #[test]
    fn classifies_operational_http_statuses() {
        assert!(matches!(
            status_error(StatusCode::UNAUTHORIZED, None),
            JiraError::AuthenticationRequired
        ));
        assert!(matches!(
            status_error(StatusCode::FORBIDDEN, None),
            JiraError::Forbidden
        ));
        assert!(matches!(
            status_error(StatusCode::TOO_MANY_REQUESTS, Some(30)),
            JiraError::RateLimited {
                retry_after_seconds: Some(30)
            }
        ));
        assert!(matches!(
            status_error(StatusCode::SERVICE_UNAVAILABLE, None),
            JiraError::ServerUnavailable
        ));
        assert!(recoverable_issue_error(&JiraError::Forbidden));
        assert!(recoverable_issue_error(&JiraError::NotFound));
        assert!(!recoverable_issue_error(&JiraError::AuthenticationRequired));
        assert!(!recoverable_issue_error(&JiraError::RateLimited {
            retry_after_seconds: None,
        }));
    }

    #[test]
    fn maps_jira_date_and_adf_comment() {
        let issue = issue();
        let key = IssueKey::new(&issue.key).expect("valid key");
        let mapped = map_worklog(
            &issue,
            &key,
            "https://example.test/issue",
            worklog("mine"),
            UtcOffset::from_hms(-3, 0, 0).expect("offset"),
        )
        .expect("valid worklog");
        assert_eq!(
            mapped.started.offset(),
            UtcOffset::from_hms(-3, 0, 0).expect("offset")
        );
        assert_eq!(mapped.comment, "Trabajo realizado");
    }

    #[test]
    fn maps_own_worklog_with_connection_identity_and_remote_issue_id() {
        let client = client();
        let issue = issue();
        let identity = identity();
        let subject = client.provider_subject(&identity).expect("valid subject");
        let resource = client
            .external_resource(&issue)
            .expect("valid external resource");
        let entry = map_own_time_entry(&issue, resource, subject, worklog("mine"), UtcOffset::UTC)
            .expect("valid time entry");

        assert_eq!(entry.destination.remote_id(), "10001");
        assert_eq!(entry.destination.display_id(), "DEMO-1");
        assert_eq!(entry.author.remote_id(), "mine");
        assert_eq!(
            entry.author.connection_id().as_str(),
            "https://example.atlassian.net/"
        );
    }

    #[test]
    fn weekly_report_filters_worklogs_by_authenticated_account() {
        let period = weekly_period();
        let target = WeeklyTarget::from_minutes(1_800).expect("valid target");
        let key = IssueKey::new("DEMO-1").expect("valid key");
        let values = vec![domain_worklog(&key, "mine"), domain_worklog(&key, "other")];
        let report = build_report(identity(), period, target, &values, Vec::new()).expect("report");
        assert_eq!(report.summary.loaded_seconds, 3_600);
        assert_eq!(report.worklogs.len(), 1);
    }

    #[test]
    fn rejects_unknown_timestamp_shapes() {
        assert!(parse_jira_timestamp("2026/09/01").is_none());
    }

    #[test]
    fn own_worklog_presence_fails_closed_for_an_unparseable_own_entry() {
        let mut invalid = worklog("mine");
        invalid.started = "not-a-timestamp".to_owned();

        assert!(matches!(
            own_worklog_presence(&[invalid], "mine"),
            Err(JiraError::InvalidWorklog)
        ));
    }

    #[test]
    fn own_worklog_presence_ignores_another_accounts_entry() {
        let mut other = worklog("other");
        other.started = "not-a-timestamp".to_owned();

        assert!(!own_worklog_presence(&[other], "mine").expect("other account is ignored"));
    }

    #[test]
    fn issue_search_matches_key_or_summary_and_obeys_the_limit() {
        let mut second = issue();
        second.key = "DEMO-2".to_owned();
        second.fields.summary = "Conciliación semanal".to_owned();
        let by_key = filter_issues(vec![issue(), second.clone()], "demo-2", 10);
        let by_summary = filter_issues(vec![issue(), second], "conciliación", 1);
        assert_eq!(by_key[0].key, "DEMO-2");
        assert_eq!(by_summary.len(), 1);
        assert_eq!(by_summary[0].key, "DEMO-2");
    }

    fn client() -> JiraClient {
        let site = JiraSiteUrl::parse("https://example.atlassian.net").expect("valid site");
        JiraClient::new(
            site,
            "user@example.com",
            "not-a-real-token",
            StdDuration::from_secs(5),
        )
        .expect("valid client")
    }

    fn loopback_client(address: SocketAddr) -> JiraClient {
        let site =
            JiraSiteUrl::loopback_for_test(&format!("http://{address}")).expect("loopback origin");
        JiraClient::new(
            site,
            "person@example.com",
            "test-token",
            TEST_REQUEST_TIMEOUT,
        )
        .expect("test client")
    }

    fn test_worklog_input() -> WorklogInput {
        let date = Date::from_calendar_date(2026, Month::September, 1).expect("date");
        let started = date.with_time(Time::MIDNIGHT).assume_utc();
        WorklogInput::new(started, 3_600, "Trabajo realizado").expect("input")
    }

    fn mutation_responses() -> Vec<TestResponse> {
        vec![
            response("GET", "/rest/api/3/myself", IDENTITY_RESPONSE, None),
            response(
                "POST",
                mutation_path(None),
                WORKLOG_RESPONSE,
                Some("\"author\""),
            ),
            response("GET", "/rest/api/3/myself", IDENTITY_RESPONSE, None),
            response("GET", worklog_path(), WORKLOG_RESPONSE, None),
            response(
                "PUT",
                mutation_path(Some("123")),
                WORKLOG_RESPONSE,
                Some("\"author\""),
            ),
            response("GET", "/rest/api/3/myself", IDENTITY_RESPONSE, None),
            response("GET", worklog_path(), WORKLOG_RESPONSE, None),
            response("DELETE", mutation_path(Some("123")), "", None),
        ]
    }

    fn unlogged_issue_responses() -> Vec<TestResponse> {
        vec![
            response("GET", "/rest/api/3/myself", IDENTITY_RESPONSE, None),
            response(
                "GET",
                "/rest/software/1.0/board/42/issue?jql=assignee+%3D+currentUser%28%29+AND+sprint+in+openSprints%28%29+ORDER+BY+updated+DESC&fields=summary%2Cassignee%2Cissuetype%2Cstatus&maxResults=50",
                r#"{"isLast":true,"issues":[{"id":"10002","key":"DEMO-2","self":"http://local/issue/10002","fields":{"summary":"Más reciente"}},{"id":"10001","key":"DEMO-1","self":"http://local/issue/10001","fields":{"summary":"Más antigua"}}]}"#,
                None,
            ),
            response(
                "GET",
                "/rest/api/3/issue/DEMO-2/worklog?startAt=0&maxResults=50&startedAfter=1788134400000&startedBefore=1788393600000",
                r#"{"startAt":0,"total":0,"worklogs":[]}"#,
                None,
            ),
            response(
                "GET",
                "/rest/api/3/issue/DEMO-1/worklog?startAt=0&maxResults=50&startedAfter=1788134400000&startedBefore=1788393600000",
                r#"{"startAt":0,"total":1,"worklogs":[{"id":"123","author":{"accountId":"mine","displayName":"Persona Demo","active":true},"started":"2026-09-01T12:00:00.000+0000","timeSpentSeconds":3600}]}"#,
                None,
            ),
        ]
    }

    fn weekly_report_responses() -> Vec<TestResponse> {
        vec![
            response("GET", "/rest/api/3/myself", IDENTITY_RESPONSE, None),
            response(
                "GET",
                "/rest/software/1.0/board/42/issue?jql=worklogDate+%3E%3D+%222026-08-31%22+AND+worklogDate+%3C%3D+%222026-09-02%22&fields=summary%2Cassignee%2Cissuetype%2Cstatus&maxResults=50",
                r#"{"isLast":true,"issues":[{"id":"10001","key":"DEMO-1","self":"http://local/issue/10001","fields":{"summary":"Tarea de prueba"}}]}"#,
                None,
            ),
            response(
                "GET",
                "/rest/api/3/issue/DEMO-1/worklog?startAt=0&maxResults=50&startedAfter=1788134400000&startedBefore=1788393600000",
                r#"{"startAt":0,"total":2,"worklogs":[{"id":"123","author":{"accountId":"mine","displayName":"Persona Demo","active":true},"started":"2026-09-01T12:00:00.000+0000","timeSpentSeconds":3600},{"id":"124","author":{"accountId":"other","displayName":"Otra persona","active":true},"started":"2026-09-01T14:00:00.000+0000","timeSpentSeconds":7200}]}"#,
                None,
            ),
        ]
    }

    const fn worklog_path() -> &'static str {
        "/rest/api/3/issue/DEMO-1/worklog/123"
    }

    const fn mutation_path(worklog_id: Option<&str>) -> &'static str {
        match worklog_id {
            Some(_) => "/rest/api/3/issue/DEMO-1/worklog/123?adjustEstimate=leave",
            None => "/rest/api/3/issue/DEMO-1/worklog?adjustEstimate=leave",
        }
    }

    const fn response(
        method: &'static str,
        path: &'static str,
        response_body: &'static str,
        forbidden_request_text: Option<&'static str>,
    ) -> TestResponse {
        TestResponse {
            method,
            path,
            status: "200 OK",
            response_body,
            forbidden_request_text,
            expected_request_body: None,
        }
    }

    const fn mutation_response(
        method: &'static str,
        path: &'static str,
        response_body: &'static str,
        expected_request_body: &'static str,
    ) -> TestResponse {
        TestResponse {
            method,
            path,
            status: "200 OK",
            response_body,
            forbidden_request_text: Some("confirmed"),
            expected_request_body: Some(expected_request_body),
        }
    }

    const fn rejected_response(
        method: &'static str,
        path: &'static str,
        response_body: &'static str,
    ) -> TestResponse {
        TestResponse {
            method,
            path,
            status: "400 Bad Request",
            response_body,
            forbidden_request_text: None,
            expected_request_body: None,
        }
    }

    fn worklog(account_id: &str) -> WorklogDto {
        WorklogDto {
            id: "123".to_owned(),
            issue_id: Some("10000".to_owned()),
            author: WorklogAuthorDto {
                account_id: account_id.to_owned(),
                display_name: "User".to_owned(),
                active: true,
            },
            started: "2026-09-01T12:00:00.000-0300".to_owned(),
            time_spent_seconds: 3_600,
            comment: Some(json!({
                "type": "doc",
                "content": [{"type": "paragraph", "content": [
                    {"type": "text", "text": "Trabajo realizado"}
                ]}]
            })),
            created: None,
            updated: None,
            visibility: None,
        }
    }

    fn issue() -> IssueDto {
        IssueDto {
            id: "10001".to_owned(),
            key: "DEMO-1".to_owned(),
            self_url: "https://example.test/api/issue/10001".to_owned(),
            fields: IssueFieldsDto {
                summary: "Tarea de prueba".to_owned(),
                assignee: None,
                issue_type: None,
                status: None,
            },
        }
    }

    fn identity() -> JiraUserDto {
        JiraUserDto {
            account_id: "mine".to_owned(),
            display_name: "Persona Demo".to_owned(),
            active: true,
            account_type: Some("atlassian".to_owned()),
            email_address: None,
            time_zone: None,
        }
    }

    fn weekly_period() -> DateRange {
        let start = Date::from_calendar_date(2026, Month::August, 31).expect("date");
        let end = Date::from_calendar_date(2026, Month::September, 6).expect("date");
        DateRange::new(start, end).expect("period")
    }

    fn current_partial_week() -> DateRange {
        let start = Date::from_calendar_date(2026, Month::August, 31).expect("date");
        let end = Date::from_calendar_date(2026, Month::September, 2).expect("date");
        DateRange::new(start, end).expect("period")
    }

    fn domain_worklog(key: &IssueKey, account: &str) -> hours_core::Worklog {
        let dto = worklog(account);
        map_worklog(
            &issue(),
            key,
            "https://example.test/issue",
            dto,
            UtcOffset::from_hms(-3, 0, 0).expect("offset"),
        )
        .expect("mapped")
    }

    fn spawn_test_server(responses: Vec<TestResponse>) -> (SocketAddr, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
        let address = listener.local_addr().expect("listener address");
        let server = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().expect("test request");
                let request = read_request(&mut stream);
                assert_eq!(request.method, response.method);
                assert_eq!(request.path, response.path);
                if let Some(forbidden) = response.forbidden_request_text {
                    assert!(!request.body.contains(forbidden));
                }
                assert_request_body(&request.body, response.expected_request_body);
                write_json_response(&mut stream, response.status, response.response_body);
            }
        });
        (address, server)
    }

    fn assert_request_body(actual: &str, expected: Option<&str>) {
        let Some(expected) = expected else {
            return;
        };
        let actual: serde_json::Value = serde_json::from_str(actual).expect("request JSON");
        let expected: serde_json::Value = serde_json::from_str(expected).expect("expected JSON");
        assert_eq!(actual, expected);
    }

    fn read_request(stream: &mut TcpStream) -> TestRequest {
        let mut reader = BufReader::new(stream.try_clone().expect("cloned stream"));
        let mut request_line = String::new();
        reader.read_line(&mut request_line).expect("request line");
        let mut parts = request_line.split_whitespace();
        let method = parts.next().expect("request method").to_owned();
        let path = parts.next().expect("request path").to_owned();
        let content_length = read_content_length(&mut reader);
        let body = read_request_body(&mut reader, content_length);
        TestRequest { method, path, body }
    }

    fn read_content_length(reader: &mut BufReader<TcpStream>) -> usize {
        let mut content_length = 0;
        loop {
            let mut header = String::new();
            reader.read_line(&mut header).expect("request header");
            if header == "\r\n" {
                return content_length;
            }
            if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:") {
                content_length = value.trim().parse().expect("content length");
            }
        }
    }

    fn read_request_body(reader: &mut BufReader<TcpStream>, content_length: usize) -> String {
        let mut body = vec![0; content_length];
        reader.read_exact(&mut body).expect("request body");
        String::from_utf8(body).expect("UTF-8 request body")
    }

    fn write_json_response(stream: &mut TcpStream, status: &str, body: &str) {
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("test response");
    }
}
