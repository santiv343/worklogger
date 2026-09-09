use serde::{Deserialize, Serialize};

use super::HoursError;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct AccountId(String);

impl AccountId {
    /// Creates a normalized Jira account identifier.
    ///
    /// # Errors
    ///
    /// Returns [`HoursError::EmptyAccountId`] when the value is blank.
    pub fn new(value: impl Into<String>) -> Result<Self, HoursError> {
        let normalized = value.into().trim().to_owned();
        if normalized.is_empty() {
            return Err(HoursError::EmptyAccountId);
        }
        Ok(Self(normalized))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
