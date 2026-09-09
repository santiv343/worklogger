use std::sync::OnceLock;

use hours_core::{AccountId, DateRange, Duration, IssueKey, WeeklySummary, WeeklyTarget, Worklog};
use serde::Deserialize;
use time::{Date, Duration as TimeDuration, OffsetDateTime, PrimitiveDateTime, Time};

const ENGLISH_DEMO_FIXTURE: &str = include_str!("../resources/demo.en.json");
const SPANISH_DEMO_FIXTURE: &str = include_str!("../resources/demo.es.json");
const MINUTES_PER_HOUR: u32 = 60;

#[derive(Clone, PartialEq)]
pub(crate) struct DemoData {
    pub personal: WeeklySummary,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DemoFixture {
    jira_browse_url: String,
    weekly_target_hours: u32,
    worklog_start_hour: u8,
    default_comment: String,
    personal_account: String,
    personal_worklogs: Vec<DemoWorklog>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DemoWorklog {
    issue: String,
    summary: String,
    day_offset: i64,
    minutes: u32,
}

impl DemoData {
    pub(crate) fn current() -> Self {
        let today = OffsetDateTime::now_utc().date();
        let week = DateRange::week_containing(today);
        let period = DateRange::new(week.start(), today).expect("today belongs to its week");
        let fixture = fixture();
        Self {
            personal: personal_summary(period, fixture),
        }
    }
}

fn fixture() -> &'static DemoFixture {
    static FIXTURE: OnceLock<DemoFixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        serde_json::from_str(selected_fixture())
            .expect("the selected demo resource must match the DemoFixture schema")
    })
}

fn selected_fixture() -> &'static str {
    match crate::copy::preferred_language() {
        worklogger_settings::Language::English => ENGLISH_DEMO_FIXTURE,
        worklogger_settings::Language::Spanish => SPANISH_DEMO_FIXTURE,
    }
}

fn personal_summary(period: DateRange, fixture: &DemoFixture) -> WeeklySummary {
    let personal_account = account(&fixture.personal_account);
    let worklogs = fixture
        .personal_worklogs
        .iter()
        .enumerate()
        .map(|(index, entry)| configured_worklog(index, entry, period, &personal_account, fixture))
        .collect::<Vec<_>>();
    WeeklySummary::calculate(&personal_account, period, target(fixture), &worklogs)
}

fn configured_worklog(
    index: usize,
    entry: &DemoWorklog,
    period: DateRange,
    account: &AccountId,
    fixture: &DemoFixture,
) -> Worklog {
    demo_worklog(
        index,
        &entry.issue,
        &entry.summary,
        period.start() + TimeDuration::days(entry.day_offset),
        entry.minutes,
        account,
        fixture,
    )
}

fn demo_worklog(
    index: usize,
    issue: &str,
    summary: &str,
    date: Date,
    minutes: u32,
    account: &AccountId,
    fixture: &DemoFixture,
) -> Worklog {
    Worklog {
        id: format!("demo-{index}"),
        issue_key: issue_key(issue),
        issue_summary: summary.to_owned(),
        author: account.clone(),
        started: configured_time(date, fixture.worklog_start_hour),
        duration: Duration::from_minutes(minutes).expect("el fixture define duraciones positivas"),
        comment: fixture.default_comment.clone(),
        issue_url: format!("{}/{issue}", fixture.jira_browse_url),
    }
}

fn configured_time(date: Date, hour: u8) -> OffsetDateTime {
    let time = Time::from_hms(hour, 0, 0).expect("the fixture defines a valid hour");
    PrimitiveDateTime::new(date, time).assume_utc()
}

fn account(value: &str) -> AccountId {
    AccountId::new(value).expect("the fixture defines a valid account")
}

fn issue_key(value: &str) -> IssueKey {
    IssueKey::new(value).expect("the fixture defines a valid issue key")
}

fn target(fixture: &DemoFixture) -> WeeklyTarget {
    WeeklyTarget::from_minutes(fixture.weekly_target_hours * MINUTES_PER_HOUR)
        .expect("the fixture defines a valid weekly target")
}
