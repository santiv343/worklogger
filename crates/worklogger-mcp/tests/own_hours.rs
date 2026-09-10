#![cfg(feature = "jira")]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use hours_core::{AccountId, DateRange, Duration, IssueKey, WeeklySummary, WeeklyTarget, Worklog};
use jira_adapter::{JiraUserDto, WeeklyReport};
use rmcp::{ServiceExt, model::CallToolRequestParams};
use serde_json::json;
use time::macros::{date, datetime};
use tokio::task::JoinHandle;
#[cfg(feature = "bitbucket")]
use worklogger_mcp::{
    BitbucketBackendError, BitbucketConfiguration, BitbucketFuture, BitbucketMutationData,
    BitbucketMutationPlan, BitbucketMutationRequest, BitbucketPlannedEffect,
    BitbucketPlannedTarget, BitbucketPullRequestBackend, BitbucketPullRequestData,
    BitbucketPullRequestListData, BitbucketPullRequestRevision, BitbucketPullRequestTarget,
    BitbucketRepositoryListData,
};
use worklogger_mcp::{
    Capability, DEFAULT_MAXIMUM_REPORT_PERIOD_DAYS, JiraConfiguration, JiraHoursConfiguration,
    JiraIssueBackend, JiraIssueBackendError, JiraIssueFuture, JiraMutationEffect, JiraMutationPlan,
    JiraMutationRequest, JiraMutationTarget, JiraPlannedEffect, JiraPlannedWorklogEffect,
    JiraWorklogBackend, JiraWorklogData, JiraWorklogEffect, JiraWorklogFuture, JiraWorklogPlan,
    MAXIMUM_REPORT_PERIOD_DAYS, McpConfiguration, ModuleConfiguration, ModuleId, OwnHoursBackend,
    OwnHoursBackendError, OwnHoursFuture, OwnHoursRequest, OwnHoursToolResponse, UnloggedIssue,
    UnloggedIssuesFuture, WorkloggerMcpServer, resolve_period,
};

#[derive(Default)]
struct PreviewJiraBackend {
    updates: Arc<AtomicUsize>,
}

impl JiraIssueBackend for PreviewJiraBackend {
    fn get_issue(
        &self,
        _request: worklogger_mcp::JiraGetIssueRequest,
    ) -> JiraIssueFuture<'_, worklogger_mcp::JiraIssueData> {
        jira_unavailable()
    }

    fn search_issues(
        &self,
        _request: worklogger_mcp::JiraSearchIssuesRequest,
    ) -> JiraIssueFuture<'_, worklogger_mcp::JiraIssueSearchData> {
        jira_unavailable()
    }

    fn get_edit_metadata(
        &self,
        _request: worklogger_mcp::JiraIssueKeyRequest,
    ) -> JiraIssueFuture<'_, worklogger_mcp::JiraEditMetadataData> {
        jira_unavailable()
    }

    fn get_transitions(
        &self,
        _request: worklogger_mcp::JiraIssueKeyRequest,
    ) -> JiraIssueFuture<'_, worklogger_mcp::JiraTransitionsData> {
        jira_unavailable()
    }

    fn update_issue(
        &self,
        _request: worklogger_mcp::JiraUpdateIssueRequest,
    ) -> JiraIssueFuture<'_, worklogger_mcp::JiraMutationData> {
        self.updates.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Ok(jira_mutation()) })
    }

    fn add_comment(
        &self,
        _request: worklogger_mcp::JiraAddCommentRequest,
    ) -> JiraIssueFuture<'_, worklogger_mcp::JiraMutationData> {
        jira_unavailable()
    }

    fn transition_issue(
        &self,
        _request: worklogger_mcp::JiraTransitionIssueRequest,
    ) -> JiraIssueFuture<'_, worklogger_mcp::JiraMutationData> {
        jira_unavailable()
    }

    fn preview_mutation(
        &self,
        _request: JiraMutationRequest,
    ) -> JiraIssueFuture<'_, JiraMutationPlan> {
        Box::pin(async { Ok(jira_preview()) })
    }
}

#[derive(Default)]
struct PreviewJiraWorklogBackend {
    worklogs: Arc<AtomicUsize>,
}

impl JiraWorklogBackend for PreviewJiraWorklogBackend {
    fn preview_create_worklog(
        &self,
        _request: worklogger_mcp::JiraCreateWorklogRequest,
    ) -> JiraWorklogFuture<'_, JiraWorklogPlan> {
        Box::pin(async { Ok(jira_worklog_preview()) })
    }

    fn create_worklog(
        &self,
        _request: worklogger_mcp::JiraCreateWorklogRequest,
    ) -> JiraWorklogFuture<'_, JiraWorklogData> {
        self.worklogs.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Ok(jira_worklog_mutation()) })
    }
}

