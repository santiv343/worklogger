use time::Date;

use crate::{Duration, IssueKey, Worklog};

pub struct PossibleDuplicatePolicy;

impl PossibleDuplicatePolicy {
    /// Returns true when an existing worklog has the same issue, date and duration.
    #[must_use]
    pub fn matches(
        worklogs: &[Worklog],
        issue_key: &IssueKey,
        date: Date,
        duration: Duration,
        excluded_worklog_id: Option<&str>,
    ) -> bool {
        worklogs.iter().any(|worklog| {
            excluded_worklog_id != Some(worklog.id.as_str())
                && worklog.issue_key == *issue_key
                && worklog.started.date() == date
                && worklog.duration == duration
        })
    }
}
