#[cfg(any(feature = "jira", feature = "bitbucket"))]
use std::sync::Arc;

#[cfg(any(feature = "jira", feature = "bitbucket"))]
use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use rmcp::{
    ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{Implementation, ServerCapabilities, ServerInfo},
    tool_handler,
};
#[cfg(any(feature = "jira", feature = "bitbucket"))]
use serde::Serialize;
#[cfg(feature = "jira")]
use time::OffsetDateTime;

#[cfg(feature = "bitbucket")]
use crate::bitbucket_pull_requests::{
    RevisionBoundRequest, bind_expected_revision, bind_resolved_reviewers,
};
#[cfg(feature = "jira")]
use crate::jira_issues::bind_expected_fields;
#[cfg(any(feature = "jira", feature = "bitbucket"))]
use crate::tool_failure::ERROR_INVALID_CONFIRMATION;
#[cfg(feature = "jira")]
use crate::tool_failure::{
    ERROR_AUTHENTICATION_REQUIRED, ERROR_FORBIDDEN, ERROR_INVALID_INPUT,
    ERROR_INVALID_PROVIDER_RESPONSE, ERROR_NOT_FOUND, ERROR_PROVIDER_UNAVAILABLE,
};
#[cfg(feature = "bitbucket")]
use crate::{
    BitbucketActivityData, BitbucketCommentRequest, BitbucketConfirmedPullRequestRequest,
    BitbucketCreatePullRequestRequest, BitbucketListPullRequestsRequest,
    BitbucketMergePullRequestRequest, BitbucketMutationData, BitbucketMutationPlan,
    BitbucketMutationRequest, BitbucketPullRequestBackend, BitbucketPullRequestData,
    BitbucketPullRequestKeyRequest, BitbucketPullRequestListData, BitbucketRepositoryListData,
    BitbucketReviewAction, BitbucketToolResponse, BitbucketUpdatePullRequestRequest,
    BitbucketWorkspaceRequest, bitbucket_backend_failure, bitbucket_confirmation_failure,
};
use crate::{Capability, McpConfiguration};
#[cfg(any(feature = "jira", feature = "bitbucket"))]
use crate::{
    ConfirmationError, ConfirmationGate, MutationConfirmation, ToolFailure,
    confirmation_payload_with_context,
};
#[cfg(feature = "jira")]
use crate::{
    JiraAddCommentRequest, JiraCreateWorklogRequest, JiraEditMetadataData, JiraGetIssueRequest,
    JiraIssueBackend, JiraIssueBackendError, JiraIssueData, JiraIssueKeyRequest,
    JiraIssueSearchData, JiraMutationData, JiraMutationPlan, JiraMutationRequest,
    JiraSearchIssuesRequest, JiraToolResponse, JiraTransitionIssueRequest, JiraTransitionsData,
    JiraUpdateIssueRequest, JiraWorklogBackend, JiraWorklogData, JiraWorklogPlan,
    JiraWorklogToolResponse, OwnHoursBackend, OwnHoursRequest, OwnHoursToolResponse,
    UnloggedIssuesToolResponse, confirmation_failure, jira_backend_failure, resolve_period,
};

const SERVER_NAME: &str = "worklogger-mcp";
const SERVER_TITLE: &str = "Worklogger MCP";
const SERVER_INSTRUCTIONS: &str = include_str!("../resources/server-instructions.es-AR.txt");
#[cfg(feature = "jira")]
const JIRA_SERVER_INSTRUCTIONS: &str =
    include_str!("../resources/server-instructions-jira.es-AR.txt");
#[cfg(feature = "bitbucket")]
const BITBUCKET_SERVER_INSTRUCTIONS: &str =
    include_str!("../resources/server-instructions-bitbucket.es-AR.txt");

#[derive(Clone)]
pub struct WorkloggerMcpServer {
    #[cfg(feature = "jira")]
    configuration: McpConfiguration,
    #[cfg(feature = "jira")]
    own_hours: Arc<dyn OwnHoursBackend>,
    #[cfg(feature = "jira")]
    jira_issues: Arc<dyn JiraIssueBackend>,
    #[cfg(feature = "jira")]
    jira_worklogs: Arc<dyn JiraWorklogBackend>,
    #[cfg(feature = "bitbucket")]
    bitbucket: Arc<dyn BitbucketPullRequestBackend>,
    #[cfg(any(feature = "jira", feature = "bitbucket"))]
    confirmations: Arc<ConfirmationGate>,
    tool_router: ToolRouter<Self>,
}

impl std::fmt::Debug for WorkloggerMcpServer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorkloggerMcpServer")
            .field("enabled_tools", &self.enabled_tool_names())
            .finish_non_exhaustive()
    }
}

impl WorkloggerMcpServer {
    #[must_use]
    pub fn new(configuration: McpConfiguration) -> Self {
        let tool_router = Self::configured_tool_router(&configuration);
        #[cfg(not(feature = "jira"))]
        drop(configuration);
        Self {
            #[cfg(feature = "jira")]
            configuration,
            #[cfg(feature = "jira")]
            own_hours: Arc::new(UnavailableOwnHoursBackend),
            #[cfg(feature = "jira")]
            jira_issues: Arc::new(UnavailableJiraIssueBackend),
            #[cfg(feature = "jira")]
            jira_worklogs: Arc::new(UnavailableJiraWorklogBackend),
            #[cfg(feature = "bitbucket")]
            bitbucket: Arc::new(UnavailableBitbucketBackend),
            #[cfg(any(feature = "jira", feature = "bitbucket"))]
            confirmations: Arc::new(ConfirmationGate::default()),
            tool_router,
        }
    }

    #[cfg(feature = "jira")]
    #[must_use]
    pub fn with_own_hours(mut self, own_hours: Arc<dyn OwnHoursBackend>) -> Self {
        self.own_hours = own_hours;
        self
    }

    #[cfg(feature = "jira")]
    #[must_use]
    pub fn with_jira_issues(mut self, jira_issues: Arc<dyn JiraIssueBackend>) -> Self {
        self.jira_issues = jira_issues;
        self
    }

    #[cfg(feature = "jira")]
    #[must_use]
    pub fn with_jira_worklogs(mut self, jira_worklogs: Arc<dyn JiraWorklogBackend>) -> Self {
        self.jira_worklogs = jira_worklogs;
        self
    }

    #[cfg(feature = "bitbucket")]
    #[must_use]
    pub fn with_bitbucket(mut self, bitbucket: Arc<dyn BitbucketPullRequestBackend>) -> Self {
        self.bitbucket = bitbucket;
        self
    }

