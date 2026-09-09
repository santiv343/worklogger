use thiserror::Error;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum HoursError {
    #[error("the connection cannot be empty")]
    EmptyConnectionId,
    #[error("the external reference is invalid")]
    InvalidExternalResource,
    #[error("the provider identity is invalid")]
    InvalidProviderSubject,
    #[error("the authenticated account cannot be empty")]
    EmptyAccountId,
    #[error("la fecha inicial no puede ser posterior a la final")]
    InvalidDateRange,
    #[error("the duration must be greater than zero")]
    InvalidDuration,
    #[error("the Jira key is invalid")]
    InvalidIssueKey,
    #[error("el objetivo semanal debe ser mayor que cero")]
    InvalidWeeklyTarget,
    #[error("you can modify time only for your authenticated Jira account")]
    WorklogOwnershipViolation,
}
