use std::collections::BTreeSet;

use dioxus::prelude::*;
use dioxus_charts::{BarChart, LineChart, PieChart};
use hours_core::{DailyHours, DateRange, TaskHours, WeeklySummary, Worklog};
use jira_adapter::{TeamReport, TeamWorklog};
use time::Date;

use crate::async_request::AsyncRequestId;
use crate::connection::load_team_period;
use crate::connection_model::ConnectionConfiguration;
use crate::copy::text;
use crate::defaults::product_defaults;
use crate::report_analytics::{
    CategoryLabel, CategorySlice, ReportAnalytics, TaskSlice, TeamAnalytics, TeamMemberAnalytics,
    TrendPoint, team_member_slices,
};
use crate::report_export::{
    ExportFormat, ExportOutcome, ReportContext, ReportDocument, TeamReportDocument, export_report,
    export_team_report,
};
use crate::ui::{Icon, IconKind};

const SECONDS_PER_HOUR: u32 = 3_600;
const SECONDS_PER_HOUR_CHART: f32 = 3_600.0;
const MINUTES_PER_HOUR: u32 = 60;
const U16_VALUE_COUNT: u32 = 65_536;
const U16_VALUE_COUNT_CHART: f32 = 65_536.0;
const FILTER_WITH_COMMENT: &str = "with";
const FILTER_WITHOUT_COMMENT: &str = "without";
const FILTER_QUERY_NAME: &str = "team-report-query";
const FILTER_MEMBER_NAME: &str = "team-report-member";
const FILTER_ISSUE_TYPE_NAME: &str = "team-report-issue-type";
const FILTER_ISSUE_STATUS_NAME: &str = "team-report-issue-status";
const FILTER_COMMENTS_NAME: &str = "team-report-comments";
const CHART_COLORS: [&str; 6] = [
    "var(--color-chart-1)",
    "var(--color-chart-2)",
    "var(--color-chart-3)",
    "var(--color-chart-4)",
    "var(--color-chart-5)",
    "var(--color-chart-6)",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReportScope {
    Individual,
    Team,
}

#[derive(Clone, Debug, PartialEq)]
enum TeamViewState {
    Idle,
    Loading(DateRange, AsyncRequestId),
    Ready(TeamReport),
    Failed(DateRange, String),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct TeamFilters {
    member_account_id: String,
    issue_type: String,
    issue_status: String,
    query: String,
    comments: CommentFilter,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum CommentFilter {
    #[default]
    All,
    WithComment,
    WithoutComment,
}

#[component]
pub(crate) fn ReportsView(
    summary: WeeklySummary,
    worklogs: Vec<Worklog>,
    identity: String,
    source_label: String,
    configuration: Option<ConnectionConfiguration>,
    can_view_team: bool,
    today: Date,
    loading: bool,
    displayed_period: Option<DateRange>,
    demo: bool,
    on_period: EventHandler<i8>,
    on_range: EventHandler<DateRange>,
) -> Element {
    let selected_period = displayed_period.unwrap_or(summary.period);
    let report_context = configuration.as_ref().map(report_context);
    let analytics = ReportAnalytics::calculate(
        &summary,
        &worklogs,
        product_defaults().reports().maximum_task_slices,
    );
    let scope = use_signal(|| ReportScope::Individual);
    let team_state = use_signal(|| TeamViewState::Idle);
    let team_filters = use_signal(TeamFilters::default);
    let mut export_notice = use_signal(|| None::<ExportNotice>);
    let exporting = use_signal(|| false);
    let document = ReportDocument {
        identity: identity.clone(),
        source_label: source_label.clone(),
        summary: summary.clone(),
        worklogs: worklogs.clone(),
        analytics: analytics.clone(),
        context: report_context.clone(),
    };
    let xlsx_document = document.clone();
    let xlsx_identity = identity.clone();
    let xlsx_context = report_context.clone();
    let pdf_context = report_context;
    let export_disabled = loading
        || exporting()
        || matches!(scope(), ReportScope::Team) && !matches!(team_state(), TeamViewState::Ready(_));
    rsx! { section { aria_labelledby: "reports-title",
        ReportHeading {
            loading: export_disabled,
            on_xlsx: move |_| run_selected_export(SelectedExport { format: ExportFormat::Xlsx, scope: scope(), personal: xlsx_document.clone(), team_state: team_state(), filters: team_filters(), identity: xlsx_identity.clone(), context: xlsx_context.clone() }, export_notice, exporting),
            on_pdf: move |_| run_selected_export(SelectedExport { format: ExportFormat::Pdf, scope: scope(), personal: document.clone(), team_state: team_state(), filters: team_filters(), identity: identity.clone(), context: pdf_context.clone() }, export_notice, exporting),
        }
        if exporting() { ExportProgress {} }
        if let Some(notice) = export_notice() { ExportStatus { notice, on_close: move |_| export_notice.set(None) } }
        ReportTabs { scope, can_view_team, period: selected_period, team_state }
        if scope() == ReportScope::Individual {
            div { id: "report-panel-individual", role: "tabpanel", aria_labelledby: "report-tab-individual",
                PersonalReportContent { summary, worklogs, analytics, source_label, period: selected_period, today, loading, demo, on_period, on_range }
            }
        } else {
            div { id: "report-panel-team", role: "tabpanel", aria_labelledby: "report-tab-team",
                TeamReportContent { state: team_state, filters: team_filters, today, on_range }
            }
        }
    } }
}

#[component]
fn ReportTabs(
    mut scope: Signal<ReportScope>,
    can_view_team: bool,
    period: DateRange,
    team_state: Signal<TeamViewState>,
) -> Element {
    rsx! { div { class: "report-tabs", role: "tablist", aria_label: text("reports.scopeAria"),
        button { id: "report-tab-individual", class: tab_class(scope() == ReportScope::Individual), role: "tab", aria_controls: "report-panel-individual", aria_selected: scope() == ReportScope::Individual, onclick: move |_| scope.set(ReportScope::Individual), {text("reports.scopeIndividual")} }
        if can_view_team { button { id: "report-tab-team", class: tab_class(scope() == ReportScope::Team), role: "tab", aria_controls: "report-panel-team", aria_selected: scope() == ReportScope::Team, onclick: move |_| { scope.set(ReportScope::Team); ensure_team_report(period, team_state); }, {text("reports.scopeTeam")} } }
    } }
}

fn tab_class(active: bool) -> &'static str {
    if active {
        "report-tab active"
    } else {
        "report-tab"
    }
}

fn ensure_team_report(period: DateRange, state: Signal<TeamViewState>) {
    if matches!(state(), TeamViewState::Ready(report) if report.period == period) {
        return;
    }
    start_team_report(period, state);
}

fn start_team_report(period: DateRange, mut state: Signal<TeamViewState>) {
    let request_id = AsyncRequestId::next();
    state.set(TeamViewState::Loading(period, request_id));
    spawn(async move {
        let next = match load_team_period(period).await {
            Ok(report) => TeamViewState::Ready(report),
            Err(error) => TeamViewState::Failed(period, error),
        };
        if team_request_is_current(&state(), request_id) {
            state.set(next);
        }
    });
}

fn team_request_is_current(state: &TeamViewState, request_id: AsyncRequestId) -> bool {
    matches!(state, TeamViewState::Loading(_, current) if *current == request_id)
}

#[component]
fn PersonalReportContent(
    summary: WeeklySummary,
    worklogs: Vec<Worklog>,
    analytics: ReportAnalytics,
    source_label: String,
    period: DateRange,
    today: Date,
    loading: bool,
    demo: bool,
    on_period: EventHandler<i8>,
    on_range: EventHandler<DateRange>,
) -> Element {
    rsx! {
        ScopeNotice { source_label: source_label.clone() }
        crate::ui::PeriodToolbar { period, today, source_label, demo, loading, on_period, on_range }
        if loading { ReportSkeleton {} } else {
            ReportMetrics { summary: summary.clone(), entries: worklogs.len(), analytics: analytics.clone() }
            div { class: "report-grid",
                ReportTaskDistribution { slices: analytics.task_slices.clone() }
                ReportTrend { points: analytics.trend.clone() }
            }
            ReportTasks { tasks: summary.tasks.clone() }
            ReportSources { worklogs }
        }
    }
}

#[component]
fn ReportHeading(
    loading: bool,
    on_xlsx: EventHandler<MouseEvent>,
    on_pdf: EventHandler<MouseEvent>,
) -> Element {
    rsx! { div { class: "view-heading",
        div { h1 { id: "reports-title", {text("reports.title")} } p { {text("reports.subtitle")} } }
        div { class: "report-export-actions", aria_label: text("reports.export.actionsAria"),
            span { class: "report-export-label", {text("reports.export.label")} }
            button { class: "icon-button report-export-action", disabled: loading, aria_label: text("reports.export.xlsxHelp"), title: text("reports.export.xlsxHelp"), onclick: on_xlsx, Icon { kind: IconKind::Spreadsheet } }
            if product_defaults().reports().pdf_export_enabled {
                button { class: "icon-button report-export-action", disabled: loading, aria_label: text("reports.export.pdfHelp"), title: text("reports.export.pdfHelp"), onclick: on_pdf, Icon { kind: IconKind::Pdf } }
            }
        }
    } }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExportNotice {
    message: String,
    error: bool,
}

fn start_personal_export(
    format: ExportFormat,
    document: ReportDocument,
    notice: Signal<Option<ExportNotice>>,
    exporting: Signal<bool>,
) {
    start_export(notice, exporting, async move {
        export_report(format, &document).await
    });
}

struct SelectedExport {
    format: ExportFormat,
    scope: ReportScope,
    personal: ReportDocument,
    team_state: TeamViewState,
    filters: TeamFilters,
    identity: String,
    context: Option<ReportContext>,
}

fn run_selected_export(
    selection: SelectedExport,
    notice: Signal<Option<ExportNotice>>,
    exporting: Signal<bool>,
) {
    if selection.scope == ReportScope::Individual {
        return start_personal_export(selection.format, selection.personal, notice, exporting);
    }
    let TeamViewState::Ready(report) = &selection.team_state else {
        return;
    };
    let filtered = filtered_team_report(report, &selection.filters);
    let filter_summary = team_filter_summary(report, &selection.filters);
    let document = TeamReportDocument::new(
        &selection.identity,
        team_source_label(filtered.warnings.len()),
        &filtered,
        filter_summary,
        report.worklogs.len(),
        selection.context,
    );
    start_export(notice, exporting, async move {
        export_team_report(selection.format, &document).await
    });
}

fn start_export(
    mut notice: Signal<Option<ExportNotice>>,
    mut exporting: Signal<bool>,
    export: impl Future<Output = Result<ExportOutcome, String>> + 'static,
) {
    if exporting() {
        return;
    }
    exporting.set(true);
    notice.set(None);
    spawn(async move {
        notice.set(export_notice(export.await));
        exporting.set(false);
    });
}

fn report_context(configuration: &ConnectionConfiguration) -> ReportContext {
    ReportContext {
        jira_site: configuration.jira.site.clone(),
        board_id: configuration.jira.board_id,
        utc_offset_minutes: configuration.hours.utc_offset_minutes,
    }
}

fn export_notice(result: Result<ExportOutcome, String>) -> Option<ExportNotice> {
    match result {
        Ok(ExportOutcome::Cancelled) => None,
        Ok(ExportOutcome::Saved(path)) => Some(ExportNotice {
            message: format!("{}{}", text("reports.export.savedPrefix"), path.display()),
            error: false,
        }),
        Err(message) => Some(ExportNotice {
            message,
            error: true,
        }),
    }
}

#[component]
fn ExportStatus(notice: ExportNotice, on_close: EventHandler<MouseEvent>) -> Element {
    let class = if notice.error {
        "status-banner error"
    } else {
        "status-banner success"
    };
    let role = if notice.error { "alert" } else { "status" };
    rsx! { div { class, role,
        span { aria_hidden: "true", if notice.error { "!" } else { "✓" } }
        span { "{notice.message}" }
        button { class: "icon-button", aria_label: text("status.close"), onclick: on_close, "×" }
    } }
}

#[component]
fn ExportProgress() -> Element {
    rsx! { div { class: "status-banner", role: "status", aria_live: "polite",
        span { class: "status-spinner", aria_hidden: "true" }
        span { {text("reports.export.exporting")} }
    } }
}

#[component]
fn ScopeNotice(source_label: String) -> Element {
    rsx! { div { class: "report-scope-note", role: "status",
        span { strong { {text("reports.personalScopeTitle")} } " " {text("reports.personalScopeDescription")} }
        span { class: "report-source-label", "{source_label}" }
    } }
}

#[component]
fn ReportMetrics(summary: WeeklySummary, entries: usize, analytics: ReportAnalytics) -> Element {
    rsx! { div { class: "report-metrics",
        ReportMetric { value: format_hours(summary.loaded_seconds), label: text("reports.metric.total") }
        ReportMetric { value: analytics.active_days.to_string(), label: text("reports.metric.days") }
        ReportMetric { value: summary.tasks.len().to_string(), label: text("reports.metric.tasks") }
        ReportMetric { value: entries.to_string(), label: text("reports.metric.entries") }
        ReportMetric { value: format_hours(analytics.average_active_day_seconds), label: text("reports.metric.average") }
        ReportMetric { value: busiest_day_label(analytics.busiest_day.as_ref()), label: text("reports.metric.busiest") }
        ReportMetric { value: format!("{}%", analytics.top_task_percentage), label: text("reports.metric.concentration") }
        ReportMetric { value: analytics.uncommented_entries.to_string(), label: text("reports.metric.uncommented") }
    } }
}

#[component]
fn ReportMetric(value: String, label: &'static str) -> Element {
    rsx! { article { class: "report-metric", strong { "{value}" } span { "{label}" } } }
}

#[component]
fn ReportTaskDistribution(slices: Vec<TaskSlice>) -> Element {
    let series = slices
        .iter()
        .map(|slice| seconds_as_chart_hours(slice.seconds))
        .collect::<Vec<_>>();
    rsx! { article { class: "card",
        ReportCardHeading { eyebrow: text("reports.distributionEyebrow"), title: text("reports.distributionTitle") }
        if slices.is_empty() { p { class: "empty-copy", {text("week.noWorklogs")} } }
        else { div { class: "report-distribution",
            div { class: "report-pie-container", aria_hidden: "true",
                PieChart { series, donut: true, donut_width: 54.0, show_labels: false, padding: 18.0, height: "210px", class_chart: "report-pie-chart", class_series: "report-pie-series", class_slice: "report-pie-slice" }
                span { class: "report-pie-center", {text("reports.distributionCenter")} }
            }
            div { class: "report-legend", role: "list", aria_label: text("reports.distributionAria"),
                for (index, slice) in slices.iter().enumerate() { ReportLegendItem { slice: slice.clone(), index } }
            }
        } }
    } }
}

#[component]
fn ReportLegendItem(slice: TaskSlice, index: usize) -> Element {
    let label = slice
        .issue_key
        .as_deref()
        .unwrap_or(text("reports.otherTasks"));
    let color = CHART_COLORS[index % CHART_COLORS.len()];
    rsx! { div { class: "report-legend-item", role: "listitem",
        span { class: "report-legend-color", style: "background: {color}" }
        span { "{label}" }
        strong { "{format_hours(slice.seconds)} · {slice.percentage}%" }
    } }
}

#[component]
fn ReportTrend(points: Vec<TrendPoint>) -> Element {
    let has_activity = points.iter().any(|point| point.seconds > 0);
    let series = vec![
        points
            .iter()
            .map(|point| seconds_as_chart_hours(point.seconds))
            .collect::<Vec<_>>(),
    ];
    let maximum_labels = product_defaults().reports().maximum_trend_labels;
    let labels = trend_labels(&points, maximum_labels);
    let visible_points = visible_trend_points(&points, maximum_labels);
    rsx! { article { class: "card",
        ReportCardHeading { eyebrow: text("reports.trendEyebrow"), title: text("reports.trendTitle") }
        if !has_activity { p { class: "empty-copy", {text("week.noWorklogs")} } }
        else { div { class: "report-trend",
            div { aria_hidden: "true", LineChart { series, labels, height: "210px", padding_top: 20, padding_bottom: 48, padding_left: 58, padding_right: 24, max_ticks: 5, show_line_labels: false, line_width: "4px", dot_size: "10px", class_chart_line: "report-line-chart", class_line_path: "report-line-path", class_line_dot: "report-line-dot", class_grid_line: "report-grid-line", class_grid_label: "report-grid-label" } }
            div { class: "report-trend-labels",
                for point in visible_points { div { span { "{point.date}" } strong { "{format_hours(point.seconds)}" } } }
            }
            ul { class: "sr-only", aria_label: text("reports.trendAria"),
                for point in points { li { "{point.date}: {format_hours(point.seconds)}" } }
            }
        } }
    } }
}

fn seconds_as_chart_hours(seconds: u32) -> f32 {
    let upper = u16::try_from(seconds / U16_VALUE_COUNT).unwrap_or(u16::MAX);
    let lower = u16::try_from(seconds % U16_VALUE_COUNT).unwrap_or_default();
    (f32::from(upper) * U16_VALUE_COUNT_CHART + f32::from(lower)) / SECONDS_PER_HOUR_CHART
}

fn trend_labels(points: &[TrendPoint], maximum: usize) -> Vec<String> {
    let maximum = maximum.max(2);
    let last_index = points.len().saturating_sub(1);
    let interval = points
        .len()
        .saturating_sub(1)
        .div_ceil(maximum.saturating_sub(1));
    points
        .iter()
        .enumerate()
        .map(|(index, point)| trend_label(index, last_index, interval, point.date))
        .collect()
}

fn trend_label(index: usize, last_index: usize, interval: usize, date: Date) -> String {
    if index == 0 || index == last_index || index.is_multiple_of(interval.max(1)) {
        return format!("{}/{}", date.day(), u8::from(date.month()));
    }
    String::new()
}

fn visible_trend_points(points: &[TrendPoint], maximum: usize) -> Vec<TrendPoint> {
    let maximum = maximum.max(2);
    let last_index = points.len().saturating_sub(1);
    let interval = points
        .len()
        .saturating_sub(1)
        .div_ceil(maximum.saturating_sub(1));
    points
        .iter()
        .enumerate()
        .filter(|(index, _)| trend_point_is_visible(*index, last_index, interval))
        .map(|(_, point)| *point)
        .collect()
}

fn trend_point_is_visible(index: usize, last_index: usize, interval: usize) -> bool {
    index == 0 || index == last_index || index.is_multiple_of(interval.max(1))
}

fn busiest_day_label(day: Option<&DailyHours>) -> String {
    day.map_or_else(
        || text("reports.noActivity").to_owned(),
        |value| format!("{} · {}", value.date, format_hours(value.duration_seconds)),
    )
}

#[component]
fn ReportTasks(tasks: Vec<TaskHours>) -> Element {
    let mut page = use_signal(|| 0_usize);
    let window = page_window(tasks.len(), page());
    let visible = tasks[window.start..window.end].to_vec();
    rsx! { article { class: "card",
        ReportCardHeading { eyebrow: text("reports.tasksEyebrow"), title: text("reports.tasksTitle") }
        div { class: "table-scroll", tabindex: "0", aria_label: text("reports.tasksAria"),
            table { caption { class: "sr-only", {text("reports.tasksAria")} }
                thead { tr { th { scope: "col", {text("table.issue")} } th { scope: "col", {text("table.entries")} } th { scope: "col", {text("table.total")} } } }
                tbody { for task in visible { ReportTaskRow { task } } }
            }
        }
        PaginationControls { window, on_page: move |selected| page.set(selected) }
    } }
}

#[component]
fn ReportTaskRow(task: TaskHours) -> Element {
    rsx! { tr { th { scope: "row", a { class: "jira-link", href: task.issue_url, target: "_blank", rel: "noreferrer", "{task.issue_key.as_str()}" } span { class: "report-task-summary", "{task.summary}" } }
        td { "{task.entries}" } td { strong { "{format_hours(task.duration_seconds)}" } }
    } }
}

#[component]
fn ReportSources(worklogs: Vec<Worklog>) -> Element {
    let mut page = use_signal(|| 0_usize);
    let window = page_window(worklogs.len(), page());
    let visible = worklogs[window.start..window.end].to_vec();
    rsx! { article { class: "card report-sources",
        ReportCardHeading { eyebrow: text("reports.sourcesEyebrow"), title: text("reports.sourcesTitle") }
        if worklogs.is_empty() { p { class: "empty-copy", {text("week.noWorklogs")} } }
        else { div { class: "table-scroll", tabindex: "0", aria_label: text("reports.sourcesAria"),
            table { caption { class: "sr-only", {text("reports.sourcesAria")} }
                thead { tr { th { scope: "col", {text("table.date")} } th { scope: "col", {text("table.issue")} } th { scope: "col", {text("table.duration")} } th { scope: "col", {text("table.comment")} } } }
                tbody { for worklog in visible { ReportSourceRow { worklog } } }
            }
        } }
        PaginationControls { window, on_page: move |selected| page.set(selected) }
    } }
}

#[component]
fn ReportSourceRow(worklog: Worklog) -> Element {
    rsx! { tr { td { "{worklog.started.date()}" }
        th { scope: "row", a { class: "jira-link", href: worklog.issue_url, target: "_blank", rel: "noreferrer", "{worklog.issue_key.as_str()}" } }
        td { strong { "{format_hours(worklog.duration.seconds())}" } }
        td { class: "comment-cell", "{display_comment(&worklog.comment)}" }
    } }
}

#[component]
fn TeamReportContent(
    state: Signal<TeamViewState>,
    filters: Signal<TeamFilters>,
    today: Date,
    on_range: EventHandler<DateRange>,
) -> Element {
    match state() {
        TeamViewState::Idle => rsx! { ReportSkeleton {} },
        TeamViewState::Loading(period, _) => rsx! {
            TeamPeriodToolbar { period, today, state, filters, loading: true, warnings: 0, on_range }
            ReportSkeleton {}
        },
        TeamViewState::Failed(period, message) => rsx! {
            TeamPeriodToolbar { period, today, state, filters, loading: false, warnings: 0, on_range }
            TeamLoadError { message, period, state, filters }
        },
        TeamViewState::Ready(report) => rsx! {
            TeamPeriodToolbar { period: report.period, today, state, filters, loading: false, warnings: report.warnings.len(), on_range }
            TeamDashboard { report, filters }
        },
    }
}

#[component]
fn TeamPeriodToolbar(
    period: DateRange,
    today: Date,
    state: Signal<TeamViewState>,
    filters: Signal<TeamFilters>,
    loading: bool,
    warnings: usize,
    on_range: EventHandler<DateRange>,
) -> Element {
    let previous_state = state;
    let range_state = state;
    let previous_filters = filters;
    let range_filters = filters;
    let previous_range = on_range;
    let custom_range = on_range;
    rsx! { crate::ui::PeriodToolbar {
        period,
        today,
        source_label: team_source_label(warnings),
        demo: false,
        loading,
        on_period: move |direction| shift_team_report(period, direction, today, previous_state, previous_filters, previous_range),
        on_range: move |selected| update_team_period(selected, range_state, range_filters, custom_range),
    } }
}

fn shift_team_report(
    period: DateRange,
    direction: i8,
    today: Date,
    state: Signal<TeamViewState>,
    filters: Signal<TeamFilters>,
    on_range: EventHandler<DateRange>,
) {
    if let Ok(selected) = crate::ui::shifted_period(period, direction, today) {
        update_team_period(selected, state, filters, on_range);
    }
}

fn update_team_period(
    period: DateRange,
    state: Signal<TeamViewState>,
    filters: Signal<TeamFilters>,
    on_range: EventHandler<DateRange>,
) {
    start_filtered_team_report(period, state, filters);
    on_range.call(period);
}

fn start_filtered_team_report(
    period: DateRange,
    state: Signal<TeamViewState>,
    mut filters: Signal<TeamFilters>,
) {
    filters.set(TeamFilters::default());
    start_team_report(period, state);
}

fn team_source_label(warnings: usize) -> String {
    if warnings == 0 {
        return text("reports.teamSourceComplete").to_owned();
    }
    format!("{}{}", text("reports.teamSourceWarning"), warnings)
}

#[component]
fn TeamLoadError(
    message: String,
    period: DateRange,
    state: Signal<TeamViewState>,
    filters: Signal<TeamFilters>,
) -> Element {
    rsx! { div { class: "status-banner error", role: "alert",
        span { aria_hidden: "true", "!" }
        span { "{message}" }
        button { class: "button ghost compact", onclick: move |_| start_filtered_team_report(period, state, filters), {text("action.retry")} }
    } }
}

#[component]
fn TeamDashboard(report: TeamReport, filters: Signal<TeamFilters>) -> Element {
    let current_filters = filters();
    let visible_report = filtered_team_report(&report, &current_filters);
    let analytics = TeamAnalytics::calculate(
        visible_report.period,
        &visible_report.members,
        &visible_report.worklogs,
        product_defaults().reports().maximum_task_slices,
    );
    let filtered_count = filter_result_label(visible_report.worklogs.len(), report.worklogs.len());
    rsx! {
        TeamReportFilters { members: report.members, worklogs: report.worklogs, filters }
        p { class: "team-filter-result", role: "status", aria_live: "polite", "{filtered_count}" }
        TeamDashboardBody { analytics, entries: visible_report.worklogs.len(), warnings: visible_report.warnings.len(), worklogs: visible_report.worklogs }
    }
}

#[component]
fn TeamDashboardBody(
    analytics: TeamAnalytics,
    entries: usize,
    warnings: usize,
    worklogs: Vec<TeamWorklog>,
) -> Element {
    rsx! {
        TeamMetrics { analytics: analytics.clone(), entries }
        TeamCoverage { members: analytics.members.clone(), uncommented_entries: analytics.uncommented_entries, warnings }
        TeamOverviewCharts { analytics: analytics.clone() }
        TeamComposition { analytics: analytics.clone() }
        TeamMembers { members: analytics.members }
        ReportTasks { tasks: analytics.tasks }
        TeamSources { worklogs }
    }
}

#[component]
fn TeamOverviewCharts(analytics: TeamAnalytics) -> Element {
    rsx! { div { class: "report-grid",
        TeamMemberChart { members: analytics.members }
        ReportTrend { points: analytics.trend }
    } }
}

#[component]
fn TeamComposition(analytics: TeamAnalytics) -> Element {
    rsx! { div { class: "report-composition-grid",
        ReportTaskDistribution { slices: analytics.task_slices }
        TeamCategoryChart { slices: analytics.issue_type_slices, eyebrow: text("reports.classificationEyebrow"), title: text("reports.issueTypeTitle"), aria_label: text("reports.issueTypeAria"), empty_label: text("reports.unspecifiedType") }
        TeamCategoryChart { slices: analytics.issue_status_slices, eyebrow: text("reports.classificationEyebrow"), title: text("reports.issueStatusTitle"), aria_label: text("reports.issueStatusAria"), empty_label: text("reports.unspecifiedStatus") }
    } }
}

fn filter_result_label(included: usize, total: usize) -> String {
    format!("{included} {} {total}", text("reports.filteredCountOf"))
}

#[component]
fn TeamReportFilters(
    members: Vec<jira_adapter::TeamMember>,
    worklogs: Vec<TeamWorklog>,
    mut filters: Signal<TeamFilters>,
) -> Element {
    let members = team_filter_members(&members);
    let issue_types = filter_option_pairs(team_filter_options(&worklogs, issue_type_value));
    let issue_statuses = filter_option_pairs(team_filter_options(&worklogs, issue_status_value));
    let current = filters();
    let has_filters = team_filters_active(&current);
    rsx! { div { class: "team-report-filters", role: "search", aria_label: text("reports.teamFiltersAria"),
        TeamSearchFilter { value: current.query, on_change: move |value| filters.write().query = value }
        TeamSelectFilter { name: FILTER_MEMBER_NAME, label: text("reports.member"), default_label: text("reports.memberFilterDefault"), selected: current.member_account_id, options: members, on_change: move |value| filters.write().member_account_id = value }
        TeamSelectFilter { name: FILTER_ISSUE_TYPE_NAME, label: text("reports.issueTypeLabel"), default_label: text("reports.issueTypeFilterDefault"), selected: current.issue_type, options: issue_types, on_change: move |value| filters.write().issue_type = value }
        TeamSelectFilter { name: FILTER_ISSUE_STATUS_NAME, label: text("reports.issueStatusLabel"), default_label: text("reports.issueStatusFilterDefault"), selected: current.issue_status, options: issue_statuses, on_change: move |value| filters.write().issue_status = value }
        TeamSelectFilter { name: FILTER_COMMENTS_NAME, label: text("reports.commentsLabel"), default_label: text("reports.commentsFilterDefault"), selected: active_comment_filter_value(current.comments).to_owned(), options: comment_filter_options(), on_change: move |value: String| filters.write().comments = parse_comment_filter(&value) }
        if has_filters { button { class: "icon-button compact-action", r#type: "button", aria_label: text("reports.clearFilters"), title: text("reports.clearFilters"), onclick: move |_| filters.set(TeamFilters::default()), "×" } }
    } }
}

#[component]
fn TeamSearchFilter(value: String, on_change: EventHandler<String>) -> Element {
    rsx! { label { class: "team-filter-search", span { class: "sr-only", {text("reports.searchLabel")} }
        span { class: "team-filter-search-icon", aria_hidden: "true", Icon { kind: IconKind::Search } }
        input { r#type: "search", name: FILTER_QUERY_NAME, autocomplete: "off", spellcheck: "false", placeholder: text("reports.searchPlaceholder"), value, oninput: move |event| on_change.call(event.value()) }
    } }
}

#[component]
fn TeamSelectFilter(
    name: &'static str,
    label: &'static str,
    default_label: &'static str,
    selected: String,
    options: Vec<(String, String)>,
    on_change: EventHandler<String>,
) -> Element {
    let active = !selected.is_empty();
    rsx! { label { span { class: "sr-only", "{label}" }
        select { name, class: filter_control_class(active), value: selected, oninput: move |event| on_change.call(event.value()),
            option { value: "", "{default_label}" }
            for (value, option_label) in options { option { value, "{option_label}" } }
        }
    } }
}

fn filter_option_pairs(values: Vec<String>) -> Vec<(String, String)> {
    values
        .into_iter()
        .map(|value| (value.clone(), value))
        .collect()
}

fn comment_filter_options() -> Vec<(String, String)> {
    vec![
        (
            FILTER_WITH_COMMENT.to_owned(),
            text("reports.commentsWith").to_owned(),
        ),
        (
            FILTER_WITHOUT_COMMENT.to_owned(),
            text("reports.commentsWithout").to_owned(),
        ),
    ]
}

const fn active_comment_filter_value(filter: CommentFilter) -> &'static str {
    match filter {
        CommentFilter::All => "",
        CommentFilter::WithComment => FILTER_WITH_COMMENT,
        CommentFilter::WithoutComment => FILTER_WITHOUT_COMMENT,
    }
}

fn team_filters_active(filters: &TeamFilters) -> bool {
    !filters.member_account_id.is_empty()
        || !filters.issue_type.is_empty()
        || !filters.issue_status.is_empty()
        || !filters.query.trim().is_empty()
        || filters.comments != CommentFilter::All
}

fn filter_control_class(active: bool) -> &'static str {
    if active {
        "team-filter-chip active"
    } else {
        "team-filter-chip"
    }
}

fn filtered_team_report(report: &TeamReport, filters: &TeamFilters) -> TeamReport {
    TeamReport {
        period: report.period,
        members: filtered_team_members(report, filters),
        worklogs: report
            .worklogs
            .iter()
            .filter(|entry| team_entry_matches(entry, filters))
            .cloned()
            .collect(),
        warnings: report.warnings.clone(),
    }
}

fn filtered_team_members(
    report: &TeamReport,
    filters: &TeamFilters,
) -> Vec<jira_adapter::TeamMember> {
    if filters.member_account_id.is_empty() {
        return report.members.clone();
    }
    report
        .members
        .iter()
        .filter(|member| member.account_id == filters.member_account_id)
        .cloned()
        .collect()
}

fn team_entry_matches(entry: &TeamWorklog, filters: &TeamFilters) -> bool {
    member_matches(entry, &filters.member_account_id)
        && optional_value_matches(entry.issue_type.as_deref(), &filters.issue_type)
        && optional_value_matches(entry.issue_status.as_deref(), &filters.issue_status)
        && query_matches(entry, &filters.query)
        && comment_matches(entry, filters.comments)
}

fn member_matches(entry: &TeamWorklog, account_id: &str) -> bool {
    account_id.is_empty() || entry.worklog.author.as_str() == account_id
}

fn query_matches(entry: &TeamWorklog, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || entry
            .worklog
            .issue_key
            .as_str()
            .to_lowercase()
            .contains(&query)
        || entry.worklog.issue_summary.to_lowercase().contains(&query)
        || entry.worklog.comment.to_lowercase().contains(&query)
}

fn comment_matches(entry: &TeamWorklog, filter: CommentFilter) -> bool {
    let has_comment = !entry.worklog.comment.trim().is_empty();
    match filter {
        CommentFilter::All => true,
        CommentFilter::WithComment => has_comment,
        CommentFilter::WithoutComment => !has_comment,
    }
}

fn optional_value_matches(value: Option<&str>, selected: &str) -> bool {
    selected.is_empty() || value.is_some_and(|value| value == selected)
}

type FilterOptionSelector = for<'entry> fn(&'entry TeamWorklog) -> Option<&'entry str>;

fn issue_type_value(entry: &TeamWorklog) -> Option<&str> {
    entry.issue_type.as_deref()
}

fn issue_status_value(entry: &TeamWorklog) -> Option<&str> {
    entry.issue_status.as_deref()
}

fn team_filter_options(worklogs: &[TeamWorklog], selector: FilterOptionSelector) -> Vec<String> {
    worklogs
        .iter()
        .filter_map(selector)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn team_filter_members(members: &[jira_adapter::TeamMember]) -> Vec<(String, String)> {
    members
        .iter()
        .map(|member| (member.account_id.clone(), member.display_name.clone()))
        .collect()
}

fn parse_comment_filter(value: &str) -> CommentFilter {
    match value {
        FILTER_WITH_COMMENT => CommentFilter::WithComment,
        FILTER_WITHOUT_COMMENT => CommentFilter::WithoutComment,
        _ => CommentFilter::All,
    }
}

fn team_filter_summary(report: &TeamReport, filters: &TeamFilters) -> String {
    let mut values = Vec::new();
    if let Some(member) = selected_member_name(report, &filters.member_account_id) {
        values.push(format!("{}: {member}", text("reports.member")));
    }
    add_filter_summary(
        &mut values,
        text("reports.issueTypeLabel"),
        &filters.issue_type,
    );
    add_filter_summary(
        &mut values,
        text("reports.issueStatusLabel"),
        &filters.issue_status,
    );
    if !filters.query.trim().is_empty() {
        values.push(format!(
            "{}: {}",
            text("reports.searchLabel"),
            filters.query.trim()
        ));
    }
    if filters.comments != CommentFilter::All {
        values.push(comment_filter_label(filters.comments).to_owned());
    }
    if values.is_empty() {
        return text("reports.filtersNone").to_owned();
    }
    values.join(" · ")
}

fn add_filter_summary(values: &mut Vec<String>, label: &str, value: &str) {
    if !value.is_empty() {
        values.push(format!("{label}: {value}"));
    }
}

fn selected_member_name<'report>(
    report: &'report TeamReport,
    account_id: &str,
) -> Option<&'report str> {
    report
        .members
        .iter()
        .find(|member| member.account_id == account_id)
        .map(|member| member.display_name.as_str())
}

fn comment_filter_label(filter: CommentFilter) -> &'static str {
    match filter {
        CommentFilter::WithComment => text("reports.commentsWith"),
        CommentFilter::WithoutComment => text("reports.commentsWithout"),
        CommentFilter::All => text("reports.commentsFilterDefault"),
    }
}

#[component]
fn TeamMetrics(analytics: TeamAnalytics, entries: usize) -> Element {
    let members_with_hours = members_with_hours(&analytics.members);
    let coverage = format!("{members_with_hours}/{}", analytics.members.len());
    rsx! { div { class: "report-metrics",
        ReportMetric { value: format_hours(analytics.loaded_seconds), label: text("reports.teamMetric.total") }
        ReportMetric { value: coverage, label: text("reports.teamMetric.coverage") }
        ReportMetric { value: analytics.tasks.len().to_string(), label: text("reports.metric.tasks") }
        ReportMetric { value: entries.to_string(), label: text("reports.metric.entries") }
    } }
}

#[component]
fn TeamCoverage(
    members: Vec<TeamMemberAnalytics>,
    uncommented_entries: usize,
    warnings: usize,
) -> Element {
    let loaded = members_with_hours(&members);
    let total = members.len();
    let missing = total.saturating_sub(loaded);
    let maximum = total.max(1);
    rsx! { article { class: "card report-coverage",
        ReportCardHeading { eyebrow: text("reports.coverageEyebrow"), title: text("reports.coverageTitle") }
        div { class: "report-coverage-body",
            div { class: "report-coverage-progress", strong { "{loaded} / {total}" } span { {text("reports.coverageLoaded")} } progress { max: "{maximum}", value: "{loaded}", aria_label: text("reports.coverageAria") } }
            div { class: "report-insights",
                TeamInsight { value: missing, label: text("reports.coverageMissing") }
                TeamInsight { value: uncommented_entries, label: text("reports.metric.uncommented") }
                TeamInsight { value: warnings, label: text("reports.coverageWarnings") }
            }
        }
    } }
}

fn members_with_hours(members: &[TeamMemberAnalytics]) -> usize {
    members.iter().filter(|member| member.seconds > 0).count()
}

#[component]
fn TeamInsight(value: usize, label: &'static str) -> Element {
    rsx! { div { class: "report-insight", strong { "{value}" } span { "{label}" } } }
}

#[component]
fn TeamCategoryChart(
    slices: Vec<CategorySlice>,
    eyebrow: &'static str,
    title: &'static str,
    aria_label: &'static str,
    empty_label: &'static str,
) -> Element {
    let series = vec![category_series(&slices)];
    let labels = category_labels(&slices, empty_label);
    rsx! { article { class: "card report-category-card",
        ReportCardHeading { eyebrow, title }
        if slices.is_empty() { p { class: "empty-copy", {text("week.noWorklogs")} } }
        else { div { class: "report-category-chart", aria_hidden: "true", BarChart { series, labels, height: "220px", horizontal_bars: true, padding_top: 10, padding_bottom: 30, padding_left: 112, padding_right: 45, max_ticks: 4, bar_width: "14px", show_series_labels: true, class_chart_bar: "report-bar-chart", class_bar: "report-category-bar", class_grid_line: "report-grid-line", class_grid_label: "report-grid-label" } }
            ul { class: "report-category-values", aria_label,
                for slice in slices { li { span { "{category_label_text(&slice.label, empty_label)}" } strong { "{format_hours(slice.seconds)} · {slice.percentage}%" } } }
            }
        }
    } }
}

fn category_series(slices: &[CategorySlice]) -> Vec<f32> {
    slices
        .iter()
        .map(|slice| seconds_as_chart_hours(slice.seconds))
        .collect()
}

fn category_labels(slices: &[CategorySlice], empty_label: &str) -> Vec<String> {
    slices
        .iter()
        .map(|slice| category_label_text(&slice.label, empty_label))
        .collect()
}

fn category_label_text(label: &CategoryLabel, empty_label: &str) -> String {
    match label {
        CategoryLabel::Value(value) => value.clone(),
        CategoryLabel::Unspecified => empty_label.to_owned(),
        CategoryLabel::Other => text("reports.otherCategories").to_owned(),
    }
}

#[component]
fn TeamMemberChart(members: Vec<TeamMemberAnalytics>) -> Element {
    let maximum = product_defaults().reports().maximum_team_chart_members;
    let visible = team_member_slices(&members, maximum);
    let series = vec![
        visible
            .iter()
            .map(|member| seconds_as_chart_hours(member.seconds))
            .collect(),
    ];
    let labels = visible
        .iter()
        .map(|member| {
            member
                .display_name
                .clone()
                .unwrap_or_else(|| text("reports.otherMembers").to_owned())
        })
        .collect::<Vec<_>>();
    rsx! { article { class: "card",
        ReportCardHeading { eyebrow: text("reports.teamMembersEyebrow"), title: text("reports.teamMembersChartTitle") }
        if members.is_empty() { p { class: "empty-copy", {text("week.noWorklogs")} } }
        else { div { aria_hidden: "true", BarChart { series, labels, height: "260px", horizontal_bars: true, padding_top: 20, padding_bottom: 30, padding_left: 130, padding_right: 45, max_ticks: 5, bar_width: "16px", show_series_labels: true, class_chart_bar: "report-bar-chart", class_bar: "report-member-bar", class_grid_line: "report-grid-line", class_grid_label: "report-grid-label" } }
            ul { class: "sr-only", aria_label: text("reports.teamMembersAria"), for member in members { li { "{member.display_name}: {format_hours(member.seconds)}" } } }
        }
    } }
}

#[component]
fn TeamMembers(members: Vec<TeamMemberAnalytics>) -> Element {
    let mut page = use_signal(|| 0_usize);
    let window = page_window(members.len(), page());
    let visible = members[window.start..window.end].to_vec();
    rsx! { article { class: "card",
        ReportCardHeading { eyebrow: text("reports.teamMembersEyebrow"), title: text("reports.teamMembersTitle") }
        div { class: "table-scroll", tabindex: "0", aria_label: text("reports.teamMembersAria"),
            table { caption { class: "sr-only", {text("reports.teamMembersAria")} }
                thead { tr { th { scope: "col", {text("reports.member")} } th { scope: "col", {text("reports.coverageState")} } th { scope: "col", {text("reports.metric.days")} } th { scope: "col", {text("reports.metric.tasks")} } th { scope: "col", {text("table.entries")} } th { scope: "col", {text("table.total")} } } }
                tbody { for member in visible { tr { th { scope: "row", "{member.display_name}" } td { span { class: member_status_class(member.seconds), "{member_status_label(member.seconds)}" } } td { "{member.active_days}" } td { "{member.task_count}" } td { "{member.entries}" } td { strong { "{format_hours(member.seconds)}" } } } } }
            }
        }
        PaginationControls { window, on_page: move |selected| page.set(selected) }
    } }
}

fn member_status_class(seconds: u32) -> &'static str {
    if seconds == 0 {
        "status-chip pending"
    } else {
        "status-chip complete"
    }
}

