use crate::{AccountId, HoursError, Worklog};

pub struct WorklogOwnershipPolicy;

impl WorklogOwnershipPolicy {
    /// Ensures a worklog belongs to the currently authenticated Jira account.
    ///
    /// # Errors
    ///
    /// Returns [`HoursError::WorklogOwnershipViolation`] for another author.
    pub fn ensure_can_modify(
        authenticated_account: &AccountId,
        worklog: &Worklog,
    ) -> Result<(), HoursError> {
        if worklog.belongs_to(authenticated_account) {
            return Ok(());
        }
        Err(HoursError::WorklogOwnershipViolation)
    }
}
