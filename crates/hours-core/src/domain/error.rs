use thiserror::Error;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum HoursError {
    #[error("la conexión no puede estar vacía")]
    EmptyConnectionId,
    #[error("la referencia externa no es válida")]
    InvalidExternalResource,
    #[error("la identidad del proveedor no es válida")]
    InvalidProviderSubject,
    #[error("la cuenta autenticada no puede estar vacía")]
    EmptyAccountId,
    #[error("la fecha inicial no puede ser posterior a la final")]
    InvalidDateRange,
    #[error("la duración debe ser mayor que cero")]
    InvalidDuration,
    #[error("la clave de Jira no es válida")]
    InvalidIssueKey,
    #[error("el objetivo semanal debe ser mayor que cero")]
    InvalidWeeklyTarget,
    #[error("sólo podés modificar horas de tu cuenta Jira autenticada")]
    WorklogOwnershipViolation,
}
