use serde::{Deserialize, Serialize};

use super::HoursError;

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ConnectionId(String);

impl ConnectionId {
    /// Creates the namespace for resources owned by one authenticated connection.
    ///
    /// # Errors
    ///
    /// Returns [`HoursError::EmptyConnectionId`] when the identifier is blank.
    pub fn new(value: impl Into<String>) -> Result<Self, HoursError> {
        let normalized = value.into().trim().to_owned();
        if normalized.is_empty() {
            return Err(HoursError::EmptyConnectionId);
        }
        Ok(Self(normalized))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
