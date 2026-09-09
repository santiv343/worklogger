//! Typed, company-neutral Bitbucket Cloud REST adapter.

use std::collections::HashSet;
use std::time::Duration;

use reqwest::{Client, Method, RequestBuilder, Response, StatusCode, Url, redirect::Policy};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const BITBUCKET_CLOUD_API_ORIGIN: &str = "https://api.bitbucket.org";
const USER_SEGMENT: &str = "user";
const REPOSITORIES_SEGMENT: &str = "repositories";
const PULL_REQUESTS_SEGMENT: &str = "pullrequests";
const EFFECTIVE_DEFAULT_REVIEWERS_SEGMENT: &str = "effective-default-reviewers";
const ACTIVITY_SEGMENT: &str = "activity";
const COMMENTS_SEGMENT: &str = "comments";
const MAXIMUM_PULL_REQUEST_PAGE_SIZE: u16 = 50;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CollectionLimitPolicy {
    RejectOverflow,
    Truncate,
}

#[derive(Debug, thiserror::Error)]
pub enum BitbucketError {
    #[error("el correo y el token de Bitbucket son obligatorios")]
    MissingCredentials,
    #[error("los límites de paginación deben ser mayores que cero")]
    InvalidPageLimits,
    #[error("los datos de la operación de Bitbucket no son válidos")]
    InvalidInput,
    #[error("Bitbucket devolvió una paginación insegura o inconsistente")]
    InvalidPagination,
    #[error("la sesión de Bitbucket no es válida")]
    AuthenticationRequired,
    #[error("la cuenta no tiene permiso para esta operación")]
    Forbidden,
    #[error("el recurso solicitado no existe")]
    NotFound,
    #[error("Bitbucket limitó temporalmente las solicitudes")]
    RateLimited,
    #[error("Bitbucket no está disponible temporalmente")]
    ServerUnavailable,
    #[error("Bitbucket respondió con HTTP {0}")]
    HttpStatus(u16),
    #[error("no fue posible comunicarse con Bitbucket")]
    Transport(#[source] reqwest::Error),
    #[error("Bitbucket devolvió una respuesta inválida")]
    InvalidResponse(#[source] reqwest::Error),
    #[error("Bitbucket devolvió datos incompletos")]
    InvalidResponseShape,
    #[error("no fue posible construir una URL segura de Bitbucket")]
    InvalidUrl,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageLimits {
    page_size: u16,
    maximum_items: usize,
    policy: CollectionLimitPolicy,
}

impl PageLimits {
    /// Creates bounded pagination limits.
    ///
    /// # Errors
    ///
    /// Returns an error when either limit is zero.
    pub fn new(page_size: u16, maximum_items: usize) -> Result<Self, BitbucketError> {
        if page_size == 0 || maximum_items == 0 {
            return Err(BitbucketError::InvalidPageLimits);
        }
        Ok(Self {
            page_size,
            maximum_items,
            policy: CollectionLimitPolicy::RejectOverflow,
        })
    }

    /// Returns limits that stop successfully once the requested item count is reached.
    #[must_use]
    pub const fn truncate_at_limit(self) -> Self {
        Self {
            policy: CollectionLimitPolicy::Truncate,
            ..self
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PullRequestState {
    Open,
    Merged,
    Declined,
    Superseded,
}

impl PullRequestState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "OPEN",
            Self::Merged => "MERGED",
            Self::Declined => "DECLINED",
            Self::Superseded => "SUPERSEDED",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    MergeCommit,
    Squash,
    FastForward,
}

#[derive(Clone, Copy)]
enum PullRequestAction {
    Approve,
    RequestChanges,
    Merge,
    Decline,
}

impl PullRequestAction {
    const fn segment(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::RequestChanges => "request-changes",
            Self::Merge => "merge",
            Self::Decline => "decline",
        }
    }
}

impl MergeStrategy {
    const fn as_str(self) -> &'static str {
        match self {
            Self::MergeCommit => "merge_commit",
            Self::Squash => "squash",
            Self::FastForward => "fast_forward",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BitbucketUser {
    pub account_id: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Repository {
    pub uuid: String,
    pub name: String,
    pub slug: String,
    pub full_name: String,
    pub links: ResourceLinks,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResourceLinks {
    pub html: ResourceLink,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResourceLink {
    pub href: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PullRequest {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub state: PullRequestState,
    pub author: BitbucketUser,
    pub source: PullRequestRef,
    pub destination: PullRequestRef,
    pub close_source_branch: bool,
    pub draft: bool,
    pub updated_on: String,
    #[serde(default)]
    pub participants: Vec<PullRequestParticipant>,
    pub links: ResourceLinks,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PullRequestRef {
    pub branch: Branch,
    pub commit: Commit,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Branch {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Commit {
    pub hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PullRequestParticipant {
    pub user: BitbucketUser,
    pub role: String,
    pub approved: bool,
    pub state: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatePullRequest {
    pub title: String,
    pub description: String,
    pub source_branch: String,
    pub destination_branch: String,
    pub reviewer_account_ids: Vec<String>,
    pub close_source_branch: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdatePullRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub destination_branch: Option<String>,
    pub close_source_branch: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PullRequestComment {
    pub id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergePullRequest {
    pub message: Option<String>,
    pub close_source_branch: bool,
    pub merge_strategy: MergeStrategy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MergePullRequestResult {
    Merged,
    Pending { task_id: String },
}

#[derive(Debug, Deserialize)]
struct Page<Item> {
    values: Vec<Item>,
    next: Option<String>,
}

#[derive(Debug, Serialize)]
struct PullRequestBranch<'request> {
    branch: PullRequestBranchName<'request>,
}

#[derive(Debug, Serialize)]
struct PullRequestBranchName<'request> {
    name: &'request str,
}

#[derive(Debug, Serialize)]
struct PullRequestReviewer<'request> {
    account_id: &'request str,
}

#[derive(Debug, Serialize)]
struct CreatePullRequestBody<'request> {
    title: &'request str,
    description: &'request str,
    source: PullRequestBranch<'request>,
    destination: PullRequestBranch<'request>,
    reviewers: Vec<PullRequestReviewer<'request>>,
    close_source_branch: bool,
}

#[derive(Debug, Serialize)]
struct UpdatePullRequestBody<'request> {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'request str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'request str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    destination: Option<PullRequestBranch<'request>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    close_source_branch: Option<bool>,
}

#[derive(Debug, Serialize)]
struct CommentBody<'request> {
    content: RawContent<'request>,
}

#[derive(Debug, Serialize)]
struct RawContent<'request> {
    raw: &'request str,
}

#[derive(Debug, Serialize)]
struct MergeBody<'request> {
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'request str>,
    close_source_branch: bool,
    merge_strategy: &'request str,
}

#[derive(Debug, Deserialize)]
struct MergeTaskSubmission {
    task_id: String,
}

pub struct BitbucketClient {
    base_url: Url,
    email: String,
    token: String,
    http: Client,
}

impl BitbucketClient {
    /// Creates an authenticated Bitbucket Cloud client.
    ///
    /// # Errors
    ///
    /// Returns an error for missing credentials or an invalid HTTP client.
    pub fn new(
        email: impl Into<String>,
        token: impl Into<String>,
        timeout: Duration,
    ) -> Result<Self, BitbucketError> {
        let email = email.into();
        let token = token.into();
        if email.trim().is_empty() || token.trim().is_empty() {
            return Err(BitbucketError::MissingCredentials);
        }
        let base_url = Url::parse(&format!("{BITBUCKET_CLOUD_API_ORIGIN}/2.0/"))
            .map_err(|_| BitbucketError::InvalidUrl)?;
        Self::build(base_url, email, token, timeout)
    }

    #[cfg(test)]
    fn loopback_for_test(
        origin: &str,
        email: &str,
        token: &str,
        timeout: Duration,
    ) -> Result<Self, BitbucketError> {
        let base_url =
            Url::parse(&format!("{origin}/2.0/")).map_err(|_| BitbucketError::InvalidUrl)?;
        Self::build(base_url, email.to_owned(), token.to_owned(), timeout)
    }

    fn build(
        base_url: Url,
        email: String,
        token: String,
        timeout: Duration,
    ) -> Result<Self, BitbucketError> {
        let http = Client::builder()
            .redirect(Policy::none())
            .timeout(timeout)
            .build()
            .map_err(BitbucketError::Transport)?;
        Ok(Self {
            base_url,
            email,
            token,
            http,
        })
    }

    /// Loads the authenticated Bitbucket account.
    ///
    /// # Errors
    ///
    /// Returns an error when Bitbucket cannot return a valid response.
    pub async fn current_user(&self) -> Result<BitbucketUser, BitbucketError> {
        let url = self.api_url(&[USER_SEGMENT])?;
        let user = self.get_json(url).await?;
        validate_user(&user)?;
        Ok(user)
    }

    /// Lists every repository visible in one workspace within explicit limits.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid scope, unsafe pagination or provider failure.
    pub async fn list_repositories(
        &self,
        workspace: &str,
        limits: PageLimits,
    ) -> Result<Vec<Repository>, BitbucketError> {
        validate_scope(workspace)?;
        let mut url = self.api_url(&[REPOSITORIES_SEGMENT, workspace])?;
        append_page_size(&mut url, limits.page_size);
        self.collect_pages(url, limits).await
    }

    /// Loads one repository by its exact workspace and slug.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid scope or provider failure.
    pub async fn get_repository(
        &self,
        workspace: &str,
        repository: &str,
    ) -> Result<Repository, BitbucketError> {
        validate_scope(workspace)?;
        validate_scope(repository)?;
        let url = self.api_url(&[REPOSITORIES_SEGMENT, workspace, repository])?;
        self.get_json(url).await
    }

    /// Lists pull requests with optional state and escaped text filters.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid filters, unsafe pagination or provider failure.
    pub async fn list_pull_requests(
        &self,
        workspace: &str,
        repository: &str,
        state: Option<PullRequestState>,
        search: Option<&str>,
        limits: PageLimits,
    ) -> Result<Vec<PullRequest>, BitbucketError> {
        let mut url = self.pull_requests_url(workspace, repository, &[])?;
        append_page_size(&mut url, pull_request_page_size(limits.page_size));
        append_pull_request_filters(&mut url, state, search);
        self.collect_pages(url, limits).await
    }

    /// Loads one pull request.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid scope or provider failure.
    pub async fn get_pull_request(
        &self,
        workspace: &str,
        repository: &str,
        pull_request_id: u64,
    ) -> Result<PullRequest, BitbucketError> {
        let id = pull_request_id.to_string();
        let url = self.pull_requests_url(workspace, repository, &[&id])?;
        self.get_json(url).await
    }

    /// Loads the bounded activity feed for one pull request.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid scope, unsafe pagination or provider failure.
    pub async fn get_pull_request_activity(
        &self,
        workspace: &str,
        repository: &str,
        pull_request_id: u64,
        limits: PageLimits,
    ) -> Result<Vec<Value>, BitbucketError> {
        let id = pull_request_id.to_string();
        let mut url = self.pull_requests_url(workspace, repository, &[&id, ACTIVITY_SEGMENT])?;
        append_page_size(&mut url, pull_request_page_size(limits.page_size));
        self.collect_pages(url, limits).await
    }

    /// Lists the reviewers Bitbucket automatically applies to a new pull request.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid scope, pagination or provider rejection.
    pub async fn effective_default_reviewers(
        &self,
        workspace: &str,
        repository: &str,
        limits: PageLimits,
    ) -> Result<Vec<BitbucketUser>, BitbucketError> {
        let mut url = self.effective_default_reviewers_url(workspace, repository)?;
        append_page_size(&mut url, pull_request_page_size(limits.page_size));
        self.collect_pages(url, limits).await
    }

    /// Creates a pull request with explicit branches and reviewers.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid input or provider rejection.
    pub async fn create_pull_request(
        &self,
        workspace: &str,
        repository: &str,
        input: &CreatePullRequest,
    ) -> Result<PullRequest, BitbucketError> {
        validate_create(input)?;
        let url = self.pull_requests_url(workspace, repository, &[])?;
        let body = create_body(input);
        self.send_json(self.auth(Method::POST, url).json(&body))
            .await
    }

    /// Updates explicit pull request fields.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty update or provider rejection.
    pub async fn update_pull_request(
        &self,
        workspace: &str,
        repository: &str,
        pull_request_id: u64,
        input: &UpdatePullRequest,
    ) -> Result<PullRequest, BitbucketError> {
        validate_update(input)?;
        let id = pull_request_id.to_string();
        let url = self.pull_requests_url(workspace, repository, &[&id])?;
        self.send_json(self.auth(Method::PUT, url).json(&update_body(input)))
            .await
    }

    /// Adds a plain-text pull request comment.
    ///
    /// # Errors
    ///
    /// Returns an error for empty text or provider rejection.
    pub async fn add_pull_request_comment(
        &self,
        workspace: &str,
        repository: &str,
        pull_request_id: u64,
        text: &str,
    ) -> Result<PullRequestComment, BitbucketError> {
        validate_text(text)?;
        let id = pull_request_id.to_string();
        let url = self.pull_requests_url(workspace, repository, &[&id, COMMENTS_SEGMENT])?;
        let body = CommentBody {
            content: RawContent { raw: text.trim() },
        };
        self.send_json(self.auth(Method::POST, url).json(&body))
            .await
    }

    /// Approves a pull request as the authenticated account.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider rejects the operation.
    pub async fn approve_pull_request(
        &self,
        workspace: &str,
        repository: &str,
        pull_request_id: u64,
    ) -> Result<(), BitbucketError> {
        self.post_pull_request_action(
            workspace,
            repository,
            pull_request_id,
            PullRequestAction::Approve,
        )
        .await
    }

    /// Removes the authenticated account's approval.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider rejects the operation.
    pub async fn unapprove_pull_request(
        &self,
        workspace: &str,
        repository: &str,
        pull_request_id: u64,
    ) -> Result<(), BitbucketError> {
        self.delete_pull_request_action(
            workspace,
            repository,
            pull_request_id,
            PullRequestAction::Approve,
        )
        .await
    }

    /// Requests changes as the authenticated account.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider rejects the operation.
    pub async fn request_changes(
        &self,
        workspace: &str,
        repository: &str,
        pull_request_id: u64,
    ) -> Result<(), BitbucketError> {
        self.post_pull_request_action(
            workspace,
            repository,
            pull_request_id,
            PullRequestAction::RequestChanges,
        )
        .await
    }

    /// Removes the authenticated account's change request.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider rejects the operation.
    pub async fn remove_change_request(
        &self,
        workspace: &str,
        repository: &str,
        pull_request_id: u64,
    ) -> Result<(), BitbucketError> {
        self.delete_pull_request_action(
            workspace,
            repository,
            pull_request_id,
            PullRequestAction::RequestChanges,
        )
        .await
    }

    /// Merges a pull request using an explicit supported strategy.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider rejects the operation.
    pub async fn merge_pull_request(
        &self,
        workspace: &str,
        repository: &str,
        pull_request_id: u64,
        input: &MergePullRequest,
    ) -> Result<MergePullRequestResult, BitbucketError> {
        let url = self.pull_request_action_url(
            workspace,
            repository,
            pull_request_id,
            PullRequestAction::Merge,
        )?;
        let body = MergeBody {
            message: input.message.as_deref(),
            close_source_branch: input.close_source_branch,
            merge_strategy: input.merge_strategy.as_str(),
        };
        let response = self
            .auth(Method::POST, url)
            .json(&body)
            .send()
            .await
            .map_err(BitbucketError::Transport)?;
        if response.status() == StatusCode::ACCEPTED {
            return response
                .json::<MergeTaskSubmission>()
                .await
                .map_err(BitbucketError::InvalidResponse)
                .and_then(merge_pending_result);
        }
        ensure_success(&response)?;
        Ok(MergePullRequestResult::Merged)
    }

    /// Declines a pull request without merging it.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider rejects the operation.
    pub async fn decline_pull_request(
        &self,
        workspace: &str,
        repository: &str,
        pull_request_id: u64,
    ) -> Result<(), BitbucketError> {
        self.post_pull_request_action(
            workspace,
            repository,
            pull_request_id,
            PullRequestAction::Decline,
        )
        .await
    }

    async fn post_pull_request_action(
        &self,
        workspace: &str,
        repository: &str,
        pull_request_id: u64,
        action: PullRequestAction,
    ) -> Result<(), BitbucketError> {
        let url = self.pull_request_action_url(workspace, repository, pull_request_id, action)?;
        self.send_empty(
            self.auth(Method::POST, url)
                .json(&Value::Object(Map::default())),
        )
        .await
    }

    async fn delete_pull_request_action(
        &self,
        workspace: &str,
        repository: &str,
        pull_request_id: u64,
        action: PullRequestAction,
    ) -> Result<(), BitbucketError> {
        let url = self.pull_request_action_url(workspace, repository, pull_request_id, action)?;
        self.send_empty(self.auth(Method::DELETE, url)).await
    }

    fn pull_request_action_url(
        &self,
        workspace: &str,
        repository: &str,
        pull_request_id: u64,
        action: PullRequestAction,
    ) -> Result<Url, BitbucketError> {
        let id = pull_request_id.to_string();
        self.pull_requests_url(workspace, repository, &[&id, action.segment()])
    }

    async fn collect_pages<Item: DeserializeOwned>(
        &self,
        initial: Url,
        limits: PageLimits,
    ) -> Result<Vec<Item>, BitbucketError> {
        let collection_path = initial.path().to_owned();
        let (mut items, mut next) = (Vec::new(), Some(initial));
        let mut seen_pages = HashSet::new();
        while let Some(url) = next {
            ensure_page_limit(&seen_pages, limits.maximum_items)?;
            remember_page(&mut seen_pages, &url)?;
            let (values, continuation) = self.load_page(url, &collection_path).await?;
            if append_page(&mut items, values, limits, continuation.is_some())? {
                return Ok(items);
            }
            next = continuation;
        }
        Ok(items)
    }

    async fn load_page<Item: DeserializeOwned>(
        &self,
        url: Url,
        collection_path: &str,
    ) -> Result<(Vec<Item>, Option<Url>), BitbucketError> {
        let page: Page<Item> = self.get_json(url).await?;
        let continuation = page
            .next
            .map(|value| self.safe_page_url(&value, collection_path))
            .transpose()?;
        Ok((page.values, continuation))
    }

    async fn get_json<Output: DeserializeOwned>(&self, url: Url) -> Result<Output, BitbucketError> {
        self.send_json(self.auth(Method::GET, url)).await
    }

    async fn send_json<Output: DeserializeOwned>(
        &self,
        request: RequestBuilder,
    ) -> Result<Output, BitbucketError> {
        let response = request.send().await.map_err(BitbucketError::Transport)?;
        ensure_success(&response)?;
        response
            .json()
            .await
            .map_err(BitbucketError::InvalidResponse)
    }

    async fn send_empty(&self, request: RequestBuilder) -> Result<(), BitbucketError> {
        let response = request.send().await.map_err(BitbucketError::Transport)?;
        ensure_success(&response)
    }

    fn auth(&self, method: Method, url: Url) -> RequestBuilder {
        self.http
            .request(method, url)
            .basic_auth(&self.email, Some(&self.token))
    }

    fn pull_requests_url(
        &self,
        workspace: &str,
        repository: &str,
        tail: &[&str],
    ) -> Result<Url, BitbucketError> {
        validate_scope(workspace)?;
        validate_scope(repository)?;
        let mut segments = vec![
            REPOSITORIES_SEGMENT,
            workspace,
            repository,
            PULL_REQUESTS_SEGMENT,
        ];
        segments.extend_from_slice(tail);
        self.api_url(&segments)
    }

    fn effective_default_reviewers_url(
        &self,
        workspace: &str,
        repository: &str,
    ) -> Result<Url, BitbucketError> {
        validate_scope(workspace)?;
        validate_scope(repository)?;
        self.api_url(&[
            REPOSITORIES_SEGMENT,
            workspace,
            repository,
            EFFECTIVE_DEFAULT_REVIEWERS_SEGMENT,
        ])
    }

    fn api_url(&self, segments: &[&str]) -> Result<Url, BitbucketError> {
        let mut url = self.base_url.clone();
        let mut path = url
            .path_segments_mut()
            .map_err(|()| BitbucketError::InvalidUrl)?;
        path.pop_if_empty();
        path.extend(segments);
        drop(path);
        Ok(url)
    }

    fn safe_page_url(&self, value: &str, collection_path: &str) -> Result<Url, BitbucketError> {
        let url = Url::parse(value).map_err(|_| BitbucketError::InvalidPagination)?;
        if url.fragment().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && same_origin(&url, &self.base_url)
            && url.path() == collection_path
        {
            return Ok(url);
        }
        Err(BitbucketError::InvalidPagination)
    }
}

fn merge_pending_result(
    submission: MergeTaskSubmission,
) -> Result<MergePullRequestResult, BitbucketError> {
    if submission.task_id.trim().is_empty() {
        return Err(BitbucketError::InvalidResponseShape);
    }
    Ok(MergePullRequestResult::Pending {
        task_id: submission.task_id,
    })
}

fn ensure_page_limit(
    seen_pages: &HashSet<Url>,
    maximum_pages: usize,
) -> Result<(), BitbucketError> {
    if seen_pages.len() < maximum_pages {
        return Ok(());
    }
    Err(BitbucketError::InvalidPagination)
}

fn remember_page(seen_pages: &mut HashSet<Url>, url: &Url) -> Result<(), BitbucketError> {
    if seen_pages.insert(url.clone()) {
        return Ok(());
    }
    Err(BitbucketError::InvalidPagination)
}

fn same_origin(candidate: &Url, expected: &Url) -> bool {
    candidate.scheme() == expected.scheme()
        && candidate.host_str() == expected.host_str()
        && candidate.port_or_known_default() == expected.port_or_known_default()
}

fn append_page_size(url: &mut Url, page_size: u16) {
    url.query_pairs_mut()
        .append_pair("pagelen", &page_size.to_string());
}

const fn pull_request_page_size(configured: u16) -> u16 {
    if configured < MAXIMUM_PULL_REQUEST_PAGE_SIZE {
        return configured;
    }
    MAXIMUM_PULL_REQUEST_PAGE_SIZE
}

fn append_pull_request_filters(
    url: &mut Url,
    state: Option<PullRequestState>,
    search: Option<&str>,
) {
    if let Some(value) = state {
        url.query_pairs_mut().append_pair("state", value.as_str());
    }
    if let Some(value) = search.filter(|value| !value.trim().is_empty()) {
        url.query_pairs_mut().append_pair("q", &search_query(value));
    }
}

fn search_query(value: &str) -> String {
    let escaped = value.trim().replace('\\', "\\\\").replace('"', "\\\"");
    format!("(title ~ \"{escaped}\" OR source.branch.name ~ \"{escaped}\")")
}

fn validate_scope(value: &str) -> Result<(), BitbucketError> {
    let valid = !value.trim().is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character));
    if valid {
        return Ok(());
    }
    Err(BitbucketError::InvalidInput)
}

fn validate_user(user: &BitbucketUser) -> Result<(), BitbucketError> {
    if user.account_id.trim().is_empty() || user.display_name.trim().is_empty() {
        return Err(BitbucketError::InvalidResponseShape);
    }
    Ok(())
}

fn validate_text(value: &str) -> Result<(), BitbucketError> {
    if value.trim().is_empty() {
        return Err(BitbucketError::InvalidInput);
    }
    Ok(())
}

fn validate_create(input: &CreatePullRequest) -> Result<(), BitbucketError> {
    validate_text(&input.title)?;
    validate_text(&input.source_branch)?;
    validate_text(&input.destination_branch)
}

fn validate_update(input: &UpdatePullRequest) -> Result<(), BitbucketError> {
    let has_value = input.title.is_some()
        || input.description.is_some()
        || input.destination_branch.is_some()
        || input.close_source_branch.is_some();
    if !has_value {
        return Err(BitbucketError::InvalidInput);
    }
    if let Some(title) = &input.title {
        validate_text(title)?;
    }
    Ok(())
}

fn create_body(input: &CreatePullRequest) -> CreatePullRequestBody<'_> {
    CreatePullRequestBody {
        title: input.title.trim(),
        description: input.description.trim(),
        source: branch(&input.source_branch),
        destination: branch(&input.destination_branch),
        reviewers: input
            .reviewer_account_ids
            .iter()
            .map(|account_id| PullRequestReviewer { account_id })
            .collect(),
        close_source_branch: input.close_source_branch,
    }
}

fn update_body(input: &UpdatePullRequest) -> UpdatePullRequestBody<'_> {
    UpdatePullRequestBody {
        title: input.title.as_deref(),
        description: input.description.as_deref(),
        destination: input.destination_branch.as_deref().map(branch),
        close_source_branch: input.close_source_branch,
    }
}

fn branch(name: &str) -> PullRequestBranch<'_> {
    PullRequestBranch {
        branch: PullRequestBranchName { name },
    }
}

fn append_page<Item>(
    target: &mut Vec<Item>,
    values: Vec<Item>,
    limits: PageLimits,
    has_next: bool,
) -> Result<bool, BitbucketError> {
    let remaining = limits.maximum_items.saturating_sub(target.len());
    if values.len() > remaining && limits.policy == CollectionLimitPolicy::RejectOverflow {
        return Err(BitbucketError::InvalidPagination);
    }
    target.extend(values.into_iter().take(remaining));
    let limit_reached = target.len() == limits.maximum_items;
    if limit_reached && has_next && limits.policy == CollectionLimitPolicy::RejectOverflow {
        return Err(BitbucketError::InvalidPagination);
    }
    Ok(limit_reached)
}

fn ensure_success(response: &Response) -> Result<(), BitbucketError> {
    match response.status() {
        status if status.is_success() => Ok(()),
        StatusCode::UNAUTHORIZED => Err(BitbucketError::AuthenticationRequired),
        StatusCode::FORBIDDEN => Err(BitbucketError::Forbidden),
        StatusCode::NOT_FOUND => Err(BitbucketError::NotFound),
        StatusCode::TOO_MANY_REQUESTS => Err(BitbucketError::RateLimited),
        status if status.is_server_error() => Err(BitbucketError::ServerUnavailable),
        status => Err(BitbucketError::HttpStatus(status.as_u16())),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{SocketAddr, TcpListener, TcpStream};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    use reqwest::Url;

    use super::{
        BitbucketClient, BitbucketError, CreatePullRequest, MergePullRequest, MergeStrategy,
        PageLimits, PullRequestState, UpdatePullRequest, remember_page,
    };

    #[test]
    fn pending_merge_returns_the_provider_task_identifier() {
        let result = super::merge_pending_result(super::MergeTaskSubmission {
            task_id: "task-1".to_owned(),
        })
        .expect("task identifier");
        assert_eq!(
            result,
            super::MergePullRequestResult::Pending {
                task_id: "task-1".to_owned()
            }
        );
    }

    #[tokio::test]
    async fn supports_daily_pull_request_operations() {
        let (address, server) = spawn_test_server(test_responses());
        let client = loopback_client(address);
        let limits = PageLimits::new(50, 100).expect("limits");

        exercise_reads(&client, limits).await;
        let pull_request_id = exercise_writes(&client).await;
        exercise_reviews(&client, pull_request_id).await;
        server.join().expect("server completed");
    }

    #[tokio::test]
    async fn caps_pull_request_page_size_to_provider_limit() {
        let path = "/2.0/repositories/workspace/repository/pullrequests?pagelen=50";
        let (address, server) = spawn_test_server(vec![response("GET", path, pull_requests())]);
        let client = loopback_client(address);
        let limits = PageLimits::new(100, 100).expect("limits");

        let pull_requests = client
            .list_pull_requests("workspace", "repository", None, None, limits)
            .await
            .expect("pull requests");

        assert_eq!(pull_requests.len(), 1);
        server.join().expect("server completed");
    }

    #[tokio::test]
    async fn lists_effective_default_reviewers() {
        let path = "/2.0/repositories/workspace/repository/effective-default-reviewers?pagelen=50";
        let body = r#"{"values":[{"account_id":"default","display_name":"Default"}]}"#;
        let (address, server) = spawn_test_server(vec![response("GET", path, body)]);
        let client = loopback_client(address);
        let limits = PageLimits::new(50, 100).expect("limits");

        let reviewers = client
            .effective_default_reviewers("workspace", "repository", limits)
            .await
            .expect("reviewers");

        assert_eq!(reviewers[0].account_id, "default");
        server.join().expect("server completed");
    }

    #[tokio::test]
    async fn truncates_pull_requests_at_the_requested_limit() {
        let (address, server) = spawn_truncated_page_server();
        let client = loopback_client(address);
        let limits = PageLimits::new(1, 1).expect("limits").truncate_at_limit();

        let pull_requests = client
            .list_pull_requests("workspace", "repository", None, None, limits)
            .await
            .expect("pull requests");

        assert_eq!(pull_requests.len(), 1);
        server.join().expect("server completed");
    }

    #[tokio::test]
    async fn rejects_a_continuation_at_the_strict_limit() {
        let (address, server) = spawn_truncated_page_server();
        let client = loopback_client(address);
        let limits = PageLimits::new(1, 1).expect("limits");

        let result = client
            .list_pull_requests("workspace", "repository", None, None, limits)
            .await;

        assert!(matches!(result, Err(BitbucketError::InvalidPagination)));
        server.join().expect("server completed");
    }

    #[test]
    fn rejects_pagination_fragments() {
        let client = BitbucketClient::new("person@example.com", "token", Duration::from_secs(2))
            .expect("client");
        let result = client.safe_page_url(
            "https://api.bitbucket.org/2.0/repositories/example?page=2#variant",
            "/2.0/repositories/example",
        );

        assert!(matches!(result, Err(BitbucketError::InvalidPagination)));
    }

    #[test]
    fn rejects_pagination_user_information() {
        let client = BitbucketClient::new("person@example.com", "token", Duration::from_secs(2))
            .expect("client");
        let path = "/2.0/repositories/example";
        for candidate in [
            "https://variant@api.bitbucket.org/2.0/repositories/example?page=2",
            "https://variant:value@api.bitbucket.org/2.0/repositories/example?page=2",
        ] {
            let result = client.safe_page_url(candidate, path);
            assert!(matches!(result, Err(BitbucketError::InvalidPagination)));
        }
    }

    async fn exercise_reads(client: &BitbucketClient, limits: PageLimits) {
        client.current_user().await.expect("identity");
        client
            .list_repositories("workspace", limits)
            .await
            .expect("repos");
        client
            .get_repository("workspace", "repository")
            .await
            .expect("repository");
        exercise_pull_request_reads(client, limits).await;
    }

    async fn exercise_pull_request_reads(client: &BitbucketClient, limits: PageLimits) {
        client
            .list_pull_requests(
                "workspace",
                "repository",
                Some(PullRequestState::Open),
                None,
                limits,
            )
            .await
            .expect("pull requests");
        client
            .get_pull_request("workspace", "repository", 7)
            .await
            .expect("detail");
        client
            .get_pull_request_activity("workspace", "repository", 7, limits)
            .await
            .expect("activity");
    }

    async fn exercise_writes(client: &BitbucketClient) -> u64 {
        let created = client
            .create_pull_request("workspace", "repository", &create_request())
            .await
            .expect("create");
        exercise_update_and_comment(client, created.id).await;
        created.id
    }

    fn create_request() -> CreatePullRequest {
        CreatePullRequest {
            title: "Useful change".to_owned(),
            description: "Context".to_owned(),
            source_branch: "feature/useful".to_owned(),
            destination_branch: "main".to_owned(),
            reviewer_account_ids: Vec::new(),
            close_source_branch: false,
        }
    }

    async fn exercise_update_and_comment(client: &BitbucketClient, pull_request_id: u64) {
        client
            .update_pull_request(
                "workspace",
                "repository",
                pull_request_id,
                &UpdatePullRequest {
                    title: Some("Better title".to_owned()),
                    description: None,
                    destination_branch: None,
                    close_source_branch: None,
                },
            )
            .await
            .expect("update");
        client
            .add_pull_request_comment("workspace", "repository", pull_request_id, "Looks good")
            .await
            .expect("comment");
    }

    async fn exercise_reviews(client: &BitbucketClient, pull_request_id: u64) {
        client
            .approve_pull_request("workspace", "repository", pull_request_id)
            .await
            .expect("approve");
        client
            .unapprove_pull_request("workspace", "repository", pull_request_id)
            .await
            .expect("unapprove");
        exercise_change_request(client, pull_request_id).await;
        exercise_merge_and_decline(client, pull_request_id).await;
    }

    async fn exercise_change_request(client: &BitbucketClient, pull_request_id: u64) {
        client
            .request_changes("workspace", "repository", pull_request_id)
            .await
            .expect("request changes");
        client
            .remove_change_request("workspace", "repository", pull_request_id)
            .await
            .expect("remove change request");
    }

    async fn exercise_merge_and_decline(client: &BitbucketClient, pull_request_id: u64) {
        let input = MergePullRequest {
            message: Some("Verified".to_owned()),
            close_source_branch: true,
            merge_strategy: MergeStrategy::Squash,
        };
        client
            .merge_pull_request("workspace", "repository", pull_request_id, &input)
            .await
            .expect("merge");
        client
            .decline_pull_request("workspace", "repository", pull_request_id)
            .await
            .expect("decline");
    }

    #[test]
    fn rejects_pagination_that_could_expose_credentials_to_another_origin() {
        let client = BitbucketClient::new("person@example.com", "token", Duration::from_secs(2))
            .expect("client");

        let result = client.safe_page_url(
            "https://attacker.example/2.0/repositories",
            "/2.0/repositories/example",
        );

        assert!(matches!(
            result,
            Err(super::BitbucketError::InvalidPagination)
        ));
    }

    #[test]
    fn rejects_a_repeated_pagination_url() {
        let url =
            Url::parse("https://api.bitbucket.org/2.0/repositories/example").expect("valid URL");
        let mut seen_pages = HashSet::new();

        remember_page(&mut seen_pages, &url).expect("first page");
        let repeated = remember_page(&mut seen_pages, &url);

        assert!(matches!(repeated, Err(BitbucketError::InvalidPagination)));
    }

    #[test]
    fn rejects_pagination_into_another_allowed_origin_collection() {
        let client = BitbucketClient::new("person@example.com", "token", Duration::from_secs(2))
            .expect("client");
        let result = client.safe_page_url(
            "https://api.bitbucket.org/2.0/repositories/other/private/pullrequests?page=2",
            "/2.0/repositories/workspace/repository/pullrequests",
        );

        assert!(matches!(result, Err(BitbucketError::InvalidPagination)));
    }

    struct TestResponse {
        method: &'static str,
        path: &'static str,
        body: &'static str,
        expected_request_body: Option<&'static str>,
    }

    struct TestRequest {
        method: String,
        path: String,
        body: String,
    }

    fn test_responses() -> Vec<TestResponse> {
        let mut responses = discovery_responses();
        responses.extend(pull_request_read_responses());
        responses.extend(mutation_responses());
        responses.extend(approval_responses());
        responses.extend(change_request_responses());
        responses.extend(completion_responses());
        responses
    }

    fn discovery_responses() -> Vec<TestResponse> {
        vec![
            response("GET", "/2.0/user", identity()),
            response(
                "GET",
                "/2.0/repositories/workspace?pagelen=50",
                repositories(),
            ),
            response(
                "GET",
                "/2.0/repositories/workspace/repository",
                repository(),
            ),
        ]
    }

    fn pull_request_read_responses() -> Vec<TestResponse> {
        vec![
            response(
                "GET",
                "/2.0/repositories/workspace/repository/pullrequests?pagelen=50&state=OPEN",
                pull_requests(),
            ),
            response(
                "GET",
                "/2.0/repositories/workspace/repository/pullrequests/7",
                pull_request(),
            ),
            response(
                "GET",
                "/2.0/repositories/workspace/repository/pullrequests/7/activity?pagelen=50",
                r#"{"values":[]}"#,
            ),
        ]
    }

    fn mutation_responses() -> Vec<TestResponse> {
        vec![
            response_with_method_and_body(
                "POST",
                "/2.0/repositories/workspace/repository/pullrequests",
                r#"{"title":"Useful change","description":"Context","source":{"branch":{"name":"feature/useful"}},"destination":{"branch":{"name":"main"}},"reviewers":[],"close_source_branch":false}"#,
                pull_request(),
            ),
            response_with_method_and_body(
                "PUT",
                "/2.0/repositories/workspace/repository/pullrequests/7",
                r#"{"title":"Better title"}"#,
                pull_request(),
            ),
            response_with_method_and_body(
                "POST",
                "/2.0/repositories/workspace/repository/pullrequests/7/comments",
                r#"{"content":{"raw":"Looks good"}}"#,
                r#"{"id":9}"#,
            ),
        ]
    }

    fn approval_responses() -> Vec<TestResponse> {
        vec![
            response(
                "POST",
                "/2.0/repositories/workspace/repository/pullrequests/7/approve",
                "{}",
            ),
            response(
                "DELETE",
                "/2.0/repositories/workspace/repository/pullrequests/7/approve",
                "",
            ),
        ]
    }

    fn change_request_responses() -> Vec<TestResponse> {
        vec![
            response(
                "POST",
                "/2.0/repositories/workspace/repository/pullrequests/7/request-changes",
                "{}",
            ),
            response(
                "DELETE",
                "/2.0/repositories/workspace/repository/pullrequests/7/request-changes",
                "",
            ),
        ]
    }

    fn completion_responses() -> Vec<TestResponse> {
        vec![
            response_with_body(
                "/2.0/repositories/workspace/repository/pullrequests/7/merge",
                r#"{"message":"Verified","close_source_branch":true,"merge_strategy":"squash"}"#,
                "{}",
            ),
            response(
                "POST",
                "/2.0/repositories/workspace/repository/pullrequests/7/decline",
                "{}",
            ),
        ]
    }

    fn loopback_client(address: SocketAddr) -> BitbucketClient {
        BitbucketClient::loopback_for_test(
            &format!("http://{address}"),
            "person@example.com",
            "token",
            Duration::from_secs(2),
        )
        .expect("client")
    }

    const fn response(
        method: &'static str,
        path: &'static str,
        body: &'static str,
    ) -> TestResponse {
        TestResponse {
            method,
            path,
            body,
            expected_request_body: None,
        }
    }

    const fn response_with_body(
        path: &'static str,
        expected_request_body: &'static str,
        body: &'static str,
    ) -> TestResponse {
        response_with_method_and_body("POST", path, expected_request_body, body)
    }

    const fn response_with_method_and_body(
        method: &'static str,
        path: &'static str,
        expected_request_body: &'static str,
        body: &'static str,
    ) -> TestResponse {
        TestResponse {
            method,
            path,
            body,
            expected_request_body: Some(expected_request_body),
        }
    }

    const fn identity() -> &'static str {
        r#"{"account_id":"account-1","display_name":"Person"}"#
    }

    const fn repositories() -> &'static str {
        r#"{"values":[{"uuid":"{repo}","name":"Repository","slug":"repository","full_name":"workspace/repository","links":{"html":{"href":"https://bitbucket.org/workspace/repository"}}}]}"#
    }

    const fn repository() -> &'static str {
        r#"{"uuid":"{repo}","name":"Repository","slug":"repository","full_name":"workspace/repository","links":{"html":{"href":"https://bitbucket.org/workspace/repository"}}}"#
    }

    const fn pull_requests() -> &'static str {
        r#"{"values":[{"id":7,"title":"Useful change","description":"Context","state":"OPEN","author":{"account_id":"account-1","display_name":"Person"},"source":{"branch":{"name":"feature/useful"},"commit":{"hash":"source-commit"}},"destination":{"branch":{"name":"main"},"commit":{"hash":"destination-commit"}},"close_source_branch":true,"draft":false,"updated_on":"2026-09-03T10:00:00+00:00","links":{"html":{"href":"https://bitbucket.org/workspace/repository/pull-requests/7"}}}]}"#
    }

    const fn pull_request() -> &'static str {
        r#"{"id":7,"title":"Useful change","description":"Context","state":"OPEN","author":{"account_id":"account-1","display_name":"Person"},"source":{"branch":{"name":"feature/useful"},"commit":{"hash":"source-commit"}},"destination":{"branch":{"name":"main"},"commit":{"hash":"destination-commit"}},"close_source_branch":true,"draft":false,"updated_on":"2026-09-03T10:00:00+00:00","links":{"html":{"href":"https://bitbucket.org/workspace/repository/pull-requests/7"}}}"#
    }

    fn spawn_test_server(responses: Vec<TestResponse>) -> (SocketAddr, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
        let address = listener.local_addr().expect("listener address");
        let server = thread::spawn(move || serve_requests(&listener, responses));
        (address, server)
    }

    fn spawn_truncated_page_server() -> (SocketAddr, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
        let address = listener.local_addr().expect("listener address");
        let server = thread::spawn(move || serve_truncated_page(&listener, address));
        (address, server)
    }

    fn serve_truncated_page(listener: &TcpListener, address: SocketAddr) {
        let (mut stream, _) = listener.accept().expect("test request");
        let request = read_request(&mut stream);
        let path = "/2.0/repositories/workspace/repository/pullrequests?pagelen=1";
        assert_eq!(request.path, path);
        let mut body = serde_json::from_str::<serde_json::Value>(pull_requests()).expect("JSON");
        let next = format!("http://{address}{path}&page=2");
        body["next"] = serde_json::Value::String(next);
        write_response(&mut stream, &body.to_string());
    }

    fn serve_requests(listener: &TcpListener, responses: Vec<TestResponse>) {
        for response in responses {
            let (mut stream, _) = listener.accept().expect("test request");
            let request = read_request(&mut stream);
            assert_eq!(request.method, response.method);
            assert_eq!(request.path, response.path);
            if let Some(expected) = response.expected_request_body {
                assert_json_eq(&request.body, expected);
            }
            write_response(&mut stream, response.body);
        }
    }

    fn read_request(stream: &mut TcpStream) -> TestRequest {
        let mut reader = BufReader::new(stream.try_clone().expect("cloned stream"));
        let mut request_line = String::new();
        reader.read_line(&mut request_line).expect("request line");
        let mut parts = request_line.split_whitespace();
        let method = parts.next().expect("request method").to_owned();
        let path = parts.next().expect("request path").to_owned();
        let content_length = read_content_length(&mut reader);
        let mut body = vec![0; content_length];
        reader.read_exact(&mut body).expect("request body");
        let body = String::from_utf8(body).expect("UTF-8 request body");
        TestRequest { method, path, body }
    }

    fn assert_json_eq(actual: &str, expected: &str) {
        let actual: serde_json::Value = serde_json::from_str(actual).expect("actual JSON");
        let expected: serde_json::Value = serde_json::from_str(expected).expect("expected JSON");
        assert_eq!(actual, expected);
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

    fn write_response(stream: &mut TcpStream, body: &str) {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).expect("response");
    }
}
