use std::{convert::Infallible, future::Future};

use hours_core::{
    ConnectionId, DateRange, Duration, ExternalResourceRef, LoadOwnTimeEntries, OwnTimeEntryBatch,
    OwnTimeEntryReader, ProviderSubject, SourceWarning, TimeEntry,
};
use time::{Date, Month, Time, UtcOffset};

#[derive(Clone)]
struct InMemoryOwnTimeEntryReader {
    batch: OwnTimeEntryBatch,
}

impl OwnTimeEntryReader for InMemoryOwnTimeEntryReader {
    type Error = Infallible;

    fn read_own_time_entries(
        &self,
        _period: DateRange,
    ) -> impl Future<Output = Result<OwnTimeEntryBatch, Self::Error>> + Send {
        let batch = self.batch.clone();
        async move { Ok(batch) }
    }
}

#[tokio::test]
async fn load_own_entries_filters_identity_and_period_and_preserves_warnings() {
    let owner = subject("owner");
    let another_user = subject("another");
    let reader = reader(owner.clone(), another_user);

    let loaded = LoadOwnTimeEntries::execute(&reader, week())
        .await
        .expect("in-memory reader cannot fail");

    assert_eq!(loaded.subject, owner);
    assert_eq!(loaded.entries.len(), 1);
    assert_eq!(loaded.entries[0].id, "inside-owner");
    assert_eq!(loaded.warnings.len(), 1);
}

fn reader(owner: ProviderSubject, another_user: ProviderSubject) -> InMemoryOwnTimeEntryReader {
    InMemoryOwnTimeEntryReader {
        batch: OwnTimeEntryBatch {
            subject: owner.clone(),
            entries: vec![
                time_entry("inside-owner", owner.clone(), 1),
                time_entry("inside-other", another_user, 1),
                time_entry("outside-owner", owner, 8),
            ],
            warnings: vec![SourceWarning::new("Una tarea no pudo consultarse")],
        },
    }
}

fn subject(remote_id: &str) -> ProviderSubject {
    ProviderSubject::new(connection(), remote_id, remote_id).expect("valid subject")
}

fn time_entry(id: &str, author: ProviderSubject, day_offset: i64) -> TimeEntry {
    let destination = ExternalResourceRef::new(connection(), "10001", "TASK-1")
        .expect("valid destination")
        .with_web_url("https://tracker.example/items/10001")
        .expect("valid URL");
    TimeEntry {
        id: id.to_owned(),
        destination,
        destination_title: "Trabajo de prueba".to_owned(),
        author,
        started: (week().start() + time::Duration::days(day_offset))
            .with_time(Time::MIDNIGHT)
            .assume_offset(UtcOffset::UTC),
        duration: Duration::from_minutes(60).expect("valid duration"),
        comment: "Trabajo realizado".to_owned(),
    }
}

fn connection() -> ConnectionId {
    ConnectionId::new("tracker:primary").expect("valid connection")
}

fn week() -> DateRange {
    let monday = Date::from_calendar_date(2026, Month::August, 24).expect("valid date");
    DateRange::week_containing(monday)
}