fn member_status_label(seconds: u32) -> &'static str {
    if seconds == 0 {
        text("reports.coverageNoHours")
    } else {
        text("reports.coverageWithHours")
    }
}

#[component]
fn TeamSources(worklogs: Vec<TeamWorklog>) -> Element {
    let mut page = use_signal(|| 0_usize);
    let window = page_window(worklogs.len(), page());
    let visible = worklogs[window.start..window.end].to_vec();
    rsx! { article { class: "card report-sources",
        ReportCardHeading { eyebrow: text("reports.sourcesEyebrow"), title: text("reports.sourcesTitle") }
        if worklogs.is_empty() { p { class: "empty-copy", {text("week.noWorklogs")} } }
        else { div { class: "table-scroll", tabindex: "0", aria_label: text("reports.sourcesAria"),
            table { caption { class: "sr-only", {text("reports.sourcesAria")} }
                thead { tr { th { scope: "col", {text("reports.member")} } th { scope: "col", {text("table.date")} } th { scope: "col", {text("table.issue")} } th { scope: "col", {text("table.duration")} } th { scope: "col", {text("table.comment")} } } }
                tbody { for entry in visible { TeamSourceRow { entry } } }
            }
        } }
        PaginationControls { window, on_page: move |selected| page.set(selected) }
    } }
}

