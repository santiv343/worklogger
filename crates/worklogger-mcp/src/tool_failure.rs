use schemars::JsonSchema;
use serde::Serialize;

#[cfg(any(feature = "jira", feature = "bitbucket"))]
pub(crate) const RESPONSE_SCHEMA_VERSION: u16 = 1;
#[cfg(feature = "jira")]
pub(crate) const SOURCE_JIRA: &str = "jira";
#[cfg(feature = "bitbucket")]
pub(crate) const SOURCE_BITBUCKET: &str = "bitbucket";
#[cfg(any(feature = "jira", feature = "bitbucket"))]
pub(crate) const ERROR_INVALID_INPUT: &str = "invalid_input";
#[cfg(feature = "jira")]
pub(crate) const ERROR_INVALID_PERIOD: &str = "invalid_period";
#[cfg(any(feature = "jira", feature = "bitbucket"))]
pub(crate) const ERROR_OUTSIDE_SCOPE: &str = "outside_scope";
#[cfg(any(feature = "jira", feature = "bitbucket"))]
pub(crate) const ERROR_CONFIRMATION_REQUIRED: &str = "confirmation_required";
#[cfg(any(feature = "jira", feature = "bitbucket"))]
pub(crate) const ERROR_INVALID_CONFIRMATION: &str = "invalid_confirmation";
#[cfg(any(feature = "jira", feature = "bitbucket"))]
pub(crate) const CONFIRMATION_REQUIRED_MESSAGE: &str =
    "the operation requires confirmed=true after reviewing its effect";
#[cfg(any(feature = "jira", feature = "bitbucket"))]
pub(crate) const ERROR_STALE_CONFIRMATION: &str = "stale_confirmation";
#[cfg(any(feature = "jira", feature = "bitbucket"))]
pub(crate) const ERROR_PROVIDER_UNAVAILABLE: &str = "provider_unavailable";
#[cfg(any(feature = "jira", feature = "bitbucket"))]
pub(crate) const ERROR_PROVIDER_REJECTED: &str = "provider_rejected";
#[cfg(any(feature = "jira", feature = "bitbucket"))]
pub(crate) const ERROR_AUTHENTICATION_REQUIRED: &str = "authentication_required";
#[cfg(any(feature = "jira", feature = "bitbucket"))]
pub(crate) const ERROR_FORBIDDEN: &str = "forbidden";
#[cfg(any(feature = "jira", feature = "bitbucket"))]
pub(crate) const ERROR_NOT_FOUND: &str = "not_found";
#[cfg(any(feature = "jira", feature = "bitbucket"))]
pub(crate) const ERROR_INVALID_PROVIDER_RESPONSE: &str = "invalid_provider_response";

/// Stable error envelope shared by every provider tool.
#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolFailure {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}