fn jira_unavailable<Output>() -> JiraIssueFuture<'static, Output> {
    Box::pin(async { Err(JiraIssueBackendError::InvalidConfiguration) })
}

fn jira_preview() -> JiraMutationPlan {
    JiraMutationPlan {
        source: "jira".to_owned(),
        actor: worklogger_mcp::JiraActorData {
            account_id: "account-1".to_owned(),
            display_name: "Taylor Example".to_owned(),
        },
        target: JiraMutationTarget {
            key: "DEMO-1".to_owned(),
            url: "https://example.atlassian.net/browse/DEMO-1".to_owned(),
        },
        effect: JiraPlannedEffect::UpdateFields {
            current_fields: BTreeMap::from([("summary".to_owned(), json!("Old"))]),
            requested_fields: BTreeMap::from([("summary".to_owned(), json!("New"))]),
        },
    }
}

fn jira_mutation() -> worklogger_mcp::JiraMutationData {
    worklogger_mcp::JiraMutationData {
        source: "jira".to_owned(),
        actor: jira_preview().actor,
        target: jira_preview().target,
        effect: JiraMutationEffect::FieldsUpdated {
            field_ids: vec!["summary".to_owned()],
        },
    }
}

fn jira_worklog_preview() -> JiraWorklogPlan {
    JiraWorklogPlan {
        source: "jira".to_owned(),
        actor: jira_preview().actor,
        board_id: 42,
        target: jira_preview().target,
        effect: JiraPlannedWorklogEffect {
            started_at: "2026-09-04T09:00:00-03:00".to_owned(),
            duration_minutes: 120,
            comment: Some("Implementation".to_owned()),
            possible_duplicate_worklog_ids: Vec::new(),
        },
    }
}

fn jira_worklog_mutation() -> JiraWorklogData {
    JiraWorklogData {
        source: "jira".to_owned(),
        actor: jira_worklog_preview().actor,
        board_id: 42,
        target: jira_worklog_preview().target,
        effect: JiraWorklogEffect {
            worklog_id: "9001".to_owned(),
            started_at: "2026-09-04T09:00:00-03:00".to_owned(),
            duration_seconds: 7_200,
            comment: Some("Implementation".to_owned()),
        },
    }
}

#[cfg(feature = "bitbucket")]
#[derive(Default)]
struct PreviewBitbucketBackend {
    merges: Arc<AtomicUsize>,
    previews: Arc<AtomicUsize>,
    change_source_after_preview: bool,
}

