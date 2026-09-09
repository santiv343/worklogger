use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use hours_core::IssueKey;
use jira_adapter::{
    JiraClient, JiraEditMetadataDto, JiraIssueDocumentDto, JiraSiteUrl, PageLimits,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tool_failure::{
    CONFIRMATION_REQUIRED_MESSAGE, ERROR_AUTHENTICATION_REQUIRED, ERROR_CONFIRMATION_REQUIRED,
    ERROR_FORBIDDEN, ERROR_INVALID_INPUT, ERROR_INVALID_PROVIDER_RESPONSE, ERROR_NOT_FOUND,
    ERROR_OUTSIDE_SCOPE, ERROR_PROVIDER_REJECTED, ERROR_PROVIDER_UNAVAILABLE,
    ERROR_STALE_CONFIRMATION, RESPONSE_SCHEMA_VERSION, SOURCE_JIRA,
};
use crate::{JiraConfiguration, MutationConfirmation, ToolFailure};

const DEFAULT_ISSUE_FIELDS: [&str; 7] = [
    "summary",
    "description",
    "status",
    "priority",
    "assignee",
    "issuetype",
    "labels",
];

pub type JiraIssueFuture<'backend, Output> =
    Pin<Box<dyn Future<Output = Result<Output, JiraIssueBackendError>> + Send + 'backend>>;

pub trait JiraIssueBackend: Send + Sync {
    fn get_issue(&self, request: JiraGetIssueRequest) -> JiraIssueFuture<'_, JiraIssueData>;
    fn search_issues(
        &self,
        request: JiraSearchIssuesRequest,
    ) -> JiraIssueFuture<'_, JiraIssueSearchData>;
    fn get_edit_metadata(
        &self,
        request: JiraIssueKeyRequest,
    ) -> JiraIssueFuture<'_, JiraEditMetadataData>;
    fn get_transitions(
        &self,
        request: JiraIssueKeyRequest,
    ) -> JiraIssueFuture<'_, JiraTransitionsData>;
    fn update_issue(
        &self,
        request: JiraUpdateIssueRequest,
    ) -> JiraIssueFuture<'_, JiraMutationData>;
    fn add_comment(&self, request: JiraAddCommentRequest) -> JiraIssueFuture<'_, JiraMutationData>;
    fn transition_issue(
        &self,
        request: JiraTransitionIssueRequest,
    ) -> JiraIssueFuture<'_, JiraMutationData>;
    fn preview_mutation(
        &self,
        request: JiraMutationRequest,
    ) -> JiraIssueFuture<'_, JiraMutationPlan>;
}

#[derive(Clone, Debug)]
pub enum JiraMutationRequest {
    Update(JiraUpdateIssueRequest),
    Comment(JiraAddCommentRequest),
    Transition(JiraTransitionIssueRequest),
}

#[derive(Debug, thiserror::Error)]
pub enum JiraIssueBackendError {
    #[error("the Jira configuration is invalid")]
    InvalidConfiguration,
    #[error("the issue is not in the configured board")]
    IssueOutsideScope,
    #[error("the operation requires explicit confirmation")]
    ConfirmationRequired,
    #[error("the issue changed after confirmation")]
    StaleConfirmation,
    #[error("the Jira session is invalid")]
    AuthenticationRequired,
    #[error("the authenticated account lacks permission for this operation")]
    Forbidden,
    #[error("the requested Jira resource does not exist")]
    NotFound,
    #[error("Jira returned invalid data or inconsistent pagination")]
    InvalidProviderResponse,
    #[error("Jira rejected or could not complete the operation")]
    Provider { retryable: bool },
    #[error("Jira rejected the operation: {detail}")]
    ProviderRejected { detail: String },
}

pub struct JiraIssueService {
    client: JiraClient,
    site: JiraSiteUrl,
    board_id: u64,
    page_size: u16,
    maximum_issue_search_results: usize,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JiraIssueKeyRequest {
    /// Jira issue key, for example PROJECT-123.
    pub key: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JiraGetIssueRequest {
    /// Jira issue key, for example PROJECT-123.
    pub key: String,
    /// Jira field IDs to return. Omit to use a concise standard set.
    pub fields: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JiraSearchIssuesRequest {
    /// JQL evaluated by Jira with the authenticated account's permissions.
    pub jql: String,
    /// Jira field IDs to return. Omit to use a concise standard set.
    pub fields: Option<Vec<String>>,
    /// Maximum complete result count for this call.
    pub max_results: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JiraUpdateIssueRequest {
    /// Jira issue key, for example PROJECT-123.
    pub key: String,
    /// Exact field ID to value map from Jira edit metadata. A plain-text
    /// `description` is encoded as the ADF document required by Jira Cloud.
    pub fields: BTreeMap<String, Value>,
    /// Must be true after the user reviews identity, destination and field changes.
    pub confirmed: bool,
    /// Single-use token returned by the preview for this exact request.
    pub confirmation_token: Option<String>,
    #[serde(skip)]
    #[schemars(skip)]
    expected_current_fields: Option<BTreeMap<String, Value>>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JiraAddCommentRequest {
    /// Jira issue key, for example PROJECT-123.
    pub key: String,
    /// Plain-text comment to add as the authenticated account.
    pub text: String,
    /// Must be true after the user reviews identity, destination and comment.
    pub confirmed: bool,
    /// Single-use token returned by the preview for this exact request.
    pub confirmation_token: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JiraTransitionIssueRequest {
    /// Jira issue key, for example PROJECT-123.
    pub key: String,
    /// Exact transition ID obtained from `jira_get_transitions`.
    pub transition_id: String,
    /// Must be true after the user reviews identity, destination and transition.
    pub confirmed: bool,
    /// Single-use token returned by the preview for this exact request.
    pub confirmation_token: Option<String>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JiraToolResponse<Data> {
    pub schema_version: u16,
    pub success: bool,
    pub data: Option<Data>,
    pub error: Option<ToolFailure>,
    pub confirmation: Option<MutationConfirmation<JiraMutationPlan>>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssueData {
    pub source: String,
    pub id: String,
    pub key: String,
    pub url: String,
    pub fields: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssueSearchData {
    pub source: String,
    pub total: usize,
    pub has_more: bool,
    pub issues: Vec<JiraIssueData>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JiraEditMetadataData {
    pub source: String,
    pub key: String,
    pub url: String,
    pub fields: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JiraTransitionsData {
    pub source: String,
    pub key: String,
    pub url: String,
    pub transitions: Vec<JiraTransitionData>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JiraTransitionData {
    pub id: String,
    pub name: String,
    pub target_id: String,
    pub target_name: String,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JiraMutationData {
    pub source: String,
    pub actor: JiraActorData,
    pub target: JiraMutationTarget,
    pub effect: JiraMutationEffect,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JiraActorData {
    pub account_id: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JiraMutationTarget {
    pub key: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum JiraMutationEffect {
    FieldsUpdated { field_ids: Vec<String> },
    CommentAdded { comment_id: String },
    TransitionApplied { transition_id: String },
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JiraMutationPlan {
    pub source: String,
    pub actor: JiraActorData,
    pub target: JiraMutationTarget,
    pub effect: JiraPlannedEffect,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum JiraPlannedEffect {
    UpdateFields {
        current_fields: BTreeMap<String, Value>,
        requested_fields: BTreeMap<String, Value>,
    },
    AddComment {
        text: String,
    },
    ApplyTransition {
        transition_id: String,
        target_status: String,
    },
}

pub(crate) fn bind_expected_fields(
    mut request: JiraUpdateIssueRequest,
    plan: &JiraMutationPlan,
) -> Result<JiraUpdateIssueRequest, JiraIssueBackendError> {
    let JiraPlannedEffect::UpdateFields { current_fields, .. } = &plan.effect else {
        return Err(JiraIssueBackendError::InvalidConfiguration);
    };
    request.expected_current_fields = Some(current_fields.clone());
    Ok(request)
}

impl JiraIssueService {
    /// Builds Jira issue operations with bounded collection and request limits.
    ///
    /// # Errors
    ///
    /// Returns an error when credentials or connection limits are invalid.
    pub fn new(
        configuration: &JiraConfiguration,
        token: String,
    ) -> Result<Self, JiraIssueBackendError> {
        let site = JiraSiteUrl::parse(&configuration.base_url)
            .map_err(|_| JiraIssueBackendError::InvalidConfiguration)?;
        let timeout = Duration::from_secs(configuration.request_timeout_seconds);
        let client = JiraClient::new(site.clone(), configuration.email.clone(), token, timeout)
            .map_err(|_| JiraIssueBackendError::InvalidConfiguration)?;
        Ok(Self {
            client,
            site,
            board_id: configuration.board_id,
            page_size: configuration.page_size,
            maximum_issue_search_results: configuration.maximum_issue_search_results,
        })
    }

    async fn load_issue(
        &self,
        request: JiraGetIssueRequest,
    ) -> Result<JiraIssueData, JiraIssueBackendError> {
        let key = issue_key(&request.key)?;
        self.ensure_issue_scope(&key).await?;
        let fields = requested_fields(request.fields);
        let issue = self
            .client
            .get_issue_detail(&key, &fields)
            .await
            .map_err(|error| provider_error(&error))?;
        self.issue_data(issue)
    }

    async fn load_search(
        &self,
        request: JiraSearchIssuesRequest,
    ) -> Result<JiraIssueSearchData, JiraIssueBackendError> {
        let limits = self.search_limits(request.max_results)?;
        let fields = requested_fields(request.fields);
        let result = self
            .client
            .search_board_issue_documents_limited(self.board_id, &request.jql, &fields, limits)
            .await
            .map_err(|error| provider_error(&error))?;
        self.issue_search_data(result.issues, result.has_more)
    }

    fn issue_search_data(
        &self,
        issues: Vec<JiraIssueDocumentDto>,
        has_more: bool,
    ) -> Result<JiraIssueSearchData, JiraIssueBackendError> {
        let issues = issues
            .into_iter()
            .map(|issue| self.issue_data(issue))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(JiraIssueSearchData {
            source: SOURCE_JIRA.to_owned(),
            total: issues.len(),
            has_more,
            issues,
        })
    }

    async fn load_edit_metadata(
        &self,
        request: JiraIssueKeyRequest,
    ) -> Result<JiraEditMetadataData, JiraIssueBackendError> {
        let key = issue_key(&request.key)?;
        self.ensure_issue_scope(&key).await?;
        let metadata = self
            .client
            .get_issue_edit_metadata(&key)
            .await
            .map_err(|error| provider_error(&error))?;
        Ok(self.edit_metadata_data(&key, metadata))
    }

    async fn load_transitions(
        &self,
        request: JiraIssueKeyRequest,
    ) -> Result<JiraTransitionsData, JiraIssueBackendError> {
        let key = issue_key(&request.key)?;
        self.ensure_issue_scope(&key).await?;
        let transitions = self
            .client
            .get_issue_transitions(&key)
            .await
            .map_err(|error| provider_error(&error))?;
        let transitions = transitions.into_iter().map(transition_data).collect();
        Ok(JiraTransitionsData {
            source: SOURCE_JIRA.to_owned(),
            key: key.as_str().to_owned(),
            url: self.issue_url(&key),
            transitions,
        })
    }

    async fn apply_update(
        &self,
        request: JiraUpdateIssueRequest,
    ) -> Result<JiraMutationData, JiraIssueBackendError> {
        require_confirmation(request.confirmed)?;
        let key = issue_key(&request.key)?;
        self.ensure_issue_scope(&key).await?;
        let field_ids = field_ids(&request.fields)?;
        let actor = self.load_actor().await?;
        self.verify_expected_fields(&key, &request).await?;
        self.client
            .update_issue(&key, &request.fields)
            .await
            .map_err(|error| provider_error(&error))?;
        Ok(self.mutation_data(&key, actor, JiraMutationEffect::FieldsUpdated { field_ids }))
    }

    async fn verify_expected_fields(
        &self,
        key: &IssueKey,
        request: &JiraUpdateIssueRequest,
    ) -> Result<(), JiraIssueBackendError> {
        let fields = field_ids(&request.fields)?;
        let current = self
            .client
            .get_issue_detail(key, &fields)
            .await
            .map_err(|error| provider_error(&error))?;
        if request.expected_current_fields.as_ref() == Some(&current.fields) {
            return Ok(());
        }
        Err(JiraIssueBackendError::StaleConfirmation)
    }

    async fn apply_comment(
        &self,
        request: JiraAddCommentRequest,
    ) -> Result<JiraMutationData, JiraIssueBackendError> {
        require_confirmation(request.confirmed)?;
        let key = issue_key(&request.key)?;
        self.ensure_issue_scope(&key).await?;
        let actor = self.load_actor().await?;
        let comment_id = self.add_comment(&key, &request.text).await?;
        Ok(self.mutation_data(&key, actor, JiraMutationEffect::CommentAdded { comment_id }))
    }

    async fn add_comment(
        &self,
        key: &IssueKey,
        text: &str,
    ) -> Result<String, JiraIssueBackendError> {
        self.client
            .add_issue_comment(key, text)
            .await
            .map(|comment| comment.id)
            .map_err(|error| provider_error(&error))
    }

    async fn apply_transition(
        &self,
        request: JiraTransitionIssueRequest,
    ) -> Result<JiraMutationData, JiraIssueBackendError> {
        require_confirmation(request.confirmed)?;
        let key = issue_key(&request.key)?;
        self.ensure_issue_scope(&key).await?;
        let actor = self.load_actor().await?;
        self.client
            .transition_issue(&key, &request.transition_id)
            .await
            .map_err(|error| provider_error(&error))?;
        Ok(self.mutation_data(
            &key,
            actor,
            JiraMutationEffect::TransitionApplied {
                transition_id: request.transition_id,
            },
        ))
    }

    async fn preview(
        &self,
        request: JiraMutationRequest,
    ) -> Result<JiraMutationPlan, JiraIssueBackendError> {
        match request {
            JiraMutationRequest::Update(request) => self.preview_update(&request).await,
            JiraMutationRequest::Comment(request) => self.preview_comment(&request).await,
            JiraMutationRequest::Transition(request) => self.preview_transition(&request).await,
        }
    }

    async fn preview_update(
        &self,
        request: &JiraUpdateIssueRequest,
    ) -> Result<JiraMutationPlan, JiraIssueBackendError> {
        let key = issue_key(&request.key)?;
        self.ensure_issue_scope(&key).await?;
        let fields = field_ids(&request.fields)?;
        let issue = self
            .client
            .get_issue_detail(&key, &fields)
            .await
            .map_err(|error| provider_error(&error))?;
        let effect = JiraPlannedEffect::UpdateFields {
            current_fields: issue.fields,
            requested_fields: request.fields.clone(),
        };
        self.mutation_plan(&key, effect).await
    }

    async fn preview_comment(
        &self,
        request: &JiraAddCommentRequest,
    ) -> Result<JiraMutationPlan, JiraIssueBackendError> {
        let key = issue_key(&request.key)?;
        self.ensure_issue_scope(&key).await?;
        if request.text.trim().is_empty() {
            return Err(JiraIssueBackendError::InvalidConfiguration);
        }
        let effect = JiraPlannedEffect::AddComment {
            text: request.text.trim().to_owned(),
        };
        self.mutation_plan(&key, effect).await
    }

    async fn preview_transition(
        &self,
        request: &JiraTransitionIssueRequest,
    ) -> Result<JiraMutationPlan, JiraIssueBackendError> {
        let key = issue_key(&request.key)?;
        self.ensure_issue_scope(&key).await?;
        let effect = self.transition_effect(request).await?;
        self.mutation_plan(&key, effect).await
    }

    async fn transition_effect(
        &self,
        request: &JiraTransitionIssueRequest,
    ) -> Result<JiraPlannedEffect, JiraIssueBackendError> {
        let transitions = self
            .client
            .get_issue_transitions(&issue_key(&request.key)?)
            .await
            .map_err(|error| provider_error(&error))?;
        let transition = transitions
            .into_iter()
            .find(|transition| transition.id == request.transition_id)
            .ok_or(JiraIssueBackendError::InvalidConfiguration)?;
        Ok(JiraPlannedEffect::ApplyTransition {
            transition_id: transition.id,
            target_status: transition.to.name,
        })
    }

    async fn mutation_plan(
        &self,
        key: &IssueKey,
        effect: JiraPlannedEffect,
    ) -> Result<JiraMutationPlan, JiraIssueBackendError> {
        Ok(JiraMutationPlan {
            source: SOURCE_JIRA.to_owned(),
            actor: self.load_actor().await?,
            target: JiraMutationTarget {
                key: key.as_str().to_owned(),
                url: self.issue_url(key),
            },
            effect,
        })
    }

    async fn load_actor(&self) -> Result<JiraActorData, JiraIssueBackendError> {
        let identity = self
            .client
            .current_user()
            .await
            .map_err(|error| provider_error(&error))?;
        Ok(JiraActorData {
            account_id: identity.account_id,
            display_name: identity.display_name,
        })
    }

    async fn ensure_issue_scope(&self, key: &IssueKey) -> Result<(), JiraIssueBackendError> {
        let limits = self.scope_limits()?;
        let in_scope = self
            .client
            .board_contains_issue(self.board_id, key, limits)
            .await
            .map_err(|error| provider_error(&error))?;
        ensure_issue_in_scope(in_scope)
    }

    fn scope_limits(&self) -> Result<PageLimits, JiraIssueBackendError> {
        self.search_limits(None)
    }

    fn issue_data(
        &self,
        issue: JiraIssueDocumentDto,
    ) -> Result<JiraIssueData, JiraIssueBackendError> {
        let key = issue_key(&issue.key)?;
        Ok(JiraIssueData {
            source: SOURCE_JIRA.to_owned(),
            id: issue.id,
            key: issue.key,
            url: self.issue_url(&key),
            fields: issue.fields,
        })
    }

    fn edit_metadata_data(
        &self,
        key: &IssueKey,
        metadata: JiraEditMetadataDto,
    ) -> JiraEditMetadataData {
        JiraEditMetadataData {
            source: SOURCE_JIRA.to_owned(),
            key: key.as_str().to_owned(),
            url: self.issue_url(key),
            fields: metadata.fields,
        }
    }

    fn mutation_data(
        &self,
        key: &IssueKey,
        actor: JiraActorData,
        effect: JiraMutationEffect,
    ) -> JiraMutationData {
        JiraMutationData {
            source: SOURCE_JIRA.to_owned(),
            actor,
            target: JiraMutationTarget {
                key: key.as_str().to_owned(),
                url: self.issue_url(key),
            },
            effect,
        }
    }

    fn issue_url(&self, key: &IssueKey) -> String {
        self.site.issue_browser_url(key)
    }

    fn search_limits(&self, requested: Option<usize>) -> Result<PageLimits, JiraIssueBackendError> {
        let maximum = requested
            .unwrap_or(self.maximum_issue_search_results)
            .min(self.maximum_issue_search_results);
        let page_size = usize::from(self.page_size).min(maximum);
        let page_size =
            u16::try_from(page_size).map_err(|_| JiraIssueBackendError::InvalidConfiguration)?;
        PageLimits::new(page_size, maximum).map_err(|_| JiraIssueBackendError::InvalidConfiguration)
    }
}

impl JiraIssueBackend for JiraIssueService {
    fn get_issue(&self, request: JiraGetIssueRequest) -> JiraIssueFuture<'_, JiraIssueData> {
        Box::pin(self.load_issue(request))
    }

    fn search_issues(
        &self,
        request: JiraSearchIssuesRequest,
    ) -> JiraIssueFuture<'_, JiraIssueSearchData> {
        Box::pin(self.load_search(request))
    }

    fn get_edit_metadata(
        &self,
        request: JiraIssueKeyRequest,
    ) -> JiraIssueFuture<'_, JiraEditMetadataData> {
        Box::pin(self.load_edit_metadata(request))
    }

    fn get_transitions(
        &self,
        request: JiraIssueKeyRequest,
    ) -> JiraIssueFuture<'_, JiraTransitionsData> {
        Box::pin(self.load_transitions(request))
    }

    fn update_issue(
        &self,
        request: JiraUpdateIssueRequest,
    ) -> JiraIssueFuture<'_, JiraMutationData> {
        Box::pin(self.apply_update(request))
    }

    fn add_comment(&self, request: JiraAddCommentRequest) -> JiraIssueFuture<'_, JiraMutationData> {
        Box::pin(self.apply_comment(request))
    }

    fn transition_issue(
        &self,
        request: JiraTransitionIssueRequest,
    ) -> JiraIssueFuture<'_, JiraMutationData> {
        Box::pin(self.apply_transition(request))
    }

    fn preview_mutation(
        &self,
        request: JiraMutationRequest,
    ) -> JiraIssueFuture<'_, JiraMutationPlan> {
        Box::pin(self.preview(request))
    }
}

impl<Data> JiraToolResponse<Data> {
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
        confirmation: MutationConfirmation<JiraMutationPlan>,
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
pub fn jira_backend_failure(error: &JiraIssueBackendError) -> ToolFailure {
    let code = match error {
        JiraIssueBackendError::InvalidConfiguration => ERROR_INVALID_INPUT,
        JiraIssueBackendError::IssueOutsideScope => ERROR_OUTSIDE_SCOPE,
        JiraIssueBackendError::ConfirmationRequired => ERROR_CONFIRMATION_REQUIRED,
        JiraIssueBackendError::StaleConfirmation => ERROR_STALE_CONFIRMATION,
        JiraIssueBackendError::AuthenticationRequired => ERROR_AUTHENTICATION_REQUIRED,
        JiraIssueBackendError::Forbidden => ERROR_FORBIDDEN,
        JiraIssueBackendError::NotFound => ERROR_NOT_FOUND,
        JiraIssueBackendError::InvalidProviderResponse => ERROR_INVALID_PROVIDER_RESPONSE,
        JiraIssueBackendError::ProviderRejected { .. } => ERROR_PROVIDER_REJECTED,
        JiraIssueBackendError::Provider { .. } => ERROR_PROVIDER_UNAVAILABLE,
    };
    ToolFailure {
        code: code.to_owned(),
        message: error.to_string(),
        retryable: matches!(error, JiraIssueBackendError::Provider { retryable: true }),
    }
}

fn requested_fields(fields: Option<Vec<String>>) -> Vec<String> {
    fields.unwrap_or_else(|| DEFAULT_ISSUE_FIELDS.map(str::to_owned).to_vec())
}

fn ensure_issue_in_scope(in_scope: bool) -> Result<(), JiraIssueBackendError> {
    if in_scope {
        return Ok(());
    }
    Err(JiraIssueBackendError::IssueOutsideScope)
}

fn issue_key(value: &str) -> Result<IssueKey, JiraIssueBackendError> {
    IssueKey::new(value).map_err(|_| JiraIssueBackendError::InvalidConfiguration)
}

fn field_ids(fields: &BTreeMap<String, Value>) -> Result<Vec<String>, JiraIssueBackendError> {
    if fields.is_empty() {
        return Err(JiraIssueBackendError::InvalidConfiguration);
    }
    Ok(fields.keys().cloned().collect())
}

fn require_confirmation(confirmed: bool) -> Result<(), JiraIssueBackendError> {
    if confirmed {
        return Ok(());
    }
    Err(JiraIssueBackendError::ConfirmationRequired)
}

pub(crate) fn provider_error(error: &jira_adapter::JiraError) -> JiraIssueBackendError {
    if jira_invalid_configuration(error) {
        return JiraIssueBackendError::InvalidConfiguration;
    }
    if let Some(access_error) = jira_access_error(error) {
        return access_error;
    }
    if jira_invalid_response(error) {
        return JiraIssueBackendError::InvalidProviderResponse;
    }
    if jira_retryable(error) {
        return JiraIssueBackendError::Provider { retryable: true };
    }
    jira_rejection(error).unwrap_or_else(|| JiraIssueBackendError::ProviderRejected {
        detail: error.to_string(),
    })
}

fn jira_rejection(error: &jira_adapter::JiraError) -> Option<JiraIssueBackendError> {
    if let jira_adapter::JiraError::ProviderRejected { detail, .. } = error {
        return Some(JiraIssueBackendError::ProviderRejected {
            detail: detail.clone(),
        });
    }
    None
}

fn jira_invalid_configuration(error: &jira_adapter::JiraError) -> bool {
    matches!(
        error,
        jira_adapter::JiraError::InvalidSiteUrl
            | jira_adapter::JiraError::MissingCredentials
            | jira_adapter::JiraError::InvalidIssueInput
            | jira_adapter::JiraError::InvalidPageLimits
            | jira_adapter::JiraError::InvalidRequestUrl
    )
}

fn jira_access_error(error: &jira_adapter::JiraError) -> Option<JiraIssueBackendError> {
    match error {
        jira_adapter::JiraError::AuthenticationRequired => {
            Some(JiraIssueBackendError::AuthenticationRequired)
        }
        jira_adapter::JiraError::Forbidden => Some(JiraIssueBackendError::Forbidden),
        jira_adapter::JiraError::NotFound => Some(JiraIssueBackendError::NotFound),
        _ => None,
    }
}

fn jira_invalid_response(error: &jira_adapter::JiraError) -> bool {
    matches!(
        error,
        jira_adapter::JiraError::InvalidPagination
            | jira_adapter::JiraError::CollectionLimitReached
            | jira_adapter::JiraError::InvalidIdentity
            | jira_adapter::JiraError::InvalidWorklog
            | jira_adapter::JiraError::InvalidDate
            | jira_adapter::JiraError::InvalidResponse(_)
    )
}

fn jira_retryable(error: &jira_adapter::JiraError) -> bool {
    matches!(
        error,
        jira_adapter::JiraError::RateLimited { .. }
            | jira_adapter::JiraError::ServerUnavailable
            | jira_adapter::JiraError::Transport(_)
    )
}

fn transition_data(transition: jira_adapter::JiraTransitionDto) -> JiraTransitionData {
    JiraTransitionData {
        id: transition.id,
        name: transition.name,
        target_id: transition.to.id,
        target_name: transition.to.name,
    }
}

#[must_use]
pub fn confirmation_failure() -> ToolFailure {
    ToolFailure {
        code: ERROR_CONFIRMATION_REQUIRED.to_owned(),
        message: CONFIRMATION_REQUIRED_MESSAGE.to_owned(),
        retryable: false,
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;

    const LIVE_TEST_TIMEOUT_SECONDS: u64 = 30;
    const LIVE_TEST_PAGE_SIZE: u16 = 100;

    #[test]
    fn preserves_provider_rejection_detail() {
        let error = jira_adapter::JiraError::ProviderRejected {
            status: 400,
            detail: "description: must use Atlassian Document Format".to_owned(),
        };
        let failure = jira_backend_failure(&provider_error(&error));
        assert_eq!(failure.code, ERROR_PROVIDER_REJECTED);
        assert!(
            failure
                .message
                .contains("must use Atlassian Document Format")
        );
        assert!(!failure.retryable);
    }
    const LIVE_TEST_MAXIMUM_ITEMS: usize = 2_000;

    #[test]
    fn issue_search_and_scope_use_the_dedicated_configured_ceiling() {
        let configuration = JiraConfiguration {
            base_url: "https://example.atlassian.net".to_owned(),
            email: "person@example.com".to_owned(),
            board_id: 42,
            request_timeout_seconds: 30,
            page_size: 50,
            maximum_collection_items: 100,
            maximum_issue_search_results: 25,
            hours: None,
        };
        let service = JiraIssueService::new(&configuration, "token".to_owned())
            .expect("fixture service is valid");

        assert_eq!(
            service.scope_limits().expect("scope limits are valid"),
            PageLimits::new(25, 25).expect("expected limits are valid")
        );
        assert_eq!(
            service
                .search_limits(Some(10_000))
                .expect("requested limit is bounded"),
            PageLimits::new(25, 25).expect("expected limits are valid")
        );
    }

    #[test]
    fn issue_missing_from_the_board_fails_closed() {
        let result = ensure_issue_in_scope(false);

        assert!(matches!(
            result,
            Err(JiraIssueBackendError::IssueOutsideScope)
        ));
    }

    #[tokio::test]
    #[ignore = "requires explicit read-only Jira test environment variables"]
    async fn live_get_issue_contract_returns_a_scoped_issue() {
        let configuration = live_test_configuration();
        let token = required_environment("WORKLOGGER_TEST_JIRA_TOKEN");
        let key = required_environment("WORKLOGGER_TEST_JIRA_ISSUE_KEY");
        let service = JiraIssueService::new(&configuration, token).expect("test service is valid");
        let issue = service
            .load_issue(JiraGetIssueRequest { key, fields: None })
            .await;

        assert!(issue.is_ok(), "live get issue failed: {issue:?}");
    }

    #[tokio::test]
    #[ignore = "requires explicit read-only Jira test environment variables"]
    async fn live_bounded_search_returns_the_requested_page() {
        let configuration = live_test_configuration();
        let token = required_environment("WORKLOGGER_TEST_JIRA_TOKEN");
        let key = required_environment("WORKLOGGER_TEST_JIRA_ISSUE_KEY");
        let service = JiraIssueService::new(&configuration, token).expect("test service is valid");
        let result = service
            .load_search(JiraSearchIssuesRequest {
                jql: format!("key = {key}"),
                fields: Some(vec!["summary".to_owned()]),
                max_results: Some(1),
            })
            .await
            .expect("bounded search");

        assert_eq!(result.issues.len(), 1);
        assert_eq!(result.issues[0].key, key);
    }

    fn live_test_configuration() -> JiraConfiguration {
        JiraConfiguration {
            base_url: required_environment("WORKLOGGER_TEST_JIRA_URL"),
            email: required_environment("WORKLOGGER_TEST_JIRA_EMAIL"),
            board_id: required_environment("WORKLOGGER_TEST_JIRA_BOARD_ID")
                .parse::<u64>()
                .expect("test board ID is numeric"),
            request_timeout_seconds: LIVE_TEST_TIMEOUT_SECONDS,
            page_size: LIVE_TEST_PAGE_SIZE,
            maximum_collection_items: LIVE_TEST_MAXIMUM_ITEMS,
            maximum_issue_search_results: LIVE_TEST_MAXIMUM_ITEMS,
            hours: None,
        }
    }

    fn required_environment(name: &str) -> String {
        env::var(name).unwrap_or_else(|_| panic!("missing {name}"))
    }
}
