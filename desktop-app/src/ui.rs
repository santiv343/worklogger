use dioxus::prelude::*;
use hours_core::{
    DailyHours, DateRange, Duration, IssueKey, PossibleDuplicatePolicy, TaskHours, WeeklySummary,
    Worklog,
};
use time::{
    Date, Duration as TimeDuration, Month, OffsetDateTime, UtcOffset, Weekday, format_description,
};

use crate::async_request::AsyncRequestId;
use crate::connection::restore;
use crate::connection::{
    connect_and_save, create_worklog, delete_worklog, disconnect, load_period, search_issues,
    unlogged_assigned_sprint_issues, update_configuration, update_worklog,
};
use crate::connection_model::{
    AccessibleIssue, ConfigurationUpdate, ConnectedSession, ConnectionConfiguration,
    ConnectionRequest, CreateWorklogCommand, DeleteWorklogCommand, UpdateWorklogCommand,
    WorklogMutationOutcome,
};
use crate::copy::text;
use crate::defaults::{branding_css, defaults_error, product_defaults};
use crate::demo::DemoData;
use crate::preferences::PreferencesDialog;
use crate::setup::ConnectionSetup;

const BRAND_LOGO: Asset = asset!("/assets/brand-logo.svg");
const SECONDS_PER_HOUR: u32 = 3_600;
const MINUTES_PER_HOUR: u32 = 60;
const DAYS_PER_WEEK: i64 = 7;
const LAST_FOURTEEN_DAYS: i64 = 14;
const MAX_PERCENT: u32 = 100;
const MAX_INITIALS: usize = 2;
const MAX_MINUTE_COMPONENT: u32 = MINUTES_PER_HOUR - 1;
const ISO_DATE_FORMAT: &str = "[year]-[month]-[day]";
const CSS: Asset = asset!("/assets/main.css");
const ICON_VIEW_BOX: &str = "0 0 24 24";

#[derive(Clone, Copy, Debug, PartialEq)]
enum NavigationTarget {
    Home,
    Jira,
    #[cfg(feature = "reports")]
    Reports,
}

#[derive(Clone, Copy, PartialEq)]
struct ModuleNavigationItem {
    target: NavigationTarget,
    label_key: &'static str,
    description_key: &'static str,
    icon: IconKind,
}

fn initial_navigation() -> NavigationTarget {
    NavigationTarget::Home
}

