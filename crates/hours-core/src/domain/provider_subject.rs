use serde::{Deserialize, Serialize};

use super::{ConnectionId, HoursError};

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ProviderSubject {
    connection_id: ConnectionId,
    remote_id: String,
    display_name: String,
}

impl ProviderSubject {
    /// Creates an identity whose identifier is valid only inside one connection.
    ///
    /// # Errors
    ///
    /// Returns [`HoursError::InvalidProviderSubject`] for blank identity data.
    pub fn new(
        connection_id: ConnectionId,
        remote_id: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Result<Self, HoursError> {
        let remote_id = required_text(remote_id)?;
        let display_name = required_text(display_name)?;
        Ok(Self {
            connection_id,
            remote_id,
            display_name,
        })
    }

    #[must_use]
    pub const fn connection_id(&self) -> &ConnectionId {
        &self.connection_id
    }

    #[must_use]
    pub fn remote_id(&self) -> &str {
        &self.remote_id
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
}

fn required_text(value: impl Into<String>) -> Result<String, HoursError> {
    let normalized = value.into().trim().to_owned();
    if normalized.is_empty() {
        return Err(HoursError::InvalidProviderSubject);
    }
    Ok(normalized)
}
