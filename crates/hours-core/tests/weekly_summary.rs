use hours_core::{
    AccountId, DateRange, Duration, IssueKey, PossibleDuplicatePolicy, WeeklySummary, WeeklyTarget,
    Worklog, WorklogOwnershipPolicy,
};
use time::{Date, Month, Time, UtcOffset};

#[test]
fn weekly_summary_uses_only_authenticated_users_worklogs_in_range() {
    let owner = AccountId::new("owner-account").expect("valid owner");
    let another_user = AccountId::new("another-account").expect("valid user");
    let period = week();
    let worklogs = vec![
        worklog("1", &owner, 0, 120),
        worklog("2", &owner, 1, 90),
        worklog("3", &another_user, 1, 600),
        worklog("4", &owner, 8, 600),
    ];

    let summary = WeeklySummary::calculate(&owner, period, target(), &worklogs);

    assert_eq!(summary.loaded_seconds, 12_600);
    assert_eq!(summary.missing_seconds, 95_400);
    assert_eq!(summary.tasks.len(), 1);
    assert_eq!(summary.days[0].duration_seconds, 7_200);
    assert_eq!(summary.days[1].duration_seconds, 5_400);
}

#[test]
fn progress_is_capped_and_missing_never_becomes_negative() {
    let owner = AccountId::new("owner-account").expect("valid owner");
    let worklogs = vec![worklog("1", &owner, 0, 1_900)];

    let summary = WeeklySummary::calculate(&owner, week(), target(), &worklogs);

    assert_eq!(summary.progress_percent, 100);
    assert_eq!(summary.missing_seconds, 0);
}

#[test]
fn invalid_domain_values_are_rejected() {
    assert!(AccountId::new(" ").is_err());
    assert!(IssueKey::new("not a key").is_err());
    assert!(Duration::from_minutes(0).is_err());
    assert!(WeeklyTarget::from_minutes(0).is_err());
}

#[test]
fn ownership_policy_rejects_another_users_worklog() {
    let owner = AccountId::new("owner-account").expect("valid owner");
    let another_user = AccountId::new("another-account").expect("valid user");
    let worklog = worklog("1", &another_user, 0, 60);

    let result = WorklogOwnershipPolicy::ensure_can_modify(&owner, &worklog);

    assert!(result.is_err());
}

#[test]
fn possible_duplicate_excludes_the_worklog_being_edited() {
    let owner = AccountId::new("owner-account").expect("valid owner");
    let existing = worklog("existing", &owner, 1, 90);
    let duration = Duration::from_minutes(90).expect("valid duration");
    let date = existing.started.date();
    assert!(PossibleDuplicatePolicy::matches(
        std::slice::from_ref(&existing),
        &existing.issue_key,
        date,
        duration,
        None,
    ));
    assert!(!PossibleDuplicatePolicy::matches(
        std::slice::from_ref(&existing),
        &existing.issue_key,
        date,
        duration,
        Some("existing"),
    ));
}

#[test]
fn summary_covers_every_day_in_a_custom_range() {
    let owner = AccountId::new("owner-account").expect("valid owner");
    let start = week().start();
    let period = DateRange::new(start, start + time::Duration::days(9)).expect("range");
    let summary = WeeklySummary::calculate(&owner, period, target(), &[]);
    assert_eq!(period.day_count(), 10);
    assert_eq!(summary.days.len(), 10);
}

fn week() -> DateRange {
    let monday = Date::from_calendar_date(2026, Month::August, 24).expect("valid date");
    DateRange::week_containing(monday)
}

fn target() -> WeeklyTarget {
    WeeklyTarget::from_minutes(1_800).expect("valid target")
}

fn worklog(id: &str, author: &AccountId, day_offset: i64, duration_minutes: u32) -> Worklog {
    let date = week().start() + time::Duration::days(day_offset);
    let started = date.with_time(Time::MIDNIGHT).assume_offset(UtcOffset::UTC);
    Worklog {
        id: id.to_owned(),
        issue_key: IssueKey::new("DEMO-101").expect("valid key"),
        issue_summary: "Resumen de prueba".to_owned(),
        author: author.clone(),
        started,
        duration: Duration::from_minutes(duration_minutes).expect("valid duration"),
        comment: "Trabajo realizado".to_owned(),
        issue_url: "https://example.atlassian.net/browse/DEMO-101".to_owned(),
    }
}
