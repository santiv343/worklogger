use std::{cmp::Reverse, collections::BTreeMap};

use serde::{Deserialize, Serialize};
use time::{Date, Weekday};

use super::{AccountId, DateRange, IssueKey, WeeklyTarget, Worklog};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WeeklySummary {
    pub period: DateRange,
    pub loaded_seconds: u32,
    pub target: WeeklyTarget,
    pub missing_seconds: u32,
    pub progress_percent: u32,
    pub days: Vec<DailyHours>,
    pub tasks: Vec<TaskHours>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DailyHours {
    pub weekday: Weekday,
    pub date: Date,
    pub duration_seconds: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TaskHours {
    pub issue_key: IssueKey,
    pub summary: String,
    pub issue_url: String,
    pub duration_seconds: u32,
    pub entries: usize,
}

impl WeeklySummary {
    #[must_use]
    pub fn calculate(
        account_id: &AccountId,
        period: DateRange,
        target: WeeklyTarget,
        worklogs: &[Worklog],
    ) -> Self {
        let selected = selected_worklogs(account_id, period, worklogs);
        let loaded_seconds = sum_seconds(&selected);
        Self {
            period,
            loaded_seconds,
            target,
            missing_seconds: target.duration().seconds().saturating_sub(loaded_seconds),
            progress_percent: progress_percent(loaded_seconds, target),
            days: summarize_days(period, &selected),
            tasks: summarize_tasks(&selected),
        }
    }
}

fn selected_worklogs<'worklog>(
    account_id: &AccountId,
    period: DateRange,
    worklogs: &'worklog [Worklog],
) -> Vec<&'worklog Worklog> {
    worklogs
        .iter()
        .filter(|worklog| worklog.belongs_to(account_id))
        .filter(|worklog| period.contains(worklog.started.date()))
        .collect()
}

fn sum_seconds(worklogs: &[&Worklog]) -> u32 {
    worklogs
        .iter()
        .map(|worklog| worklog.duration.seconds())
        .sum()
}

fn progress_percent(loaded_seconds: u32, target: WeeklyTarget) -> u32 {
    let target_seconds = u64::from(target.duration().seconds());
    let percentage = u64::from(loaded_seconds).saturating_mul(100) / target_seconds;
    u32::try_from(percentage.min(100)).expect("percentage is capped at 100")
}

fn summarize_days(period: DateRange, worklogs: &[&Worklog]) -> Vec<DailyHours> {
    period
        .weekday_dates()
        .map(|(weekday, date)| DailyHours {
            weekday,
            date,
            duration_seconds: seconds_for_date(date, worklogs),
        })
        .collect()
}

fn seconds_for_date(date: Date, worklogs: &[&Worklog]) -> u32 {
    worklogs
        .iter()
        .filter(|worklog| worklog.started.date() == date)
        .map(|worklog| worklog.duration.seconds())
        .sum()
}

fn summarize_tasks(worklogs: &[&Worklog]) -> Vec<TaskHours> {
    let mut grouped = BTreeMap::<IssueKey, Vec<&Worklog>>::new();
    for worklog in worklogs {
        grouped
            .entry(worklog.issue_key.clone())
            .or_default()
            .push(worklog);
    }
    let mut tasks = grouped
        .into_values()
        .map(|worklogs| summarize_task(&worklogs))
        .collect::<Vec<_>>();
    tasks.sort_by_key(|task| Reverse(task.duration_seconds));
    tasks
}

fn summarize_task(worklogs: &[&Worklog]) -> TaskHours {
    let first = worklogs.first().expect("grouped worklogs cannot be empty");
    TaskHours {
        issue_key: first.issue_key.clone(),
        summary: first.issue_summary.clone(),
        issue_url: first.issue_url.clone(),
        duration_seconds: sum_seconds(worklogs),
        entries: worklogs.len(),
    }
}
