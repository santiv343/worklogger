use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use hours_core::{DailyHours, DateRange, IssueKey, TaskHours, WeeklySummary, Worklog};
use jira_adapter::{TeamMember, TeamWorklog};
use time::Date;

const MAX_PERCENT: u32 = 100;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TaskSlice {
    pub issue_key: Option<String>,
    pub seconds: u32,
    pub percentage: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TrendPoint {
    pub date: Date,
    pub seconds: u32,
    pub cumulative_seconds: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReportAnalytics {
    pub task_slices: Vec<TaskSlice>,
    pub trend: Vec<TrendPoint>,
    pub active_days: usize,
    pub average_active_day_seconds: u32,
    pub busiest_day: Option<DailyHours>,
    pub top_task_percentage: u32,
    pub uncommented_entries: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TeamMemberAnalytics {
    pub account_id: String,
    pub display_name: String,
    pub seconds: u32,
    pub active_days: usize,
    pub task_count: usize,
    pub entries: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TeamMemberSlice {
    pub display_name: Option<String>,
    pub seconds: u32,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum CategoryLabel {
    Value(String),
    Unspecified,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CategorySlice {
    pub label: CategoryLabel,
    pub seconds: u32,
    pub percentage: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TeamAnalytics {
    pub loaded_seconds: u32,
    pub members: Vec<TeamMemberAnalytics>,
    pub tasks: Vec<TaskHours>,
    pub task_slices: Vec<TaskSlice>,
    pub issue_type_slices: Vec<CategorySlice>,
    pub issue_status_slices: Vec<CategorySlice>,
    pub trend: Vec<TrendPoint>,
    pub uncommented_entries: usize,
}

#[derive(Default)]
struct MemberAccumulator {
    display_name: String,
    seconds: u32,
    dates: BTreeSet<Date>,
    issues: BTreeSet<IssueKey>,
    entries: usize,
}

struct CategoryBreakdown {
    issue_types: Vec<CategorySlice>,
    issue_statuses: Vec<CategorySlice>,
}

impl ReportAnalytics {
    pub(crate) fn calculate(
        summary: &WeeklySummary,
        worklogs: &[Worklog],
        maximum_task_slices: usize,
    ) -> Self {
        let active_days = active_day_count(&summary.days);
        Self {
            task_slices: task_slices(&summary.tasks, summary.loaded_seconds, maximum_task_slices),
            trend: trend_points(&summary.days),
            active_days,
            average_active_day_seconds: average_seconds(summary.loaded_seconds, active_days),
            busiest_day: busiest_day(&summary.days),
            top_task_percentage: top_task_percentage(&summary.tasks, summary.loaded_seconds),
            uncommented_entries: uncommented_entry_count(worklogs),
        }
    }
}

impl TeamAnalytics {
    pub(crate) fn calculate(
        period: DateRange,
        members: &[TeamMember],
        team_worklogs: &[TeamWorklog],
        maximum_task_slices: usize,
    ) -> Self {
        let worklogs = team_worklogs
            .iter()
            .map(|entry| &entry.worklog)
            .collect::<Vec<_>>();
        let loaded_seconds = worklogs
            .iter()
            .map(|worklog| worklog.duration.seconds())
            .sum();
        let tasks = summarize_team_tasks(&worklogs);
        let categories = team_categories(team_worklogs, loaded_seconds, maximum_task_slices);
        Self {
            loaded_seconds,
            members: summarize_members(members, team_worklogs),
            task_slices: task_slices(&tasks, loaded_seconds, maximum_task_slices),
            issue_type_slices: categories.issue_types,
            issue_status_slices: categories.issue_statuses,
            trend: trend_points(&summarize_team_days(period, &worklogs)),
            uncommented_entries: worklogs
                .iter()
                .filter(|worklog| worklog.comment.trim().is_empty())
                .count(),
            tasks,
        }
    }
}

fn team_categories(
    worklogs: &[TeamWorklog],
    total_seconds: u32,
    maximum: usize,
) -> CategoryBreakdown {
    CategoryBreakdown {
        issue_types: category_slices(worklogs, total_seconds, maximum, issue_type),
        issue_statuses: category_slices(worklogs, total_seconds, maximum, issue_status),
    }
}

type CategorySelector = for<'entry> fn(&'entry TeamWorklog) -> Option<&'entry str>;

fn issue_type(entry: &TeamWorklog) -> Option<&str> {
    entry.issue_type.as_deref()
}

fn issue_status(entry: &TeamWorklog) -> Option<&str> {
    entry.issue_status.as_deref()
}

fn category_slices(
    worklogs: &[TeamWorklog],
    total_seconds: u32,
    maximum: usize,
    selector: CategorySelector,
) -> Vec<CategorySlice> {
    let grouped = group_category_seconds(worklogs, selector);
    let mut slices = grouped
        .into_iter()
        .map(|(label, seconds)| category_slice(label, seconds, total_seconds))
        .collect::<Vec<_>>();
    slices.sort_by(category_slice_order);
    limit_category_slices(slices, total_seconds, maximum)
}

fn group_category_seconds(
    worklogs: &[TeamWorklog],
    selector: CategorySelector,
) -> BTreeMap<CategoryLabel, u32> {
    let mut grouped = BTreeMap::new();
    for entry in worklogs {
        let label = category_label(selector(entry));
        let seconds = entry.worklog.duration.seconds();
        let total = grouped.entry(label).or_insert(0_u32);
        *total = total.saturating_add(seconds);
    }
    grouped
}

fn category_label(value: Option<&str>) -> CategoryLabel {
    value
        .filter(|label| !label.trim().is_empty())
        .map_or(CategoryLabel::Unspecified, |label| {
            CategoryLabel::Value(label.to_owned())
        })
}

fn category_slice(label: CategoryLabel, seconds: u32, total_seconds: u32) -> CategorySlice {
    CategorySlice {
        label,
        seconds,
        percentage: percentage(seconds, total_seconds),
    }
}

fn category_slice_order(left: &CategorySlice, right: &CategorySlice) -> std::cmp::Ordering {
    right
        .seconds
        .cmp(&left.seconds)
        .then_with(|| left.label.cmp(&right.label))
}

fn limit_category_slices(
    slices: Vec<CategorySlice>,
    total_seconds: u32,
    maximum: usize,
) -> Vec<CategorySlice> {
    let maximum = maximum.max(2);
    if slices.len() <= maximum {
        return slices;
    }
    grouped_category_slices(slices, total_seconds, maximum)
}

fn grouped_category_slices(
    mut slices: Vec<CategorySlice>,
    total_seconds: u32,
    maximum: usize,
) -> Vec<CategorySlice> {
    let grouped = slices.split_off(maximum.saturating_sub(1));
    let other_seconds = grouped.iter().map(|slice| slice.seconds).sum::<u32>();
    slices.push(category_slice(
        CategoryLabel::Other,
        other_seconds,
        total_seconds,
    ));
    slices
}

fn summarize_members(roster: &[TeamMember], worklogs: &[TeamWorklog]) -> Vec<TeamMemberAnalytics> {
    let mut members = BTreeMap::<String, MemberAccumulator>::new();
    for member in roster {
        members.insert(
            member.account_id.clone(),
            MemberAccumulator {
                display_name: member.display_name.clone(),
                ..MemberAccumulator::default()
            },
        );
    }
    for entry in worklogs {
        accumulate_member(
            members
                .entry(entry.worklog.author.as_str().to_owned())
                .or_default(),
            entry,
        );
    }
    let mut summaries = members.into_iter().map(member_summary).collect::<Vec<_>>();
    summaries.sort_by_key(|member| (Reverse(member.seconds), member.display_name.clone()));
    summaries
}

fn accumulate_member(member: &mut MemberAccumulator, entry: &TeamWorklog) {
    member.display_name.clone_from(&entry.author_display_name);
    member.seconds = member
        .seconds
        .saturating_add(entry.worklog.duration.seconds());
    member.dates.insert(entry.worklog.started.date());
    member.issues.insert(entry.worklog.issue_key.clone());
    member.entries = member.entries.saturating_add(1);
}

fn member_summary((account_id, member): (String, MemberAccumulator)) -> TeamMemberAnalytics {
    TeamMemberAnalytics {
        account_id,
        display_name: member.display_name,
        seconds: member.seconds,
        active_days: member.dates.len(),
        task_count: member.issues.len(),
        entries: member.entries,
    }
}

fn summarize_team_tasks(worklogs: &[&Worklog]) -> Vec<TaskHours> {
    let mut tasks = BTreeMap::<IssueKey, TaskHours>::new();
    for worklog in worklogs {
        let task = tasks
            .entry(worklog.issue_key.clone())
            .or_insert_with(|| task_from(worklog));
        task.duration_seconds = task
            .duration_seconds
            .saturating_add(worklog.duration.seconds());
        task.entries = task.entries.saturating_add(1);
    }
    let mut summaries = tasks.into_values().collect::<Vec<_>>();
    summaries.sort_by_key(|task| Reverse(task.duration_seconds));
    summaries
}

fn task_from(worklog: &Worklog) -> TaskHours {
    TaskHours {
        issue_key: worklog.issue_key.clone(),
        summary: worklog.issue_summary.clone(),
        issue_url: worklog.issue_url.clone(),
        duration_seconds: 0,
        entries: 0,
    }
}

fn summarize_team_days(period: DateRange, worklogs: &[&Worklog]) -> Vec<DailyHours> {
    period
        .weekday_dates()
        .map(|(weekday, date)| DailyHours {
            weekday,
            date,
            duration_seconds: worklogs
                .iter()
                .filter(|worklog| worklog.started.date() == date)
                .map(|worklog| worklog.duration.seconds())
                .sum(),
        })
        .collect()
}

fn active_day_count(days: &[DailyHours]) -> usize {
    days.iter().filter(|day| day.duration_seconds > 0).count()
}

fn average_seconds(total_seconds: u32, active_days: usize) -> u32 {
    u32::try_from(active_days)
        .ok()
        .and_then(|count| total_seconds.checked_div(count))
        .unwrap_or_default()
}

fn busiest_day(days: &[DailyHours]) -> Option<DailyHours> {
    days.iter()
        .filter(|day| day.duration_seconds > 0)
        .max_by_key(|day| day.duration_seconds)
        .cloned()
}

fn top_task_percentage(tasks: &[TaskHours], total_seconds: u32) -> u32 {
    tasks
        .first()
        .map_or(0, |task| percentage(task.duration_seconds, total_seconds))
}

fn uncommented_entry_count(worklogs: &[Worklog]) -> usize {
    worklogs
        .iter()
        .filter(|worklog| worklog.comment.trim().is_empty())
        .count()
}

fn task_slices(tasks: &[TaskHours], total_seconds: u32, maximum: usize) -> Vec<TaskSlice> {
    let maximum = maximum.max(2);
    if tasks.len() <= maximum {
        return tasks
            .iter()
            .map(|task| task_slice(task, total_seconds))
            .collect();
    }
    grouped_task_slices(tasks, total_seconds, maximum)
}

fn grouped_task_slices(tasks: &[TaskHours], total_seconds: u32, maximum: usize) -> Vec<TaskSlice> {
    let visible_count = maximum.saturating_sub(1);
    let mut slices = tasks[..visible_count]
        .iter()
        .map(|task| task_slice(task, total_seconds))
        .collect::<Vec<_>>();
    let other_seconds = tasks[visible_count..]
        .iter()
        .map(|task| task.duration_seconds)
        .sum();
    slices.push(TaskSlice {
        issue_key: None,
        seconds: other_seconds,
        percentage: percentage(other_seconds, total_seconds),
    });
    slices
}

fn task_slice(task: &TaskHours, total_seconds: u32) -> TaskSlice {
    TaskSlice {
        issue_key: Some(task.issue_key.as_str().to_owned()),
        seconds: task.duration_seconds,
        percentage: percentage(task.duration_seconds, total_seconds),
    }
}

pub(crate) fn team_member_slices(
    members: &[TeamMemberAnalytics],
    maximum: usize,
) -> Vec<TeamMemberSlice> {
    let maximum = maximum.max(2);
    if members.len() <= maximum {
        return members.iter().map(team_member_slice).collect();
    }
    grouped_team_member_slices(members, maximum)
}

fn grouped_team_member_slices(
    members: &[TeamMemberAnalytics],
    maximum: usize,
) -> Vec<TeamMemberSlice> {
    let visible_count = maximum.saturating_sub(1);
    let mut slices = members[..visible_count]
        .iter()
        .map(team_member_slice)
        .collect::<Vec<_>>();
    let other_seconds = members[visible_count..]
        .iter()
        .map(|member| member.seconds)
        .sum();
    slices.push(TeamMemberSlice {
        display_name: None,
        seconds: other_seconds,
    });
    slices
}

fn team_member_slice(member: &TeamMemberAnalytics) -> TeamMemberSlice {
    TeamMemberSlice {
        display_name: Some(member.display_name.clone()),
        seconds: member.seconds,
    }
}

fn trend_points(days: &[DailyHours]) -> Vec<TrendPoint> {
    let mut cumulative_seconds = 0_u32;
    days.iter()
        .map(|day| {
            cumulative_seconds = cumulative_seconds.saturating_add(day.duration_seconds);
            trend_point(day, cumulative_seconds)
        })
        .collect()
}

fn trend_point(day: &DailyHours, cumulative_seconds: u32) -> TrendPoint {
    TrendPoint {
        date: day.date,
        seconds: day.duration_seconds,
        cumulative_seconds,
    }
}

fn percentage(value: u32, total: u32) -> u32 {
    value
        .saturating_mul(MAX_PERCENT)
        .checked_div(total)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hours_core::{AccountId, DateRange, Duration, IssueKey, WeeklyTarget};
    use time::{Duration as TimeDuration, Month, Time};

    #[test]
    fn groups_small_tasks_into_other_slice() {
        let summary = summary_with_tasks();
        let analytics = ReportAnalytics::calculate(&summary, &[], 2);
        assert_eq!(analytics.task_slices.len(), 2);
        assert_eq!(analytics.task_slices[1].issue_key, None);
        assert_eq!(analytics.task_slices[1].seconds, 7_200);
    }

    #[test]
    fn calculates_cumulative_trend_and_comment_gaps() {
        let worklogs = vec![worklog(0, ""), worklog(1, "Revisión")];
        let summary = calculated_summary(&worklogs);
        let analytics = ReportAnalytics::calculate(&summary, &worklogs, 6);
        assert_eq!(analytics.uncommented_entries, 1);
        assert_eq!(
            analytics.trend.last().map(|point| point.cumulative_seconds),
            Some(7_200)
        );
    }

    #[test]
    fn consolidates_team_worklogs_by_authenticated_jira_author() {
        let first = team_worklog("one", "Ana", 0, Some("Tarea"), Some("En curso"));
        let second = team_worklog("two", "Beto", 1, Some("Bug"), Some("Finalizado"));
        let period = DateRange::week_containing(first.worklog.started.date());
        let roster = vec![TeamMember {
            account_id: "zero".to_owned(),
            display_name: "Cero Horas".to_owned(),
            active: true,
        }];
        let analytics = TeamAnalytics::calculate(period, &roster, &[first, second], 6);
        assert_eq!(analytics.loaded_seconds, 7_200);
        assert_eq!(analytics.members.len(), 3);
        assert!(analytics.members.iter().any(|member| member.seconds == 0));
        assert_eq!(analytics.tasks.len(), 2);
        assert_eq!(analytics.issue_type_slices.len(), 2);
        assert_eq!(analytics.issue_status_slices.len(), 2);
    }

    #[test]
    fn groups_missing_and_overflowing_categories_without_losing_time() {
        let worklogs = vec![
            team_worklog("one", "Ana", 0, Some("Tarea"), None),
            team_worklog("one", "Ana", 1, Some("Bug"), None),
            team_worklog("one", "Ana", 2, Some("Historia"), None),
        ];
        let period = DateRange::week_containing(worklogs[0].worklog.started.date());
        let analytics = TeamAnalytics::calculate(period, &[], &worklogs, 2);
        assert_eq!(analytics.issue_type_slices.len(), 2);
        assert!(
            analytics
                .issue_status_slices
                .iter()
                .any(|slice| slice.label == CategoryLabel::Unspecified)
        );
        assert_eq!(
            analytics
                .issue_type_slices
                .iter()
                .map(|slice| slice.seconds)
                .sum::<u32>(),
            analytics.loaded_seconds
        );
    }

    #[test]
    fn groups_members_beyond_the_chart_limit() {
        let members = vec![
            member("Ana", 10_800),
            member("Beto", 7_200),
            member("Caro", 3_600),
        ];
        let slices = team_member_slices(&members, 2);
        assert_eq!(slices.len(), 2);
        assert_eq!(slices[0].display_name.as_deref(), Some("Ana"));
        assert_eq!(slices[1].display_name, None);
        assert_eq!(slices[1].seconds, 10_800);
    }

    fn member(display_name: &str, seconds: u32) -> TeamMemberAnalytics {
        TeamMemberAnalytics {
            account_id: display_name.to_lowercase(),
            display_name: display_name.to_owned(),
            seconds,
            active_days: 1,
            task_count: 1,
            entries: 1,
        }
    }

    fn summary_with_tasks() -> WeeklySummary {
        let worklogs = vec![worklog(0, "Uno"), worklog(1, "Dos"), worklog(2, "Tres")];
        calculated_summary(&worklogs)
    }

    fn calculated_summary(worklogs: &[Worklog]) -> WeeklySummary {
        let account = AccountId::new("account").expect("valid account");
        let period = DateRange::week_containing(worklogs[0].started.date());
        let target = WeeklyTarget::from_minutes(1_800).expect("valid target");
        WeeklySummary::calculate(&account, period, target, worklogs)
    }

    fn worklog(day_offset: i64, comment: &str) -> Worklog {
        let date = Date::from_calendar_date(2026, Month::September, 1).expect("valid date");
        Worklog {
            id: day_offset.to_string(),
            issue_key: IssueKey::new(format!("TASK-{}", day_offset + 1)).expect("valid issue"),
            issue_summary: format!("Tarea {day_offset}"),
            author: AccountId::new("account").expect("valid account"),
            started: (date + TimeDuration::days(day_offset))
                .with_time(Time::MIDNIGHT)
                .assume_utc(),
            duration: Duration::from_seconds(3_600).expect("valid duration"),
            comment: comment.to_owned(),
            issue_url: format!("https://example.test/TASK-{}", day_offset + 1),
        }
    }

    fn team_worklog(
        account: &str,
        display_name: &str,
        day_offset: i64,
        issue_type: Option<&str>,
        issue_status: Option<&str>,
    ) -> TeamWorklog {
        let mut worklog = worklog(day_offset, "Trabajo");
        worklog.author = AccountId::new(account).expect("valid account");
        TeamWorklog {
            worklog,
            author_display_name: display_name.to_owned(),
            issue_type: issue_type.map(str::to_owned),
            issue_status: issue_status.map(str::to_owned),
            assignee_display_name: None,
            created: None,
            updated: None,
        }
    }
}