    #[must_use]
    pub fn enabled_tool_names(&self) -> Vec<String> {
        tool_names(&self.tool_router)
    }

    #[must_use]
    pub fn configured_tool_names(configuration: &McpConfiguration) -> Vec<String> {
        tool_names(&Self::configured_tool_router(configuration))
    }

    #[must_use]
    #[cfg(feature = "jira")]
    pub fn own_hours_tool_name() -> String {
        Self::jira_get_my_hours_tool_attr().name.into_owned()
    }

    #[must_use]
    #[cfg(feature = "jira")]
    pub fn unlogged_issues_tool_name() -> String {
        Self::jira_get_my_unlogged_issues_tool_attr()
            .name
            .into_owned()
    }

    fn configured_tool_router(configuration: &McpConfiguration) -> ToolRouter<Self> {
        let mut router = ToolRouter::new();
        #[cfg(feature = "jira")]
        router.merge(Self::jira_tool_router());
        #[cfg(feature = "bitbucket")]
        router.merge(Self::bitbucket_tool_router());
        let enabled = Self::tool_capabilities()
            .into_iter()
            .filter(|(capability, _)| configuration.capability_enabled(*capability))
            .map(|(_, name)| name)
            .collect::<std::collections::BTreeSet<_>>();
        for tool_name in tool_names(&router) {
            if !enabled.contains(&tool_name) {
                router.disable_route(tool_name);
            }
        }
        router
    }

    fn tool_capabilities() -> Vec<(Capability, String)> {
        let tools = Vec::new();
        #[cfg(feature = "jira")]
        let tools = append_capabilities(tools, Self::jira_tool_capabilities());
        #[cfg(feature = "bitbucket")]
        let tools = append_capabilities(tools, Self::bitbucket_tool_capabilities());
        tools
    }

    #[cfg(feature = "jira")]
    fn jira_tool_capabilities() -> Vec<(Capability, String)> {
        let tools = append_capabilities(Vec::new(), Self::jira_primary_read_tools());
        let tools = append_capabilities(tools, Self::jira_metadata_read_tools());
        append_capabilities(tools, Self::jira_mutation_tools())
    }

    #[cfg(feature = "jira")]
    fn jira_primary_read_tools() -> [(Capability, String); 4] {
        [
            (Capability::ReadOwnTimeEntries, Self::own_hours_tool_name()),
            (
                Capability::ReadOwnTimeEntries,
                Self::unlogged_issues_tool_name(),
            ),
            (
                Capability::ReadJiraIssues,
                Self::jira_get_issue_tool_attr().name.into_owned(),
            ),
            (
                Capability::ReadJiraIssues,
                Self::jira_search_issues_tool_attr().name.into_owned(),
            ),
        ]
    }

    #[cfg(feature = "jira")]
    fn jira_metadata_read_tools() -> [(Capability, String); 2] {
        [
            (
                Capability::ReadJiraIssues,
                Self::jira_get_edit_metadata_tool_attr().name.into_owned(),
            ),
            (
                Capability::ReadJiraIssues,
                Self::jira_get_transitions_tool_attr().name.into_owned(),
            ),
        ]
    }

    #[cfg(feature = "jira")]
    fn jira_mutation_tools() -> [(Capability, String); 4] {
        [
            (
                Capability::EditJiraIssues,
                Self::jira_update_issue_tool_attr().name.into_owned(),
            ),
            (
                Capability::CommentJiraIssues,
                Self::jira_add_comment_tool_attr().name.into_owned(),
            ),
            (
                Capability::TransitionJiraIssues,
                Self::jira_transition_issue_tool_attr().name.into_owned(),
            ),
            (
                Capability::WriteOwnTimeEntries,
                Self::jira_create_worklog_tool_attr().name.into_owned(),
            ),
        ]
    }

    #[cfg(feature = "bitbucket")]
    fn bitbucket_tool_capabilities() -> Vec<(Capability, String)> {
        let tools = append_capabilities(Vec::new(), Self::bitbucket_repository_tools());
        let tools = append_capabilities(tools, Self::bitbucket_pull_request_read_tools());
        let tools = append_capabilities(tools, Self::bitbucket_create_tool());
        let tools = append_capabilities(tools, Self::bitbucket_content_mutation_tools());
        let tools = append_capabilities(tools, Self::bitbucket_review_tools());
        let tools = append_capabilities(tools, Self::bitbucket_change_request_tools());
        append_capabilities(tools, Self::bitbucket_completion_tools())
    }

    #[cfg(feature = "bitbucket")]
    fn bitbucket_repository_tools() -> [(Capability, String); 2] {
        [
            (
                Capability::ReadBitbucketPullRequests,
                Self::bitbucket_list_repositories_tool_attr()
                    .name
                    .into_owned(),
            ),
            (
                Capability::ReadBitbucketPullRequests,
                Self::bitbucket_list_pull_requests_tool_attr()
                    .name
                    .into_owned(),
            ),
        ]
    }

    #[cfg(feature = "bitbucket")]
    fn bitbucket_pull_request_read_tools() -> [(Capability, String); 2] {
        [
            (
                Capability::ReadBitbucketPullRequests,
                Self::bitbucket_get_pull_request_tool_attr()
                    .name
                    .into_owned(),
            ),
            (
                Capability::ReadBitbucketPullRequests,
                Self::bitbucket_get_pull_request_activity_tool_attr()
                    .name
                    .into_owned(),
            ),
        ]
    }

    #[cfg(feature = "bitbucket")]
    fn bitbucket_create_tool() -> [(Capability, String); 1] {
        [(
            Capability::CreateBitbucketPullRequests,
            Self::bitbucket_create_pull_request_tool_attr()
                .name
                .into_owned(),
        )]
    }

    #[cfg(feature = "bitbucket")]
    fn bitbucket_content_mutation_tools() -> [(Capability, String); 2] {
        [
            (
                Capability::EditBitbucketPullRequests,
                Self::bitbucket_update_pull_request_tool_attr()
                    .name
                    .into_owned(),
            ),
            (
                Capability::CommentBitbucketPullRequests,
                Self::bitbucket_add_pull_request_comment_tool_attr()
                    .name
                    .into_owned(),
            ),
        ]
    }

