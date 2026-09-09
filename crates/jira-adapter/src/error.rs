use thiserror::Error;

#[derive(Debug, Error)]
pub enum JiraError {
    #[error("la URL debe ser exactamente https://<sitio>.atlassian.net")]
    InvalidSiteUrl,
    #[error("el correo y el token de Jira son obligatorios")]
    MissingCredentials,
    #[error("la identidad devuelta por Jira no es confiable")]
    InvalidIdentity,
    #[error("the worklog identifier is invalid")]
    InvalidWorklogId,
    #[error("the worklog duration must be greater than zero")]
    InvalidWorklogInput,
    #[error("the issue operation data is invalid")]
    InvalidIssueInput,
    #[error("pagination limits must be greater than zero")]
    InvalidPageLimits,
    #[error("Jira returned inconsistent pagination")]
    InvalidPagination,
    #[error("the result exceeds the configured limit and cannot be presented as complete")]
    CollectionLimitReached,
    #[error("Jira returned an invalid worklog")]
    InvalidWorklog,
    #[error("el worklog pertenece a otra cuenta y no puede modificarse")]
    WorklogOwnershipMismatch,
    #[error("the Jira session is invalid")]
    AuthenticationRequired,
    #[error("the account is not permitted to perform this operation")]
    Forbidden,
    #[error("the board is not associated with a Jira project")]
    BoardProjectUnavailable,
    #[error("the requested resource does not exist or is no longer available")]
    NotFound,
    #[error("Jira temporarily rate limited requests")]
    RateLimited { retry_after_seconds: Option<u64> },
    #[error("Jira is temporarily unavailable")]
    ServerUnavailable,
    #[error("Jira responded with HTTP {0}")]
    HttpStatus(u16),
    #[error("Jira rejected the request (HTTP {status}): {detail}")]
    ProviderRejected { status: u16, detail: String },
    #[error("no fue posible comunicarse con Jira")]
    Transport(#[source] reqwest::Error),
    #[error("Jira returned an invalid response")]
    InvalidResponse(#[source] reqwest::Error),
    #[error("no fue posible construir la solicitud a Jira")]
    InvalidRequestUrl,
    #[error("Jira returned an invalid date")]
    InvalidDate,
}
