use std::future::Future;
use std::pin::Pin;
use std::time::Duration as StandardDuration;

use hours_core::{DateRange, Duration as HoursDuration, IssueKey};
use jira_adapter::{JiraClient, JiraSiteUrl, PageLimits, WorklogDto, WorklogInput};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};

use crate::jira_issues::{JiraActorData, JiraIssueBackendError, JiraMutationTarget};
use crate::tool_failure::RESPONSE_SCHEMA_VERSION;
use crate::{JiraConfiguration, MutationConfirmation, ToolFailure};

const MINUTES_PER_HOUR: u32 = 60;
const SECONDS_PER_MINUTE: i32 = 60;

pub type JiraWorklogFuture<'backend, Output> =
    Pin<Box<dyn Future<Output = Result<Output, JiraIssueBackendError>> + Send + 'backend>>;

pub trait JiraWorklogBackend: Send + Sync {
    fn preview_create_worklog(
        &self,
        request: JiraCreateWorklogRequest,
    ) -> JiraWorklogFuture<'_, JiraWorklogPlan>;
    fn create_worklog(
        &self,
        request: JiraCreateWorklogRequest,
    ) -> JiraWorklogFuture<'_, JiraWorklogData>;
}

pub struct JiraWorklogService {
    client: JiraClient,
    site: JiraSiteUrl,
    board_id: u64,
    offset: UtcOffset,
    page_size: u16,
    scope_maximum_items: usize,
    worklog_maximum_items: usize,
    maximum_worklog_minutes: u32,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JiraCreateWorklogRequest {
    /// Jira issue key, for example PROJECT-123.
    pub key: String,
    /// Exact RFC 3339 start instant including the configured local UTC offset.
    pub started_at: String,
    /// Positive whole-minute duration, bounded by the configured daily limit.
    pub duration_minutes: u32,
    /// Optional plain-text description of the work performed.
    pub comment: Option<String>,
    /// Must be true after the user reviews identity, destination and worklog details.
    pub confirmed: bool,
    /// Single-use token returned by the preview for this exact request.
    pub confirmation_token: Option<String>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JiraWorklogPlan {
    pub source: String,
    pub actor: JiraActorData,
    pub board_id: u64,
    pub target: JiraMutationTarget,
    pub effect: JiraPlannedWorklogEffect,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JiraWorklogData {
    pub source: String,
    pub actor: JiraActorData,
    pub board_id: u64,
    pub target: JiraMutationTarget,
    pub effect: JiraWorklogEffect,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JiraPlannedWorklogEffect {
    pub started_at: String,
    pub duration_minutes: u32,
    pub comment: Option<String>,
    pub possible_duplicate_worklog_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JiraWorklogEffect {
    pub worklog_id: String,
    pub started_at: String,
    pub duration_seconds: u32,
    pub comment: Option<String>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JiraWorklogToolResponse {
    pub schema_version: u16,
    pub success: bool,
    pub data: Option<JiraWorklogData>,
    pub error: Option<ToolFailure>,
    pub confirmation: Option<MutationConfirmation<JiraWorklogPlan>>,
}

impl JiraWorklogService {
    /// Builds the write port with board scope and the configured hours offset.
    ///
    /// # Errors
    ///
    /// Returns an error when Jira or Hours configuration is invalid.
    pub fn new(
        configuration: &JiraConfiguration,
        token: String,
    ) -> Result<Self, JiraIssueBackendError> {
        let site = JiraSiteUrl::parse(&configuration.base_url)
            .map_err(|_| JiraIssueBackendError::InvalidConfiguration)?;
        let timeout = StandardDuration::from_secs(configuration.request_timeout_seconds);
        let client = JiraClient::new(site.clone(), configuration.email.clone(), token, timeout)
            .map_err(|_| JiraIssueBackendError::InvalidConfiguration)?;
        let offset = configured_offset(configuration)?;
        let maximum_worklog_minutes = configured_maximum_worklog_minutes(configuration)?;
        Ok(Self {
            client,
            site,
            board_id: configuration.board_id,
            offset,
            page_size: configuration.page_size,
            scope_maximum_items: configuration.maximum_issue_search_results,
            worklog_maximum_items: configuration.maximum_collection_items,
            maximum_worklog_minutes,
        })
    }

    async fn preview(
        &self,
        request: JiraCreateWorklogRequest,
    ) -> Result<JiraWorklogPlan, JiraIssueBackendError> {
        let key = issue_key(&request.key)?;
        self.ensure_issue_scope(&key).await?;
        let input = worklog_input(
            &request,
            self.offset,
            self.maximum_worklog_minutes,
            OffsetDateTime::now_utc(),
        )?;
        let actor = self.load_actor().await?;
        let duplicates = self.possible_duplicates(&key, &actor, &input).await?;
        Ok(JiraWorklogPlan {
            source: crate::tool_failure::SOURCE_JIRA.to_owned(),
            actor,
            board_id: self.board_id,
            target: self.target(&key),
            effect: planned_effect(&request, duplicates),
        })
    }

    async fn create(
        &self,
        request: JiraCreateWorklogRequest,
    ) -> Result<JiraWorklogData, JiraIssueBackendError> {
        require_confirmation(request.confirmed)?;
        let key = issue_key(&request.key)?;
        self.ensure_issue_scope(&key).await?;
        let input = worklog_input(
            &request,
            self.offset,
            self.maximum_worklog_minutes,
            OffsetDateTime::now_utc(),
        )?;
        let created = self
            .client
            .create_own_worklog(&key, &input)
            .await
            .map_err(|error| crate::jira_issues::provider_error(&error))?;
        Ok(self.created_data(&key, &created, &request))
    }

    async fn ensure_issue_scope(&self, key: &IssueKey) -> Result<(), JiraIssueBackendError> {
        let in_scope = self
            .client
            .board_contains_issue(self.board_id, key, self.limits(self.scope_maximum_items)?)
            .await
            .map_err(|error| crate::jira_issues::provider_error(&error))?;
        if in_scope {
            return Ok(());
        }
        Err(JiraIssueBackendError::IssueOutsideScope)
    }

    async fn load_actor(&self) -> Result<JiraActorData, JiraIssueBackendError> {
        self.client
            .current_user()
            .await
            .map(actor_data)
            .map_err(|error| crate::jira_issues::provider_error(&error))
    }

    async fn possible_duplicates(
        &self,
        key: &IssueKey,
        actor: &JiraActorData,
        input: &WorklogInput,
    ) -> Result<Vec<String>, JiraIssueBackendError> {
        let worklogs = self.worklogs_for_date(key, input.started.date()).await?;
        Ok(duplicate_worklog_ids(&worklogs, actor, input))
    }

    async fn worklogs_for_date(
        &self,
        key: &IssueKey,
        date: Date,
    ) -> Result<Vec<WorklogDto>, JiraIssueBackendError> {
        let period =
            DateRange::new(date, date).map_err(|_| JiraIssueBackendError::InvalidConfiguration)?;
        self.client
            .list_worklogs(
                key,
                period,
                self.offset,
                self.limits(self.worklog_maximum_items)?,
            )
            .await
            .map_err(|error| crate::jira_issues::provider_error(&error))
    }

    fn limits(&self, maximum_items: usize) -> Result<PageLimits, JiraIssueBackendError> {
        let page_size = usize::from(self.page_size).min(maximum_items);
        let page_size =
            u16::try_from(page_size).map_err(|_| JiraIssueBackendError::InvalidConfiguration)?;
        PageLimits::new(page_size, maximum_items)
            .map_err(|_| JiraIssueBackendError::InvalidConfiguration)
    }

    fn target(&self, key: &IssueKey) -> JiraMutationTarget {
        JiraMutationTarget {
            key: key.as_str().to_owned(),
            url: self.site.issue_browser_url(key),
        }
    }

    fn created_data(
        &self,
        key: &IssueKey,
        created: &WorklogDto,
        request: &JiraCreateWorklogRequest,
    ) -> JiraWorklogData {
        JiraWorklogData {
            source: crate::tool_failure::SOURCE_JIRA.to_owned(),
            actor: worklog_actor_data(&created.author),
            board_id: self.board_id,
            target: self.target(key),
            effect: created_effect(created, request),
        }
    }
}

impl JiraWorklogBackend for JiraWorklogService {
    fn preview_create_worklog(
        &self,
        request: JiraCreateWorklogRequest,
    ) -> JiraWorklogFuture<'_, JiraWorklogPlan> {
        Box::pin(self.preview(request))
    }

    fn create_worklog(
        &self,
        request: JiraCreateWorklogRequest,
    ) -> JiraWorklogFuture<'_, JiraWorklogData> {
        Box::pin(self.create(request))
    }
}

impl JiraWorklogToolResponse {
    #[must_use]
    pub const fn success(data: JiraWorklogData) -> Self {
        Self {
            schema_version: RESPONSE_SCHEMA_VERSION,
            success: true,
            data: Some(data),
            error: None,
            confirmation: None,
        }
    }

    #[must_use]
    pub const fn failure(error: ToolFailure) -> Self {
        Self {
            schema_version: RESPONSE_SCHEMA_VERSION,
            success: false,
            data: None,
            error: Some(error),
            confirmation: None,
        }
    }

    #[must_use]
    pub const fn confirmation_required(
        error: ToolFailure,
        confirmation: MutationConfirmation<JiraWorklogPlan>,
    ) -> Self {
        Self {
            schema_version: RESPONSE_SCHEMA_VERSION,
            success: false,
            data: None,
            error: Some(error),
            confirmation: Some(confirmation),
        }
    }
}

fn configured_offset(
    configuration: &JiraConfiguration,
) -> Result<UtcOffset, JiraIssueBackendError> {
    let minutes = configuration
        .hours
        .as_ref()
        .ok_or(JiraIssueBackendError::InvalidConfiguration)?
        .utc_offset_minutes;
    let seconds = i32::from(minutes) * SECONDS_PER_MINUTE;
    UtcOffset::from_whole_seconds(seconds).map_err(|_| JiraIssueBackendError::InvalidConfiguration)
}

fn configured_maximum_worklog_minutes(
    configuration: &JiraConfiguration,
) -> Result<u32, JiraIssueBackendError> {
    let hours = configuration
        .hours
        .as_ref()
        .ok_or(JiraIssueBackendError::InvalidConfiguration)?;
    Ok(u32::from(hours.maximum_daily_hours) * MINUTES_PER_HOUR)
}

fn worklog_input(
    request: &JiraCreateWorklogRequest,
    offset: UtcOffset,
    maximum_worklog_minutes: u32,
    now: OffsetDateTime,
) -> Result<WorklogInput, JiraIssueBackendError> {
    let started = parse_worklog_start(&request.started_at, offset)?;
    validate_worklog_date(started, now)?;
    let duration = HoursDuration::from_minutes(request.duration_minutes)
        .map_err(|_| JiraIssueBackendError::InvalidConfiguration)?;
    if request.duration_minutes > maximum_worklog_minutes {
        return Err(JiraIssueBackendError::InvalidConfiguration);
    }
    WorklogInput::new(
        started,
        duration.seconds(),
        normalized_comment(request.comment.as_deref()).unwrap_or_default(),
    )
    .map_err(|_| JiraIssueBackendError::InvalidConfiguration)
}

fn parse_worklog_start(
    value: &str,
    expected_offset: UtcOffset,
) -> Result<OffsetDateTime, JiraIssueBackendError> {
    let started = OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|_| JiraIssueBackendError::InvalidConfiguration)?;
    if started.offset() == expected_offset {
        return Ok(started);
    }
    Err(JiraIssueBackendError::InvalidConfiguration)
}

fn validate_worklog_date(
    started: OffsetDateTime,
    now: OffsetDateTime,
) -> Result<(), JiraIssueBackendError> {
    if started.date() <= now.to_offset(started.offset()).date() {
        return Ok(());
    }
    Err(JiraIssueBackendError::InvalidConfiguration)
}

fn issue_key(value: &str) -> Result<IssueKey, JiraIssueBackendError> {
    IssueKey::new(value).map_err(|_| JiraIssueBackendError::InvalidConfiguration)
}

fn require_confirmation(confirmed: bool) -> Result<(), JiraIssueBackendError> {
    if confirmed {
        return Ok(());
    }
    Err(JiraIssueBackendError::ConfirmationRequired)
}

fn normalized_comment(comment: Option<&str>) -> Option<String> {
    comment
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn duplicate_worklog_ids(
    worklogs: &[WorklogDto],
    actor: &JiraActorData,
    input: &WorklogInput,
) -> Vec<String> {
    worklogs
        .iter()
        .filter(|worklog| worklog.author.account_id == actor.account_id)
        .filter(|worklog| worklog.time_spent_seconds == input.time_spent_seconds)
        .map(|worklog| worklog.id.clone())
        .collect()
}

fn actor_data(user: jira_adapter::JiraUserDto) -> JiraActorData {
    JiraActorData {
        account_id: user.account_id,
        display_name: user.display_name,
    }
}

fn worklog_actor_data(author: &jira_adapter::WorklogAuthorDto) -> JiraActorData {
    JiraActorData {
        account_id: author.account_id.clone(),
        display_name: author.display_name.clone(),
    }
}

fn planned_effect(
    request: &JiraCreateWorklogRequest,
    duplicate_ids: Vec<String>,
) -> JiraPlannedWorklogEffect {
    JiraPlannedWorklogEffect {
        started_at: request.started_at.clone(),
        duration_minutes: request.duration_minutes,
        comment: normalized_comment(request.comment.as_deref()),
        possible_duplicate_worklog_ids: duplicate_ids,
    }
}

fn created_effect(created: &WorklogDto, request: &JiraCreateWorklogRequest) -> JiraWorklogEffect {
    JiraWorklogEffect {
        worklog_id: created.id.clone(),
        started_at: created.started.clone(),
        duration_seconds: created.time_spent_seconds,
        comment: normalized_comment(request.comment.as_deref()),
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use jira_adapter::WorklogAuthorDto;
    use time::macros::datetime;

    use super::*;

    const LIVE_TEST_TIMEOUT_SECONDS: u64 = 30;
    const LIVE_TEST_PAGE_SIZE: u16 = 100;
    const LIVE_TEST_MAXIMUM_ITEMS: usize = 2_000;

    #[test]
    fn worklog_start_requires_the_configured_offset_and_rejects_future_dates() {
        let expected_offset = configured_test_offset(-180);
        let valid =
            parse_worklog_start("2026-09-04T09:00:00-03:00", expected_offset).expect("valid start");
        let wrong_offset = parse_worklog_start("2026-09-04T12:00:00Z", expected_offset);
        let future = validate_worklog_date(valid, datetime!(2026-09-03 23:00 -03:00));

        assert!(matches!(
            wrong_offset,
            Err(JiraIssueBackendError::InvalidConfiguration)
        ));
        assert!(matches!(
            future,
            Err(JiraIssueBackendError::InvalidConfiguration)
        ));
    }

    #[test]
    fn duplicate_detection_only_reports_own_matching_duration() {
        let actor = JiraActorData {
            account_id: "mine".to_owned(),
            display_name: "Taylor Example".to_owned(),
        };
        let input =
            WorklogInput::new(datetime!(2026-09-04 09:00 -03:00), 7_200, "Work").expect("input");
        let worklogs = [
            worklog_dto("1", "mine", 7_200),
            worklog_dto("2", "other", 7_200),
        ];

        assert_eq!(duplicate_worklog_ids(&worklogs, &actor, &input), ["1"]);
    }

    #[tokio::test]
    #[ignore = "requires explicit read-only Jira test environment variables"]
    async fn live_preview_reads_identity_scope_and_duplicates_without_writing() {
        let configuration = live_test_configuration();
        let token = required_environment("WORKLOGGER_TEST_JIRA_TOKEN");
        let key = required_environment("WORKLOGGER_TEST_JIRA_ISSUE_KEY");
        let service = JiraWorklogService::new(&configuration, token).expect("test service");
        let preview = service
            .preview(live_worklog_request(&configuration, key))
            .await
            .expect("worklog preview");

        assert!(!preview.actor.account_id.is_empty());
        assert_eq!(preview.effect.duration_minutes, 1);
    }

    fn live_worklog_request(
        configuration: &JiraConfiguration,
        key: String,
    ) -> JiraCreateWorklogRequest {
        let offset = configured_offset(configuration).expect("test offset");
        JiraCreateWorklogRequest {
            key,
            started_at: today_at_midnight(offset),
            duration_minutes: 1,
            comment: Some("Read-only preview test".to_owned()),
            confirmed: false,
            confirmation_token: None,
        }
    }

    fn live_test_configuration() -> JiraConfiguration {
        JiraConfiguration {
            base_url: required_environment("WORKLOGGER_TEST_JIRA_URL"),
            email: required_environment("WORKLOGGER_TEST_JIRA_EMAIL"),
            board_id: required_environment("WORKLOGGER_TEST_JIRA_BOARD_ID")
                .parse()
                .expect("test board ID is numeric"),
            request_timeout_seconds: LIVE_TEST_TIMEOUT_SECONDS,
            page_size: LIVE_TEST_PAGE_SIZE,
            maximum_collection_items: LIVE_TEST_MAXIMUM_ITEMS,
            maximum_issue_search_results: LIVE_TEST_MAXIMUM_ITEMS,
            hours: Some(crate::JiraHoursConfiguration {
                weekly_target_hours: 40,
                utc_offset_minutes: required_environment("WORKLOGGER_TEST_JIRA_UTC_OFFSET_MINUTES")
                    .parse()
                    .expect("test UTC offset is numeric"),
                maximum_daily_hours: 24,
                maximum_concurrent_worklog_requests: 1,
            }),
        }
    }

    fn today_at_midnight(offset: UtcOffset) -> String {
        OffsetDateTime::now_utc()
            .to_offset(offset)
            .replace_time(time::Time::MIDNIGHT)
            .format(&Rfc3339)
            .expect("RFC 3339 start")
    }

    fn configured_test_offset(minutes: i16) -> UtcOffset {
        UtcOffset::from_whole_seconds(i32::from(minutes) * SECONDS_PER_MINUTE).expect("offset")
    }

    fn required_environment(name: &str) -> String {
        env::var(name).unwrap_or_else(|_| panic!("missing {name}"))
    }

    fn worklog_dto(id: &str, account_id: &str, seconds: u32) -> WorklogDto {
        WorklogDto {
            id: id.to_owned(),
            issue_id: Some("10001".to_owned()),
            author: WorklogAuthorDto {
                account_id: account_id.to_owned(),
                display_name: "Taylor Example".to_owned(),
                active: true,
            },
            started: "2026-09-04T09:00:00.000-0300".to_owned(),
            time_spent_seconds: seconds,
            comment: None,
            created: None,
            updated: None,
            visibility: None,
        }
    }
}