    #[cfg(feature = "bitbucket")]
    fn bitbucket_review_tools() -> [(Capability, String); 2] {
        [
            (
                Capability::ReviewBitbucketPullRequests,
                Self::bitbucket_approve_pull_request_tool_attr()
                    .name
                    .into_owned(),
            ),
            (
                Capability::ReviewBitbucketPullRequests,
                Self::bitbucket_unapprove_pull_request_tool_attr()
                    .name
                    .into_owned(),
            ),
        ]
    }

    #[cfg(feature = "bitbucket")]
    fn bitbucket_change_request_tools() -> [(Capability, String); 2] {
        [
            (
                Capability::ReviewBitbucketPullRequests,
                Self::bitbucket_request_changes_tool_attr()
                    .name
                    .into_owned(),
            ),
            (
                Capability::ReviewBitbucketPullRequests,
                Self::bitbucket_remove_change_request_tool_attr()
                    .name
                    .into_owned(),
            ),
        ]
    }

    #[cfg(feature = "bitbucket")]
    fn bitbucket_completion_tools() -> [(Capability, String); 2] {
        [
            (
                Capability::MergeBitbucketPullRequests,
                Self::bitbucket_merge_pull_request_tool_attr()
                    .name
                    .into_owned(),
            ),
            (
                Capability::DeclineBitbucketPullRequests,
                Self::bitbucket_decline_pull_request_tool_attr()
                    .name
                    .into_owned(),
            ),
        ]
    }
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn append_capabilities(
    mut target: Vec<(Capability, String)>,
    capabilities: impl IntoIterator<Item = (Capability, String)>,
) -> Vec<(Capability, String)> {
    target.extend(capabilities);
    target
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
enum ConfirmationDecision<Preview> {
    Execute(Preview),
    Preview(MutationConfirmation<Preview>),
    Reject(ToolFailure),
}

#[cfg(feature = "bitbucket")]
type BitbucketConfirmationDecision = Result<
    ConfirmationDecision<BitbucketMutationPlan>,
    Box<BitbucketToolResponse<BitbucketMutationData>>,
>;

#[cfg(feature = "jira")]
type JiraConfirmationDecision =
    Result<ConfirmationDecision<JiraMutationPlan>, Box<JiraToolResponse<JiraMutationData>>>;

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn confirmation_decision<Request: Serialize, Preview: Serialize>(
    gate: &ConfirmationGate,
    request: &Request,
    preview: Preview,
    confirmed: bool,
    token: Option<&str>,
    required: ToolFailure,
) -> ConfirmationDecision<Preview> {
    let payload = match confirmation_payload_with_context(request, &preview) {
        Ok(payload) => payload,
        Err(error) => return ConfirmationDecision::Reject(confirmation_error_failure(&error)),
    };
    decide_confirmation(gate, preview, payload, confirmed, token, required)
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn decide_confirmation<Preview: Serialize>(
    gate: &ConfirmationGate,
    preview: Preview,
    payload: Vec<u8>,
    confirmed: bool,
    token: Option<&str>,
    required: ToolFailure,
) -> ConfirmationDecision<Preview> {
    let Some(token) = token else {
        return prepare_confirmation(gate, preview, payload);
    };
    if !confirmed {
        return ConfirmationDecision::Reject(required);
    }
    match gate.consume(token, &payload) {
        Ok(()) => ConfirmationDecision::Execute(preview),
        Err(error) => ConfirmationDecision::Reject(confirmation_error_failure(&error)),
    }
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn prepare_confirmation<Preview>(
    gate: &ConfirmationGate,
    preview: Preview,
    payload: Vec<u8>,
) -> ConfirmationDecision<Preview>
where
    Preview: Serialize,
{
    match gate.prepare(payload) {
        Ok(token) => ConfirmationDecision::Preview(MutationConfirmation {
            token,
            visible_preview: visible_preview(&preview),
            preview,
        }),
        Err(error) => ConfirmationDecision::Reject(confirmation_error_failure(&error)),
    }
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn visible_preview<Preview: Serialize>(preview: &Preview) -> String {
    serde_json::to_string_pretty(preview).unwrap_or_default()
}

#[cfg(feature = "jira")]
#[tool_router(router = jira_tool_router)]
impl WorkloggerMcpServer {
    /// Obtiene las horas propias de la cuenta autenticada para un período de hasta 7 días.
    #[tool(annotations(
        title = "Consultar mis horas",
        read_only_hint = true,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn jira_get_my_hours(
        &self,
        Parameters(request): Parameters<OwnHoursRequest>,
    ) -> Json<OwnHoursToolResponse> {
        let generated_at = OffsetDateTime::now_utc();
        let response = match self.own_hours_query(&request, generated_at) {
            Ok((period, _)) => self.load_report(period, generated_at).await,
            Err(error) => OwnHoursToolResponse::failure(error),
        };
        Json(response)
    }

    /// Lista issues asignados a la cuenta autenticada sin horas propias en el período.
    #[tool(annotations(
        title = "Consultar mis tareas sin horas",
        read_only_hint = true,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn jira_get_my_unlogged_issues(
        &self,
        Parameters(request): Parameters<OwnHoursRequest>,
    ) -> Json<UnloggedIssuesToolResponse> {
        let generated_at = OffsetDateTime::now_utc();
        let response = match self.own_hours_query(&request, generated_at) {
            Ok((period, board_id)) => {
                self.load_unlogged_issues(period, board_id, generated_at)
                    .await
            }
            Err(error) => UnloggedIssuesToolResponse::failure(error),
        };
        Json(response)
    }

    /// Obtiene campos seleccionados de un issue visible para la cuenta autenticada.
    #[tool(annotations(
        title = "Consultar issue Jira",
        read_only_hint = true,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn jira_get_issue(
        &self,
        Parameters(request): Parameters<JiraGetIssueRequest>,
    ) -> Json<JiraToolResponse<JiraIssueData>> {
        Json(jira_response(self.jira_issues.get_issue(request).await))
    }

    /// Busca issues mediante JQL con paginación y límites configurados.
    #[tool(annotations(
        title = "Buscar issues Jira",
        read_only_hint = true,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn jira_search_issues(
        &self,
        Parameters(request): Parameters<JiraSearchIssuesRequest>,
    ) -> Json<JiraToolResponse<JiraIssueSearchData>> {
        Json(jira_response(self.jira_issues.search_issues(request).await))
    }

    /// Lista los campos que Jira permite editar actualmente en un issue.
    #[tool(annotations(
        title = "Consultar campos editables Jira",
        read_only_hint = true,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn jira_get_edit_metadata(
        &self,
        Parameters(request): Parameters<JiraIssueKeyRequest>,
    ) -> Json<JiraToolResponse<JiraEditMetadataData>> {
        Json(jira_response(
            self.jira_issues.get_edit_metadata(request).await,
        ))
    }

    /// Lista las transiciones disponibles actualmente para un issue.
    #[tool(annotations(
        title = "Consultar transiciones Jira",
        read_only_hint = true,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn jira_get_transitions(
        &self,
        Parameters(request): Parameters<JiraIssueKeyRequest>,
    ) -> Json<JiraToolResponse<JiraTransitionsData>> {
        Json(jira_response(
            self.jira_issues.get_transitions(request).await,
        ))
    }

    /// Modifica campos explícitos de un issue como la cuenta autenticada.
    #[tool(annotations(
        title = "Editar issue Jira",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn jira_update_issue(
        &self,
        Parameters(request): Parameters<JiraUpdateIssueRequest>,
    ) -> Json<JiraToolResponse<JiraMutationData>> {
        let mutation = JiraMutationRequest::Update(request.clone());
        let request = match self.confirmed_jira_update(request, mutation).await {
            Ok(request) => request,
            Err(response) => return Json(*response),
        };
        Json(jira_response(self.jira_issues.update_issue(request).await))
    }

    /// Agrega un comentario de texto como la cuenta autenticada.
    #[tool(annotations(
        title = "Comentar issue Jira",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = false,
        open_world_hint = true
    ))]
    async fn jira_add_comment(
        &self,
        Parameters(request): Parameters<JiraAddCommentRequest>,
    ) -> Json<JiraToolResponse<JiraMutationData>> {
        let mutation = JiraMutationRequest::Comment(request.clone());
        if let Some(response) = self
            .jira_confirmation_step(
                mutation,
                &request,
                request.confirmed,
                request.confirmation_token.as_deref(),
            )
            .await
        {
            return Json(response);
        }
        Json(jira_response(self.jira_issues.add_comment(request).await))
    }

    /// Aplica una transición disponible por ID como la cuenta autenticada.
    #[tool(annotations(
        title = "Cambiar estado de issue Jira",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = false,
        open_world_hint = true
    ))]
    async fn jira_transition_issue(
        &self,
        Parameters(request): Parameters<JiraTransitionIssueRequest>,
    ) -> Json<JiraToolResponse<JiraMutationData>> {
        let mutation = JiraMutationRequest::Transition(request.clone());
        if let Some(response) = self
            .jira_confirmation_step(
                mutation,
                &request,
                request.confirmed,
                request.confirmation_token.as_deref(),
            )
            .await
        {
            return Json(response);
        }
        Json(jira_response(
            self.jira_issues.transition_issue(request).await,
        ))
    }

    /// Crea una carga horaria propia en un issue del tablero configurado.
    #[tool(annotations(
        title = "Cargar mis horas en Jira",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = false,
        open_world_hint = true
    ))]
    async fn jira_create_worklog(
        &self,
        Parameters(request): Parameters<JiraCreateWorklogRequest>,
    ) -> Json<JiraWorklogToolResponse> {
        if let Some(response) = self.jira_worklog_confirmation_step(&request).await {
            return Json(response);
        }
        Json(jira_worklog_response(
            self.jira_worklogs.create_worklog(request).await,
        ))
    }

    async fn jira_worklog_confirmation_step(
        &self,
        request: &JiraCreateWorklogRequest,
    ) -> Option<JiraWorklogToolResponse> {
        match self.jira_worklog_confirmation_decision(request).await {
            Ok(decision) => jira_worklog_confirmation_result(decision),
            Err(error) => Some(JiraWorklogToolResponse::failure(jira_backend_failure(
                &error,
            ))),
        }
    }

    async fn jira_worklog_confirmation_decision(
        &self,
        request: &JiraCreateWorklogRequest,
    ) -> Result<ConfirmationDecision<JiraWorklogPlan>, JiraIssueBackendError> {
        let preview = self
            .jira_worklogs
            .preview_create_worklog(request.clone())
            .await?;
        Ok(confirmation_decision(
            &self.confirmations,
            request,
            preview,
            request.confirmed,
            request.confirmation_token.as_deref(),
            confirmation_failure(),
        ))
    }

    async fn load_report(
        &self,
        period: hours_core::DateRange,
        generated_at: OffsetDateTime,
    ) -> OwnHoursToolResponse {
        match self.own_hours.load_own_hours(period).await {
            Ok(report) => OwnHoursToolResponse::success(&report, generated_at),
            Err(error) => OwnHoursToolResponse::failure(backend_failure(&error)),
        }
    }

    async fn load_unlogged_issues(
        &self,
        period: hours_core::DateRange,
        board_id: u64,
        generated_at: OffsetDateTime,
    ) -> UnloggedIssuesToolResponse {
        match self.own_hours.load_unlogged_issues(period).await {
            Ok(issues) => {
                UnloggedIssuesToolResponse::success(issues, board_id, period, generated_at)
            }
            Err(error) => UnloggedIssuesToolResponse::failure(backend_failure(&error)),
        }
    }

    fn own_hours_query(
        &self,
        request: &OwnHoursRequest,
        generated_at: OffsetDateTime,
    ) -> Result<(hours_core::DateRange, u64), ToolFailure> {
        let jira = self.configuration.jira.as_ref().ok_or_else(hours_failure)?;
        let hours = jira.hours.as_ref().ok_or_else(hours_failure)?;
        resolve_period(request, hours.utc_offset_minutes, generated_at)
            .map(|period| (period, jira.board_id))
    }

    async fn jira_confirmation_step<Request: Serialize>(
        &self,
        mutation: JiraMutationRequest,
        request: &Request,
        confirmed: bool,
        token: Option<&str>,
    ) -> Option<JiraToolResponse<JiraMutationData>> {
        self.jira_confirmation_decision(mutation, request, confirmed, token)
            .await
            .and_then(jira_confirmation_result)
            .err()
            .map(|response| *response)
    }

    async fn jira_confirmation_decision<Request: Serialize>(
        &self,
        mutation: JiraMutationRequest,
        request: &Request,
        confirmed: bool,
        token: Option<&str>,
    ) -> JiraConfirmationDecision {
        let preview = self
            .jira_issues
            .preview_mutation(mutation)
            .await
            .map_err(|error| Box::new(JiraToolResponse::failure(jira_backend_failure(&error))))?;
        Ok(confirmation_decision(
            &self.confirmations,
            request,
            preview,
            confirmed,
            token,
            confirmation_failure(),
        ))
    }

    async fn confirmed_jira_update(
        &self,
        request: JiraUpdateIssueRequest,
        mutation: JiraMutationRequest,
    ) -> Result<JiraUpdateIssueRequest, Box<JiraToolResponse<JiraMutationData>>> {
        let decision = self
            .jira_confirmation_decision(
                mutation,
                &request,
                request.confirmed,
                request.confirmation_token.as_deref(),
            )
            .await?;
        let plan = jira_confirmation_result(decision)?;
        bind_expected_fields(request, &plan)
            .map_err(|error| Box::new(JiraToolResponse::failure(jira_backend_failure(&error))))
    }
}

