use std::future::Future;

use crate::{DateRange, ExternalResourceRef, ProviderSubject, TimeEntry};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceWarning {
    resource: Option<ExternalResourceRef>,
    message: String,
}

impl SourceWarning {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            resource: None,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn for_resource(resource: ExternalResourceRef, message: impl Into<String>) -> Self {
        Self {
            resource: Some(resource),
            message: message.into(),
        }
    }

    #[must_use]
    pub fn resource(&self) -> Option<&ExternalResourceRef> {
        self.resource.as_ref()
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnTimeEntryBatch {
    pub subject: ProviderSubject,
    pub entries: Vec<TimeEntry>,
    pub warnings: Vec<SourceWarning>,
}

pub trait OwnTimeEntryReader {
    type Error;

    fn read_own_time_entries(
        &self,
        period: DateRange,
    ) -> impl Future<Output = Result<OwnTimeEntryBatch, Self::Error>> + Send;
}