#[component]
fn TeamSourceRow(entry: TeamWorklog) -> Element {
    let worklog = entry.worklog;
    rsx! { tr { th { scope: "row", "{entry.author_display_name}" }
        td { "{worklog.started.date()}" }
        td { a { class: "jira-link", href: worklog.issue_url, target: "_blank", rel: "noreferrer", "{worklog.issue_key.as_str()}" } }
        td { strong { "{format_hours(worklog.duration.seconds())}" } }
        td { class: "comment-cell", "{display_comment(&worklog.comment)}" }
    } }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PageWindow {
    current: usize,
    total: usize,
    start: usize,
    end: usize,
}

fn page_window(item_count: usize, requested_page: usize) -> PageWindow {
    let page_size = product_defaults().reports().table_page_size.max(1);
    let total = item_count.max(1).div_ceil(page_size);
    let current = requested_page.min(total.saturating_sub(1));
    let start = current.saturating_mul(page_size).min(item_count);
    PageWindow {
        current,
        total,
        start,
        end: start.saturating_add(page_size).min(item_count),
    }
}

#[component]
fn PaginationControls(window: PageWindow, on_page: EventHandler<usize>) -> Element {
    if window.total <= 1 {
        return rsx! {};
    }
    let previous = window.current.saturating_sub(1);
    let next = window
        .current
        .saturating_add(1)
        .min(window.total.saturating_sub(1));
    let label = format!(
        "{} {} {} {}",
        text("reports.page"),
        window.current + 1,
        text("reports.pageOf"),
        window.total
    );
    rsx! { nav { class: "table-pagination", aria_label: text("reports.paginationAria"),
        span { "{label}" }
        div {
            button { class: "icon-button compact-action", disabled: window.current == 0, aria_label: text("reports.previousPage"), title: text("reports.previousPage"), onclick: move |_| on_page.call(previous), "‹" }
            button { class: "icon-button compact-action", disabled: window.current + 1 >= window.total, aria_label: text("reports.nextPage"), title: text("reports.nextPage"), onclick: move |_| on_page.call(next), "›" }
        }
    } }
}

#[component]
fn ReportCardHeading(eyebrow: &'static str, title: &'static str) -> Element {
    rsx! { div { class: "card-heading", div { span { class: "eyebrow", "{eyebrow}" } h2 { "{title}" } } } }
}

#[component]
fn ReportSkeleton() -> Element {
    rsx! { div { class: "week-skeleton", role: "status", aria_live: "polite",
        span { class: "sr-only", {text("week.loading")} }
        div { class: "skeleton-card skeleton-summary", div { class: "skeleton-line wide" } div { class: "skeleton-line medium" } }
        div { class: "skeleton-grid", div { class: "skeleton-card tall" } div { class: "skeleton-card tall" } }
    } }
}

fn format_hours(seconds: u32) -> String {
    let hours = seconds / SECONDS_PER_HOUR;
    let minutes = seconds % SECONDS_PER_HOUR / MINUTES_PER_HOUR;
    if minutes == 0 {
        return format!("{hours} h");
    }
    format!("{hours} h {minutes} min")
}

fn display_comment(comment: &str) -> &str {
    if comment.trim().is_empty() {
        text("worklog.noComment")
    } else {
        comment
    }
}

#[cfg(test)]
mod tests {
    use hours_core::{AccountId, Duration, IssueKey, Worklog};
    use jira_adapter::TeamWorklog;
    use time::{Date, Month, Time};

    use super::{
        CommentFilter, MINUTES_PER_HOUR, PageWindow, TeamFilters, page_window,
        seconds_as_chart_hours, team_entry_matches,
    };

    #[test]
    fn chart_hours_do_not_saturate_at_the_u16_minute_limit() {
        let previous_limit = u32::from(u16::MAX) * MINUTES_PER_HOUR;

        assert!(seconds_as_chart_hours(u32::MAX) > seconds_as_chart_hours(previous_limit));
    }

    #[test]
    fn pagination_clamps_stale_pages_after_the_data_changes() {
        let window = page_window(11, usize::MAX);
        assert_eq!(
            window,
            PageWindow {
                current: 1,
                total: 2,
                start: 10,
                end: 11,
            }
        );
    }

    #[test]
    fn team_filters_match_member_text_and_comment_state() {
        let entry = team_entry("account-one", "Ana", "Análisis funcional");
        let matching = TeamFilters {
            member_account_id: "account-one".to_owned(),
            issue_type: "Tarea".to_owned(),
            issue_status: "En curso".to_owned(),
            query: "demo-1".to_owned(),
            comments: CommentFilter::WithComment,
        };
        let without_comment = TeamFilters {
            comments: CommentFilter::WithoutComment,
            ..matching.clone()
        };
        let wrong_status = TeamFilters {
            issue_status: "Finalizado".to_owned(),
            ..matching.clone()
        };
        assert!(team_entry_matches(&entry, &matching));
        assert!(!team_entry_matches(&entry, &without_comment));
        assert!(!team_entry_matches(&entry, &wrong_status));
    }

    fn team_entry(account: &str, name: &str, comment: &str) -> TeamWorklog {
        let date = Date::from_calendar_date(2026, Month::September, 1).expect("valid date");
        TeamWorklog {
            worklog: Worklog {
                id: "worklog-one".to_owned(),
                issue_key: IssueKey::new("DEMO-1").expect("valid issue"),
                issue_summary: "Tarea demostrativa".to_owned(),
                author: AccountId::new(account).expect("valid account"),
                started: date.with_time(Time::MIDNIGHT).assume_utc(),
                duration: Duration::from_seconds(3_600).expect("valid duration"),
                comment: comment.to_owned(),
                issue_url: "https://example.test/browse/DEMO-1".to_owned(),
            },
            author_display_name: name.to_owned(),
            issue_type: Some("Tarea".to_owned()),
            issue_status: Some("En curso".to_owned()),
            assignee_display_name: None,
            created: None,
            updated: None,
        }
    }
}