#[cfg(feature = "bitbucket")]
#[tool_router(router = bitbucket_tool_router)]
impl WorkloggerMcpServer {
    /// Lista repositorios visibles dentro de un workspace configurado.
    #[cfg(feature = "bitbucket")]
    #[tool(annotations(
        title = "Listar repositorios Bitbucket",
        read_only_hint = true,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn bitbucket_list_repositories(
        &self,
        Parameters(request): Parameters<BitbucketWorkspaceRequest>,
    ) -> Json<BitbucketToolResponse<BitbucketRepositoryListData>> {
        Json(bitbucket_response(
            self.bitbucket.list_repositories(request).await,
        ))
    }

    /// Lista pull requests de un repositorio dentro del ámbito configurado.
    #[cfg(feature = "bitbucket")]
    #[tool(annotations(
        title = "Listar pull requests Bitbucket",
        read_only_hint = true,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn bitbucket_list_pull_requests(
        &self,
        Parameters(request): Parameters<BitbucketListPullRequestsRequest>,
    ) -> Json<BitbucketToolResponse<BitbucketPullRequestListData>> {
        Json(bitbucket_response(
            self.bitbucket.list_pull_requests(request).await,
        ))
    }

    /// Obtiene el detalle de un pull request visible.
    #[cfg(feature = "bitbucket")]
    #[tool(annotations(
        title = "Consultar pull request Bitbucket",
        read_only_hint = true,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn bitbucket_get_pull_request(
        &self,
        Parameters(request): Parameters<BitbucketPullRequestKeyRequest>,
    ) -> Json<BitbucketToolResponse<BitbucketPullRequestData>> {
        Json(bitbucket_response(
            self.bitbucket.get_pull_request(request).await,
        ))
    }