fn bundled_modules() -> Vec<ModuleNavigationItem> {
    let mut modules = vec![ModuleNavigationItem {
        target: NavigationTarget::Jira,
        label_key: "navigation.jira",
        description_key: "home.jiraDescription",
        icon: IconKind::Ticket,
    }];
    #[cfg(feature = "reports")]
    if product_defaults().modules.reports.is_some() {
        modules.push(ModuleNavigationItem {
            target: NavigationTarget::Reports,
            label_key: "navigation.reports",
            description_key: "home.reportsDescription",
            icon: IconKind::Report,
        });
    }
    modules
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum IconKind {
    ChevronDown,
    ChevronRight,
    Clock,
    Delete,
    Disconnect,
    Edit,
    External,
    Home,
    #[cfg(feature = "reports")]
    Pdf,
    #[cfg(feature = "reports")]
    Report,
    Search,
    Settings,
    #[cfg(feature = "reports")]
    Spreadsheet,
    Ticket,
}

#[derive(Clone, Copy, PartialEq)]
enum DialogStep {
    Form,
    Review,
}

#[derive(Clone, Default, PartialEq)]
enum WorklogSubmission {
    #[default]
    Idle,
    Pending,
    Succeeded,
    Failed(String),
}

#[derive(Clone, Default, PartialEq)]
pub(crate) enum ConfigurationSubmission {
    #[default]
    Idle,
    Pending,
    Succeeded,
    Failed(String),
}

#[derive(Clone, PartialEq)]
struct WorklogDraft {
    issue_key: String,
    date: String,
    hours: String,
    minutes: String,
    comment: String,
}

#[derive(Clone, PartialEq)]
struct SuggestedIssue {
    issue: AccessibleIssue,
    loaded_seconds: u32,
}

#[derive(Clone, Copy, PartialEq)]
enum RangePreset {
    CurrentWeek,
    PreviousWeek,
    LastFourteenDays,
    CurrentMonth,
}

#[derive(Clone, Copy)]
struct RangePresetItem {
    preset: RangePreset,
    label_key: &'static str,
}

const RANGE_PRESETS: [RangePresetItem; 4] = [
    RangePresetItem {
        preset: RangePreset::CurrentWeek,
        label_key: "period.preset.currentWeek",
    },
    RangePresetItem {
        preset: RangePreset::PreviousWeek,
        label_key: "period.preset.previousWeek",
    },
    RangePresetItem {
        preset: RangePreset::LastFourteenDays,
        label_key: "period.preset.lastFourteenDays",
    },
    RangePresetItem {
        preset: RangePreset::CurrentMonth,
        label_key: "period.preset.currentMonth",
    },
];

impl Default for WorklogDraft {
    fn default() -> Self {
        let default_minutes = product_defaults().hours().default_duration_minutes;
        Self {
            issue_key: String::new(),
            date: OffsetDateTime::now_utc().date().to_string(),
            hours: (default_minutes / MINUTES_PER_HOUR).to_string(),
            minutes: (default_minutes % MINUTES_PER_HOUR).to_string(),
            comment: String::new(),
        }
    }
}

impl WorklogDraft {
    fn for_date(date: Date) -> Self {
        Self {
            date: date.to_string(),
            ..Self::default()
        }
    }

    fn from_worklog(worklog: &Worklog) -> Self {
        let total_minutes = worklog.duration.seconds() / MINUTES_PER_HOUR;
        Self {
            issue_key: worklog.issue_key.as_str().to_owned(),
            date: worklog.started.date().to_string(),
            hours: (total_minutes / MINUTES_PER_HOUR).to_string(),
            minutes: (total_minutes % MINUTES_PER_HOUR).to_string(),
            comment: worklog.comment.clone(),
        }
    }
}

#[derive(Clone, PartialEq)]
enum AppState {
    Starting,
    Setup(Option<String>, bool),
    Connected(Box<ConnectedSession>, Option<String>),
    Refreshing(Box<ConnectedSession>, DateRange, AsyncRequestId),
    Disconnecting,
    Demo,
}

#[allow(non_snake_case)]
pub(crate) fn App() -> Element {
    let state = use_signal(initial_state);
    let configuration_revision = use_signal(|| 0_u64);
    let title = text("app.title");
    let _ = configuration_revision();
    let branding = branding_css();
    let mut restoring_state = state;
    use_future(move || async move {
        restoring_state.set(restored_state(restore().await));
    });
    rsx! {
        document::Title { "{title}" }
        document::Stylesheet { href: CSS }
        document::Style { "{branding}" }
        {render_state(state, configuration_revision)}
    }
}

fn initial_state() -> AppState {
    AppState::Starting
}

fn render_state(mut state: Signal<AppState>, mut configuration_revision: Signal<u64>) -> Element {
    match state() {
        AppState::Starting => rsx! { LoadingScreen {} },
        AppState::Setup(error, loading) => rsx! {
            ConnectionSetup {
                key: "{configuration_revision()}",
                loading,
                error,
                configuration_warning: defaults_error(),
                on_connect: move |request| start_connection(request, state),
                on_configuration_loaded: move |()| {
                    configuration_revision += 1;
                    state.set(AppState::Setup(None, false));
                },
                on_demo: move |_| state.set(AppState::Demo),
            }
        },
        AppState::Connected(session, error) => {
            rsx! { LiveDashboard { session: *session, error, loading: false, displayed_period: None, state, configuration_revision } }
        }
        AppState::Refreshing(session, period, _) => {
            rsx! { LiveDashboard { session: *session, error: None, loading: true, displayed_period: Some(period), state, configuration_revision } }
        }
        AppState::Disconnecting => {
            rsx! { LoadingScreen {} }
        }
        AppState::Demo => rsx! { DemoDashboard { state, configuration_revision } },
    }
}

fn start_connection(request: ConnectionRequest, mut state: Signal<AppState>) {
    state.set(AppState::Setup(None, true));
    spawn(async move {
        let next = match connect_and_save(request).await {
            Ok(session) => AppState::Connected(Box::new(session), None),
            Err(error) => AppState::Setup(Some(error), false),
        };
        state.set(next);
    });
}

fn restored_state(result: Result<Option<ConnectedSession>, String>) -> AppState {
    match result {
        Ok(Some(session)) => AppState::Connected(Box::new(session), None),
        Ok(None) => AppState::Setup(None, false),
        Err(error) => AppState::Setup(Some(error), false),
    }
}

#[component]
fn LoadingScreen() -> Element {
    let company_name = product_defaults().branding.company_name.clone();
    rsx! { main { class: "loading-screen", role: "status",
        BrandLogo { class: "loading-logo", width: "132", height: "42", company_name }
        p { {text("app.loading")} }
    } }
}

#[component]
fn LiveDashboard(
    session: ConnectedSession,
    error: Option<String>,
    loading: bool,
    displayed_period: Option<DateRange>,
    state: Signal<AppState>,
    mut configuration_revision: Signal<u64>,
) -> Element {
    let submission = use_signal(WorklogSubmission::default);
    let configuration_submission = use_signal(ConfigurationSubmission::default);
    let warning_count = session.report.warnings.len();
    let fallback = session.clone();
    let delete_fallback = session.clone();
    let update_fallback = session.clone();
    let period_fallback = session.clone();
    let range_fallback = session.clone();
    let preferences_fallback = session.clone();
    let clear_fallback = session.clone();
    rsx! {
        Dashboard {
            summary: session.report.summary,
            worklogs: session.report.worklogs,
            identity: session.report.identity.display_name,
            source_label: live_source_label(warning_count),
            configuration: Some(session.configuration),
            can_view_team_report: session.can_view_team_report,
            project_permissions: session.project_permissions,
            demo: false,
            loading,
            submission,
            configuration_submission,
            displayed_period,
            error,
            on_create: move |command| start_worklog(command, fallback.clone(), state, submission),
            on_delete: move |command| start_delete(command, delete_fallback.clone(), state),
            on_update: move |command| start_update(command, update_fallback.clone(), state, submission),
            on_period: move |direction| start_period(direction, period_fallback.clone(), state),
            on_range: move |period| start_selected_period(period, range_fallback.clone(), state),
            on_configuration: move |configuration| start_configuration(configuration, preferences_fallback.clone(), state, configuration_submission),
            configuration_revision,
            on_configuration_loaded: move |()| reload_after_configuration_import(state, configuration_revision),
            on_error_close: move |_| state.set(AppState::Connected(Box::new(clear_fallback.clone()), None)),
            on_disconnect: move |_| start_disconnect(state),
        }
    }
}

#[component]
fn DemoDashboard(state: Signal<AppState>, configuration_revision: Signal<u64>) -> Element {
    let data = use_hook(DemoData::current);
    let submission = use_signal(WorklogSubmission::default);
    let configuration_submission = use_signal(ConfigurationSubmission::default);
    rsx! {
        Dashboard {
            summary: data.personal.clone(),
            worklogs: Vec::new(),
            identity: text("demo.person").to_owned(),
            source_label: text("demo.source").to_owned(),
            configuration: None,
            can_view_team_report: false,
            project_permissions: None,
            demo: true,
            loading: false,
            submission,
            configuration_submission,
            displayed_period: None,
            error: None,
            on_create: move |_| {},
            on_delete: move |_| {},
            on_update: move |_| {},
            on_period: move |_| {},
            on_range: move |_| {},
            on_configuration: move |_| {},
            configuration_revision,
            on_configuration_loaded: move |()| configuration_revision += 1,
            on_error_close: move |_| {},
            on_disconnect: move |_| state.set(AppState::Setup(None, false)),
        }
    }
}

#[component]
fn Dashboard(
    summary: WeeklySummary,
    worklogs: Vec<Worklog>,
    identity: String,
    source_label: String,
    configuration: Option<ConnectionConfiguration>,
    can_view_team_report: bool,
    project_permissions: Option<jira_adapter::ProjectPermissions>,
    demo: bool,
    loading: bool,
    submission: Signal<WorklogSubmission>,
    configuration_submission: Signal<ConfigurationSubmission>,
    displayed_period: Option<DateRange>,
    error: Option<String>,
    on_create: EventHandler<CreateWorklogCommand>,
    on_delete: EventHandler<DeleteWorklogCommand>,
    on_update: EventHandler<UpdateWorklogCommand>,
    on_period: EventHandler<i8>,
    on_range: EventHandler<DateRange>,
    on_configuration: EventHandler<ConfigurationUpdate>,
    configuration_revision: Signal<u64>,
    on_configuration_loaded: EventHandler<()>,
    on_error_close: EventHandler<MouseEvent>,
    on_disconnect: EventHandler<MouseEvent>,
) -> Element {
    let dialog_worklogs = worklogs.clone();
    let today = local_today(configuration.as_ref());
    let mut active_view = use_signal(initial_navigation);
    let dialog = use_signal(|| None::<DialogStep>);
    let mut notice = use_signal(|| false);
    let pending_delete = use_signal(|| None::<Worklog>);
    let editing = use_signal(|| None::<Worklog>);
    let disconnect_confirmation = use_signal(|| false);
    let preferences_open = use_signal(|| false);
    #[cfg(feature = "reports")]
    let report_view = report_view(ReportViewInput {
        active: active_view(),
        summary: summary.clone(),
        worklogs: worklogs.clone(),
        identity: identity.clone(),
        source_label: source_label.clone(),
        configuration: configuration.clone(),
        can_view_team_report,
        today,
        loading,
        displayed_period,
        demo,
        on_period,
        on_range,
    });
    #[cfg(not(feature = "reports"))]
    let report_view = report_view();
    rsx! {
        div { class: "application-shell",
            ModuleNavigation { active: active_view, on_select: move |target| active_view.set(target) }
            div { class: "shell-content",
                AppHeader { active: active_view(), identity, demo, preferences_open, disconnect_confirmation, on_disconnect }
                main { class: "app-content",
                    if let Some(message) = error { ErrorNotice { message, on_close: on_error_close } }
                    if notice() { DemoNotice { on_close: move |_| notice.set(false) } }
                    if active_view() == NavigationTarget::Home {
                        HomeView { on_select: move |target| active_view.set(target) }
                    }
                    if active_view() == NavigationTarget::Jira {
                        MyWeek { summary, worklogs, source_label, demo, loading, displayed_period, today, dialog, pending_delete, editing, on_period, on_range }
                    }
                    {report_view}
                }
            }
        }
        if let Some(step) = dialog() {
            WorklogDialog { step, dialog, notice, demo, editing, worklogs: dialog_worklogs, today, submission, on_create, on_update }
        }
        if let Some(worklog) = pending_delete() {
            DeleteDialog { worklog, pending_delete, on_delete }
        }
        if disconnect_confirmation() {
            DisconnectDialog { disconnect_confirmation, on_disconnect }
        }
        if preferences_open() {
            if let Some(configuration) = configuration {
                PreferencesDialog {
                    configuration,
                    permissions: project_permissions,
                    open: preferences_open,
                    submission: configuration_submission,
                    on_configuration_loaded,
                    on_save: move |value| on_configuration.call(value)
                }
            }
        }
    }
}

fn reload_after_configuration_import(
    mut state: Signal<AppState>,
    mut configuration_revision: Signal<u64>,
) {
    configuration_revision += 1;
    state.set(AppState::Starting);
    spawn(async move {
        state.set(restored_state(restore().await));
    });
}

fn local_today(configuration: Option<&ConnectionConfiguration>) -> Date {
    let offset_minutes = configuration.map_or(0, |value| value.hours.utc_offset_minutes);
    let seconds_per_minute = i32::try_from(MINUTES_PER_HOUR).expect("minutes per hour fits i32");
    let offset_seconds = i32::from(offset_minutes) * seconds_per_minute;
    let offset = UtcOffset::from_whole_seconds(offset_seconds).unwrap_or(UtcOffset::UTC);
    OffsetDateTime::now_utc().to_offset(offset).date()
}

#[cfg(feature = "reports")]
fn report_view(input: ReportViewInput) -> Element {
    if input.active != NavigationTarget::Reports {
        return rsx! {};
    }
    rsx! { crate::reports::ReportsView {
        summary: input.summary,
        worklogs: input.worklogs,
        identity: input.identity,
        source_label: input.source_label,
        configuration: input.configuration,
        can_view_team: input.can_view_team_report,
        today: input.today,
        loading: input.loading,
        displayed_period: input.displayed_period,
        demo: input.demo,
        on_period: input.on_period,
        on_range: input.on_range,
    } }
}

#[cfg(not(feature = "reports"))]
fn report_view() -> Element {
    rsx! {}
}

#[cfg(feature = "reports")]
struct ReportViewInput {
    active: NavigationTarget,
    summary: WeeklySummary,
    worklogs: Vec<Worklog>,
    identity: String,
    source_label: String,
    configuration: Option<ConnectionConfiguration>,
    can_view_team_report: bool,
    today: Date,
    loading: bool,
    displayed_period: Option<DateRange>,
    demo: bool,
    on_period: EventHandler<i8>,
    on_range: EventHandler<DateRange>,
}

#[component]
fn ModuleNavigation(
    active: Signal<NavigationTarget>,
    on_select: EventHandler<NavigationTarget>,
) -> Element {
    let company_name = product_defaults().branding.company_name.clone();
    rsx! { aside { class: "module-sidebar",
        button { class: "sidebar-brand", r#type: "button", aria_label: text("navigation.goHome"), title: text("navigation.goHome"), onclick: move |_| on_select.call(NavigationTarget::Home),
            BrandLogo { class: "brand-logo", width: "116", height: "37", company_name }
        }
        nav { class: "module-navigation", aria_label: text("navigation.aria"),
            NavigationButton { target: NavigationTarget::Home, label_key: "navigation.home", icon: IconKind::Home, active: active(), on_select }
            span { class: "navigation-label", {text("navigation.modules")} }
            for module in bundled_modules() {
                ModuleButton { module, active: active(), on_select }
            }
        }
    } }
}

#[component]
fn BrandLogo(
    class: &'static str,
    width: &'static str,
    height: &'static str,
    company_name: String,
) -> Element {
    let mut remote_failed = use_signal(|| false);
    let logo_url = product_defaults().branding.logo_url.clone();
    if let Some(url) = logo_url.filter(|_| !remote_failed()) {
        return rsx! { img { class, src: "{url}", alt: "{company_name}", width, height, referrerpolicy: "no-referrer", onerror: move |_| remote_failed.set(true) } };
    }
    rsx! { img { class, src: BRAND_LOGO, alt: "{company_name}", width, height } }
}

#[component]
fn ModuleButton(
    module: ModuleNavigationItem,
    active: NavigationTarget,
    on_select: EventHandler<NavigationTarget>,
) -> Element {
    rsx! { div {
        NavigationButton { target: module.target, label_key: module.label_key, icon: module.icon, active, on_select }
        if module.target == active { div { class: "module-section active",
            Icon { kind: IconKind::Clock }
            span { {text("navigation.hours")} }
        } }
    } }
}

#[component]
fn NavigationButton(
    target: NavigationTarget,
    label_key: &'static str,
    icon: IconKind,
    active: NavigationTarget,
    on_select: EventHandler<NavigationTarget>,
) -> Element {
    let class = if target == active {
        "module-button active"
    } else {
        "module-button"
    };
    let current = if target == active { "page" } else { "false" };
    rsx! {
    button { class, r#type: "button", aria_current: current, onclick: move |_| on_select.call(target),
        Icon { kind: icon }
        span { "{text(label_key)}" }
    } }
}

#[component]
fn HomeView(on_select: EventHandler<NavigationTarget>) -> Element {
    rsx! { section { class: "home-view", aria_labelledby: "home-title",
        div { class: "view-heading",
            div { h1 { id: "home-title", {text("home.title")} } }
        }
        section { aria_labelledby: "available-modules-title",
            h2 { id: "available-modules-title", class: "home-section-title", {text("home.availableTitle")} }
            div { class: "home-module-grid",
                for module in bundled_modules() {
                    HomeModuleCard { module, on_select }
                }
            }
        }
    } }
}

#[component]
fn HomeModuleCard(
    module: ModuleNavigationItem,
    on_select: EventHandler<NavigationTarget>,
) -> Element {
    rsx! { button { class: "home-module-card", r#type: "button", onclick: move |_| on_select.call(module.target),
        span { class: "home-module-icon", Icon { kind: module.icon } }
        span { class: "home-module-copy",
            strong { "{text(module.label_key)}" }
            span { "{text(module.description_key)}" }
        }
        Icon { kind: IconKind::ChevronRight }
    } }
}

fn start_disconnect(mut state: Signal<AppState>) {
    state.set(AppState::Disconnecting);
    let next = match disconnect() {
        Ok(()) => AppState::Setup(None, false),
        Err(error) => AppState::Setup(Some(error), false),
    };
    state.set(next);
}

fn start_period(direction: i8, fallback: ConnectedSession, mut state: Signal<AppState>) {
    let today = local_today(Some(&fallback.configuration));
    let period = match shifted_period(fallback.report.summary.period, direction, today) {
        Ok(period) => period,
        Err(error) => return state.set(AppState::Connected(Box::new(fallback), Some(error))),
    };
    let request_id = begin_period_refresh(&fallback, period, state);
    spawn(async move {
        let next = match load_period(period).await {
            Ok(session) => AppState::Connected(Box::new(session), None),
            Err(error) => AppState::Connected(Box::new(fallback), Some(error)),
        };
        finish_refresh(request_id, next, state);
    });
}

fn start_selected_period(period: DateRange, fallback: ConnectedSession, state: Signal<AppState>) {
    let request_id = begin_period_refresh(&fallback, period, state);
    spawn(async move {
        let next = match load_period(period).await {
            Ok(session) => AppState::Connected(Box::new(session), None),
            Err(error) => AppState::Connected(Box::new(fallback), Some(error)),
        };
        finish_refresh(request_id, next, state);
    });
}

fn start_configuration(
    configuration: ConfigurationUpdate,
    fallback: ConnectedSession,
    state: Signal<AppState>,
    mut submission: Signal<ConfigurationSubmission>,
) {
    submission.set(ConfigurationSubmission::Pending);
    let request_id = begin_refresh(&fallback, state);
    spawn(async move {
        let next = match update_configuration(configuration).await {
            Ok(session) => {
                submission.set(ConfigurationSubmission::Succeeded);
                AppState::Connected(Box::new(session), None)
            }
            Err(error) => {
                submission.set(ConfigurationSubmission::Failed(error.clone()));
                AppState::Connected(Box::new(fallback), Some(error))
            }
        };
        finish_refresh(request_id, next, state);
    });
}

pub(crate) fn shifted_period(
    period: DateRange,
    direction: i8,
    today: Date,
) -> Result<DateRange, String> {
    let days = i64::from(direction)
        .checked_mul(DAYS_PER_WEEK)
        .ok_or_else(|| text("period.error.shift").to_owned())?;
    let candidate = DateRange::week_containing(period.start() + TimeDuration::days(days));
    if candidate.start() > today {
        return Err(text("period.futureDate").to_owned());
    }
    DateRange::new(candidate.start(), candidate.end().min(today))
        .map_err(|_| text("period.error.shift").to_owned())
}

fn begin_refresh(fallback: &ConnectedSession, state: Signal<AppState>) -> AsyncRequestId {
    let period = fallback.report.summary.period;
    begin_period_refresh(fallback, period, state)
}

fn begin_period_refresh(
    fallback: &ConnectedSession,
    period: DateRange,
    mut state: Signal<AppState>,
) -> AsyncRequestId {
    let request_id = AsyncRequestId::next();
    state.set(AppState::Refreshing(
        Box::new(fallback.clone()),
        period,
        request_id,
    ));
    request_id
}

fn finish_refresh(request_id: AsyncRequestId, next: AppState, mut state: Signal<AppState>) {
    if is_current_refresh(&state(), request_id) {
        state.set(next);
    }
}

fn is_current_refresh(state: &AppState, request_id: AsyncRequestId) -> bool {
    matches!(state, AppState::Refreshing(_, _, current) if *current == request_id)
}

fn start_delete(
    command: DeleteWorklogCommand,
    fallback: ConnectedSession,
    state: Signal<AppState>,
) {
    let request_id = begin_refresh(&fallback, state);
    spawn(async move {
        let next = mutation_state(delete_worklog(command).await, fallback);
        finish_refresh(request_id, next, state);
    });
}

fn start_update(
    command: UpdateWorklogCommand,
    fallback: ConnectedSession,
    state: Signal<AppState>,
    mut submission: Signal<WorklogSubmission>,
) {
    submission.set(WorklogSubmission::Pending);
    spawn(async move {
        let result = update_worklog(command).await;
        finish_worklog_submission(result, fallback, state, submission);
    });
}

fn start_worklog(
    command: CreateWorklogCommand,
    fallback: ConnectedSession,
    state: Signal<AppState>,
    mut submission: Signal<WorklogSubmission>,
) {
    submission.set(WorklogSubmission::Pending);
    spawn(async move {
        let result = create_worklog(command).await;
        finish_worklog_submission(result, fallback, state, submission);
    });
}

fn finish_worklog_submission(
    result: Result<WorklogMutationOutcome, String>,
    fallback: ConnectedSession,
    mut state: Signal<AppState>,
    mut submission: Signal<WorklogSubmission>,
) {
    let next_submission = match &result {
        Ok(_) => WorklogSubmission::Succeeded,
        Err(message) => WorklogSubmission::Failed(message.clone()),
    };
    state.set(mutation_state(result, fallback));
    submission.set(next_submission);
}

fn mutation_state(
    result: Result<WorklogMutationOutcome, String>,
    fallback: ConnectedSession,
) -> AppState {
    match result {
        Ok(WorklogMutationOutcome::Refreshed(session)) => AppState::Connected(session, None),
        Ok(WorklogMutationOutcome::CommittedWithoutRefresh(message)) | Err(message) => {
            AppState::Connected(Box::new(fallback), Some(message))
        }
    }
}

#[component]
fn AppHeader(
    active: NavigationTarget,
    identity: String,
    demo: bool,
    mut preferences_open: Signal<bool>,
    mut disconnect_confirmation: Signal<bool>,
    on_disconnect: EventHandler<MouseEvent>,
) -> Element {
    let action = if demo {
        text("action.leaveDemo")
    } else {
        text("action.disconnect")
    };
    let (module, section) = module_context(active);
    rsx! {
        header { class: "app-header",
            div { class: "module-context",
                span { class: "eyebrow", "{module}" }
                strong { "{section}" }
            }
            div { class: "identity", span { class: "avatar", "{initials(&identity)}" }
                span { class: "connection-dot", aria_hidden: "true" } span { "{identity}" }
                if !demo { button { class: "icon-button header-action", aria_label: text("action.preferences"), title: text("action.preferences"), onclick: move |_| preferences_open.set(true),
                    Icon { kind: IconKind::Settings }
                } }
                button { class: "icon-button header-action", aria_label: action, onclick: move |event| {
                    if demo { on_disconnect.call(event); } else { disconnect_confirmation.set(true); }
                }, title: action,
                    Icon { kind: IconKind::Disconnect }
                }
            }
        }
    }
}

fn module_context(active: NavigationTarget) -> (&'static str, &'static str) {
    match active {
        NavigationTarget::Home => (text("navigation.home"), text("home.availableTitle")),
        NavigationTarget::Jira => (text("navigation.jira"), text("navigation.hours")),
        #[cfg(feature = "reports")]
        NavigationTarget::Reports => (text("navigation.reports"), text("navigation.hours")),
    }
}

#[component]
pub(crate) fn Icon(kind: IconKind) -> Element {
    let path = icon_path(kind);
    rsx! { svg { class: "icon", view_box: ICON_VIEW_BOX, fill: "none",
        path { d: path }
    } }
}

fn icon_path(kind: IconKind) -> &'static str {
    match kind {
        IconKind::ChevronDown => "M6 9l6 6 6-6",
        IconKind::ChevronRight => "M9 6l6 6-6 6",
        IconKind::Clock => "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20 M12 6v6l4 2",
        IconKind::Delete => "M3 6h18 M8 6V4h8v2 M19 6l-1 14H6L5 6 M10 11v5 M14 11v5",
        IconKind::Disconnect => "M12 2v10 M18.4 6.6a9 9 0 1 1-12.8 0",
        IconKind::Edit => "M12 20h9 M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z",
        IconKind::External => {
            "M14 3h7v7 M10 14 21 3 M21 14v6a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h6"
        }
        IconKind::Home => "M3 11.5 12 4l9 7.5 M5 10v10h14V10 M9 20v-6h6v6",
        #[cfg(feature = "reports")]
        IconKind::Pdf => "M6 2h8l4 4v16H6Z M14 2v5h5 M9 12h6 M9 16h6",
        #[cfg(feature = "reports")]
        IconKind::Report => "M4 19V9 M10 19V5 M16 19v-7 M3 19h18 M15 3h6v6 M21 3l-8 8",
        IconKind::Search => "M21 21l-4.35-4.35 M19 11a8 8 0 1 1-16 0 8 8 0 0 1 16 0",
        IconKind::Settings => {
            "M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7 M19 12a7 7 0 0 0-.1-1l2-1.5-2-3.5-2.3 1a8 8 0 0 0-1.7-1L14.5 3h-5L9 6a8 8 0 0 0-1.7 1L5 6 3 9.5 5.1 11a7 7 0 0 0 0 2L3 14.5 5 18l2.3-1a8 8 0 0 0 1.7 1l.5 3h5l.5-3a8 8 0 0 0 1.7-1l2.3 1 2-3.5-2.1-1.5a7 7 0 0 0 .1-1Z"
        }
        #[cfg(feature = "reports")]
        IconKind::Spreadsheet => "M5 2h14v20H5Z M5 8h14 M5 13h14 M10 8v14 M15 8v14",
        IconKind::Ticket => "M4 4h16v5a3 3 0 0 0 0 6v5H4v-5a3 3 0 0 0 0-6V4Z M9 4v16",
    }
}

