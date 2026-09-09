use thiserror::Error;

#[derive(Debug, Error)]
pub enum JiraError {
    #[error("la URL debe ser exactamente https://<sitio>.atlassian.net")]
    InvalidSiteUrl,
    #[error("el correo y el token de Jira son obligatorios")]
    MissingCredentials,
    #[error("la identidad devuelta por Jira no es confiable")]
    InvalidIdentity,
    #[error("el identificador de worklog no es válido")]
    InvalidWorklogId,
    #[error("la duración del worklog debe ser mayor que cero")]
    InvalidWorklogInput,
    #[error("los datos de la operación sobre el issue no son válidos")]
    InvalidIssueInput,
    #[error("los límites de paginación deben ser mayores que cero")]
    InvalidPageLimits,
    #[error("Jira devolvió una paginación inconsistente")]
    InvalidPagination,
    #[error("el resultado supera el límite configurado y no puede presentarse como completo")]
    CollectionLimitReached,
    #[error("Jira devolvió un worklog inválido")]
    InvalidWorklog,
    #[error("el worklog pertenece a otra cuenta y no puede modificarse")]
    WorklogOwnershipMismatch,
    #[error("la sesión de Jira no es válida")]
    AuthenticationRequired,
    #[error("la cuenta no tiene permiso para esta operación")]
    Forbidden,
    #[error("el tablero no está asociado a un proyecto de Jira")]
    BoardProjectUnavailable,
    #[error("el recurso solicitado no existe o dejó de estar disponible")]
    NotFound,
    #[error("Jira limitó temporalmente las solicitudes")]
    RateLimited { retry_after_seconds: Option<u64> },
    #[error("Jira no está disponible temporalmente")]
    ServerUnavailable,
    #[error("Jira respondió con HTTP {0}")]
    HttpStatus(u16),
    #[error("Jira rechazó la solicitud (HTTP {status}): {detail}")]
    ProviderRejected { status: u16, detail: String },
    #[error("no fue posible comunicarse con Jira")]
    Transport(#[source] reqwest::Error),
    #[error("Jira devolvió una respuesta inválida")]
    InvalidResponse(#[source] reqwest::Error),
    #[error("no fue posible construir la solicitud a Jira")]
    InvalidRequestUrl,
    #[error("Jira devolvió una fecha inválida")]
    InvalidDate,
}