    /// Obtiene la actividad de un pull request visible.
    #[cfg(feature = "bitbucket")]
    #[tool(annotations(
        title = "Consultar actividad de pull request",
        read_only_hint = true,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn bitbucket_get_pull_request_activity(
        &self,
        Parameters(request): Parameters<BitbucketPullRequestKeyRequest>,
    ) -> Json<BitbucketToolResponse<BitbucketActivityData>> {
        Json(bitbucket_response(
            self.bitbucket.get_activity(request).await,
        ))
    }

    /// Crea un pull request con ramas y reviewers explícitos.
    #[cfg(feature = "bitbucket")]
    #[tool(annotations(
        title = "Crear pull request Bitbucket",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = false,
        open_world_hint = true
    ))]
    async fn bitbucket_create_pull_request(
        &self,
        Parameters(request): Parameters<BitbucketCreatePullRequestRequest>,
    ) -> Json<BitbucketToolResponse<BitbucketMutationData>> {
        let request = match self.confirmed_bitbucket_create(request).await {
            Ok(request) => request,
            Err(response) => return Json(*response),
        };
        Json(bitbucket_response(
            self.bitbucket.create_pull_request(request).await,
        ))
    }

    /// Edita los atributos explícitos de un pull request.
    #[cfg(feature = "bitbucket")]
    #[tool(annotations(
        title = "Editar pull request Bitbucket",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn bitbucket_update_pull_request(
        &self,
        Parameters(request): Parameters<BitbucketUpdatePullRequestRequest>,
    ) -> Json<BitbucketToolResponse<BitbucketMutationData>> {
        let mutation = BitbucketMutationRequest::Update(request.clone());
        let request = match self.confirmed_bitbucket_mutation(request, mutation).await {
            Ok(request) => request,
            Err(response) => return Json(*response),
        };
        Json(bitbucket_response(
            self.bitbucket.update_pull_request(request).await,
        ))
    }

    /// Agrega un comentario al pull request como la cuenta autenticada.
    #[cfg(feature = "bitbucket")]
    #[tool(annotations(
        title = "Comentar pull request Bitbucket",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = false,
        open_world_hint = true
    ))]
    async fn bitbucket_add_pull_request_comment(
        &self,
        Parameters(request): Parameters<BitbucketCommentRequest>,
    ) -> Json<BitbucketToolResponse<BitbucketMutationData>> {
        let mutation = BitbucketMutationRequest::Comment(request.clone());
        let request = match self.confirmed_bitbucket_mutation(request, mutation).await {
            Ok(request) => request,
            Err(response) => return Json(*response),
        };
        Json(bitbucket_response(
            self.bitbucket.add_comment(request).await,
        ))
    }

    /// Aprueba el pull request como la cuenta autenticada.
    #[cfg(feature = "bitbucket")]
    #[tool(annotations(
        title = "Aprobar pull request Bitbucket",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn bitbucket_approve_pull_request(
        &self,
        Parameters(request): Parameters<BitbucketConfirmedPullRequestRequest>,
    ) -> Json<BitbucketToolResponse<BitbucketMutationData>> {
        self.bitbucket_review_tool(request, BitbucketReviewAction::Approve)
            .await
    }

    /// Retira la aprobación propia del pull request.
    #[cfg(feature = "bitbucket")]
    #[tool(annotations(
        title = "Retirar aprobación Bitbucket",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn bitbucket_unapprove_pull_request(
        &self,
        Parameters(request): Parameters<BitbucketConfirmedPullRequestRequest>,
    ) -> Json<BitbucketToolResponse<BitbucketMutationData>> {
        self.bitbucket_review_tool(request, BitbucketReviewAction::Unapprove)
            .await
    }

    /// Solicita cambios en el pull request como la cuenta autenticada.
    #[cfg(feature = "bitbucket")]
    #[tool(annotations(
        title = "Solicitar cambios Bitbucket",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn bitbucket_request_changes(
        &self,
        Parameters(request): Parameters<BitbucketConfirmedPullRequestRequest>,
    ) -> Json<BitbucketToolResponse<BitbucketMutationData>> {
        self.bitbucket_review_tool(request, BitbucketReviewAction::RequestChanges)
            .await
    }

    /// Retira la solicitud propia de cambios del pull request.
    #[cfg(feature = "bitbucket")]
    #[tool(annotations(
        title = "Retirar solicitud de cambios Bitbucket",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn bitbucket_remove_change_request(
        &self,
        Parameters(request): Parameters<BitbucketConfirmedPullRequestRequest>,
    ) -> Json<BitbucketToolResponse<BitbucketMutationData>> {
        self.bitbucket_review_tool(request, BitbucketReviewAction::RemoveChangeRequest)
            .await
    }

    /// Fusiona el pull request con estrategia y destino explícitos.
    #[cfg(feature = "bitbucket")]
    #[tool(annotations(
        title = "Fusionar pull request Bitbucket",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = false,
        open_world_hint = true
    ))]
    async fn bitbucket_merge_pull_request(
        &self,
        Parameters(request): Parameters<BitbucketMergePullRequestRequest>,
    ) -> Json<BitbucketToolResponse<BitbucketMutationData>> {
        let mutation = BitbucketMutationRequest::Merge(request.clone());
        let request = match self.confirmed_bitbucket_mutation(request, mutation).await {
            Ok(request) => request,
            Err(response) => return Json(*response),
        };
        Json(bitbucket_response(self.bitbucket.merge(request).await))
    }

    /// Declina el pull request sin fusionarlo.
    #[cfg(feature = "bitbucket")]
    #[tool(annotations(
        title = "Declinar pull request Bitbucket",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = false,
        open_world_hint = true
    ))]
    async fn bitbucket_decline_pull_request(
        &self,
        Parameters(request): Parameters<BitbucketConfirmedPullRequestRequest>,
    ) -> Json<BitbucketToolResponse<BitbucketMutationData>> {
        let mutation = BitbucketMutationRequest::Decline(request.clone());
        let request = match self.confirmed_bitbucket_mutation(request, mutation).await {
            Ok(request) => request,
            Err(response) => return Json(*response),
        };
        Json(bitbucket_response(self.bitbucket.decline(request).await))
    }

    async fn bitbucket_confirmation_decision<Request: Serialize>(
        &self,
        mutation: BitbucketMutationRequest,
        request: &Request,
        confirmed: bool,
        token: Option<&str>,
    ) -> BitbucketConfirmationDecision {
        let preview = self
            .bitbucket
            .preview_mutation(mutation)
            .await
            .map_err(|error| Box::new(bitbucket_failure_response(&error)))?;
        Ok(confirmation_decision(
            &self.confirmations,
            request,
            preview,
            confirmed,
            token,
            bitbucket_confirmation_failure(),
        ))
    }

    async fn confirmed_bitbucket_mutation<Request>(
        &self,
        request: Request,
        mutation: BitbucketMutationRequest,
    ) -> Result<Request, Box<BitbucketToolResponse<BitbucketMutationData>>>
    where
        Request: Serialize + RevisionBoundRequest,
    {
        let confirmed = request.confirmed();
        let token = request.confirmation_token().map(str::to_owned);
        let decision = self
            .bitbucket_confirmation_decision(mutation, &request, confirmed, token.as_deref())
            .await?;
        let plan = bitbucket_confirmation_result(decision)?;
        bind_expected_revision(request, &plan)
            .map_err(|error| Box::new(bitbucket_failure_response(&error)))
    }

    async fn confirmed_bitbucket_create(
        &self,
        request: BitbucketCreatePullRequestRequest,
    ) -> Result<BitbucketCreatePullRequestRequest, Box<BitbucketToolResponse<BitbucketMutationData>>>
    {
        let mutation = BitbucketMutationRequest::Create(request.clone());
        let decision = self
            .bitbucket_confirmation_decision(
                mutation,
                &request,
                request.confirmed,
                request.confirmation_token.as_deref(),
            )
            .await?;
        let plan = bitbucket_confirmation_result(decision)?;
        bind_resolved_reviewers(request, &plan)
            .map_err(|error| Box::new(bitbucket_failure_response(&error)))
    }

    async fn bitbucket_review_tool(
        &self,
        request: BitbucketConfirmedPullRequestRequest,
        action: BitbucketReviewAction,
    ) -> Json<BitbucketToolResponse<BitbucketMutationData>> {
        let mutation = BitbucketMutationRequest::Review(request.clone(), action);
        let request = match self.confirmed_bitbucket_mutation(request, mutation).await {
            Ok(request) => request,
            Err(response) => return Json(*response),
        };
        Json(bitbucket_response(
            action.execute(&*self.bitbucket, request).await,
        ))
    }
}