#[component]
fn DisconnectDialog(
    mut disconnect_confirmation: Signal<bool>,
    on_disconnect: EventHandler<MouseEvent>,
) -> Element {
    rsx! { div { class: "dialog-overlay", onclick: move |_| disconnect_confirmation.set(false), onkeydown: move |event| { if event.key() == Key::Escape { disconnect_confirmation.set(false); } },
        div { class: "dialog compact-dialog", role: "alertdialog", aria_modal: "true", aria_labelledby: "disconnect-title", tabindex: "-1", onclick: move |event| event.stop_propagation(),
            span { class: "eyebrow danger-text", {text("disconnect.eyebrow")} }
            h2 { id: "disconnect-title", {text("disconnect.title")} }
            p { {text("disconnect.description")} }
            div { class: "dialog-actions",
                button { class: "button ghost", autofocus: true, onclick: move |_| disconnect_confirmation.set(false), {text("action.cancel")} }
                button { class: "button danger", onclick: move |event| { disconnect_confirmation.set(false); on_disconnect.call(event); }, {text("disconnect.confirm")} }
            }
        }
    } }
}

#[component]
fn DemoNotice(on_close: EventHandler<MouseEvent>) -> Element {
    let prefix = text("status.demoPrefix");
    let message = text("status.demoMessage");
    rsx! {
        div { class: "status-banner success", role: "status",
            span { aria_hidden: "true", "✓" }
            span { strong { "{prefix}" } "{message}" }
            button { class: "icon-button", aria_label: text("status.close"), onclick: on_close, "×" }
        }
    }
}

