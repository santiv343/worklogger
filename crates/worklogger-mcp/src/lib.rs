//! Standalone MCP adapter for Worklogger.

#[cfg(feature = "bitbucket")]
mod bitbucket_pull_requests;
mod client_registration;
mod configuration;
mod confirmation;
#[cfg(feature = "jira")]
mod jira_connections;
#[cfg(feature = "jira")]
mod jira_hours;
mod jira_issues;
#[cfg(feature = "jira")]
mod jira_worklogs;
mod runtime_installation;
mod server;
mod shared_configuration;
mod tool_failure;

#[cfg(feature = "bitbucket")]
pub use bitbucket_adapter::{BITBUCKET_CLOUD_API_ORIGIN, MergeStrategy, PullRequestState};
#[cfg(feature = "bitbucket")]
pub use bitbucket_pull_requests::{
    BitbucketActivityData, BitbucketActivityEvent, BitbucketActivityKind, BitbucketActorData,
    BitbucketBackendError, BitbucketCommentRequest, BitbucketConfirmedPullRequestRequest,
    BitbucketCreatePullRequestRequest, BitbucketFuture, BitbucketListPullRequestsRequest,
    BitbucketMergePullRequestRequest, BitbucketMutationData, BitbucketMutationEffect,
    BitbucketMutationPlan, BitbucketMutationRequest, BitbucketParticipantData,
    BitbucketPlannedEffect, BitbucketPlannedTarget, BitbucketPullRequestBackend,
    BitbucketPullRequestData, BitbucketPullRequestKeyRequest, BitbucketPullRequestListData,
    BitbucketPullRequestRevision, BitbucketPullRequestService, BitbucketPullRequestTarget,
    BitbucketRepositoryData, BitbucketRepositoryListData, BitbucketReviewAction,
    BitbucketToolResponse, BitbucketUpdatePullRequestRequest, BitbucketWorkspaceRequest,
    bitbucket_backend_failure, bitbucket_confirmation_failure,
};
pub use client_registration::{
    ClientRegistrationError, ClientRegistrationService, MCP_SERVER_REGISTRATION_NAME,
    MCP_SERVER_SERVE_ARGUMENT, MCP_SERVERS_PROPERTY, McpClientId, McpClientStatus,
    RegistrationState,
};
pub use configuration::{
    BitbucketConfiguration, BitbucketPullRequestDefaults, Capability, ConfigurationError,
    ConfigurationStore, DEFAULT_MAXIMUM_ISSUE_SEARCH_RESULTS, DEFAULT_MAXIMUM_REPORT_PERIOD_DAYS,
    JiraConfiguration, JiraHoursConfiguration, MAXIMUM_REPORT_PERIOD_DAYS, McpConfiguration,
    ModuleConfiguration, ModuleId,
};
pub use confirmation::{
    ConfirmationError, ConfirmationGate, MutationConfirmation, confirmation_payload,
    confirmation_payload_with_context,
};
#[cfg(feature = "jira")]
pub use jira_connections::RoutedJiraIssueBackend;
#[cfg(feature = "jira")]
pub use jira_hours::{
    JiraOwnHoursBackend, OwnHoursBackend, OwnHoursBackendError, OwnHoursFuture, OwnHoursReportData,
    OwnHoursRequest, OwnHoursToolResponse, UnloggedIssue, UnloggedIssuesData, UnloggedIssuesFuture,
    UnloggedIssuesToolResponse, resolve_period,
};
pub use jira_issues::{
    JiraActorData, JiraAddCommentRequest, JiraEditMetadataData, JiraGetIssueRequest,
    JiraIssueBackend, JiraIssueBackendError, JiraIssueData, JiraIssueFuture, JiraIssueKeyRequest,
    JiraIssueSearchData, JiraIssueService, JiraMutationData, JiraMutationEffect, JiraMutationPlan,
    JiraMutationRequest, JiraMutationTarget, JiraPlannedEffect, JiraSearchIssuesRequest,
    JiraToolResponse, JiraTransitionData, JiraTransitionIssueRequest, JiraTransitionsData,
    JiraUpdateIssueRequest, confirmation_failure, jira_backend_failure,
};
#[cfg(feature = "jira")]
pub use jira_worklogs::{
    JiraCreateWorklogRequest, JiraPlannedWorklogEffect, JiraWorklogBackend, JiraWorklogData,
    JiraWorklogEffect, JiraWorklogFuture, JiraWorklogPlan, JiraWorklogService,
    JiraWorklogToolResponse,
};
pub use runtime_installation::{McpServerInstallation, RuntimeInstallationError};
pub use server::WorkloggerMcpServer;
pub use tool_failure::ToolFailure;

#[cfg(feature = "jira")]
pub const JIRA_API_TOKEN_ENVIRONMENT_VARIABLE: &str = "WORKLOGGER_JIRA_API_TOKEN";
#[cfg(feature = "bitbucket")]
pub const BITBUCKET_API_TOKEN_ENVIRONMENT_VARIABLE: &str = "WORKLOGGER_BITBUCKET_API_TOKEN";
