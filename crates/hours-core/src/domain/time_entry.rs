use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::{Duration, ExternalResourceRef, ProviderSubject};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TimeEntry {
    pub id: String,
    pub destination: ExternalResourceRef,
    pub destination_title: String,
    pub author: ProviderSubject,
    pub started: OffsetDateTime,
    pub duration: Duration,
    pub comment: String,
}

impl TimeEntry {
    #[must_use]
    pub fn belongs_to(&self, subject: &ProviderSubject) -> bool {
        self.author == *subject
    }
}