#[cfg(feature = "jira")]
fn jira_confirmation_result(
    decision: ConfirmationDecision<JiraMutationPlan>,
) -> Result<JiraMutationPlan, Box<JiraToolResponse<JiraMutationData>>> {
    match decision {
        ConfirmationDecision::Execute(plan) => Ok(plan),
        ConfirmationDecision::Preview(confirmation) => Err(Box::new(
            JiraToolResponse::confirmation_required(confirmation_failure(), confirmation),
        )),
        ConfirmationDecision::Reject(error) => Err(Box::new(JiraToolResponse::failure(error))),
    }
}

#[cfg(feature = "jira")]
fn jira_worklog_confirmation_result(
    decision: ConfirmationDecision<JiraWorklogPlan>,
) -> Option<JiraWorklogToolResponse> {
    match decision {
        ConfirmationDecision::Execute(_) => None,
        ConfirmationDecision::Preview(confirmation) => Some(
            JiraWorklogToolResponse::confirmation_required(confirmation_failure(), confirmation),
        ),
        ConfirmationDecision::Reject(error) => Some(JiraWorklogToolResponse::failure(error)),
    }
}

#[cfg(feature = "bitbucket")]
fn bitbucket_confirmation_result(
    decision: ConfirmationDecision<BitbucketMutationPlan>,
) -> Result<BitbucketMutationPlan, Box<BitbucketToolResponse<BitbucketMutationData>>> {
    match decision {
        ConfirmationDecision::Execute(plan) => Ok(plan),
        ConfirmationDecision::Preview(confirmation) => {
            Err(Box::new(BitbucketToolResponse::confirmation_required(
                bitbucket_confirmation_failure(),
                confirmation,
            )))
        }
        ConfirmationDecision::Reject(error) => Err(Box::new(BitbucketToolResponse::failure(error))),
    }
}