#[cfg(feature = "bitbucket")]
impl BitbucketPullRequestBackend for PreviewBitbucketBackend {
    fn list_repositories(
        &self,
        _request: worklogger_mcp::BitbucketWorkspaceRequest,
    ) -> BitbucketFuture<'_, BitbucketRepositoryListData> {
        bitbucket_unavailable()
    }

    fn list_pull_requests(
        &self,
        _request: worklogger_mcp::BitbucketListPullRequestsRequest,
    ) -> BitbucketFuture<'_, BitbucketPullRequestListData> {
        bitbucket_unavailable()
    }

    fn get_pull_request(
        &self,
        _request: worklogger_mcp::BitbucketPullRequestKeyRequest,
    ) -> BitbucketFuture<'_, BitbucketPullRequestData> {
        bitbucket_unavailable()
    }

    fn get_activity(
        &self,
        _request: worklogger_mcp::BitbucketPullRequestKeyRequest,
    ) -> BitbucketFuture<'_, worklogger_mcp::BitbucketActivityData> {
        bitbucket_unavailable()
    }

    fn create_pull_request(
        &self,
        _request: worklogger_mcp::BitbucketCreatePullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData> {
        bitbucket_unavailable()
    }

    fn update_pull_request(
        &self,
        _request: worklogger_mcp::BitbucketUpdatePullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData> {
        bitbucket_unavailable()
    }

    fn add_comment(
        &self,
        _request: worklogger_mcp::BitbucketCommentRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData> {
        bitbucket_unavailable()
    }

    fn approve(
        &self,
        _request: worklogger_mcp::BitbucketConfirmedPullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData> {
        bitbucket_unavailable()
    }

    fn unapprove(
        &self,
        _request: worklogger_mcp::BitbucketConfirmedPullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData> {
        bitbucket_unavailable()
    }

    fn request_changes(
        &self,
        _request: worklogger_mcp::BitbucketConfirmedPullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData> {
        bitbucket_unavailable()
    }

    fn remove_change_request(
        &self,
        _request: worklogger_mcp::BitbucketConfirmedPullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData> {
        bitbucket_unavailable()
    }

    fn merge(
        &self,
        _request: worklogger_mcp::BitbucketMergePullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData> {
        self.merges.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Ok(bitbucket_mutation()) })
    }

    fn decline(
        &self,
        _request: worklogger_mcp::BitbucketConfirmedPullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData> {
        bitbucket_unavailable()
    }

    fn preview_mutation(
        &self,
        _request: BitbucketMutationRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationPlan> {
        let preview_number = self.previews.fetch_add(1, Ordering::Relaxed);
        let source_commit = if self.change_source_after_preview && preview_number > 0 {
            "changed-source-commit"
        } else {
            "source-commit"
        };
        let preview = bitbucket_preview_with_source(source_commit);
        Box::pin(async move { Ok(preview) })
    }
}

#[cfg(feature = "bitbucket")]
fn bitbucket_unavailable<Output>() -> BitbucketFuture<'static, Output> {
    Box::pin(async { Err(BitbucketBackendError::InvalidConfiguration) })
}

#[cfg(feature = "bitbucket")]
fn bitbucket_preview() -> BitbucketMutationPlan {
    bitbucket_preview_with_source("source-commit")
}

#[cfg(feature = "bitbucket")]
fn bitbucket_preview_with_source(source_commit: &str) -> BitbucketMutationPlan {
    let mut target = bitbucket_target();
    source_commit.clone_into(&mut target.revision.source_commit);
    BitbucketMutationPlan {
        source: "bitbucket".to_owned(),
        actor: worklogger_mcp::BitbucketActorData {
            account_id: "account-1".to_owned(),
            display_name: "Taylor Example".to_owned(),
        },
        target: BitbucketPlannedTarget::PullRequest(Box::new(target)),
        effect: BitbucketPlannedEffect::Merge {
            strategy: worklogger_mcp::MergeStrategy::Squash,
            message: None,
            close_source_branch: false,
        },
    }
}

#[cfg(feature = "bitbucket")]
fn bitbucket_target() -> BitbucketPullRequestTarget {
    BitbucketPullRequestTarget {
        workspace: "workspace".to_owned(),
        repository: "repository".to_owned(),
        pull_request_id: 7,
        url: "https://bitbucket.org/workspace/repository/pull-requests/7".to_owned(),
        source_branch: "feature".to_owned(),
        destination_branch: "main".to_owned(),
        state: worklogger_mcp::PullRequestState::Open,
        revision: bitbucket_revision(),
    }
}

#[cfg(feature = "bitbucket")]
fn bitbucket_revision() -> BitbucketPullRequestRevision {
    BitbucketPullRequestRevision {
        title: "Useful change".to_owned(),
        description: "Context".to_owned(),
        source_branch: "feature".to_owned(),
        destination_branch: "main".to_owned(),
        state: worklogger_mcp::PullRequestState::Open,
        source_commit: "source-commit".to_owned(),
        destination_commit: "destination-commit".to_owned(),
        updated_on: "2026-09-03T10:00:00+00:00".to_owned(),
        close_source_branch: true,
        draft: false,
        participants: Vec::new(),
    }
}

#[cfg(feature = "bitbucket")]
fn bitbucket_mutation() -> BitbucketMutationData {
    BitbucketMutationData {
        source: "bitbucket".to_owned(),
        actor: bitbucket_preview().actor,
        target: bitbucket_target(),
        effect: worklogger_mcp::BitbucketMutationEffect::Merged,
    }
}

#[test]
fn default_period_is_the_current_week_capped_at_today() {
    let request = OwnHoursRequest::default();
    let now = datetime!(2026-09-03 12:00 UTC);

    let period = resolve_period(&request, 0, DEFAULT_MAXIMUM_REPORT_PERIOD_DAYS, now)
        .expect("default period is valid");

    assert_eq!(period.start(), date!(2026 - 08 - 31));
    assert_eq!(period.end(), date!(2026 - 09 - 03));
}

#[test]
fn future_and_partial_explicit_periods_are_rejected() {
    let now = datetime!(2026-09-03 12:00 UTC);
    let partial = request(Some("2026-09-01"), None);
    let future = request(Some("2026-09-01"), Some("2026-09-04"));

    assert_eq!(
        resolve_period(&partial, 0, DEFAULT_MAXIMUM_REPORT_PERIOD_DAYS, now)
            .expect_err("partial period is rejected")
            .code,
        "invalid_period"
    );
    assert_eq!(
        resolve_period(&future, 0, DEFAULT_MAXIMUM_REPORT_PERIOD_DAYS, now)
            .expect_err("future period is rejected")
            .code,
        "invalid_period"
    );
}

#[test]
fn periods_longer_than_the_configured_limit_are_rejected() {
    let request = request(Some("2026-08-01"), Some("2026-08-08"));

    let error = resolve_period(
        &request,
        0,
        DEFAULT_MAXIMUM_REPORT_PERIOD_DAYS,
        datetime!(2026-09-03 12:00 UTC),
    )
    .expect_err("long period is rejected");

    assert_eq!(error.code, "invalid_period");
}

#[test]
fn configured_monthly_period_is_accepted() {
    let request = request(Some("2026-08-04"), Some("2026-09-03"));

    let period = resolve_period(
        &request,
        0,
        MAXIMUM_REPORT_PERIOD_DAYS,
        datetime!(2026-09-03 12:00 UTC),
    )
    .expect("monthly period is valid");

    assert_eq!(period.day_count(), u64::from(MAXIMUM_REPORT_PERIOD_DAYS));
}

#[test]
fn report_response_preserves_identity_totals_and_issue_links() {
    let report = report();
    let response = OwnHoursToolResponse::success(&report, datetime!(2026-09-03 12:00 UTC));
    let data = response.data.expect("successful response has data");

    assert!(response.success);
    assert_eq!(data.identity.display_name, "Taylor Example");
    assert_eq!(data.totals.loaded_seconds, 7_200);
    assert_eq!(data.entries[0].issue_key, "DEMO-42");
    assert_eq!(
        data.entries[0].issue_url,
        "https://example.atlassian.net/browse/DEMO-42"
    );
}

#[test]
fn server_only_lists_tools_enabled_by_module_capabilities() {
    let enabled_configuration = configuration(true);
    let disabled_configuration = configuration(false);
    let enabled_catalog = WorkloggerMcpServer::configured_tool_names(&enabled_configuration);
    let disabled_catalog = WorkloggerMcpServer::configured_tool_names(&disabled_configuration);
    let enabled =
        WorkloggerMcpServer::new(enabled_configuration).with_own_hours(Arc::new(FakeBackend));
    let disabled =
        WorkloggerMcpServer::new(disabled_configuration).with_own_hours(Arc::new(FakeBackend));

    let expected = BTreeSet::from([
        WorkloggerMcpServer::own_hours_tool_name(),
        WorkloggerMcpServer::unlogged_issues_tool_name(),
    ]);
    assert_eq!(
        enabled
            .enabled_tool_names()
            .into_iter()
            .collect::<BTreeSet<_>>(),
        expected
    );
    assert_eq!(enabled.enabled_tool_names(), enabled_catalog);
    assert!(disabled.enabled_tool_names().is_empty());
    assert!(disabled_catalog.is_empty());
}

#[test]
fn worklog_write_tool_has_its_own_capability() {
    let read_only = configuration(true);
    let read_write = configuration_with_capabilities(BTreeSet::from([
        Capability::ReadOwnTimeEntries,
        Capability::WriteOwnTimeEntries,
    ]));

    assert!(
        !WorkloggerMcpServer::configured_tool_names(&read_only)
            .contains(&"jira_create_worklog".to_owned())
    );
    assert!(
        WorkloggerMcpServer::configured_tool_names(&read_write)
            .contains(&"jira_create_worklog".to_owned())
    );
}

#[tokio::test]
async fn mcp_handshake_lists_and_calls_the_configured_tool() {
    let server =
        WorkloggerMcpServer::new(configuration(true)).with_own_hours(Arc::new(SuccessBackend));
    let (server_task, client) = start_mcp(server).await;
    let tools = listed_tools(&client).await;
    assert_eq!(tools.len(), 2);
    let names = tools
        .iter()
        .map(|tool| tool.name.as_ref())
        .collect::<BTreeSet<_>>();
    let own_hours = WorkloggerMcpServer::own_hours_tool_name();
    let unlogged_issues = WorkloggerMcpServer::unlogged_issues_tool_name();
    assert_eq!(
        names,
        BTreeSet::from([own_hours.as_str(), unlogged_issues.as_str()])
    );
    assert_successful_call(&client, CallToolRequestParams::new(own_hours)).await;
    assert_unlogged_issue_call(&client, unlogged_issues).await;
    stop_mcp(client, server_task).await;
}

#[tokio::test]
async fn jira_tools_follow_capabilities_and_reject_unconfirmed_writes() {
    let configuration = configuration_with_capabilities(jira_issue_capabilities());
    let updates = Arc::new(AtomicUsize::new(0));
    let backend = PreviewJiraBackend {
        updates: updates.clone(),
    };
    let server = WorkloggerMcpServer::new(configuration)
        .with_own_hours(Arc::new(FakeBackend))
        .with_jira_issues(Arc::new(backend));
    let (server_task, client) = start_mcp(server).await;
    assert_jira_tool_catalog(&client).await;
    let token = preview_jira_update(&client).await;
    confirm_jira_update(&client, &token).await;
    assert_eq!(updates.load(Ordering::Relaxed), 1);
    stop_mcp(client, server_task).await;
}

#[tokio::test]
async fn worklog_write_requires_preview_and_never_accepts_an_author() {
    let capabilities = BTreeSet::from([
        Capability::ReadOwnTimeEntries,
        Capability::WriteOwnTimeEntries,
    ]);
    let worklogs = Arc::new(AtomicUsize::new(0));
    let worklog_backend = PreviewJiraWorklogBackend {
        worklogs: worklogs.clone(),
    };
    let server = WorkloggerMcpServer::new(configuration_with_capabilities(capabilities))
        .with_own_hours(Arc::new(FakeBackend))
        .with_jira_worklogs(Arc::new(worklog_backend));
    let (server_task, client) = start_mcp(server).await;
    let tool = listed_tools(&client)
        .await
        .into_iter()
        .find(|tool| tool.name == "jira_create_worklog")
        .expect("worklog tool");
    let properties = tool.input_schema["properties"]
        .as_object()
        .expect("input properties");
    assert!(!properties.contains_key("author"));
    assert!(!properties.contains_key("accountId"));
    let token = preview_worklog(&client).await;
    assert_eq!(worklogs.load(Ordering::Relaxed), 0);
    confirm_worklog(&client, &token).await;
    assert_eq!(worklogs.load(Ordering::Relaxed), 1);
    stop_mcp(client, server_task).await;
}

#[tokio::test]
#[cfg(feature = "bitbucket")]
async fn bitbucket_tools_follow_capabilities_and_separate_destructive_actions() {
    let merges = Arc::new(AtomicUsize::new(0));
    let backend = bitbucket_backend(merges.clone(), false);
    let server = WorkloggerMcpServer::new(bitbucket_configuration(bitbucket_capabilities()))
        .with_own_hours(Arc::new(FakeBackend))
        .with_bitbucket(Arc::new(backend));
    let (server_task, client) = start_mcp(server).await;
    assert_bitbucket_tool_catalog(&client).await;
    let token = preview_merge(&client).await;
    let response = call_merge(&client, "bitbucket_merge_pull_request", true, Some(&token)).await;
    assert_eq!(
        response.structured_content.expect("output")["success"],
        true
    );
    assert_eq!(merges.load(Ordering::Relaxed), 1);
    stop_mcp(client, server_task).await;
}

#[tokio::test]
#[cfg(feature = "bitbucket")]
async fn bitbucket_merge_confirmation_expires_after_a_new_source_commit() {
    let capabilities = BTreeSet::from([
        Capability::ReadBitbucketPullRequests,
        Capability::MergeBitbucketPullRequests,
    ]);
    let merges = Arc::new(AtomicUsize::new(0));
    let backend = bitbucket_backend(merges.clone(), true);
    let server = WorkloggerMcpServer::new(bitbucket_configuration(capabilities))
        .with_own_hours(Arc::new(FakeBackend))
        .with_bitbucket(Arc::new(backend));
    let (server_task, client) = start_mcp(server).await;
    let merge = "bitbucket_merge_pull_request";
    let token = preview_merge(&client).await;
    let response = call_merge(&client, merge, true, Some(&token)).await;
    let content = response.structured_content.expect("rejection output");
    assert_eq!(content["error"]["code"], "invalid_confirmation");
    assert_eq!(merges.load(Ordering::Relaxed), 0);
    stop_mcp(client, server_task).await;
}

type TestMcpClient = rmcp::service::RunningService<rmcp::RoleClient, ()>;

async fn start_mcp(server: WorkloggerMcpServer) -> (JoinHandle<()>, TestMcpClient) {
    let (server_transport, client_transport) = tokio::io::duplex(16_384);
    let server_task = tokio::spawn(async move {
        let service = server.serve(server_transport).await.expect("server starts");
        service.waiting().await.expect("server finishes cleanly");
    });
    let client = ().serve(client_transport).await.expect("handshake succeeds");
    (server_task, client)
}

async fn listed_tools(client: &TestMcpClient) -> Vec<rmcp::model::Tool> {
    client
        .list_all_tools()
        .await
        .expect("tool listing succeeds")
}

async fn assert_successful_call(client: &TestMcpClient, request: CallToolRequestParams) {
    let result = client.call_tool(request).await.expect("tool call succeeds");
    assert_eq!(
        result.structured_content.expect("structured output")["success"],
        true
    );
}

async fn assert_unlogged_issue_call(client: &TestMcpClient, tool_name: String) {
    let result = client
        .call_tool(CallToolRequestParams::new(tool_name))
        .await
        .expect("unlogged issue call succeeds");
    let content = result.structured_content.expect("structured output");
    assert_eq!(content["success"], true);
    assert_eq!(content["data"]["boardId"], 42);
    assert_eq!(content["data"]["issues"][0]["issueKey"], "DEMO-99");
}

async fn stop_mcp(client: TestMcpClient, server_task: JoinHandle<()>) {
    client.cancel().await.expect("client stops");
    server_task.await.expect("server task joins");
}

fn jira_issue_capabilities() -> BTreeSet<Capability> {
    BTreeSet::from([
        Capability::ReadJiraIssues,
        Capability::EditJiraIssues,
        Capability::CommentJiraIssues,
        Capability::TransitionJiraIssues,
    ])
}

async fn assert_jira_tool_catalog(client: &TestMcpClient) {
    let tools = listed_tools(client).await;
    let names = tools
        .iter()
        .map(|tool| tool.name.as_ref())
        .collect::<BTreeSet<_>>();
    let expected = BTreeSet::from([
        "jira_add_comment",
        "jira_get_edit_metadata",
        "jira_get_issue",
        "jira_get_transitions",
        "jira_search_issues",
        "jira_transition_issue",
        "jira_update_issue",
    ]);
    assert_eq!(names, expected);
}

async fn preview_jira_update(client: &TestMcpClient) -> String {
    let arguments = json!({
        "key": "DEMO-1", "fields": {"summary": "Updated"}, "confirmed": false
    });
    let request = CallToolRequestParams::new("jira_update_issue")
        .with_arguments(arguments.as_object().expect("object").clone());
    let result = client.call_tool(request).await.expect("preview returns");
    let content = result.structured_content.expect("structured output");
    assert_eq!(content["error"]["code"], "confirmation_required");
    assert_eq!(
        content["confirmation"]["preview"]["target"]["key"],
        "DEMO-1"
    );
    assert!(
        content["confirmation"]["visiblePreview"]
            .as_str()
            .is_some_and(|preview| preview.contains("\"key\": \"DEMO-1\""))
    );
    content["confirmation"]["token"]
        .as_str()
        .expect("token")
        .to_owned()
}

async fn confirm_jira_update(client: &TestMcpClient, token: &str) {
    let arguments = json!({
        "key": "DEMO-1", "fields": {"summary": "Updated"},
        "confirmed": true, "confirmationToken": token
    });
    let request = CallToolRequestParams::new("jira_update_issue")
        .with_arguments(arguments.as_object().expect("object").clone());
    assert_successful_call(client, request).await;
}

async fn preview_worklog(client: &TestMcpClient) -> String {
    let result = call_worklog(client, false, None).await;
    let content = result.structured_content.expect("preview output");
    assert_eq!(content["error"]["code"], "confirmation_required");
    assert_eq!(
        content["confirmation"]["preview"]["actor"]["displayName"],
        "Taylor Example"
    );
    assert!(
        content["confirmation"]["visiblePreview"]
            .as_str()
            .is_some_and(|preview| preview.contains("\"durationMinutes\": 120"))
    );
    assert_eq!(content["confirmation"]["preview"]["boardId"], 42);
    content["confirmation"]["token"]
        .as_str()
        .expect("token")
        .to_owned()
}

async fn confirm_worklog(client: &TestMcpClient, token: &str) {
    let result = call_worklog(client, true, Some(token)).await;
    let content = result.structured_content.expect("mutation output");
    assert_eq!(content["success"], true);
    assert_eq!(content["data"]["boardId"], 42);
    assert_eq!(content["data"]["effect"]["worklogId"], "9001");
}

async fn call_worklog(
    client: &TestMcpClient,
    confirmed: bool,
    confirmation_token: Option<&str>,
) -> rmcp::model::CallToolResult {
    let arguments = json!({
        "key": "DEMO-1",
        "startedAt": "2026-09-04T09:00:00-03:00",
        "durationMinutes": 120,
        "comment": "Implementation",
        "confirmed": confirmed,
        "confirmationToken": confirmation_token
    });
    client
        .call_tool(
            CallToolRequestParams::new("jira_create_worklog")
                .with_arguments(arguments.as_object().expect("object").clone()),
        )
        .await
        .expect("worklog tool returns")
}

#[cfg(feature = "bitbucket")]
fn bitbucket_capabilities() -> BTreeSet<Capability> {
    BTreeSet::from([
        Capability::ReadBitbucketPullRequests,
        Capability::CreateBitbucketPullRequests,
        Capability::EditBitbucketPullRequests,
        Capability::CommentBitbucketPullRequests,
        Capability::ReviewBitbucketPullRequests,
        Capability::MergeBitbucketPullRequests,
        Capability::DeclineBitbucketPullRequests,
    ])
}

#[cfg(feature = "bitbucket")]
fn bitbucket_backend(merges: Arc<AtomicUsize>, stale: bool) -> PreviewBitbucketBackend {
    PreviewBitbucketBackend {
        merges,
        previews: Arc::new(AtomicUsize::new(0)),
        change_source_after_preview: stale,
    }
}

#[cfg(feature = "bitbucket")]
async fn assert_bitbucket_tool_catalog(client: &TestMcpClient) {
    let tools = listed_tools(client).await;
    assert_eq!(tools.len(), 13);
    let merge = tools
        .iter()
        .find(|tool| tool.name == "bitbucket_merge_pull_request")
        .expect("merge tool");
    let destructive = merge
        .annotations
        .as_ref()
        .and_then(|value| value.destructive_hint);
    assert_eq!(destructive, Some(true));
}

#[cfg(feature = "bitbucket")]
async fn preview_merge(client: &TestMcpClient) -> String {
    let preview = call_merge(client, "bitbucket_merge_pull_request", false, None).await;
    let content = preview.structured_content.expect("preview output");
    assert_eq!(content["error"]["code"], "confirmation_required");
    assert_eq!(
        content["confirmation"]["preview"]["target"]["pullRequestId"],
        7
    );
    assert!(
        content["confirmation"]["visiblePreview"]
            .as_str()
            .is_some_and(|preview| preview.contains("\"pullRequestId\": 7"))
    );
    content["confirmation"]["token"]
        .as_str()
        .expect("token")
        .to_owned()
}

#[cfg(feature = "bitbucket")]
async fn call_merge(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    tool_name: &str,
    confirmed: bool,
    confirmation_token: Option<&str>,
) -> rmcp::model::CallToolResult {
    let arguments = json!({
        "workspace": "workspace", "repository": "repository", "pullRequestId": 7,
        "mergeStrategy": "squash", "confirmed": confirmed,
        "confirmationToken": confirmation_token
    });
    client
        .call_tool(
            CallToolRequestParams::new(tool_name.to_owned())
                .with_arguments(arguments.as_object().expect("object").clone()),
        )
        .await
        .expect("merge tool returns")
}

fn request(date_from: Option<&str>, date_to: Option<&str>) -> OwnHoursRequest {
    OwnHoursRequest {
        date_from: date_from.map(str::to_owned),
        date_to: date_to.map(str::to_owned),
    }
}

fn configuration(enabled: bool) -> McpConfiguration {
    let capabilities = BTreeSet::from([Capability::ReadOwnTimeEntries]);
    let module = ModuleConfiguration {
        enabled,
        capabilities,
    };
    McpConfiguration::new(
        jira_configuration(),
        BTreeMap::from([(ModuleId::Jira, module)]),
    )
    .expect("configuration is valid")
}

fn configuration_with_capabilities(capabilities: BTreeSet<Capability>) -> McpConfiguration {
    let module = ModuleConfiguration {
        enabled: true,
        capabilities,
    };
    McpConfiguration::new(
        jira_configuration(),
        BTreeMap::from([(ModuleId::Jira, module)]),
    )
    .expect("configuration is valid")
}

#[cfg(feature = "bitbucket")]
fn bitbucket_configuration(capabilities: BTreeSet<Capability>) -> McpConfiguration {
    let modules = BTreeMap::from([
        (ModuleId::Jira, disabled_module()),
        (ModuleId::Bitbucket, enabled_module(capabilities)),
    ]);
    McpConfiguration::new_with_bitbucket(jira_configuration(), bitbucket_provider(), modules)
        .expect("configuration is valid")
}

#[cfg(feature = "bitbucket")]
fn bitbucket_provider() -> BitbucketConfiguration {
    BitbucketConfiguration {
        email: "person@example.com".to_owned(),
        workspaces: BTreeMap::from([(
            "workspace".to_owned(),
            BTreeSet::from(["repository".to_owned()]),
        )]),
        request_timeout_seconds: 30,
        page_size: 50,
        maximum_collection_items: 1_000,
        pull_request_defaults: worklogger_mcp::BitbucketPullRequestDefaults::default(),
    }
}

#[cfg(feature = "bitbucket")]
fn enabled_module(capabilities: BTreeSet<Capability>) -> ModuleConfiguration {
    ModuleConfiguration {
        enabled: true,
        capabilities,
    }
}

#[cfg(feature = "bitbucket")]
fn disabled_module() -> ModuleConfiguration {
    ModuleConfiguration {
        enabled: false,
        capabilities: BTreeSet::new(),
    }
}

fn jira_configuration() -> JiraConfiguration {
    JiraConfiguration {
        base_url: "https://example.atlassian.net".to_owned(),
        email: "person@example.com".to_owned(),
        board_id: 42,
        request_timeout_seconds: 30,
        page_size: 100,
        maximum_collection_items: 2_000,
        maximum_issue_search_results: 1_000,
        hours: Some(JiraHoursConfiguration {
            weekly_target_hours: 40,
            utc_offset_minutes: 0,
            maximum_daily_hours: 24,
            maximum_report_period_days: 7,
            maximum_concurrent_worklog_requests: 8,
        }),
    }
}

fn report() -> WeeklyReport {
    let period = DateRange::new(date!(2026 - 08 - 31), date!(2026 - 09 - 03)).expect("valid dates");
    let account = AccountId::new("account-1").expect("valid account");
    let worklogs = vec![worklog(account.clone())];
    let target = WeeklyTarget::from_minutes(40 * 60).expect("valid target");
    WeeklyReport {
        identity: identity(),
        summary: WeeklySummary::calculate(&account, period, target, &worklogs),
        worklogs,
        warnings: Vec::new(),
    }
}

fn identity() -> JiraUserDto {
    JiraUserDto {
        account_id: "account-1".to_owned(),
        display_name: "Taylor Example".to_owned(),
        active: true,
        account_type: None,
        email_address: None,
        time_zone: None,
    }
}

fn worklog(author: AccountId) -> Worklog {
    Worklog {
        id: "worklog-1".to_owned(),
        issue_key: IssueKey::new("DEMO-42").expect("valid issue key"),
        issue_summary: "Build a useful feature".to_owned(),
        author,
        started: datetime!(2026-09-02 10:00 UTC),
        duration: Duration::from_minutes(120).expect("valid duration"),
        comment: "Implementation".to_owned(),
        issue_url: "https://example.atlassian.net/browse/DEMO-42".to_owned(),
    }
}

struct FakeBackend;

impl OwnHoursBackend for FakeBackend {
    fn load_own_hours(&self, _period: DateRange) -> OwnHoursFuture<'_> {
        Box::pin(async { Err(OwnHoursBackendError::Provider { retryable: true }) })
    }

    fn load_unlogged_issues(&self, _period: DateRange) -> UnloggedIssuesFuture<'_> {
        Box::pin(async { Err(OwnHoursBackendError::Provider { retryable: true }) })
    }
}

struct SuccessBackend;

impl OwnHoursBackend for SuccessBackend {
    fn load_own_hours(&self, _period: DateRange) -> OwnHoursFuture<'_> {
        Box::pin(async { Ok(report()) })
    }

    fn load_unlogged_issues(&self, _period: DateRange) -> UnloggedIssuesFuture<'_> {
        Box::pin(async {
            Ok(vec![UnloggedIssue {
                issue_key: "DEMO-99".to_owned(),
                summary: "Issue without own hours".to_owned(),
                issue_url: "https://example.atlassian.net/browse/DEMO-99".to_owned(),
            }])
        })
    }
}
