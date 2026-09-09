use crate::{DateRange, OwnTimeEntryBatch, OwnTimeEntryReader};

pub struct LoadOwnTimeEntries;

impl LoadOwnTimeEntries {
    /// Loads entries and enforces the authenticated subject and requested period.
    ///
    /// # Errors
    ///
    /// Returns the source error without discarding its provider-specific context.
    pub async fn execute<Reader>(
        reader: &Reader,
        period: DateRange,
    ) -> Result<OwnTimeEntryBatch, Reader::Error>
    where
        Reader: OwnTimeEntryReader,
    {
        let mut batch = reader.read_own_time_entries(period).await?;
        let subject = batch.subject.clone();
        batch
            .entries
            .retain(|entry| entry.belongs_to(&subject) && period.contains(entry.started.date()));
        Ok(batch)
    }
}