#[cfg(feature = "bitbucket")]
fn bitbucket_failure_response(
    error: &crate::BitbucketBackendError,
) -> BitbucketToolResponse<BitbucketMutationData> {
    BitbucketToolResponse::failure(bitbucket_backend_failure(error))
}

#[cfg(feature = "bitbucket")]
impl BitbucketReviewAction {
    fn execute(
        self,
        backend: &dyn BitbucketPullRequestBackend,
        request: BitbucketConfirmedPullRequestRequest,
    ) -> crate::BitbucketFuture<'_, BitbucketMutationData> {
        match self {
            Self::Approve => backend.approve(request),
            Self::Unapprove => backend.unapprove(request),
            Self::RequestChanges => backend.request_changes(request),
            Self::RemoveChangeRequest => backend.remove_change_request(request),
        }
    }
}

#[cfg(feature = "jira")]
fn jira_response<Data>(
    result: Result<Data, crate::JiraIssueBackendError>,
) -> JiraToolResponse<Data> {
    match result {
        Ok(data) => JiraToolResponse::success(data),
        Err(error) => JiraToolResponse::failure(jira_backend_failure(&error)),
    }
}

#[cfg(feature = "jira")]
fn jira_worklog_response(
    result: Result<JiraWorklogData, crate::JiraIssueBackendError>,
) -> JiraWorklogToolResponse {
    match result {
        Ok(data) => JiraWorklogToolResponse::success(data),
        Err(error) => JiraWorklogToolResponse::failure(jira_backend_failure(&error)),
    }
}

#[cfg(feature = "bitbucket")]
fn bitbucket_response<Data>(
    result: Result<Data, crate::BitbucketBackendError>,
) -> BitbucketToolResponse<Data> {
    match result {
        Ok(data) => BitbucketToolResponse::success(data),
        Err(error) => BitbucketToolResponse::failure(bitbucket_backend_failure(&error)),
    }
}

#[cfg(feature = "jira")]
struct UnavailableJiraIssueBackend;

#[cfg(feature = "jira")]
struct UnavailableJiraWorklogBackend;

#[cfg(feature = "jira")]
struct UnavailableOwnHoursBackend;

#[cfg(feature = "jira")]
impl OwnHoursBackend for UnavailableOwnHoursBackend {
    fn load_own_hours(&self, _period: hours_core::DateRange) -> crate::OwnHoursFuture<'_> {
        Box::pin(async { Err(crate::OwnHoursBackendError::InvalidConfiguration) })
    }

    fn load_unlogged_issues(
        &self,
        _period: hours_core::DateRange,
    ) -> crate::UnloggedIssuesFuture<'_> {
        Box::pin(async { Err(crate::OwnHoursBackendError::InvalidConfiguration) })
    }
}

#[cfg(feature = "jira")]
impl JiraIssueBackend for UnavailableJiraIssueBackend {
    fn get_issue(
        &self,
        _request: JiraGetIssueRequest,
    ) -> crate::JiraIssueFuture<'_, JiraIssueData> {
        unavailable_jira_future()
    }

    fn search_issues(
        &self,
        _request: JiraSearchIssuesRequest,
    ) -> crate::JiraIssueFuture<'_, JiraIssueSearchData> {
        unavailable_jira_future()
    }

    fn get_edit_metadata(
        &self,
        _request: JiraIssueKeyRequest,
    ) -> crate::JiraIssueFuture<'_, JiraEditMetadataData> {
        unavailable_jira_future()
    }

    fn get_transitions(
        &self,
        _request: JiraIssueKeyRequest,
    ) -> crate::JiraIssueFuture<'_, JiraTransitionsData> {
        unavailable_jira_future()
    }

    fn update_issue(
        &self,
        _request: JiraUpdateIssueRequest,
    ) -> crate::JiraIssueFuture<'_, JiraMutationData> {
        unavailable_jira_future()
    }

    fn add_comment(
        &self,
        _request: JiraAddCommentRequest,
    ) -> crate::JiraIssueFuture<'_, JiraMutationData> {
        unavailable_jira_future()
    }

    fn transition_issue(
        &self,
        _request: JiraTransitionIssueRequest,
    ) -> crate::JiraIssueFuture<'_, JiraMutationData> {
        unavailable_jira_future()
    }
    fn preview_mutation(
        &self,
        _request: JiraMutationRequest,
    ) -> crate::JiraIssueFuture<'_, JiraMutationPlan> {
        unavailable_jira_future()
    }
}

#[cfg(feature = "jira")]
impl JiraWorklogBackend for UnavailableJiraWorklogBackend {
    fn preview_create_worklog(
        &self,
        _request: JiraCreateWorklogRequest,
    ) -> crate::JiraWorklogFuture<'_, JiraWorklogPlan> {
        unavailable_worklog_future()
    }

    fn create_worklog(
        &self,
        _request: JiraCreateWorklogRequest,
    ) -> crate::JiraWorklogFuture<'_, JiraWorklogData> {
        unavailable_worklog_future()
    }
}

#[cfg(feature = "jira")]
fn unavailable_worklog_future<Data>() -> crate::JiraWorklogFuture<'static, Data> {
    Box::pin(async { Err(crate::JiraIssueBackendError::InvalidConfiguration) })
}

#[cfg(feature = "jira")]
fn unavailable_jira_future<Data>() -> crate::JiraIssueFuture<'static, Data> {
    Box::pin(async { Err(crate::JiraIssueBackendError::InvalidConfiguration) })
}

#[cfg(feature = "bitbucket")]
struct UnavailableBitbucketBackend;