#[component]
fn ErrorNotice(message: String, on_close: EventHandler<MouseEvent>) -> Element {
    let prefix = text("status.operationError");
    rsx! { div { class: "status-banner error", role: "alert",
        span { aria_hidden: "true", "!" }
        span { strong { "{prefix}" } "{message}" }
        button { class: "icon-button", aria_label: text("status.close"), onclick: on_close, "×" }
    } }
}

#[component]
fn MyWeek(
    summary: WeeklySummary,
    worklogs: Vec<Worklog>,
    source_label: String,
    demo: bool,
    loading: bool,
    displayed_period: Option<DateRange>,
    today: Date,
    mut dialog: Signal<Option<DialogStep>>,
    pending_delete: Signal<Option<Worklog>>,
    editing: Signal<Option<Worklog>>,
    on_period: EventHandler<i8>,
    on_range: EventHandler<DateRange>,
) -> Element {
    let action = week_action(demo, loading, dialog, editing);
    let period = displayed_period.unwrap_or(summary.period);
    rsx! {
        section { aria_labelledby: "my-week-title",
            ViewHeading { title: text("week.title"), subtitle: text("week.subtitle"),
                action
            }
            PeriodToolbar { period, today, source_label, demo, loading, on_period, on_range }
            div { class: "week-data", aria_busy: loading,
                if loading {
                    WeekDataSkeleton {}
                } else {
                    SummaryCard { summary: summary.clone(), show_target: is_week_view(summary.period, today), today }
                    div { class: "dashboard-grid",
                        DailyChart { days: summary.days.clone() }
                        TaskTable { tasks: summary.tasks.clone() }
                    }
                    if !demo { WorklogTable { worklogs, pending_delete, editing, dialog } }
                }
            }
        }
    }
}

fn week_action(
    demo: bool,
    loading: bool,
    mut dialog: Signal<Option<DialogStep>>,
    mut editing: Signal<Option<Worklog>>,
) -> Element {
    let label = if demo {
        text("week.demoAdd")
    } else {
        text("week.add")
    };
    rsx! { button { class: "button primary", disabled: loading, onclick: move |_| { editing.set(None); dialog.set(Some(DialogStep::Form)); }, "{label}" } }
}

#[component]
fn WeekDataSkeleton() -> Element {
    rsx! { div { class: "week-skeleton", role: "status", aria_live: "polite",
        span { class: "sr-only", {text("week.loading")} }
        div { class: "skeleton-card skeleton-summary",
            div { class: "skeleton-line wide" }
            div { class: "skeleton-line medium" }
            div { class: "skeleton-line progress" }
        }
        div { class: "skeleton-grid",
            div { class: "skeleton-card tall", div { class: "skeleton-line medium" } }
            div { class: "skeleton-card tall", div { class: "skeleton-line wide" } }
        }
        div { class: "skeleton-card table", div { class: "skeleton-line wide" } }
    } }
}

#[component]
fn ViewHeading(title: &'static str, subtitle: &'static str, action: Element) -> Element {
    rsx! {
        div { class: "view-heading",
            div { h1 { id: "my-week-title", "{title}" } p { "{subtitle}" } }
            {action}
        }
    }
}

#[component]
pub(crate) fn PeriodToolbar(
    period: DateRange,
    today: Date,
    source_label: String,
    demo: bool,
    loading: bool,
    on_period: EventHandler<i8>,
    on_range: EventHandler<DateRange>,
) -> Element {
    let updated = format!("{}{}", text("period.updatedPrefix"), source_label);
    let mut custom_range = use_signal(|| false);
    let week_view = is_week_view(period, today);
    rsx! {
        div { class: "period-toolbar", aria_label: text("period.aria"),
            if week_view { button { class: "icon-button", aria_label: text("week.previous"), disabled: demo || loading, onclick: move |_| on_period.call(-1), "‹" } }
            strong { "{format_period(period)}" }
            if week_view { button { class: "icon-button", aria_label: text("week.next"), disabled: demo || loading || period.end() >= today, onclick: move |_| on_period.call(1), "›" } }
            if period == current_week_to_date(today) {
                span { class: "current-week", {text("week.current")} }
            }
            button { class: "button ghost compact", disabled: demo || loading, onclick: move |_| custom_range.set(true), {text("period.custom")} }
            span { class: "updated", "{updated}" }
        }
        if custom_range() { RangeDialog { period, today, custom_range, on_range } }
    }
}

