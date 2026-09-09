use serde::{Deserialize, Serialize};

use super::{ConnectionId, HoursError};

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ExternalResourceRef {
    connection_id: ConnectionId,
    remote_id: String,
    display_id: String,
    web_url: Option<String>,
}

impl ExternalResourceRef {
    /// Creates a resource reference namespaced by its authenticated connection.
    ///
    /// # Errors
    ///
    /// Returns [`HoursError::InvalidExternalResource`] for blank identifiers.
    pub fn new(
        connection_id: ConnectionId,
        remote_id: impl Into<String>,
        display_id: impl Into<String>,
    ) -> Result<Self, HoursError> {
        let remote_id = required_text(remote_id)?;
        let display_id = required_text(display_id)?;
        Ok(Self {
            connection_id,
            remote_id,
            display_id,
            web_url: None,
        })
    }

    /// Adds a provider URL used only for traceability and navigation.
    ///
    /// # Errors
    ///
    /// Returns [`HoursError::InvalidExternalResource`] when the URL is blank.
    pub fn with_web_url(mut self, value: impl Into<String>) -> Result<Self, HoursError> {
        self.web_url = Some(required_text(value)?);
        Ok(self)
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
    pub fn display_id(&self) -> &str {
        &self.display_id
    }

    #[must_use]
    pub fn web_url(&self) -> Option<&str> {
        self.web_url.as_deref()
    }
}

fn required_text(value: impl Into<String>) -> Result<String, HoursError> {
    let normalized = value.into().trim().to_owned();
    if normalized.is_empty() {
        return Err(HoursError::InvalidExternalResource);
    }
    Ok(normalized)
}
