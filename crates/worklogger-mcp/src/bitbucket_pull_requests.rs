use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use bitbucket_adapter::{
    BitbucketClient, CreatePullRequest, MergePullRequest, MergeStrategy, PageLimits, PullRequest,
    PullRequestState, Repository, UpdatePullRequest,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tool_failure::{
    CONFIRMATION_REQUIRED_MESSAGE, ERROR_AUTHENTICATION_REQUIRED, ERROR_CONFIRMATION_REQUIRED,
    ERROR_FORBIDDEN, ERROR_INVALID_INPUT, ERROR_INVALID_PROVIDER_RESPONSE, ERROR_NOT_FOUND,
    ERROR_OUTSIDE_SCOPE, ERROR_PROVIDER_REJECTED, ERROR_PROVIDER_UNAVAILABLE,
    ERROR_STALE_CONFIRMATION, RESPONSE_SCHEMA_VERSION, SOURCE_BITBUCKET,
};
use crate::{BitbucketConfiguration, MutationConfirmation, ToolFailure};

pub type BitbucketFuture<'backend, Output> =
    Pin<Box<dyn Future<Output = Result<Output, BitbucketBackendError>> + Send + 'backend>>;

pub trait BitbucketPullRequestBackend: Send + Sync {
    fn list_repositories(
        &self,
        request: BitbucketWorkspaceRequest,
    ) -> BitbucketFuture<'_, BitbucketRepositoryListData>;
    fn list_pull_requests(
        &self,
        request: BitbucketListPullRequestsRequest,
    ) -> BitbucketFuture<'_, BitbucketPullRequestListData>;
    fn get_pull_request(
        &self,
        request: BitbucketPullRequestKeyRequest,
    ) -> BitbucketFuture<'_, BitbucketPullRequestData>;
    fn get_activity(
        &self,
        request: BitbucketPullRequestKeyRequest,
    ) -> BitbucketFuture<'_, BitbucketActivityData>;
    fn create_pull_request(
        &self,
        request: BitbucketCreatePullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData>;
    fn update_pull_request(
        &self,
        request: BitbucketUpdatePullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData>;
    fn add_comment(
        &self,
        request: BitbucketCommentRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData>;
    fn approve(
        &self,
        request: BitbucketConfirmedPullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData>;
    fn unapprove(
        &self,
        request: BitbucketConfirmedPullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData>;
    fn request_changes(
        &self,
        request: BitbucketConfirmedPullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData>;
    fn remove_change_request(
        &self,
        request: BitbucketConfirmedPullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData>;
    fn merge(
        &self,
        request: BitbucketMergePullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData>;
    fn decline(
        &self,
        request: BitbucketConfirmedPullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData>;
    fn preview_mutation(
        &self,
        request: BitbucketMutationRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationPlan>;
}

#[derive(Clone, Debug)]
pub enum BitbucketMutationRequest {
    Create(BitbucketCreatePullRequestRequest),
    Update(BitbucketUpdatePullRequestRequest),
    Comment(BitbucketCommentRequest),
    Review(BitbucketConfirmedPullRequestRequest, BitbucketReviewAction),
    Merge(BitbucketMergePullRequestRequest),
    Decline(BitbucketConfirmedPullRequestRequest),
}

#[derive(Clone, Copy, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum BitbucketReviewAction {
    Approve,
    Unapprove,
    RequestChanges,
    RemoveChangeRequest,
}

#[derive(Debug, thiserror::Error)]
pub enum BitbucketBackendError {
    #[error("la configuración o el destino de Bitbucket no es válido")]
    InvalidConfiguration,
    #[error("el workspace no pertenece al ámbito configurado")]
    WorkspaceOutsideScope,
    #[error("el repositorio no pertenece al ámbito configurado")]
    RepositoryOutsideScope,
    #[error("la operación requiere confirmación explícita")]
    ConfirmationRequired,
    #[error("el pull request cambió después de la confirmación")]
    StaleConfirmation,
    #[error("la sesión de Bitbucket no es válida")]
    AuthenticationRequired,
    #[error("la cuenta autenticada no tiene permiso para esta operación")]
    Forbidden,
    #[error("el recurso Bitbucket solicitado no existe")]
    NotFound,
    #[error("Bitbucket devolvió datos inválidos o una paginación inconsistente")]
    InvalidProviderResponse,
    #[error("Bitbucket rechazó o no pudo completar la operación")]
    Provider { retryable: bool },
    #[error("Bitbucket rechazó la operación")]
    ProviderRejected,
}

pub struct BitbucketPullRequestService {
    client: BitbucketClient,
    workspaces: BTreeMap<String, BTreeSet<String>>,
    page_size: u16,
    maximum_collection_items: usize,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BitbucketWorkspaceRequest {
    pub workspace: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BitbucketPullRequestKeyRequest {
    pub workspace: String,
    pub repository: String,
    pub pull_request_id: u64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BitbucketListPullRequestsRequest {
    pub workspace: String,
    pub repository: String,
    pub state: Option<PullRequestState>,
    pub search: Option<String>,
    pub max_results: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BitbucketCreatePullRequestRequest {
    pub workspace: String,
    pub repository: String,
    pub title: String,
    pub description: Option<String>,
    pub source_branch: String,
    pub destination_branch: String,
    pub reviewer_account_ids: Option<Vec<String>>,
    pub close_source_branch: Option<bool>,
    pub confirmed: bool,
    pub confirmation_token: Option<String>,
    #[serde(skip)]
    #[schemars(skip)]
    resolved_reviewer_account_ids: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BitbucketUpdatePullRequestRequest {
    pub workspace: String,
    pub repository: String,
    pub pull_request_id: u64,
    pub title: Option<String>,
    pub description: Option<String>,
    pub destination_branch: Option<String>,
    pub close_source_branch: Option<bool>,
    pub confirmed: bool,
    pub confirmation_token: Option<String>,
    #[serde(skip)]
    #[schemars(skip)]
    expected_revision: Option<BitbucketPullRequestRevision>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BitbucketCommentRequest {
    pub workspace: String,
    pub repository: String,
    pub pull_request_id: u64,
    pub text: String,
    pub confirmed: bool,
    pub confirmation_token: Option<String>,
    #[serde(skip)]
    #[schemars(skip)]
    expected_revision: Option<BitbucketPullRequestRevision>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BitbucketConfirmedPullRequestRequest {
    pub workspace: String,
    pub repository: String,
    pub pull_request_id: u64,
    pub confirmed: bool,
    pub confirmation_token: Option<String>,
    #[serde(skip)]
    #[schemars(skip)]
    expected_revision: Option<BitbucketPullRequestRevision>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BitbucketMergePullRequestRequest {
    pub workspace: String,
    pub repository: String,
    pub pull_request_id: u64,
    pub message: Option<String>,
    pub close_source_branch: Option<bool>,
    pub merge_strategy: MergeStrategy,
    pub confirmed: bool,
    pub confirmation_token: Option<String>,
    #[serde(skip)]
    #[schemars(skip)]
    expected_revision: Option<BitbucketPullRequestRevision>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BitbucketToolResponse<Data> {
    pub schema_version: u16,
    pub success: bool,
    pub data: Option<Data>,
    pub error: Option<ToolFailure>,
    pub confirmation: Option<MutationConfirmation<BitbucketMutationPlan>>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BitbucketActorData {
    pub account_id: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BitbucketRepositoryData {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub full_name: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BitbucketRepositoryListData {
    pub source: String,
    pub workspace: String,
    pub total: usize,
    pub repositories: Vec<BitbucketRepositoryData>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BitbucketPullRequestData {
    pub source: String,
    pub id: u64,
    pub title: String,
    pub description: String,
    pub state: PullRequestState,
    pub author: BitbucketActorData,
    pub source_branch: String,
    pub destination_branch: String,
    pub url: String,
    pub participants: Vec<BitbucketParticipantData>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BitbucketParticipantData {
    pub account_id: String,
    pub display_name: String,
    pub role: String,
    pub approved: bool,
    pub state: Option<String>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BitbucketPullRequestListData {
    pub source: String,
    pub total: usize,
    pub pull_requests: Vec<BitbucketPullRequestData>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BitbucketActivityData {
    pub source: String,
    pub target: BitbucketPullRequestTarget,
    pub events: Vec<BitbucketActivityEvent>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BitbucketActivityKind {
    Update,
    Approval,
    ChangesRequested,
    Comment,
    Unknown,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BitbucketActivityEvent {
    pub kind: BitbucketActivityKind,
    pub actor: Option<BitbucketActorData>,
    pub occurred_at: Option<String>,
    pub summary: Option<String>,
    pub url: Option<String>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BitbucketMutationData {
    pub source: String,
    pub actor: BitbucketActorData,
    pub target: BitbucketPullRequestTarget,
    pub effect: BitbucketMutationEffect,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BitbucketPullRequestTarget {
    pub workspace: String,
    pub repository: String,
    pub pull_request_id: u64,
    pub url: String,
    pub source_branch: String,
    pub destination_branch: String,
    pub state: PullRequestState,
    pub revision: BitbucketPullRequestRevision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BitbucketPullRequestRevision {
    pub title: String,
    pub description: String,
    pub source_branch: String,
    pub destination_branch: String,
    pub state: PullRequestState,
    pub source_commit: String,
    pub destination_commit: String,
    pub updated_on: String,
    pub close_source_branch: bool,
    pub draft: bool,
    pub participants: Vec<BitbucketParticipantData>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum BitbucketMutationEffect {
    PullRequestCreated,
    PullRequestUpdated,
    CommentAdded { comment_id: u64 },
    Approved,
    ApprovalRemoved,
    ChangesRequested,
    ChangeRequestRemoved,
    Merged,
    Declined,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BitbucketMutationPlan {
    pub source: String,
    pub actor: BitbucketActorData,
    pub target: BitbucketPlannedTarget,
    pub effect: BitbucketPlannedEffect,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum BitbucketPlannedTarget {
    Repository {
        workspace: String,
        repository: String,
        source_branch: String,
        destination_branch: String,
    },
    PullRequest(Box<BitbucketPullRequestTarget>),
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum BitbucketPlannedEffect {
    Create {
        title: String,
        description: String,
        reviewer_account_ids: Vec<String>,
        close_source_branch: bool,
    },
    Update {
        title: Option<String>,
        description: Option<String>,
        destination_branch: Option<String>,
        close_source_branch: Option<bool>,
    },
    AddComment {
        text: String,
    },
    Review {
        action: BitbucketReviewAction,
    },
    Merge {
        strategy: MergeStrategy,
        message: Option<String>,
        close_source_branch: bool,
    },
    Decline,
}

pub(crate) trait RevisionBoundRequest {
    fn expected_revision(&self) -> Option<&BitbucketPullRequestRevision>;
    fn set_expected_revision(&mut self, revision: BitbucketPullRequestRevision);
    fn confirmed(&self) -> bool;
    fn confirmation_token(&self) -> Option<&str>;
}

pub(crate) fn bind_expected_revision<Request: RevisionBoundRequest>(
    mut request: Request,
    plan: &BitbucketMutationPlan,
) -> Result<Request, BitbucketBackendError> {
    let BitbucketPlannedTarget::PullRequest(target) = &plan.target else {
        return Err(BitbucketBackendError::InvalidConfiguration);
    };
    request.set_expected_revision(target.revision.clone());
    Ok(request)
}

impl BitbucketPullRequestService {
    /// Builds a scoped Bitbucket Cloud pull request service.
    ///
    /// # Errors
    ///
    /// Returns an error when credentials, limits or HTTP configuration are invalid.
    pub fn new(
        configuration: &BitbucketConfiguration,
        token: String,
    ) -> Result<Self, BitbucketBackendError> {
        validate_configured_repository_count(configuration)?;
        let timeout = Duration::from_secs(configuration.request_timeout_seconds);
        let client = BitbucketClient::new(configuration.email.clone(), token, timeout)
            .map_err(|error| provider_error(&error))?;
        Ok(Self {
            client,
            workspaces: configuration.workspaces.clone(),
            page_size: configuration.page_size,
            maximum_collection_items: configuration.maximum_collection_items,
        })
    }

    fn ensure_workspace(&self, workspace: &str) -> Result<(), BitbucketBackendError> {
        if self.workspaces.contains_key(workspace) {
            return Ok(());
        }
        Err(BitbucketBackendError::WorkspaceOutsideScope)
    }

    fn ensure_repository(
        &self,
        workspace: &str,
        repository: &str,
    ) -> Result<(), BitbucketBackendError> {
        let configured = self
            .workspaces
            .get(workspace)
            .ok_or(BitbucketBackendError::WorkspaceOutsideScope)?;
        if configured.contains(repository) {
            return Ok(());
        }
        Err(BitbucketBackendError::RepositoryOutsideScope)
    }

    fn limits(&self, requested: Option<usize>) -> Result<PageLimits, BitbucketBackendError> {
        let maximum = requested
            .unwrap_or(self.maximum_collection_items)
            .min(self.maximum_collection_items);
        let page_size = usize::from(self.page_size).min(maximum);
        let page_size =
            u16::try_from(page_size).map_err(|_| BitbucketBackendError::InvalidConfiguration)?;
        let limits = PageLimits::new(page_size, maximum).map_err(|error| provider_error(&error))?;
        if requested.is_some_and(|value| value < self.maximum_collection_items) {
            return Ok(limits.truncate_at_limit());
        }
        Ok(limits)
    }

    async fn actor(&self) -> Result<BitbucketActorData, BitbucketBackendError> {
        let actor = self
            .client
            .current_user()
            .await
            .map_err(|error| provider_error(&error))?;
        Ok(BitbucketActorData {
            account_id: actor.account_id,
            display_name: actor.display_name,
        })
    }

    async fn target(
        &self,
        request: &BitbucketPullRequestKeyRequest,
    ) -> Result<BitbucketPullRequestTarget, BitbucketBackendError> {
        let pull_request = self
            .client
            .get_pull_request(
                &request.workspace,
                &request.repository,
                request.pull_request_id,
            )
            .await
            .map_err(|error| provider_error(&error))?;
        Ok(target_from_pull_request(request, &pull_request))
    }

    fn mutation(
        actor: BitbucketActorData,
        target: BitbucketPullRequestTarget,
        effect: BitbucketMutationEffect,
    ) -> BitbucketMutationData {
        BitbucketMutationData {
            source: SOURCE_BITBUCKET.to_owned(),
            actor,
            target,
            effect,
        }
    }

    async fn load_repositories(
        &self,
        request: BitbucketWorkspaceRequest,
    ) -> Result<BitbucketRepositoryListData, BitbucketBackendError> {
        self.ensure_workspace(&request.workspace)?;
        let repositories = self.configured_repositories(&request.workspace).await?;
        Ok(BitbucketRepositoryListData {
            source: SOURCE_BITBUCKET.to_owned(),
            workspace: request.workspace,
            total: repositories.len(),
            repositories,
        })
    }

    async fn configured_repositories(
        &self,
        workspace: &str,
    ) -> Result<Vec<BitbucketRepositoryData>, BitbucketBackendError> {
        let configured = self
            .workspaces
            .get(workspace)
            .ok_or(BitbucketBackendError::WorkspaceOutsideScope)?;
        let mut repositories = Vec::with_capacity(configured.len());
        for repository in configured {
            repositories.push(self.configured_repository(workspace, repository).await?);
        }
        Ok(repositories)
    }

    async fn configured_repository(
        &self,
        workspace: &str,
        repository: &str,
    ) -> Result<BitbucketRepositoryData, BitbucketBackendError> {
        self.client
            .get_repository(workspace, repository)
            .await
            .map(repository_data)
            .map_err(|error| provider_error(&error))
    }

    async fn load_pull_requests(
        &self,
        request: BitbucketListPullRequestsRequest,
    ) -> Result<BitbucketPullRequestListData, BitbucketBackendError> {
        self.ensure_repository(&request.workspace, &request.repository)?;
        let pull_requests = self.pull_requests(&request).await?;
        let pull_requests = pull_requests
            .into_iter()
            .map(pull_request_data)
            .collect::<Vec<_>>();
        Ok(BitbucketPullRequestListData {
            source: SOURCE_BITBUCKET.to_owned(),
            total: pull_requests.len(),
            pull_requests,
        })
    }

    async fn pull_requests(
        &self,
        request: &BitbucketListPullRequestsRequest,
    ) -> Result<Vec<PullRequest>, BitbucketBackendError> {
        self.client
            .list_pull_requests(
                &request.workspace,
                &request.repository,
                request.state,
                request.search.as_deref(),
                self.limits(request.max_results)?,
            )
            .await
            .map_err(|error| provider_error(&error))
    }

    async fn load_pull_request(
        &self,
        request: BitbucketPullRequestKeyRequest,
    ) -> Result<BitbucketPullRequestData, BitbucketBackendError> {
        self.ensure_repository(&request.workspace, &request.repository)?;
        self.client
            .get_pull_request(
                &request.workspace,
                &request.repository,
                request.pull_request_id,
            )
            .await
            .map(pull_request_data)
            .map_err(|error| provider_error(&error))
    }

    async fn load_activity(
        &self,
        request: BitbucketPullRequestKeyRequest,
    ) -> Result<BitbucketActivityData, BitbucketBackendError> {
        self.ensure_repository(&request.workspace, &request.repository)?;
        let target = self.target(&request).await?;
        let provider_events = self.pull_request_activity(&request).await?;
        let events = activity_events(&provider_events);
        Ok(BitbucketActivityData {
            source: SOURCE_BITBUCKET.to_owned(),
            target,
            events,
        })
    }

    async fn pull_request_activity(
        &self,
        request: &BitbucketPullRequestKeyRequest,
    ) -> Result<Vec<Value>, BitbucketBackendError> {
        self.client
            .get_pull_request_activity(
                &request.workspace,
                &request.repository,
                request.pull_request_id,
                self.limits(None)?,
            )
            .await
            .map_err(|error| provider_error(&error))
    }

    async fn apply_create(
        &self,
        request: BitbucketCreatePullRequestRequest,
    ) -> Result<BitbucketMutationData, BitbucketBackendError> {
        require_confirmation(request.confirmed)?;
        self.ensure_repository(&request.workspace, &request.repository)?;
        let actor = self.actor().await?;
        let reviewers = self.resolved_reviewers(&request).await?;
        let input = create_input(&request, reviewers);
        let pull_request = self
            .client
            .create_pull_request(&request.workspace, &request.repository, &input)
            .await
            .map_err(|error| provider_error(&error))?;
        let target = target_from_parts(&request.workspace, &request.repository, &pull_request);
        Ok(Self::mutation(
            actor,
            target,
            BitbucketMutationEffect::PullRequestCreated,
        ))
    }

    async fn apply_update(
        &self,
        request: BitbucketUpdatePullRequestRequest,
    ) -> Result<BitbucketMutationData, BitbucketBackendError> {
        require_confirmation(request.confirmed)?;
        self.ensure_repository(&request.workspace, &request.repository)?;
        let actor = self.actor().await?;
        let current = self.target(&key_request(&request)).await?;
        verify_expected_revision(&request, &current)?;
        let pull_request = self.update_pull_request(&request).await?;
        let target = target_from_parts(&request.workspace, &request.repository, &pull_request);
        Ok(Self::mutation(
            actor,
            target,
            BitbucketMutationEffect::PullRequestUpdated,
        ))
    }

    async fn update_pull_request(
        &self,
        request: &BitbucketUpdatePullRequestRequest,
    ) -> Result<PullRequest, BitbucketBackendError> {
        self.client
            .update_pull_request(
                &request.workspace,
                &request.repository,
                request.pull_request_id,
                &update_input(request),
            )
            .await
            .map_err(|error| provider_error(&error))
    }

    async fn apply_comment(
        &self,
        request: BitbucketCommentRequest,
    ) -> Result<BitbucketMutationData, BitbucketBackendError> {
        require_confirmation(request.confirmed)?;
        self.ensure_repository(&request.workspace, &request.repository)?;
        let actor = self.actor().await?;
        let target = self.target(&key_request(&request)).await?;
        verify_expected_revision(&request, &target)?;
        let comment_id = self.add_comment(&request).await?;
        Ok(Self::mutation(
            actor,
            target,
            BitbucketMutationEffect::CommentAdded { comment_id },
        ))
    }

    async fn add_comment(
        &self,
        request: &BitbucketCommentRequest,
    ) -> Result<u64, BitbucketBackendError> {
        self.client
            .add_pull_request_comment(
                &request.workspace,
                &request.repository,
                request.pull_request_id,
                &request.text,
            )
            .await
            .map(|comment| comment.id)
            .map_err(|error| provider_error(&error))
    }

    async fn apply_action(
        &self,
        request: BitbucketConfirmedPullRequestRequest,
        action: ReviewAction,
    ) -> Result<BitbucketMutationData, BitbucketBackendError> {
        require_confirmation(request.confirmed)?;
        self.ensure_repository(&request.workspace, &request.repository)?;
        let actor = self.actor().await?;
        let target = self.target(&key_request(&request)).await?;
        verify_expected_revision(&request, &target)?;
        action.execute(&self.client, &request).await?;
        Ok(Self::mutation(actor, target, action.effect()))
    }

    async fn apply_merge(
        &self,
        request: BitbucketMergePullRequestRequest,
    ) -> Result<BitbucketMutationData, BitbucketBackendError> {
        require_confirmation(request.confirmed)?;
        self.ensure_repository(&request.workspace, &request.repository)?;
        let actor = self.actor().await?;
        let target = self.target(&key_request(&request)).await?;
        verify_expected_revision(&request, &target)?;
        self.merge_pull_request(&request).await?;
        Ok(Self::mutation(
            actor,
            target,
            BitbucketMutationEffect::Merged,
        ))
    }

    async fn merge_pull_request(
        &self,
        request: &BitbucketMergePullRequestRequest,
    ) -> Result<(), BitbucketBackendError> {
        let input = merge_input(request);
        self.client
            .merge_pull_request(
                &request.workspace,
                &request.repository,
                request.pull_request_id,
                &input,
            )
            .await
            .map(|_| ())
            .map_err(|error| provider_error(&error))
    }

    async fn preview(
        &self,
        request: BitbucketMutationRequest,
    ) -> Result<BitbucketMutationPlan, BitbucketBackendError> {
        match request {
            BitbucketMutationRequest::Create(request) => self.preview_create(&request).await,
            BitbucketMutationRequest::Update(request) => self.preview_update(&request).await,
            BitbucketMutationRequest::Comment(request) => self.preview_comment(&request).await,
            BitbucketMutationRequest::Review(request, action) => {
                self.preview_existing(&request, BitbucketPlannedEffect::Review { action })
                    .await
            }
            BitbucketMutationRequest::Merge(request) => self.preview_merge(&request).await,
            BitbucketMutationRequest::Decline(request) => {
                self.preview_existing(&request, BitbucketPlannedEffect::Decline)
                    .await
            }
        }
    }

    async fn preview_create(
        &self,
        request: &BitbucketCreatePullRequestRequest,
    ) -> Result<BitbucketMutationPlan, BitbucketBackendError> {
        self.ensure_repository(&request.workspace, &request.repository)?;
        let target = BitbucketPlannedTarget::Repository {
            workspace: request.workspace.clone(),
            repository: request.repository.clone(),
            source_branch: request.source_branch.clone(),
            destination_branch: request.destination_branch.clone(),
        };
        let reviewers = self.resolved_reviewers(request).await?;
        self.plan(target, create_effect(request, reviewers)).await
    }

    async fn preview_update(
        &self,
        request: &BitbucketUpdatePullRequestRequest,
    ) -> Result<BitbucketMutationPlan, BitbucketBackendError> {
        let effect = BitbucketPlannedEffect::Update {
            title: request.title.clone(),
            description: request.description.clone(),
            destination_branch: request.destination_branch.clone(),
            close_source_branch: request.close_source_branch,
        };
        self.preview_existing(request, effect).await
    }

    async fn preview_comment(
        &self,
        request: &BitbucketCommentRequest,
    ) -> Result<BitbucketMutationPlan, BitbucketBackendError> {
        if request.text.trim().is_empty() {
            return Err(BitbucketBackendError::InvalidConfiguration);
        }
        let effect = BitbucketPlannedEffect::AddComment {
            text: request.text.trim().to_owned(),
        };
        self.preview_existing(request, effect).await
    }

    async fn preview_merge(
        &self,
        request: &BitbucketMergePullRequestRequest,
    ) -> Result<BitbucketMutationPlan, BitbucketBackendError> {
        self.preview_existing(request, merge_effect(request)).await
    }

    async fn preview_existing<Request: PullRequestLocation>(
        &self,
        request: &Request,
        effect: BitbucketPlannedEffect,
    ) -> Result<BitbucketMutationPlan, BitbucketBackendError> {
        self.ensure_repository(request.workspace(), request.repository())?;
        let key = key_request(request);
        let target = BitbucketPlannedTarget::PullRequest(Box::new(self.target(&key).await?));
        self.plan(target, effect).await
    }

    async fn plan(
        &self,
        target: BitbucketPlannedTarget,
        effect: BitbucketPlannedEffect,
    ) -> Result<BitbucketMutationPlan, BitbucketBackendError> {
        Ok(BitbucketMutationPlan {
            source: SOURCE_BITBUCKET.to_owned(),
            actor: self.actor().await?,
            target,
            effect,
        })
    }

    async fn resolved_reviewers(
        &self,
        request: &BitbucketCreatePullRequestRequest,
    ) -> Result<Vec<String>, BitbucketBackendError> {
        if let Some(reviewers) = &request.resolved_reviewer_account_ids {
            return Ok(reviewers.clone());
        }
        let defaults = self.default_reviewer_account_ids(request).await?;
        Ok(merge_reviewer_account_ids(
            defaults,
            request.reviewer_account_ids.as_deref(),
        ))
    }

    async fn default_reviewer_account_ids(
        &self,
        request: &BitbucketCreatePullRequestRequest,
    ) -> Result<Vec<String>, BitbucketBackendError> {
        Ok(self
            .client
            .effective_default_reviewers(
                &request.workspace,
                &request.repository,
                self.limits(None)?,
            )
            .await
            .map_err(|error| provider_error(&error))?
            .into_iter()
            .map(|reviewer| reviewer.account_id)
            .collect::<Vec<_>>())
    }
}

#[derive(Clone, Copy)]
enum ReviewAction {
    Approve,
    Unapprove,
    RequestChanges,
    RemoveChangeRequest,
    Decline,
}

impl ReviewAction {
    async fn execute(
        self,
        client: &BitbucketClient,
        request: &BitbucketConfirmedPullRequestRequest,
    ) -> Result<(), BitbucketBackendError> {
        let result = match self {
            Self::Approve => approve(client, request).await,
            Self::Unapprove => unapprove(client, request).await,
            Self::RequestChanges => request_changes(client, request).await,
            Self::RemoveChangeRequest => remove_change_request(client, request).await,
            Self::Decline => decline(client, request).await,
        };
        result.map_err(|error| provider_error(&error))
    }

    const fn effect(self) -> BitbucketMutationEffect {
        match self {
            Self::Approve => BitbucketMutationEffect::Approved,
            Self::Unapprove => BitbucketMutationEffect::ApprovalRemoved,
            Self::RequestChanges => BitbucketMutationEffect::ChangesRequested,
            Self::RemoveChangeRequest => BitbucketMutationEffect::ChangeRequestRemoved,
            Self::Decline => BitbucketMutationEffect::Declined,
        }
    }
}

async fn approve(
    client: &BitbucketClient,
    request: &BitbucketConfirmedPullRequestRequest,
) -> Result<(), bitbucket_adapter::BitbucketError> {
    client
        .approve_pull_request(
            &request.workspace,
            &request.repository,
            request.pull_request_id,
        )
        .await
}

async fn unapprove(
    client: &BitbucketClient,
    request: &BitbucketConfirmedPullRequestRequest,
) -> Result<(), bitbucket_adapter::BitbucketError> {
    client
        .unapprove_pull_request(
            &request.workspace,
            &request.repository,
            request.pull_request_id,
        )
        .await
}

async fn request_changes(
    client: &BitbucketClient,
    request: &BitbucketConfirmedPullRequestRequest,
) -> Result<(), bitbucket_adapter::BitbucketError> {
    client
        .request_changes(
            &request.workspace,
            &request.repository,
            request.pull_request_id,
        )
        .await
}

async fn remove_change_request(
    client: &BitbucketClient,
    request: &BitbucketConfirmedPullRequestRequest,
) -> Result<(), bitbucket_adapter::BitbucketError> {
    client
        .remove_change_request(
            &request.workspace,
            &request.repository,
            request.pull_request_id,
        )
        .await
}

async fn decline(
    client: &BitbucketClient,
    request: &BitbucketConfirmedPullRequestRequest,
) -> Result<(), bitbucket_adapter::BitbucketError> {
    client
        .decline_pull_request(
            &request.workspace,
            &request.repository,
            request.pull_request_id,
        )
        .await
}

impl BitbucketPullRequestBackend for BitbucketPullRequestService {
    fn list_repositories(
        &self,
        request: BitbucketWorkspaceRequest,
    ) -> BitbucketFuture<'_, BitbucketRepositoryListData> {
        Box::pin(self.load_repositories(request))
    }
    fn list_pull_requests(
        &self,
        request: BitbucketListPullRequestsRequest,
    ) -> BitbucketFuture<'_, BitbucketPullRequestListData> {
        Box::pin(self.load_pull_requests(request))
    }
    fn get_pull_request(
        &self,
        request: BitbucketPullRequestKeyRequest,
    ) -> BitbucketFuture<'_, BitbucketPullRequestData> {
        Box::pin(self.load_pull_request(request))
    }
    fn get_activity(
        &self,
        request: BitbucketPullRequestKeyRequest,
    ) -> BitbucketFuture<'_, BitbucketActivityData> {
        Box::pin(self.load_activity(request))
    }
    fn create_pull_request(
        &self,
        request: BitbucketCreatePullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData> {
        Box::pin(self.apply_create(request))
    }
    fn update_pull_request(
        &self,
        request: BitbucketUpdatePullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData> {
        Box::pin(self.apply_update(request))
    }
    fn add_comment(
        &self,
        request: BitbucketCommentRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData> {
        Box::pin(self.apply_comment(request))
    }
    fn approve(
        &self,
        request: BitbucketConfirmedPullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData> {
        Box::pin(self.apply_action(request, ReviewAction::Approve))
    }
    fn unapprove(
        &self,
        request: BitbucketConfirmedPullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData> {
        Box::pin(self.apply_action(request, ReviewAction::Unapprove))
    }
    fn request_changes(
        &self,
        request: BitbucketConfirmedPullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData> {
        Box::pin(self.apply_action(request, ReviewAction::RequestChanges))
    }
    fn remove_change_request(
        &self,
        request: BitbucketConfirmedPullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData> {
        Box::pin(self.apply_action(request, ReviewAction::RemoveChangeRequest))
    }
    fn merge(
        &self,
        request: BitbucketMergePullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData> {
        Box::pin(self.apply_merge(request))
    }
    fn decline(
        &self,
        request: BitbucketConfirmedPullRequestRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationData> {
        Box::pin(self.apply_action(request, ReviewAction::Decline))
    }
    fn preview_mutation(
        &self,
        request: BitbucketMutationRequest,
    ) -> BitbucketFuture<'_, BitbucketMutationPlan> {
        Box::pin(self.preview(request))
    }
}

impl<Data> BitbucketToolResponse<Data> {
    #[must_use]
    pub const fn success(data: Data) -> Self {
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
        confirmation: MutationConfirmation<BitbucketMutationPlan>,
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

#[must_use]
pub fn bitbucket_backend_failure(error: &BitbucketBackendError) -> ToolFailure {
    let code = match error {
        BitbucketBackendError::InvalidConfiguration => ERROR_INVALID_INPUT,
        BitbucketBackendError::WorkspaceOutsideScope
        | BitbucketBackendError::RepositoryOutsideScope => ERROR_OUTSIDE_SCOPE,
        BitbucketBackendError::ConfirmationRequired => ERROR_CONFIRMATION_REQUIRED,
        BitbucketBackendError::StaleConfirmation => ERROR_STALE_CONFIRMATION,
        BitbucketBackendError::AuthenticationRequired => ERROR_AUTHENTICATION_REQUIRED,
        BitbucketBackendError::Forbidden => ERROR_FORBIDDEN,
        BitbucketBackendError::NotFound => ERROR_NOT_FOUND,
        BitbucketBackendError::InvalidProviderResponse => ERROR_INVALID_PROVIDER_RESPONSE,
        BitbucketBackendError::ProviderRejected => ERROR_PROVIDER_REJECTED,
        BitbucketBackendError::Provider { .. } => ERROR_PROVIDER_UNAVAILABLE,
    };
    ToolFailure {
        code: code.to_owned(),
        message: error.to_string(),
        retryable: matches!(error, BitbucketBackendError::Provider { retryable: true }),
    }
}

#[must_use]
pub fn bitbucket_confirmation_failure() -> ToolFailure {
    ToolFailure {
        code: ERROR_CONFIRMATION_REQUIRED.to_owned(),
        message: CONFIRMATION_REQUIRED_MESSAGE.to_owned(),
        retryable: false,
    }
}

fn require_confirmation(confirmed: bool) -> Result<(), BitbucketBackendError> {
    if confirmed {
        return Ok(());
    }
    Err(BitbucketBackendError::ConfirmationRequired)
}

fn provider_error(error: &bitbucket_adapter::BitbucketError) -> BitbucketBackendError {
    if bitbucket_invalid_configuration(error) {
        return BitbucketBackendError::InvalidConfiguration;
    }
    if let Some(access_error) = bitbucket_access_error(error) {
        return access_error;
    }
    if bitbucket_invalid_response(error) {
        return BitbucketBackendError::InvalidProviderResponse;
    }
    if bitbucket_retryable(error) {
        return BitbucketBackendError::Provider { retryable: true };
    }
    BitbucketBackendError::ProviderRejected
}

fn bitbucket_invalid_configuration(error: &bitbucket_adapter::BitbucketError) -> bool {
    matches!(
        error,
        bitbucket_adapter::BitbucketError::MissingCredentials
            | bitbucket_adapter::BitbucketError::InvalidPageLimits
            | bitbucket_adapter::BitbucketError::InvalidInput
            | bitbucket_adapter::BitbucketError::InvalidUrl
    )
}

fn bitbucket_access_error(
    error: &bitbucket_adapter::BitbucketError,
) -> Option<BitbucketBackendError> {
    match error {
        bitbucket_adapter::BitbucketError::AuthenticationRequired => {
            Some(BitbucketBackendError::AuthenticationRequired)
        }
        bitbucket_adapter::BitbucketError::Forbidden => Some(BitbucketBackendError::Forbidden),
        bitbucket_adapter::BitbucketError::NotFound => Some(BitbucketBackendError::NotFound),
        _ => None,
    }
}

fn bitbucket_invalid_response(error: &bitbucket_adapter::BitbucketError) -> bool {
    matches!(
        error,
        bitbucket_adapter::BitbucketError::InvalidPagination
            | bitbucket_adapter::BitbucketError::InvalidResponse(_)
            | bitbucket_adapter::BitbucketError::InvalidResponseShape
    )
}

fn bitbucket_retryable(error: &bitbucket_adapter::BitbucketError) -> bool {
    matches!(
        error,
        bitbucket_adapter::BitbucketError::RateLimited
            | bitbucket_adapter::BitbucketError::ServerUnavailable
            | bitbucket_adapter::BitbucketError::Transport(_)
    )
}

fn repository_data(repository: Repository) -> BitbucketRepositoryData {
    BitbucketRepositoryData {
        id: repository.uuid,
        name: repository.name,
        slug: repository.slug,
        full_name: repository.full_name,
        url: repository.links.html.href,
    }
}

const ACTIVITY_UPDATE_FIELD: &str = "update";
const ACTIVITY_APPROVAL_FIELD: &str = "approval";
const ACTIVITY_CHANGES_REQUESTED_FIELD: &str = "changes_requested";
const ACTIVITY_COMMENT_FIELD: &str = "comment";
const ACTIVITY_ACTOR_FIELDS: [&str; 3] = ["user", "actor", "author"];
const ACTIVITY_DATE_FIELDS: [&str; 3] = ["date", "created_on", "updated_on"];
const ACTIVITY_ACCOUNT_FIELDS: [&str; 2] = ["account_id", "uuid"];
const ACTIVITY_DISPLAY_NAME_PATH: [&str; 1] = ["display_name"];
const ACTIVITY_URL_PATH: [&str; 3] = ["links", "html", "href"];
const ACTIVITY_CONTENT_PATH: [&str; 2] = ["content", "raw"];
const ACTIVITY_REASON_PATH: [&str; 1] = ["reason"];
const ACTIVITY_DESCRIPTION_PATH: [&str; 1] = ["description"];

fn activity_events(events: &[Value]) -> Vec<BitbucketActivityEvent> {
    events.iter().map(activity_event).collect()
}

fn activity_event(event: &Value) -> BitbucketActivityEvent {
    let (kind, payload) = activity_payload(event);
    BitbucketActivityEvent {
        kind,
        actor: activity_actor(payload),
        occurred_at: first_string(payload, &ACTIVITY_DATE_FIELDS),
        summary: activity_summary(payload),
        url: nested_string(payload, &ACTIVITY_URL_PATH),
    }
}

fn activity_payload(event: &Value) -> (BitbucketActivityKind, &Value) {
    let candidates = [
        (ACTIVITY_UPDATE_FIELD, BitbucketActivityKind::Update),
        (ACTIVITY_APPROVAL_FIELD, BitbucketActivityKind::Approval),
        (
            ACTIVITY_CHANGES_REQUESTED_FIELD,
            BitbucketActivityKind::ChangesRequested,
        ),
        (ACTIVITY_COMMENT_FIELD, BitbucketActivityKind::Comment),
    ];
    candidates
        .into_iter()
        .find_map(|(field, kind)| event.get(field).map(|payload| (kind, payload)))
        .unwrap_or((BitbucketActivityKind::Unknown, event))
}

fn activity_actor(payload: &Value) -> Option<BitbucketActorData> {
    let actor = ACTIVITY_ACTOR_FIELDS
        .iter()
        .find_map(|field| payload.get(field))?;
    Some(BitbucketActorData {
        account_id: first_string(actor, &ACTIVITY_ACCOUNT_FIELDS)?,
        display_name: nested_string(actor, &ACTIVITY_DISPLAY_NAME_PATH)?,
    })
}

fn activity_summary(payload: &Value) -> Option<String> {
    nested_string(payload, &ACTIVITY_CONTENT_PATH)
        .or_else(|| nested_string(payload, &ACTIVITY_REASON_PATH))
        .or_else(|| nested_string(payload, &ACTIVITY_DESCRIPTION_PATH))
}

fn first_string(value: &Value, fields: &[&str]) -> Option<String> {
    fields
        .iter()
        .find_map(|field| nested_string(value, &[*field]))
}

fn nested_string(value: &Value, fields: &[&str]) -> Option<String> {
    let mut current = value;
    for field in fields {
        current = current.get(field)?;
    }
    current.as_str().map(str::to_owned)
}

fn pull_request_data(pull_request: PullRequest) -> BitbucketPullRequestData {
    BitbucketPullRequestData {
        source: SOURCE_BITBUCKET.to_owned(),
        id: pull_request.id,
        title: pull_request.title,
        description: pull_request.description,
        state: pull_request.state,
        author: BitbucketActorData {
            account_id: pull_request.author.account_id,
            display_name: pull_request.author.display_name,
        },
        source_branch: pull_request.source.branch.name,
        destination_branch: pull_request.destination.branch.name,
        url: pull_request.links.html.href,
        participants: participants_data(pull_request.participants),
    }
}

fn participants_data(
    participants: Vec<bitbucket_adapter::PullRequestParticipant>,
) -> Vec<BitbucketParticipantData> {
    let mut participants = participants
        .into_iter()
        .map(|participant| BitbucketParticipantData {
            account_id: participant.user.account_id,
            display_name: participant.user.display_name,
            role: participant.role,
            approved: participant.approved,
            state: participant.state,
        })
        .collect::<Vec<_>>();
    participants.sort_by(|first, second| {
        (&first.account_id, &first.role).cmp(&(&second.account_id, &second.role))
    });
    participants
}

fn target_from_pull_request(
    request: &BitbucketPullRequestKeyRequest,
    pull_request: &PullRequest,
) -> BitbucketPullRequestTarget {
    target_from_parts(&request.workspace, &request.repository, pull_request)
}

fn target_from_parts(
    workspace: &str,
    repository: &str,
    pull_request: &PullRequest,
) -> BitbucketPullRequestTarget {
    BitbucketPullRequestTarget {
        workspace: workspace.to_owned(),
        repository: repository.to_owned(),
        pull_request_id: pull_request.id,
        url: pull_request.links.html.href.clone(),
        source_branch: pull_request.source.branch.name.clone(),
        destination_branch: pull_request.destination.branch.name.clone(),
        state: pull_request.state,
        revision: pull_request_revision(pull_request),
    }
}

fn pull_request_revision(pull_request: &PullRequest) -> BitbucketPullRequestRevision {
    BitbucketPullRequestRevision {
        title: pull_request.title.clone(),
        description: pull_request.description.clone(),
        source_branch: pull_request.source.branch.name.clone(),
        destination_branch: pull_request.destination.branch.name.clone(),
        state: pull_request.state,
        source_commit: pull_request.source.commit.hash.clone(),
        destination_commit: pull_request.destination.commit.hash.clone(),
        updated_on: pull_request.updated_on.clone(),
        close_source_branch: pull_request.close_source_branch,
        draft: pull_request.draft,
        participants: participants_data(pull_request.participants.clone()),
    }
}

fn verify_expected_revision(
    request: &impl RevisionBoundRequest,
    target: &BitbucketPullRequestTarget,
) -> Result<(), BitbucketBackendError> {
    if request.expected_revision() == Some(&target.revision) {
        return Ok(());
    }
    Err(BitbucketBackendError::StaleConfirmation)
}

fn validate_configured_repository_count(
    configuration: &BitbucketConfiguration,
) -> Result<(), BitbucketBackendError> {
    let valid = configuration
        .workspaces
        .values()
        .all(|repositories| repositories.len() <= configuration.maximum_collection_items);
    if valid {
        return Ok(());
    }
    Err(BitbucketBackendError::InvalidConfiguration)
}

fn key_request<Request: PullRequestLocation>(request: &Request) -> BitbucketPullRequestKeyRequest {
    BitbucketPullRequestKeyRequest {
        workspace: request.workspace().to_owned(),
        repository: request.repository().to_owned(),
        pull_request_id: request.pull_request_id(),
    }
}

trait PullRequestLocation {
    fn workspace(&self) -> &str;
    fn repository(&self) -> &str;
    fn pull_request_id(&self) -> u64;
}

impl PullRequestLocation for BitbucketUpdatePullRequestRequest {
    fn workspace(&self) -> &str {
        &self.workspace
    }
    fn repository(&self) -> &str {
        &self.repository
    }
    fn pull_request_id(&self) -> u64 {
        self.pull_request_id
    }
}

impl PullRequestLocation for BitbucketCommentRequest {
    fn workspace(&self) -> &str {
        &self.workspace
    }
    fn repository(&self) -> &str {
        &self.repository
    }
    fn pull_request_id(&self) -> u64 {
        self.pull_request_id
    }
}

impl PullRequestLocation for BitbucketConfirmedPullRequestRequest {
    fn workspace(&self) -> &str {
        &self.workspace
    }
    fn repository(&self) -> &str {
        &self.repository
    }
    fn pull_request_id(&self) -> u64 {
        self.pull_request_id
    }
}

impl PullRequestLocation for BitbucketMergePullRequestRequest {
    fn workspace(&self) -> &str {
        &self.workspace
    }
    fn repository(&self) -> &str {
        &self.repository
    }
    fn pull_request_id(&self) -> u64 {
        self.pull_request_id
    }
}

impl RevisionBoundRequest for BitbucketUpdatePullRequestRequest {
    fn expected_revision(&self) -> Option<&BitbucketPullRequestRevision> {
        self.expected_revision.as_ref()
    }

    fn set_expected_revision(&mut self, revision: BitbucketPullRequestRevision) {
        self.expected_revision = Some(revision);
    }

    fn confirmed(&self) -> bool {
        self.confirmed
    }

    fn confirmation_token(&self) -> Option<&str> {
        self.confirmation_token.as_deref()
    }
}

impl RevisionBoundRequest for BitbucketCommentRequest {
    fn expected_revision(&self) -> Option<&BitbucketPullRequestRevision> {
        self.expected_revision.as_ref()
    }

    fn set_expected_revision(&mut self, revision: BitbucketPullRequestRevision) {
        self.expected_revision = Some(revision);
    }

    fn confirmed(&self) -> bool {
        self.confirmed
    }

    fn confirmation_token(&self) -> Option<&str> {
        self.confirmation_token.as_deref()
    }
}

impl RevisionBoundRequest for BitbucketConfirmedPullRequestRequest {
    fn expected_revision(&self) -> Option<&BitbucketPullRequestRevision> {
        self.expected_revision.as_ref()
    }

    fn set_expected_revision(&mut self, revision: BitbucketPullRequestRevision) {
        self.expected_revision = Some(revision);
    }

    fn confirmed(&self) -> bool {
        self.confirmed
    }

    fn confirmation_token(&self) -> Option<&str> {
        self.confirmation_token.as_deref()
    }
}

impl RevisionBoundRequest for BitbucketMergePullRequestRequest {
    fn expected_revision(&self) -> Option<&BitbucketPullRequestRevision> {
        self.expected_revision.as_ref()
    }

    fn set_expected_revision(&mut self, revision: BitbucketPullRequestRevision) {
        self.expected_revision = Some(revision);
    }

    fn confirmed(&self) -> bool {
        self.confirmed
    }

    fn confirmation_token(&self) -> Option<&str> {
        self.confirmation_token.as_deref()
    }
}

fn create_input(
    request: &BitbucketCreatePullRequestRequest,
    reviewer_account_ids: Vec<String>,
) -> CreatePullRequest {
    CreatePullRequest {
        title: request.title.clone(),
        description: request.description.clone().unwrap_or_default(),
        source_branch: request.source_branch.clone(),
        destination_branch: request.destination_branch.clone(),
        reviewer_account_ids,
        close_source_branch: request.close_source_branch.unwrap_or(false),
    }
}

fn create_effect(
    request: &BitbucketCreatePullRequestRequest,
    reviewer_account_ids: Vec<String>,
) -> BitbucketPlannedEffect {
    BitbucketPlannedEffect::Create {
        title: request.title.clone(),
        description: request.description.clone().unwrap_or_default(),
        reviewer_account_ids,
        close_source_branch: request.close_source_branch.unwrap_or(false),
    }
}

pub(crate) fn bind_resolved_reviewers(
    mut request: BitbucketCreatePullRequestRequest,
    plan: &BitbucketMutationPlan,
) -> Result<BitbucketCreatePullRequestRequest, BitbucketBackendError> {
    let BitbucketPlannedEffect::Create {
        reviewer_account_ids,
        ..
    } = &plan.effect
    else {
        return Err(BitbucketBackendError::InvalidConfiguration);
    };
    request.resolved_reviewer_account_ids = Some(reviewer_account_ids.clone());
    Ok(request)
}

fn merge_reviewer_account_ids(
    default_reviewer_account_ids: Vec<String>,
    requested_reviewer_account_ids: Option<&[String]>,
) -> Vec<String> {
    let mut reviewer_account_ids = BTreeSet::from_iter(default_reviewer_account_ids);
    reviewer_account_ids.extend(
        requested_reviewer_account_ids
            .unwrap_or_default()
            .iter()
            .cloned(),
    );
    reviewer_account_ids.into_iter().collect()
}

fn update_input(request: &BitbucketUpdatePullRequestRequest) -> UpdatePullRequest {
    UpdatePullRequest {
        title: request.title.clone(),
        description: request.description.clone(),
        destination_branch: request.destination_branch.clone(),
        close_source_branch: request.close_source_branch,
    }
}

fn merge_input(request: &BitbucketMergePullRequestRequest) -> MergePullRequest {
    MergePullRequest {
        message: request.message.clone(),
        close_source_branch: request.close_source_branch.unwrap_or(false),
        merge_strategy: request.merge_strategy,
    }
}

fn merge_effect(request: &BitbucketMergePullRequestRequest) -> BitbucketPlannedEffect {
    BitbucketPlannedEffect::Merge {
        strategy: request.merge_strategy,
        message: request.message.clone(),
        close_source_branch: request.close_source_branch.unwrap_or(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_scope_fails_closed() {
        let service = BitbucketPullRequestService::new(&configuration(), "token".to_owned())
            .expect("fixture is valid");

        assert!(service.ensure_repository("workspace", "allowed").is_ok());
        assert!(matches!(
            service.ensure_repository("workspace", "other"),
            Err(BitbucketBackendError::RepositoryOutsideScope)
        ));
    }

    #[test]
    fn explicit_result_limits_truncate_but_the_safety_ceiling_rejects() {
        let service = BitbucketPullRequestService::new(&configuration(), "token".to_owned())
            .expect("fixture is valid");
        let strict = service.limits(None).expect("strict limits");
        let truncated = service.limits(Some(5)).expect("truncated limits");

        assert_eq!(strict, PageLimits::new(50, 100).expect("limits"));
        assert_eq!(
            service.limits(Some(100)).expect("ceiling limits"),
            PageLimits::new(50, 100).expect("limits")
        );
        assert_eq!(
            truncated,
            PageLimits::new(5, 5).expect("limits").truncate_at_limit()
        );
    }

    #[test]
    fn activity_is_normalized_without_exposing_provider_json() {
        let event = serde_json::json!({
            "comment": {
                "user": {"account_id": "actor-1", "display_name": "Taylor"},
                "created_on": "2026-09-03T12:00:00Z",
                "content": {"raw": "Please cover this case"},
                "links": {"html": {"href": "https://example.test/comment/1"}}
            }
        });

        let normalized = activity_event(&event);

        assert_eq!(normalized.kind, BitbucketActivityKind::Comment);
        assert_eq!(
            normalized.summary.as_deref(),
            Some("Please cover this case")
        );
        assert_eq!(
            normalized.occurred_at.as_deref(),
            Some("2026-09-03T12:00:00Z")
        );
    }

    #[test]
    fn create_preview_exposes_every_applied_value() {
        let request: BitbucketCreatePullRequestRequest =
            serde_json::from_value(serde_json::json!({
                "workspace": "workspace", "repository": "allowed", "title": "Change",
                "description": "Context", "sourceBranch": "feature", "destinationBranch": "main",
                "reviewerAccountIds": ["reviewer"], "closeSourceBranch": true, "confirmed": false
            }))
            .expect("request");
        let effect = serde_json::to_value(create_effect(&request, vec!["reviewer".to_owned()]))
            .expect("effect");
        assert_eq!(effect["description"], "Context");
        assert_eq!(effect["reviewerAccountIds"][0], "reviewer");
        assert_eq!(effect["closeSourceBranch"], true);
    }

    #[test]
    fn reviewers_include_bitbucket_defaults_and_explicit_request_without_duplicates() {
        let reviewers = merge_reviewer_account_ids(
            vec!["default".to_owned(), "shared".to_owned()],
            Some(&["shared".to_owned(), "requested".to_owned()]),
        );

        assert_eq!(reviewers, vec!["default", "requested", "shared"]);
    }

    #[test]
    fn merge_preview_exposes_every_applied_value() {
        let request: BitbucketMergePullRequestRequest = serde_json::from_value(serde_json::json!({
            "workspace": "workspace", "repository": "allowed", "pullRequestId": 7,
            "message": "Ship it", "closeSourceBranch": true, "mergeStrategy": "squash",
            "confirmed": false
        }))
        .expect("request");
        let effect = serde_json::to_value(merge_effect(&request)).expect("effect");
        assert_eq!(effect["message"], "Ship it");
        assert_eq!(effect["closeSourceBranch"], true);
    }

    fn configuration() -> BitbucketConfiguration {
        BitbucketConfiguration {
            email: "person@example.com".to_owned(),
            workspaces: BTreeMap::from([(
                "workspace".to_owned(),
                BTreeSet::from(["allowed".to_owned()]),
            )]),
            request_timeout_seconds: 30,
            page_size: 50,
            maximum_collection_items: 100,
        }
    }
}