#[cfg(feature = "bitbucket")]
impl BitbucketPullRequestBackend for UnavailableBitbucketBackend {
    fn list_repositories(
        &self,
        _request: BitbucketWorkspaceRequest,
    ) -> crate::BitbucketFuture<'_, BitbucketRepositoryListData> {
        unavailable_bitbucket_future()
    }
    fn list_pull_requests(
        &self,
        _request: BitbucketListPullRequestsRequest,
    ) -> crate::BitbucketFuture<'_, BitbucketPullRequestListData> {
        unavailable_bitbucket_future()
    }
    fn get_pull_request(
        &self,
        _request: BitbucketPullRequestKeyRequest,
    ) -> crate::BitbucketFuture<'_, BitbucketPullRequestData> {
        unavailable_bitbucket_future()
    }
    fn get_activity(
        &self,
        _request: BitbucketPullRequestKeyRequest,
    ) -> crate::BitbucketFuture<'_, BitbucketActivityData> {
        unavailable_bitbucket_future()
    }
    fn create_pull_request(
        &self,
        _request: BitbucketCreatePullRequestRequest,
    ) -> crate::BitbucketFuture<'_, BitbucketMutationData> {
        unavailable_bitbucket_future()
    }
    fn update_pull_request(
        &self,
        _request: BitbucketUpdatePullRequestRequest,
    ) -> crate::BitbucketFuture<'_, BitbucketMutationData> {
        unavailable_bitbucket_future()
    }
    fn add_comment(
        &self,
        _request: BitbucketCommentRequest,
    ) -> crate::BitbucketFuture<'_, BitbucketMutationData> {
        unavailable_bitbucket_future()
    }
    fn approve(
        &self,
        _request: BitbucketConfirmedPullRequestRequest,
    ) -> crate::BitbucketFuture<'_, BitbucketMutationData> {
        unavailable_bitbucket_future()
    }
    fn unapprove(
        &self,
        _request: BitbucketConfirmedPullRequestRequest,
    ) -> crate::BitbucketFuture<'_, BitbucketMutationData> {
        unavailable_bitbucket_future()
    }
    fn request_changes(
        &self,
        _request: BitbucketConfirmedPullRequestRequest,
    ) -> crate::BitbucketFuture<'_, BitbucketMutationData> {
        unavailable_bitbucket_future()
    }
    fn remove_change_request(
        &self,
        _request: BitbucketConfirmedPullRequestRequest,
    ) -> crate::BitbucketFuture<'_, BitbucketMutationData> {
        unavailable_bitbucket_future()
    }
    fn merge(
        &self,
        _request: BitbucketMergePullRequestRequest,
    ) -> crate::BitbucketFuture<'_, BitbucketMutationData> {
        unavailable_bitbucket_future()
    }
    fn decline(
        &self,
        _request: BitbucketConfirmedPullRequestRequest,
    ) -> crate::BitbucketFuture<'_, BitbucketMutationData> {
        unavailable_bitbucket_future()
    }
    fn preview_mutation(
        &self,
        _request: BitbucketMutationRequest,
    ) -> crate::BitbucketFuture<'_, BitbucketMutationPlan> {
        unavailable_bitbucket_future()
    }
}

#[cfg(feature = "bitbucket")]
fn unavailable_bitbucket_future<Data>() -> crate::BitbucketFuture<'static, Data> {
    Box::pin(async { Err(crate::BitbucketBackendError::InvalidConfiguration) })
}

fn tool_names(router: &ToolRouter<WorkloggerMcpServer>) -> Vec<String> {
    router
        .list_all()
        .into_iter()
        .map(|tool| tool.name.into_owned())
        .collect()
}

#[tool_handler(router = self.tool_router)]
// rmcp generates async trait methods; the lint exists after the workspace MSRV.
#[allow(unknown_lints)]
#[allow(clippy::unused_async_trait_impl)]
impl ServerHandler for WorkloggerMcpServer {
    #[allow(deprecated)]
    fn get_info(&self) -> ServerInfo {
        let mut implementation = Implementation::new(SERVER_NAME, env!("CARGO_PKG_VERSION"));
        implementation.title = Some(SERVER_TITLE.to_owned());
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(implementation)
            .with_instructions(server_instructions())
    }
}

fn server_instructions() -> String {
    let sections = [
        SERVER_INSTRUCTIONS,
        #[cfg(feature = "jira")]
        JIRA_SERVER_INSTRUCTIONS,
        #[cfg(feature = "bitbucket")]
        BITBUCKET_SERVER_INSTRUCTIONS,
    ];
    sections.map(str::trim).join("\n")
}

#[cfg(feature = "jira")]
fn backend_failure(error: &crate::OwnHoursBackendError) -> ToolFailure {
    let code = match error {
        crate::OwnHoursBackendError::InvalidConfiguration => ERROR_INVALID_INPUT,
        crate::OwnHoursBackendError::AuthenticationRequired => ERROR_AUTHENTICATION_REQUIRED,
        crate::OwnHoursBackendError::Forbidden => ERROR_FORBIDDEN,
        crate::OwnHoursBackendError::NotFound => ERROR_NOT_FOUND,
        crate::OwnHoursBackendError::InvalidProviderResponse => ERROR_INVALID_PROVIDER_RESPONSE,
        crate::OwnHoursBackendError::Provider { .. } => ERROR_PROVIDER_UNAVAILABLE,
    };
    ToolFailure {
        code: code.to_owned(),
        message: error.to_string(),
        retryable: matches!(
            error,
            crate::OwnHoursBackendError::Provider { retryable: true }
        ),
    }
}

#[cfg(feature = "jira")]
fn hours_failure() -> ToolFailure {
    backend_failure(&crate::OwnHoursBackendError::InvalidConfiguration)
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn confirmation_error_failure(error: &ConfirmationError) -> ToolFailure {
    ToolFailure {
        code: ERROR_INVALID_CONFIRMATION.to_owned(),
        message: error.to_string(),
        retryable: false,
    }
}

#[cfg(test)]
mod instruction_tests {
    #[cfg(feature = "bitbucket")]
    use super::BITBUCKET_SERVER_INSTRUCTIONS;
    #[cfg(feature = "jira")]
    use super::JIRA_SERVER_INSTRUCTIONS;
    use super::{SERVER_INSTRUCTIONS, server_instructions};

    #[test]
    fn instructions_always_include_common_safety_guidance() {
        assert!(server_instructions().contains(SERVER_INSTRUCTIONS.trim()));
        assert!(server_instructions().contains("confirmation.visiblePreview"));
        assert!(server_instructions().contains("Vista previa"));
    }

    #[test]
    #[cfg(feature = "jira")]
    fn jira_build_includes_jira_guidance() {
        assert!(server_instructions().contains(JIRA_SERVER_INSTRUCTIONS.trim()));
    }

    #[test]
    #[cfg(not(feature = "jira"))]
    fn build_without_jira_omits_jira_guidance() {
        assert!(!server_instructions().contains("tareas faltan cargar"));
    }

    #[test]
    #[cfg(feature = "bitbucket")]
    fn bitbucket_build_includes_bitbucket_guidance() {
        assert!(server_instructions().contains(BITBUCKET_SERVER_INSTRUCTIONS.trim()));
    }

    #[test]
    #[cfg(not(feature = "bitbucket"))]
    fn build_without_bitbucket_omits_bitbucket_guidance() {
        assert!(!server_instructions().contains("pull requests de Bitbucket"));
    }
}