#[component]
fn RangeDialog(
    period: DateRange,
    today: Date,
    mut custom_range: Signal<bool>,
    on_range: EventHandler<DateRange>,
) -> Element {
    let mut start = use_signal(|| period.start().to_string());
    let mut end = use_signal(|| period.end().to_string());
    let error = use_signal(|| None::<String>);
    rsx! { div { class: "dialog-overlay", onclick: move |_| custom_range.set(false), onkeydown: move |event| { if event.key() == Key::Escape { custom_range.set(false); } },
        div { class: "dialog compact-dialog", role: "dialog", aria_modal: "true", aria_labelledby: "range-title", tabindex: "-1", onclick: move |event| event.stop_propagation(),
            span { class: "eyebrow", {text("period.customEyebrow")} }
            h2 { id: "range-title", {text("period.customTitle")} }
            RangePresets { today, custom_range, on_range }
            div { class: "range-fields",
                Field { id: "range-start", label: text("period.start").to_owned(), help: text("period.startHelp").to_owned(), control: rsx! { input { id: "range-start", r#type: "date", autofocus: true, max: today.to_string(), value: start(), oninput: move |event| start.set(event.value()) } } }
                Field { id: "range-end", label: text("period.end").to_owned(), help: text("period.endHelp").to_owned(), control: rsx! { input { id: "range-end", r#type: "date", max: today.to_string(), value: end(), oninput: move |event| end.set(event.value()) } } }
            }
            if let Some(message) = error() { div { class: "setup-error", role: "alert", "{message}" } }
            div { class: "dialog-actions",
                button { class: "button ghost", onclick: move |_| custom_range.set(false), {text("action.cancel")} }
                button { class: "button primary", onclick: move |_| apply_range(&start(), &end(), today, custom_range, error, on_range), {text("period.apply")} }
            }
        }
    } }
}

#[component]
fn RangePresets(
    today: Date,
    custom_range: Signal<bool>,
    on_range: EventHandler<DateRange>,
) -> Element {
    rsx! { div { class: "range-presets", aria_label: text("period.presetsAria"),
        for item in RANGE_PRESETS {
            button { class: "preset-button", r#type: "button", onclick: move |_| apply_preset(item.preset, today, custom_range, on_range),
                {text(item.label_key)}
            }
        }
    } }
}

fn apply_preset(
    preset: RangePreset,
    today: Date,
    mut custom_range: Signal<bool>,
    on_range: EventHandler<DateRange>,
) {
    custom_range.set(false);
    on_range.call(preset_range(preset, today));
}

fn preset_range(preset: RangePreset, today: Date) -> DateRange {
    match preset {
        RangePreset::CurrentWeek => current_week_to_date(today),
        RangePreset::PreviousWeek => previous_week(today),
        RangePreset::LastFourteenDays => trailing_days(today, LAST_FOURTEEN_DAYS),
        RangePreset::CurrentMonth => current_month(today),
    }
}

fn previous_week(today: Date) -> DateRange {
    let current = DateRange::week_containing(today);
    date_range(
        current.start() - TimeDuration::days(DAYS_PER_WEEK),
        current.end() - TimeDuration::days(DAYS_PER_WEEK),
    )
}

fn trailing_days(today: Date, day_count: i64) -> DateRange {
    let days_before_today = day_count.saturating_sub(1);
    date_range(today - TimeDuration::days(days_before_today), today)
}

fn current_month(today: Date) -> DateRange {
    let first = Date::from_calendar_date(today.year(), today.month(), 1)
        .expect("the first day of a calendar month is valid");
    date_range(first, today)
}

fn current_week_to_date(today: Date) -> DateRange {
    let week = DateRange::week_containing(today);
    date_range(week.start(), today)
}

fn is_week_view(period: DateRange, today: Date) -> bool {
    period == DateRange::week_containing(period.start()) || period == current_week_to_date(today)
}

fn date_range(start: Date, end: Date) -> DateRange {
    DateRange::new(start, end).expect("preset dates form an ordered range")
}

fn apply_range(
    start: &str,
    end: &str,
    today: Date,
    mut custom_range: Signal<bool>,
    mut error: Signal<Option<String>>,
    on_range: EventHandler<DateRange>,
) {
    let period = parse_range(start, end, today);
    match period {
        Ok(value) => {
            custom_range.set(false);
            on_range.call(value);
        }
        Err(message) => error.set(Some(message)),
    }
}

fn parse_range(start: &str, end: &str, today: Date) -> Result<DateRange, String> {
    let start_date = parse_iso_date(start)?;
    let end_date = parse_iso_date(end)?;
    if end_date > today {
        return Err(text("period.futureDate").to_owned());
    }
    let period =
        DateRange::new(start_date, end_date).map_err(|_| text("period.invalidOrder").to_owned())?;
    let maximum_days = u64::from(product_defaults().hours().maximum_custom_range_days);
    if period.day_count() > maximum_days {
        return Err(format!(
            "{}{maximum_days}{}",
            text("period.maximumPrefix"),
            text("period.maximumSuffix")
        ));
    }
    Ok(period)
}

#[component]
fn SummaryCard(summary: WeeklySummary, show_target: bool, today: Date) -> Element {
    let loaded_label = loaded_period_label(summary.period, today);
    if !show_target {
        return rsx! { article { class: "summary-card",
            div { class: "metrics single",
                Metric { value: format_hours(summary.loaded_seconds), label: loaded_label, prominent: true }
            }
            p { class: "progress-copy", {text("period.noTarget")} }
        } };
    }
    let target = summary.target.duration().seconds();
    let target_label = format!("{}{}", text("week.targetPrefix"), format_hours(target));
    let missing_copy = format!(
        "{}{}{}",
        text("week.missingPrefix"),
        format_hours(summary.missing_seconds),
        text("week.missingSuffix")
    );
    rsx! {
        article { class: "summary-card",
            div { class: "metrics",
                Metric { value: format_hours(summary.loaded_seconds), label: loaded_label, prominent: true }
                Metric { value: format_hours(summary.missing_seconds), label: text("week.missing"), prominent: false }
                Metric { value: format!("{}%", summary.progress_percent), label: target_label, prominent: false }
            }
            progress { max: MAX_PERCENT.to_string(), value: "{summary.progress_percent}", aria_label: text("week.progressAria"), "{summary.progress_percent}%" }
            p { class: "progress-copy", "{missing_copy}" }
        }
    }
}

#[component]
fn Metric(value: String, label: String, prominent: bool) -> Element {
    let class = if prominent {
        "metric prominent"
    } else {
        "metric"
    };
    rsx! { div { class, strong { "{value}" } span { "{label}" } } }
}

#[component]
fn DailyChart(days: Vec<DailyHours>) -> Element {
    rsx! {
        article { class: "card daily-card",
            CardHeading { title: text("week.dailyTitle"), eyebrow: text("week.dailyEyebrow") }
            div { class: "bar-chart", role: "img", aria_label: chart_description(&days),
                for day in &days { DayBar { day: day.clone() } }
            }
        }
    }
}

#[component]
fn DayBar(day: DailyHours) -> Element {
    let scale = u32::from(product_defaults().hours().maximum_daily_hours) * SECONDS_PER_HOUR;
    let height = day
        .duration_seconds
        .saturating_mul(MAX_PERCENT)
        .checked_div(scale)
        .unwrap_or_default()
        .min(MAX_PERCENT);
    rsx! {
        div { class: "bar-item", span { class: "bar-value", "{format_hours(day.duration_seconds)}" }
            div { class: "bar-track", div { class: "bar-fill", style: "height: {height}%" } }
            span { class: "bar-label", "{weekday_label(day.weekday)}" }
        }
    }
}

#[component]
fn CardHeading(title: &'static str, eyebrow: &'static str) -> Element {
    rsx! { div { class: "card-heading", div { span { class: "eyebrow", "{eyebrow}" } h2 { "{title}" } } } }
}

#[component]
fn TaskTable(tasks: Vec<TaskHours>) -> Element {
    rsx! {
        article { class: "card tasks-card",
            CardHeading { title: text("week.tasksTitle"), eyebrow: text("week.tasksEyebrow") }
            div { class: "table-scroll", tabindex: "0", aria_label: text("table.tasksAria"),
                table { caption { class: "sr-only", {text("table.tasksCaption")} }
                    thead { tr { th { scope: "col", {text("table.issue")} } th { scope: "col", {text("table.entries")} } th { scope: "col", {text("table.total")} } th { scope: "col", {text("table.action")} } } }
                    tbody { for task in &tasks { TaskRow { task: task.clone() } } }
                }
            }
        }
    }
}

#[component]
fn TaskRow(task: TaskHours) -> Element {
    rsx! {
        tr { th { scope: "row", div { class: "issue-cell",
            strong { "{task.issue_key.as_str()}" } span { "{task.summary}" }
        } } td { "{task.entries}" } td { strong { "{format_hours(task.duration_seconds)}" } }
        td { a { class: "icon-button compact-action jira-action", aria_label: text("action.openJira"), title: text("action.openJira"), href: task.issue_url, target: "_blank", rel: "noreferrer",
            Icon { kind: IconKind::External }
        } } }
    }
}

#[component]
fn WorklogTable(
    worklogs: Vec<Worklog>,
    pending_delete: Signal<Option<Worklog>>,
    editing: Signal<Option<Worklog>>,
    dialog: Signal<Option<DialogStep>>,
) -> Element {
    rsx! { article { class: "card worklogs-card",
        CardHeading { title: text("week.worklogsTitle"), eyebrow: text("week.worklogsEyebrow") }
        if worklogs.is_empty() {
            p { class: "empty-copy", {text("week.noWorklogs")} }
        } else {
            div { class: "table-scroll", tabindex: "0", aria_label: text("table.worklogsAria"),
                table { caption { class: "sr-only", {text("table.worklogsCaption")} }
                    thead { tr { th { scope: "col", {text("table.date")} } th { scope: "col", {text("table.issue")} } th { scope: "col", {text("table.duration")} } th { scope: "col", {text("table.comment")} } th { scope: "col", {text("table.actions")} } } }
                    tbody { for worklog in worklogs { WorklogRow { worklog, pending_delete, editing, dialog } } }
                }
            }
        }
    } }
}

#[component]
fn WorklogRow(
    worklog: Worklog,
    mut pending_delete: Signal<Option<Worklog>>,
    mut editing: Signal<Option<Worklog>>,
    mut dialog: Signal<Option<DialogStep>>,
) -> Element {
    let selected_for_delete = worklog.clone();
    let selected_for_edit = worklog.clone();
    rsx! { tr {
        td { "{worklog.started.date()}" }
        th { scope: "row", a { class: "jira-link link-with-icon", href: worklog.issue_url.clone(), target: "_blank", rel: "noreferrer",
            span { "{worklog.issue_key.as_str()}" }
            Icon { kind: IconKind::External }
        } }
        td { strong { "{format_hours(worklog.duration.seconds())}" } }
        td { class: "comment-cell", "{display_comment(&worklog.comment)}" }
        td { div { class: "row-actions",
            button { class: "icon-button compact-action", aria_label: text("action.edit"), title: text("action.edit"), onclick: move |_| { editing.set(Some(selected_for_edit.clone())); dialog.set(Some(DialogStep::Form)); },
                Icon { kind: IconKind::Edit }
            }
            button { class: "icon-button compact-action danger-action", aria_label: text("action.delete"), title: text("action.delete"), onclick: move |_| pending_delete.set(Some(selected_for_delete.clone())),
                Icon { kind: IconKind::Delete }
            }
        } }
    } }
}

#[component]
fn DeleteDialog(
    worklog: Worklog,
    mut pending_delete: Signal<Option<Worklog>>,
    on_delete: EventHandler<DeleteWorklogCommand>,
) -> Element {
    let command = DeleteWorklogCommand {
        issue_key: worklog.issue_key.as_str().to_owned(),
        worklog_id: worklog.id.clone(),
    };
    let summary = format!(
        "{}{}{}{}{}{}.",
        text("delete.summaryPrefix"),
        format_hours(worklog.duration.seconds()),
        text("delete.summaryIssueSeparator"),
        worklog.issue_key.as_str(),
        text("delete.summaryDateSeparator"),
        worklog.started.date()
    );
    rsx! { div { class: "dialog-overlay", onclick: move |_| pending_delete.set(None), onkeydown: move |event| { if event.key() == Key::Escape { pending_delete.set(None); } },
        div { class: "dialog compact-dialog", role: "alertdialog", aria_modal: "true", aria_labelledby: "delete-title", tabindex: "-1", onclick: move |event| event.stop_propagation(),
            span { class: "eyebrow danger-text", {text("delete.eyebrow")} }
            h2 { id: "delete-title", {text("delete.title")} }
            p { "{summary}" }
            p { class: "safety-copy", {text("delete.safety")} }
            div { class: "dialog-actions",
                button { class: "button ghost", autofocus: true, onclick: move |_| pending_delete.set(None), {text("action.cancel")} }
                button { class: "button danger", onclick: move |_| { pending_delete.set(None); on_delete.call(command.clone()); }, {text("delete.confirm")} }
            }
        }
    } }
}

#[component]
fn WorklogDialog(
    step: DialogStep,
    mut dialog: Signal<Option<DialogStep>>,
    mut notice: Signal<bool>,
    demo: bool,
    editing: Signal<Option<Worklog>>,
    worklogs: Vec<Worklog>,
    today: Date,
    submission: Signal<WorklogSubmission>,
    on_create: EventHandler<CreateWorklogCommand>,
    on_update: EventHandler<UpdateWorklogCommand>,
) -> Element {
    let initial_draft = editing()
        .as_ref()
        .map_or_else(|| WorklogDraft::for_date(today), WorklogDraft::from_worklog);
    let draft = use_signal(|| initial_draft);
    let validation_error = use_signal(|| None::<String>);
    let issue_results = use_signal(Vec::<SuggestedIssue>::new);
    let issue_searching = use_signal(|| false);
    let issue_loaded = use_signal(|| false);
    let issue_search_error = use_signal(|| None::<String>);
    let issue_dropdown_open = use_signal(|| false);
    let issue_active_option = use_signal(|| None::<usize>);
    let mut submission_effect = submission;
    let mut dialog_effect = dialog;
    let mut editing_effect = editing;
    use_effect(move || match submission_effect() {
        WorklogSubmission::Succeeded => {
            dialog_effect.set(None);
            editing_effect.set(None);
            submission_effect.set(WorklogSubmission::Idle);
        }
        WorklogSubmission::Idle | WorklogSubmission::Pending | WorklogSubmission::Failed(_) => {}
    });
    use_future(move || {
        load_assigned_suggestions(
            demo,
            issue_results,
            issue_searching,
            issue_loaded,
            issue_search_error,
        )
    });
    rsx! { div { class: "dialog-overlay", onclick: move |_| close_issue_dropdown(issue_dropdown_open, issue_active_option), onkeydown: move |event| { if event.key() == Key::Escape { close_worklog_dialog(submission, dialog); } },
        div { class: "dialog", role: "dialog", aria_modal: "true", aria_labelledby: "dialog-title", tabindex: "-1", onclick: move |event| {
            close_issue_dropdown(issue_dropdown_open, issue_active_option);
            event.stop_propagation();
        },
            DialogHeader { step, close_disabled: submission() == WorklogSubmission::Pending, on_close: move |_| close_worklog_dialog(submission, dialog) }
            if step == DialogStep::Form {
                WorklogForm { dialog, draft, validation_error, issue_results, issue_searching, issue_loaded, issue_search_error, issue_dropdown_open, issue_active_option, worklogs: worklogs.clone(), today, demo }
            } else {
                WorklogReview { dialog, notice, draft, demo, editing, worklogs, submission, on_create, on_update }
            }
        }
    } }
}

#[component]
fn DialogHeader(
    step: DialogStep,
    close_disabled: bool,
    on_close: EventHandler<MouseEvent>,
) -> Element {
    let step_label = if step == DialogStep::Form {
        text("worklog.stepData")
    } else {
        text("worklog.stepReview")
    };
    rsx! { div { class: "dialog-header", div { span { class: "eyebrow", "{step_label}" } h2 { id: "dialog-title", {text("worklog.dialogTitle")} } }
        button { class: "icon-button", disabled: close_disabled, aria_label: text("action.close"), onclick: on_close, "×" }
    } }
}

fn close_worklog_dialog(
    mut submission: Signal<WorklogSubmission>,
    mut dialog: Signal<Option<DialogStep>>,
) {
    if submission() != WorklogSubmission::Pending {
        submission.set(WorklogSubmission::Idle);
        dialog.set(None);
    }
}

#[component]
fn WorklogForm(
    mut dialog: Signal<Option<DialogStep>>,
    mut draft: Signal<WorklogDraft>,
    mut validation_error: Signal<Option<String>>,
    issue_results: Signal<Vec<SuggestedIssue>>,
    issue_searching: Signal<bool>,
    issue_loaded: Signal<bool>,
    issue_search_error: Signal<Option<String>>,
    issue_dropdown_open: Signal<bool>,
    issue_active_option: Signal<Option<usize>>,
    worklogs: Vec<Worklog>,
    today: Date,
    demo: bool,
) -> Element {
    let hours_help = numeric_range(product_defaults().hours().maximum_daily_hours);
    rsx! { form { class: "worklog-form", onsubmit: move |event| review_draft(&event, draft, dialog, validation_error),
        IssueSearchField { draft, issue_results, issue_searching, issue_loaded, issue_search_error, dropdown_open: issue_dropdown_open, active_option: issue_active_option, worklogs, demo }
        div { class: "field-row",
            Field { id: "date", label: text("worklog.date").to_owned(), help: text("worklog.dateHelp").to_owned(), control: rsx! { input { id: "date", r#type: "date", required: true, max: today.to_string(), value: draft().date, oninput: move |event| draft.write().date = event.value() } } }
            div { class: "duration-fields",
                Field { id: "hours", label: text("worklog.hours").to_owned(), help: hours_help, control: rsx! { input { id: "hours", r#type: "number", min: "0", max: product_defaults().hours().maximum_daily_hours.to_string(), required: true, value: draft().hours, oninput: move |event| draft.write().hours = event.value() } } }
                Field { id: "minutes", label: text("worklog.minutes").to_owned(), help: numeric_range(MAX_MINUTE_COMPONENT), control: rsx! { input { id: "minutes", r#type: "number", min: "0", max: MAX_MINUTE_COMPONENT.to_string(), required: true, value: draft().minutes, oninput: move |event| draft.write().minutes = event.value() } } }
            }
        }
        Field { id: "comment", label: text("worklog.comment").to_owned(), help: text("worklog.commentHelp").to_owned(), control: rsx! { textarea { id: "comment", rows: "3", value: draft().comment, oninput: move |event| draft.write().comment = event.value() } } }
        if let Some(message) = validation_error() { div { class: "setup-error", role: "alert", "{message}" } }
        div { class: "dialog-actions", button { r#type: "button", class: "button ghost", onclick: move |_| dialog.set(None), {text("action.cancel")} }
            button { r#type: "submit", class: "button primary", {text("action.review")} }
        }
    } }
}

fn numeric_range(maximum: impl std::fmt::Display) -> String {
    format!("0{}{maximum}", text("duration.rangeSeparator"))
}

#[component]
fn IssueSearchField(
    mut draft: Signal<WorklogDraft>,
    issue_results: Signal<Vec<SuggestedIssue>>,
    issue_searching: Signal<bool>,
    issue_loaded: Signal<bool>,
    issue_search_error: Signal<Option<String>>,
    dropdown_open: Signal<bool>,
    active_option: Signal<Option<usize>>,
    worklogs: Vec<Worklog>,
    demo: bool,
) -> Element {
    let active_option_id = active_option()
        .map(|index| format!("issue-option-{index}"))
        .unwrap_or_default();
    let action = if issue_searching() {
        text("worklog.issueSearching")
    } else {
        text("worklog.issueSearch")
    };
    rsx! { div { class: "field issue-search", onclick: move |event| event.stop_propagation(), onkeydown: move |event| close_issue_dropdown_on_escape(&event, dropdown_open, active_option),
        label { r#for: "issue", {text("worklog.issue")} }
        div { class: "issue-combobox",
            div { class: "issue-search-control",
                input { id: "issue", name: "issue", role: "combobox", required: true, autofocus: true, autocomplete: "off", spellcheck: "false", aria_autocomplete: "list", aria_haspopup: "listbox", aria_expanded: dropdown_open(), aria_controls: "issue-options", aria_describedby: "issue-help issue-error", placeholder: text("worklog.issuePlaceholder"), value: draft().issue_key,
                    aria_activedescendant: active_option_id,
                    onfocus: move |_| open_dropdown(dropdown_open),
                    oninput: move |event| update_issue_query(event.value(), draft, dropdown_open, active_option),
                    onkeydown: move |event| handle_issue_key(&event, issue_results(), draft, dropdown_open, active_option)
                }
                button { class: "icon-button search-action", r#type: "button", aria_label: action, title: action, disabled: issue_searching() || demo, onclick: move |_| {
                    open_dropdown(dropdown_open);
                    start_issue_search(draft().issue_key, worklogs.clone(), issue_results, issue_searching, issue_search_error);
                }, Icon { kind: IconKind::Search } }
                button { class: "icon-button dropdown-action", r#type: "button", aria_label: text("worklog.issueToggle"), title: text("worklog.issueToggle"), aria_haspopup: "listbox", aria_controls: "issue-options", aria_expanded: dropdown_open(), onclick: move |_| toggle_dropdown(dropdown_open),
                    Icon { kind: IconKind::ChevronDown }
                }
            }
            if dropdown_open() {
                IssueDropdown { results: issue_results(), query: draft().issue_key, searching: issue_searching(), loaded: issue_loaded(), draft, dropdown_open, active_option }
            }
        }
        small { id: "issue-help", {text("worklog.issueHelp")} }
        if demo { small { {text("worklog.issueDemoHelp")} } }
        if let Some(error) = issue_search_error() { span { id: "issue-error", class: "field-error", role: "alert", "{error}" } }
    } }
}

#[component]
fn IssueDropdown(
    results: Vec<SuggestedIssue>,
    query: String,
    searching: bool,
    loaded: bool,
    mut draft: Signal<WorklogDraft>,
    mut dropdown_open: Signal<bool>,
    mut active_option: Signal<Option<usize>>,
) -> Element {
    let visible = filter_suggestions(results, &query);
    rsx! { div { class: "issue-dropdown", id: "issue-options",
        if searching { div { class: "dropdown-status", role: "status", {text("worklog.issueSearching")} } }
        if !searching && loaded && visible.is_empty() { div { class: "dropdown-status", {text("worklog.issueAssignedEmpty")} } }
        if !visible.is_empty() { ul { class: "issue-results", role: "listbox", aria_label: text("worklog.issueResults"),
            for (index, suggestion) in visible.into_iter().enumerate() { li { role: "presentation",
                button { id: "issue-option-{index}", r#type: "button", role: "option", aria_selected: active_option() == Some(index), onmouseenter: move |_| active_option.set(Some(index)), onclick: move |_| select_issue(&suggestion.issue.key, draft, dropdown_open, active_option),
                    div { strong { "{suggestion.issue.key}" } span { class: "issue-hours", "{suggestion_hours(suggestion.loaded_seconds)}" } }
                    span { "{suggestion.issue.summary}" }
                }
            } }
        } }
    } }
}

fn update_issue_query(
    value: String,
    mut draft: Signal<WorklogDraft>,
    mut dropdown_open: Signal<bool>,
    mut active_option: Signal<Option<usize>>,
) {
    draft.write().issue_key = value;
    dropdown_open.set(true);
    active_option.set(None);
}

fn select_issue(
    issue_key: &str,
    mut draft: Signal<WorklogDraft>,
    mut dropdown_open: Signal<bool>,
    mut active_option: Signal<Option<usize>>,
) {
    issue_key.clone_into(&mut draft.write().issue_key);
    dropdown_open.set(false);
    active_option.set(None);
}

fn close_issue_dropdown(mut dropdown_open: Signal<bool>, mut active_option: Signal<Option<usize>>) {
    dropdown_open.set(false);
    active_option.set(None);
}

fn handle_issue_key(
    event: &KeyboardEvent,
    results: Vec<SuggestedIssue>,
    draft: Signal<WorklogDraft>,
    dropdown_open: Signal<bool>,
    active_option: Signal<Option<usize>>,
) {
    let visible = filter_suggestions(results, &draft().issue_key);
    match event.key() {
        Key::ArrowDown => move_active_option(event, visible.len(), 1, dropdown_open, active_option),
        Key::ArrowUp => move_active_option(event, visible.len(), -1, dropdown_open, active_option),
        Key::Enter => select_active_option(event, &visible, draft, dropdown_open, active_option),
        _ => {}
    }
}

fn close_issue_dropdown_on_escape(
    event: &KeyboardEvent,
    dropdown_open: Signal<bool>,
    active_option: Signal<Option<usize>>,
) {
    if event.key() != Key::Escape || !dropdown_open() {
        return;
    }
    event.stop_propagation();
    close_issue_dropdown(dropdown_open, active_option);
}

fn move_active_option(
    event: &KeyboardEvent,
    option_count: usize,
    direction: i8,
    mut dropdown_open: Signal<bool>,
    mut active_option: Signal<Option<usize>>,
) {
    event.prevent_default();
    dropdown_open.set(true);
    active_option.set(next_option(active_option(), option_count, direction));
}

fn next_option(current: Option<usize>, option_count: usize, direction: i8) -> Option<usize> {
    if option_count == 0 {
        return None;
    }
    let current = current.unwrap_or(if direction > 0 { option_count - 1 } else { 0 });
    if direction > 0 {
        return Some((current + 1) % option_count);
    }
    Some(current.checked_sub(1).unwrap_or(option_count - 1))
}

fn select_active_option(
    event: &KeyboardEvent,
    visible: &[SuggestedIssue],
    draft: Signal<WorklogDraft>,
    dropdown_open: Signal<bool>,
    active_option: Signal<Option<usize>>,
) {
    let Some(suggestion) = active_option().and_then(|index| visible.get(index)) else {
        return;
    };
    event.prevent_default();
    select_issue(&suggestion.issue.key, draft, dropdown_open, active_option);
}

fn open_dropdown(mut dropdown_open: Signal<bool>) {
    dropdown_open.set(true);
}

fn toggle_dropdown(mut dropdown_open: Signal<bool>) {
    let next = !dropdown_open();
    dropdown_open.set(next);
}

fn start_issue_search(
    query: String,
    worklogs: Vec<Worklog>,
    issue_results: Signal<Vec<SuggestedIssue>>,
    mut issue_searching: Signal<bool>,
    mut issue_search_error: Signal<Option<String>>,
) {
    if query.trim().is_empty() {
        return issue_search_error.set(Some(text("worklog.issueSearchRequired").to_owned()));
    }
    issue_searching.set(true);
    issue_search_error.set(None);
    spawn(async move {
        match search_issues(query).await {
            Ok(results) => {
                apply_issue_results(results, &worklogs, issue_results, issue_search_error);
            }
            Err(error) => issue_search_error.set(Some(error)),
        }
        issue_searching.set(false);
    });
}

fn apply_issue_results(
    results: Vec<AccessibleIssue>,
    worklogs: &[Worklog],
    mut issue_results: Signal<Vec<SuggestedIssue>>,
    mut issue_search_error: Signal<Option<String>>,
) {
    if results.is_empty() {
        return issue_search_error.set(Some(text("worklog.issueNoResults").to_owned()));
    }
    issue_results.set(suggestions_for(results, worklogs));
}

async fn load_assigned_suggestions(
    demo: bool,
    issue_results: Signal<Vec<SuggestedIssue>>,
    mut issue_searching: Signal<bool>,
    mut issue_loaded: Signal<bool>,
    mut issue_search_error: Signal<Option<String>>,
) {
    if demo {
        issue_loaded.set(true);
        return;
    }
    issue_searching.set(true);
    let result = unlogged_assigned_sprint_issues().await;
    if let Err(error) = apply_assigned_result(result, issue_results) {
        issue_search_error.set(Some(error));
    }
    issue_searching.set(false);
    issue_loaded.set(true);
}

fn apply_assigned_result(
    result: Result<Vec<AccessibleIssue>, String>,
    mut issue_results: Signal<Vec<SuggestedIssue>>,
) -> Result<(), String> {
    issue_results.set(suggestions_for(result?, &[]));
    Ok(())
}

fn suggestions_for(issues: Vec<AccessibleIssue>, worklogs: &[Worklog]) -> Vec<SuggestedIssue> {
    issues
        .into_iter()
        .map(|issue| SuggestedIssue {
            loaded_seconds: loaded_seconds_for(&issue.key, worklogs),
            issue,
        })
        .collect()
}

fn loaded_seconds_for(issue_key: &str, worklogs: &[Worklog]) -> u32 {
    worklogs
        .iter()
        .filter(|worklog| worklog.issue_key.as_str() == issue_key)
        .fold(0, |total, worklog| {
            total.saturating_add(worklog.duration.seconds())
        })
}

fn filter_suggestions(results: Vec<SuggestedIssue>, query: &str) -> Vec<SuggestedIssue> {
    let normalized_query = query.trim().to_lowercase();
    if normalized_query.is_empty() {
        return results;
    }
    results
        .into_iter()
        .filter(|suggestion| suggestion_matches(suggestion, &normalized_query))
        .collect()
}

fn suggestion_matches(suggestion: &SuggestedIssue, normalized_query: &str) -> bool {
    suggestion
        .issue
        .key
        .to_lowercase()
        .contains(normalized_query)
        || suggestion
            .issue
            .summary
            .to_lowercase()
            .contains(normalized_query)
}

fn suggestion_hours(seconds: u32) -> String {
    if seconds == 0 {
        return text("worklog.issueNoHours").to_owned();
    }
    format_hours(seconds)
}

#[component]
fn Field(id: &'static str, label: String, help: String, control: Element) -> Element {
    rsx! { div { class: "field", label { r#for: id, "{label}" } {control}
        small { id: "{id}-help", "{help}" }
    } }
}

#[component]
fn WorklogReview(
    mut dialog: Signal<Option<DialogStep>>,
    mut notice: Signal<bool>,
    draft: Signal<WorklogDraft>,
    demo: bool,
    editing: Signal<Option<Worklog>>,
    worklogs: Vec<Worklog>,
    submission: Signal<WorklogSubmission>,
    on_create: EventHandler<CreateWorklogCommand>,
    on_update: EventHandler<UpdateWorklogCommand>,
) -> Element {
    let command = build_command(&draft.read()).expect("the form validated the entry");
    let current_worklog = editing();
    let duplicate = possible_duplicate(&command, current_worklog.as_ref(), &worklogs);
    let mode = if demo {
        text("worklog.demoMode")
    } else {
        text("worklog.liveMode")
    };
    let is_editing = current_worklog.is_some();
    let submitting = submission() == WorklogSubmission::Pending;
    let submission_error = match submission() {
        WorklogSubmission::Failed(message) => Some(message),
        _ => None,
    };
    let action = if submitting {
        text("worklog.saving")
    } else if demo {
        text("worklog.simulate")
    } else if is_editing {
        text("worklog.confirmUpdate")
    } else {
        text("worklog.confirmCreate")
    };
    rsx! { div { class: "review",
        div { class: "demo-chip", "{mode}" }
        h3 { {text("worklog.reviewTitle")} }
        dl { ReviewRow { label: text("worklog.reviewIssue"), value: command.issue_key.clone() }
            ReviewRow { label: text("worklog.reviewDate"), value: command.date.clone() }
            ReviewRow { label: text("worklog.reviewDuration"), value: format_minutes(command.minutes) }
            ReviewRow { label: text("worklog.reviewComment"), value: display_comment(&command.comment) }
        }
        if duplicate {
            div { class: "duplicate-warning", role: "status",
                strong { {text("worklog.duplicateTitle")} }
                span { {text("worklog.duplicateWarning")} }
            }
        }
        if let Some(message) = submission_error {
            div { class: "setup-error", role: "alert", "{message}" }
        }
        div { class: "dialog-actions", button { class: "button ghost", disabled: submitting, onclick: move |_| dialog.set(Some(DialogStep::Form)), {text("action.back")} }
            button { class: "button primary", disabled: submitting, onclick: move |_| finish_worklog(command.clone(), current_worklog.clone(), demo, dialog, notice, on_create, on_update), "{action}" }
        }
    } }
}

fn possible_duplicate(
    command: &CreateWorklogCommand,
    editing: Option<&Worklog>,
    worklogs: &[Worklog],
) -> bool {
    let Ok(issue_key) = IssueKey::new(&command.issue_key) else {
        return false;
    };
    let Ok(duration) = Duration::from_minutes(command.minutes) else {
        return false;
    };
    let Ok(date) = parse_iso_date(&command.date) else {
        return false;
    };
    let excluded_id = editing.map(|worklog| worklog.id.as_str());
    PossibleDuplicatePolicy::matches(worklogs, &issue_key, date, duration, excluded_id)
}

fn parse_iso_date(value: &str) -> Result<Date, String> {
    let format = format_description::parse(ISO_DATE_FORMAT)
        .map_err(|_| text("period.invalidDate").to_owned())?;
    Date::parse(value, &format).map_err(|_| text("period.invalidDate").to_owned())
}

fn finish_worklog(
    command: CreateWorklogCommand,
    editing: Option<Worklog>,
    demo: bool,
    mut dialog: Signal<Option<DialogStep>>,
    mut notice: Signal<bool>,
    on_create: EventHandler<CreateWorklogCommand>,
    on_update: EventHandler<UpdateWorklogCommand>,
) {
    if demo {
        dialog.set(None);
        return notice.set(true);
    }
    let Some(worklog) = editing else {
        return on_create.call(command);
    };
    on_update.call(UpdateWorklogCommand {
        issue_key: command.issue_key,
        worklog_id: worklog.id,
        date: command.date,
        minutes: command.minutes,
        comment: command.comment,
        original_started: worklog.started,
        original_duration_seconds: worklog.duration.seconds(),
        preserve_original_duration: command.minutes
            == worklog.duration.seconds() / MINUTES_PER_HOUR,
    });
}

#[component]
fn ReviewRow(label: &'static str, value: String) -> Element {
    rsx! { div { dt { "{label}" } dd { "{value}" } } }
}

fn review_draft(
    event: &FormEvent,
    draft: Signal<WorklogDraft>,
    mut dialog: Signal<Option<DialogStep>>,
    mut validation_error: Signal<Option<String>>,
) {
    event.prevent_default();
    match build_command(&draft.read()) {
        Ok(_) => dialog.set(Some(DialogStep::Review)),
        Err(error) => validation_error.set(Some(error)),
    }
}

fn build_command(draft: &WorklogDraft) -> Result<CreateWorklogCommand, String> {
    let maximum_hours = u32::from(product_defaults().hours().maximum_daily_hours);
    let hours = parse_duration_part(&draft.hours, maximum_hours, text("duration.hoursName"))?;
    let minutes = parse_duration_part(
        &draft.minutes,
        MAX_MINUTE_COMPONENT,
        text("duration.minutesName"),
    )?;
    let total = hours
        .checked_mul(MINUTES_PER_HOUR)
        .and_then(|value| value.checked_add(minutes))
        .ok_or_else(|| text("duration.tooLarge").to_owned())?;
    if total == 0 || draft.issue_key.trim().is_empty() || draft.date.trim().is_empty() {
        return Err(text("duration.required").to_owned());
    }
    IssueKey::new(&draft.issue_key).map_err(|_| text("worklog.issueInvalid").to_owned())?;
    Ok(CreateWorklogCommand {
        issue_key: draft.issue_key.trim().to_owned(),
        date: draft.date.clone(),
        minutes: total,
        comment: draft.comment.trim().to_owned(),
    })
}

fn parse_duration_part(value: &str, maximum: u32, label: &str) -> Result<u32, String> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| format!("{}{label}.", text("duration.invalidPrefix")))?;
    if parsed > maximum {
        return Err(format!(
            "{}{label}{}",
            text("duration.maximumPrefix"),
            text("duration.maximumSuffix")
        ));
    }
    Ok(parsed)
}

fn format_minutes(minutes: u32) -> String {
    format_hours(minutes.saturating_mul(MINUTES_PER_HOUR))
}

fn display_comment(comment: &str) -> String {
    if comment.is_empty() {
        return text("worklog.noComment").to_owned();
    }
    comment.to_owned()
}

fn format_hours(seconds: u32) -> String {
    let hours = seconds / SECONDS_PER_HOUR;
    let minutes = seconds % SECONDS_PER_HOUR / MINUTES_PER_HOUR;
    if minutes == 0 {
        return format!("{hours} {}", text("unit.hour"));
    }
    format!(
        "{hours} {} {minutes} {}",
        text("unit.hour"),
        text("unit.minute")
    )
}

fn format_period(period: DateRange) -> String {
    let start = period.start();
    let end = period.end();
    if start.month() == end.month() {
        return format!("{}–{} {}", start.day(), end.day(), month_label(end.month()));
    }
    format!(
        "{} {}–{} {}",
        start.day(),
        month_label(start.month()),
        end.day(),
        month_label(end.month())
    )
}

fn loaded_period_label(period: DateRange, today: Date) -> String {
    if period == current_week_to_date(today) {
        return text("period.loadedCurrentWeek").to_owned();
    }
    if period == current_month(today) {
        return text("period.loadedCurrentMonth").to_owned();
    }
    format!(
        "{}{}",
        text("period.loadedPrefix"),
        format_short_period(period)
    )
}

fn format_short_period(period: DateRange) -> String {
    let start = period.start();
    let end = period.end();
    let end_month = month_label(end.month()).to_lowercase();
    if start.month() == end.month() {
        return format!("{}–{} {end_month}", start.day(), end.day());
    }
    let start_month = month_label(start.month()).to_lowercase();
    format!("{} {start_month} – {} {end_month}", start.day(), end.day())
}

fn month_label(month: Month) -> &'static str {
    match month {
        Month::January => text("month.january"),
        Month::February => text("month.february"),
        Month::March => text("month.march"),
        Month::April => text("month.april"),
        Month::May => text("month.may"),
        Month::June => text("month.june"),
        Month::July => text("month.july"),
        Month::August => text("month.august"),
        Month::September => text("month.september"),
        Month::October => text("month.october"),
        Month::November => text("month.november"),
        Month::December => text("month.december"),
    }
}

fn weekday_label(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Monday => text("weekday.monday"),
        Weekday::Tuesday => text("weekday.tuesday"),
        Weekday::Wednesday => text("weekday.wednesday"),
        Weekday::Thursday => text("weekday.thursday"),
        Weekday::Friday => text("weekday.friday"),
        Weekday::Saturday => text("weekday.saturday"),
        Weekday::Sunday => text("weekday.sunday"),
    }
}

fn chart_description(days: &[DailyHours]) -> String {
    days.iter()
        .map(|day| {
            format!(
                "{} {}",
                weekday_label(day.weekday),
                format_hours(day.duration_seconds)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn live_source_label(warning_count: usize) -> String {
    if warning_count == 0 {
        return text("status.sourceComplete").to_owned();
    }
    format!(
        "{}{warning_count}{}",
        text("status.sourceWarningPrefix"),
        text("status.sourceWarningSuffix")
    )
}

fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|part| part.chars().next())
        .take(MAX_INITIALS)
        .collect::<String>()
        .to_uppercase()
}

#[cfg(test)]
mod tests {
    use time::{Date, Month};

    use super::{
        NavigationTarget, RangePreset, bundled_modules, date_range, initial_navigation,
        loaded_period_label, next_option, parse_range, preset_range, shifted_period,
        suggestions_for,
    };
    use crate::connection_model::AccessibleIssue;

    #[test]
    fn navigation_starts_at_home_and_cards_only_list_modules() {
        assert_eq!(initial_navigation(), NavigationTarget::Home);
        assert!(
            bundled_modules()
                .iter()
                .all(|module| module.target != NavigationTarget::Home)
        );
    }

    #[test]
    fn weekly_presets_never_extend_beyond_today() {
        let today = date(2026, Month::September, 2);
        let current = preset_range(RangePreset::CurrentWeek, today);
        let previous = preset_range(RangePreset::PreviousWeek, today);
        assert_eq!(current.start(), date(2026, Month::August, 31));
        assert_eq!(current.end(), today);
        assert_eq!(previous.start(), date(2026, Month::August, 24));
        assert_eq!(previous.end(), date(2026, Month::August, 30));
    }

    #[test]
    fn extended_presets_cross_boundaries_safely() {
        let today = date(2026, Month::September, 2);
        let trailing = preset_range(RangePreset::LastFourteenDays, today);
        let month = preset_range(RangePreset::CurrentMonth, today);
        assert_eq!(trailing.start(), date(2026, Month::August, 20));
        assert_eq!(trailing.end(), today);
        assert_eq!(month.start(), date(2026, Month::September, 1));
        assert_eq!(month.end(), today);
    }

    #[test]
    fn weekly_navigation_returns_to_the_current_partial_week() {
        let today = date(2026, Month::September, 2);
        let previous = preset_range(RangePreset::PreviousWeek, today);
        let current = shifted_period(previous, 1, today).expect("current week is available");
        assert_eq!(current, preset_range(RangePreset::CurrentWeek, today));
    }

    #[test]
    fn manual_ranges_reject_future_dates() {
        let today = date(2026, Month::September, 2);
        let result = parse_range("2026-09-01", "2026-09-03", today);
        assert!(result.is_err());
    }

    #[test]
    fn loaded_metric_describes_the_selected_period() {
        let today = date(2026, Month::September, 2);
        let current = preset_range(RangePreset::CurrentWeek, today);
        let custom = date_range(
            date(2026, Month::August, 19),
            date(2026, Month::September, 1),
        );
        assert_eq!(loaded_period_label(current, today), "Cargadas esta semana");
        assert_eq!(
            loaded_period_label(custom, today),
            "Cargadas 19 ago – 1 sep"
        );
    }

    #[test]
    fn issue_keyboard_navigation_wraps_both_directions() {
        assert_eq!(next_option(None, 3, 1), Some(0));
        assert_eq!(next_option(Some(2), 3, 1), Some(0));
        assert_eq!(next_option(None, 3, -1), Some(2));
        assert_eq!(next_option(Some(0), 3, -1), Some(2));
        assert_eq!(next_option(Some(0), 0, 1), None);
    }

    #[test]
    fn issue_suggestions_preserve_jiras_recent_first_order() {
        let issues = vec![issue("DEMO-2"), issue("DEMO-1")];
        let suggestions = suggestions_for(issues, &[]);
        let keys = suggestions
            .iter()
            .map(|suggestion| suggestion.issue.key.as_str())
            .collect::<Vec<_>>();
        assert_eq!(keys, vec!["DEMO-2", "DEMO-1"]);
    }

    fn issue(key: &str) -> AccessibleIssue {
        AccessibleIssue {
            key: key.to_owned(),
            summary: "Tarea".to_owned(),
        }
    }

    fn date(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("test date is valid")
    }
}
