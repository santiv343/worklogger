use dioxus::prelude::*;

use crate::board_selector::BoardSelector;
use crate::connection::discover_configured_boards;
#[cfg(feature = "reports")]
use crate::connection::team_report_permission_granted;
use crate::connection_model::{
    AccessibleBoard, ConfigurationUpdate, ConnectionConfiguration, HoursConfiguration,
    JiraConfigurationUpdate, ReportsConfiguration,
};
use crate::copy::{preferred_language, save_language, text};
use crate::defaults::product_defaults;
#[cfg(organization_configuration_mutable)]
use crate::organization_config::OrganizationConfigurationPicker;
#[cfg(feature = "mcp-management")]
use worklogger_mcp::{McpClientId, McpClientStatus, RegistrationState};

const MAXIMUM_CLOCK_HOUR: u8 = 23;
const MAXIMUM_CLOCK_MINUTE: u8 = 59;

#[derive(Clone, PartialEq)]
struct ConfigurationDraft {
    replacement_token: String,
    jira: JiraDraft,
    hours: HoursDraft,
    reports: ReportsDraft,
}

#[derive(Clone, PartialEq)]
struct JiraDraft {
    board_id: String,
    request_timeout_seconds: String,
    page_size: String,
    maximum_collection_items: String,
    maximum_issue_search_results: String,
    maximum_concurrent_worklog_requests: String,
}

#[derive(Clone, PartialEq)]
struct HoursDraft {
    weekly_target_hours: String,
    utc_offset_minutes: String,
    maximum_daily_hours: String,
    maximum_report_period_days: String,
    default_worklog_start_hour: String,
    default_worklog_start_minute: String,
}

#[derive(Clone, PartialEq)]
struct ReportsDraft {
    enable_team_reports: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum SettingsPage {
    General,
    #[cfg(feature = "mcp-management")]
    Mcp,
    Jira,
    #[cfg(feature = "reports")]
    Reports,
}

#[cfg(feature = "mcp-management")]
#[derive(Clone, Copy, PartialEq)]
enum McpClientAction {
    Register,
    Update,
    Unregister,
}

#[cfg(feature = "mcp-management")]
#[derive(Clone, PartialEq)]
struct PendingMcpClientChange {
    client: McpClientId,
    action: McpClientAction,
    target: String,
}

#[derive(Clone, PartialEq)]
enum BoardOptionsState {
    Loading,
    Ready(Vec<AccessibleBoard>),
    Failed(String),
}

impl From<&ConnectionConfiguration> for ConfigurationDraft {
    fn from(configuration: &ConnectionConfiguration) -> Self {
        Self {
            replacement_token: String::new(),
            jira: JiraDraft::from(configuration),
            hours: HoursDraft::from(configuration),
            reports: ReportsDraft::from(configuration),
        }
    }
}

impl From<&ConnectionConfiguration> for JiraDraft {
    fn from(configuration: &ConnectionConfiguration) -> Self {
        Self {
            board_id: configuration.jira.board_id.to_string(),
            request_timeout_seconds: configuration.jira.request_timeout_seconds.to_string(),
            page_size: configuration.jira.page_size.to_string(),
            maximum_collection_items: configuration.jira.maximum_collection_items.to_string(),
            maximum_issue_search_results: configuration
                .jira
                .maximum_issue_search_results
                .to_string(),
            maximum_concurrent_worklog_requests: configuration
                .jira
                .maximum_concurrent_worklog_requests
                .to_string(),
        }
    }
}

impl From<&ConnectionConfiguration> for HoursDraft {
    fn from(configuration: &ConnectionConfiguration) -> Self {
        Self {
            weekly_target_hours: configuration.hours.weekly_target_hours.to_string(),
            utc_offset_minutes: configuration.hours.utc_offset_minutes.to_string(),
            maximum_daily_hours: configuration.hours.maximum_daily_hours.to_string(),
            maximum_report_period_days: configuration.hours.maximum_report_period_days.to_string(),
            default_worklog_start_hour: configuration.hours.default_worklog_start_hour.to_string(),
            default_worklog_start_minute: configuration
                .hours
                .default_worklog_start_minute
                .to_string(),
        }
    }
}

impl From<&ConnectionConfiguration> for ReportsDraft {
    fn from(configuration: &ConnectionConfiguration) -> Self {
        Self {
            enable_team_reports: configuration.reports.enable_team_reports,
        }
    }
}

#[component]
pub(crate) fn PreferencesDialog(
    configuration: ConnectionConfiguration,
    permissions: Option<jira_adapter::ProjectPermissions>,
    mut open: Signal<bool>,
    submission: Signal<crate::ui::ConfigurationSubmission>,
    on_configuration_loaded: EventHandler<()>,
    on_save: EventHandler<ConfigurationUpdate>,
) -> Element {
    let draft = use_signal(|| ConfigurationDraft::from(&configuration));
    let boards = use_signal(|| BoardOptionsState::Loading);
    let error = use_signal(|| None::<String>);
    let page = use_signal(|| SettingsPage::General);
    let mcp_status = use_signal(crate::mcp_management::loading_status);
    #[cfg(feature = "mcp-management")]
    use_mcp_status(mcp_status, error);
    let current_submission = submission();
    let saving = current_submission == crate::ui::ConfigurationSubmission::Pending;
    let submission_error = configuration_submission_error(current_submission);
    let mut submission_effect = submission;
    let mut open_effect = open;
    let escape_submission = submission;
    use_effect(move || {
        if submission_effect() == crate::ui::ConfigurationSubmission::Succeeded {
            open_effect.set(false);
            submission_effect.set(crate::ui::ConfigurationSubmission::Idle);
        }
    });
    let mut board_options = boards;
    use_future(move || async move {
        board_options.set(match discover_configured_boards().await {
            Ok(boards) => BoardOptionsState::Ready(boards),
            Err(message) => BoardOptionsState::Failed(message),
        });
    });
    rsx! { div { class: "dialog-overlay", onkeydown: move |event| { if event.key() == Key::Escape && !saving { close_preferences(open, escape_submission); } },
        form { class: "dialog settings-dialog", role: "dialog", aria_modal: "true", aria_labelledby: "preferences-title", aria_busy: saving, tabindex: "-1", onclick: move |event| event.stop_propagation(), onsubmit: move |event| submit(&event, saving, draft, error, on_save),
            DialogHeader { open, submission, saving }
            SettingsLayout { configuration, permissions, draft, page, boards, on_configuration_loaded, mcp_status }
            if let Some(message) = error() { div { class: "setup-error", role: "alert", "{message}" } }
            if let Some(message) = submission_error { div { class: "setup-error", role: "alert", "{message}" } }
            DialogActions { open, submission, saving }
        }
    } }
}

fn configuration_submission_error(
    submission: crate::ui::ConfigurationSubmission,
) -> Option<String> {
    match submission {
        crate::ui::ConfigurationSubmission::Failed(message) => Some(message),
        _ => None,
    }
}

fn close_preferences(
    mut open: Signal<bool>,
    mut submission: Signal<crate::ui::ConfigurationSubmission>,
) {
    submission.set(crate::ui::ConfigurationSubmission::Idle);
    open.set(false);
}

#[component]
fn DialogHeader(
    open: Signal<bool>,
    submission: Signal<crate::ui::ConfigurationSubmission>,
    saving: bool,
) -> Element {
    rsx! { header { class: "dialog-header",
        div { span { class: "eyebrow", {text("preferences.eyebrow")} }
            h2 { id: "preferences-title", {text("preferences.title")} }
        }
        button { class: "icon-button", r#type: "button", aria_label: text("action.close"), autofocus: true, disabled: saving, onclick: move |_| close_preferences(open, submission), "×" }
    } }
}

#[component]
fn DialogActions(
    open: Signal<bool>,
    submission: Signal<crate::ui::ConfigurationSubmission>,
    saving: bool,
) -> Element {
    rsx! { div { class: "dialog-actions",
        button { class: "button ghost", r#type: "button", disabled: saving, onclick: move |_| close_preferences(open, submission), {text("action.cancel")} }
        button { class: "button primary", r#type: "submit", disabled: saving, if saving { {text("preferences.saving")} } else { {text("preferences.save")} } }
    } }
}

#[component]
fn SettingsLayout(
    configuration: ConnectionConfiguration,
    permissions: Option<jira_adapter::ProjectPermissions>,
    draft: Signal<ConfigurationDraft>,
    page: Signal<SettingsPage>,
    boards: Signal<BoardOptionsState>,
    on_configuration_loaded: EventHandler<()>,
    mcp_status: Signal<crate::mcp_management::McpStatus>,
) -> Element {
    let content = settings_content(
        configuration,
        permissions,
        draft,
        page(),
        boards,
        on_configuration_loaded,
        mcp_status,
    );
    rsx! { div { class: "settings-layout",
        SettingsNavigation { page }
        div { class: "settings-content",
            {content}
            p { class: "safety-copy", {text("preferences.scope")} }
        }
    } }
}

fn settings_content(
    configuration: ConnectionConfiguration,
    permissions: Option<jira_adapter::ProjectPermissions>,
    draft: Signal<ConfigurationDraft>,
    page: SettingsPage,
    boards: Signal<BoardOptionsState>,
    on_configuration_loaded: EventHandler<()>,
    mcp_status: Signal<crate::mcp_management::McpStatus>,
) -> Element {
    #[cfg(not(feature = "mcp-management"))]
    let _ = mcp_status;
    match page {
        SettingsPage::General => {
            rsx! { GeneralSettings { draft, on_configuration_loaded } }
        }
        #[cfg(feature = "mcp-management")]
        SettingsPage::Mcp => rsx! { McpSettings { configuration, status: mcp_status } },
        SettingsPage::Jira => rsx! { JiraSettings { configuration, permissions, draft, boards } },
        #[cfg(feature = "reports")]
        SettingsPage::Reports => rsx! { ReportsSettings { draft, permissions } },
    }
}

#[component]
fn SettingsNavigation(mut page: Signal<SettingsPage>) -> Element {
    rsx! { aside { class: "settings-navigation", aria_label: text("preferences.navigationAria"),
        button { class: settings_button_class(page() == SettingsPage::General), r#type: "button", onclick: move |_| page.set(SettingsPage::General), {text("preferences.generalTitle")} }
        {mcp_navigation_button(page)}
        span { class: "settings-navigation-label", {text("preferences.installedModules")} }
        button { class: settings_button_class(page() == SettingsPage::Jira), r#type: "button", onclick: move |_| page.set(SettingsPage::Jira), {text("preferences.jiraTitle")} }
        {reports_navigation_button(page)}
    } }
}

#[cfg(feature = "mcp-management")]
fn mcp_navigation_button(mut page: Signal<SettingsPage>) -> Element {
    rsx! { button { class: settings_button_class(page() == SettingsPage::Mcp), r#type: "button", onclick: move |_| page.set(SettingsPage::Mcp), {text("preferences.mcpTitle")} } }
}

#[cfg(not(feature = "mcp-management"))]
fn mcp_navigation_button(_page: Signal<SettingsPage>) -> Element {
    rsx! {}
}

#[cfg(feature = "reports")]
fn reports_navigation_button(mut page: Signal<SettingsPage>) -> Element {
    if product_defaults().modules.reports.is_none() {
        return rsx! {};
    }
    rsx! { button { class: settings_button_class(page() == SettingsPage::Reports), r#type: "button", onclick: move |_| page.set(SettingsPage::Reports), {text("preferences.reportsTitle")} } }
}

#[cfg(not(feature = "reports"))]
fn reports_navigation_button(_page: Signal<SettingsPage>) -> Element {
    rsx! {}
}

fn settings_button_class(active: bool) -> &'static str {
    if active {
        "settings-navigation-button active"
    } else {
        "settings-navigation-button"
    }
}

#[component]
fn GeneralSettings(
    mut draft: Signal<ConfigurationDraft>,
    on_configuration_loaded: EventHandler<()>,
) -> Element {
    let options = product_defaults().hours().utc_offset_options.clone();
    let mut language = use_signal(preferred_language);
    rsx! { div { class: "settings-page",
        {organization_configuration(on_configuration_loaded)}
        section { class: "settings-section", aria_labelledby: "general-settings-title",
            SettingsHeading { id: "general-settings-title", mark: "W", title: text("preferences.generalTitle"), description: text("preferences.generalDescription"), module: false }
            label { class: "field", r#for: "preferences-language", span { {text("preferences.languageLabel")} }
                select { id: "preferences-language", value: language_value(language()), oninput: move |event| {
                    let selected = parse_language(&event.value());
                    if save_language(selected).is_ok() { language.set(selected); }
                },
                    option { value: "english", {text("preferences.languageEnglish")} }
                    option { value: "spanish", {text("preferences.languageSpanish")} }
                }
                small { {text("preferences.languageRestart")} }
            }
            label { class: "field", r#for: "preferences-offset", span { {text("setup.timeZoneLabel")} }
                select { id: "preferences-offset", value: draft().hours.utc_offset_minutes, oninput: move |event| draft.write().hours.utc_offset_minutes = event.value(),
                    for option in options { option { value: option.minutes.to_string(), "{option.label}" } }
                }
                small { {text("setup.timeZoneHelp")} }
            }
        }
    } }
}

fn language_value(language: worklogger_settings::Language) -> &'static str {
    match language {
        worklogger_settings::Language::English => "english",
        worklogger_settings::Language::Spanish => "spanish",
    }
}

fn parse_language(value: &str) -> worklogger_settings::Language {
    if value == "spanish" {
        return worklogger_settings::Language::Spanish;
    }
    worklogger_settings::Language::English
}

#[cfg(organization_configuration_mutable)]
fn organization_configuration(on_loaded: EventHandler<()>) -> Element {
    rsx! { OrganizationConfigurationPicker { initial_error: None, allow_export: true, on_loaded } }
}

#[cfg(not(organization_configuration_mutable))]
fn organization_configuration(_on_loaded: EventHandler<()>) -> Element {
    rsx! {}
}

#[cfg(feature = "mcp-management")]
#[component]
fn McpSettings(
    configuration: ConnectionConfiguration,
    mut status: Signal<crate::mcp_management::McpStatus>,
) -> Element {
    let error = use_signal(|| None::<String>);
    let pending_change = use_signal(|| None::<PendingMcpClientChange>);
    let current = status();
    let clients_loading = current.clients_loading;
    rsx! { div { class: "settings-page",
        div { class: "settings-page-heading", h3 { {text("preferences.mcpTitle")} } p { {text("preferences.mcpDescription")} } }
        if cfg!(feature = "mcp-jira") && organization_module_available(worklogger_mcp::ModuleId::Jira) { McpJiraSettings { configuration: configuration.clone(), current: current.clone(), status, error, pending_change } }
        if cfg!(feature = "mcp-bitbucket") && organization_module_available(worklogger_mcp::ModuleId::Bitbucket) { McpBitbucketSettings { configuration, current: current.clone(), status, error, pending_change } }
        McpRuntimeStatus { status: current.clone() }
        McpClientList { status: current, pending_change }
        if let Some(change) = pending_change() { McpClientConfirmation { change, busy: clients_loading, status, error, pending_change } }
        if let Some(message) = error() { div { class: "setup-error compact", role: "alert", "{message}" } }
    } }
}

#[cfg(feature = "mcp-management")]
#[component]
fn McpJiraSettings(
    configuration: ConnectionConfiguration,
    current: crate::mcp_management::McpStatus,
    status: Signal<crate::mcp_management::McpStatus>,
    error: Signal<Option<String>>,
    pending_change: Signal<Option<PendingMcpClientChange>>,
) -> Element {
    rsx! { section { class: "settings-section", aria_labelledby: "mcp-jira-title",
        SettingsHeading { id: "mcp-jira-title", mark: "J", title: text("preferences.mcpJiraTitle"), description: text("preferences.mcpJiraDescription"), module: true }
        CapabilityGroupHeading { title: text("preferences.mcpJiraHoursGroup") }
        for option in JIRA_HOURS_MCP_CAPABILITIES {
            McpCapabilitySwitch { configuration: configuration.clone(), option, enabled: current.enabled_capabilities.contains(&option.capability), disabled: !organization_allows(option.capability), status, error, pending_change }
        }
        CapabilityGroupHeading { title: text("preferences.mcpJiraIssuesGroup") }
        for option in JIRA_ISSUES_MCP_CAPABILITIES {
            McpCapabilitySwitch { configuration: configuration.clone(), option, enabled: current.enabled_capabilities.contains(&option.capability), disabled: !organization_allows(option.capability), status, error, pending_change }
        }
    } }
}

#[cfg(feature = "mcp-management")]
#[component]
fn McpBitbucketSettings(
    configuration: ConnectionConfiguration,
    current: crate::mcp_management::McpStatus,
    status: Signal<crate::mcp_management::McpStatus>,
    error: Signal<Option<String>>,
    pending_change: Signal<Option<PendingMcpClientChange>>,
) -> Element {
    let configured = current
        .configured_modules
        .contains(&worklogger_mcp::ModuleId::Bitbucket);
    rsx! { section { class: "settings-section", aria_labelledby: "mcp-bitbucket-title",
        SettingsHeading { id: "mcp-bitbucket-title", mark: "B", title: text("preferences.mcpBitbucketTitle"), description: text("preferences.mcpBitbucketDescription"), module: true }
        if !configured { p { class: "mcp-client-empty", {text("preferences.mcpBitbucketMissing")} } }
        for option in BITBUCKET_MCP_CAPABILITIES {
            McpCapabilitySwitch { configuration: configuration.clone(), option, enabled: current.enabled_capabilities.contains(&option.capability), disabled: !configured || !organization_allows(option.capability), status, error, pending_change }
        }
    } }
}

#[cfg(feature = "mcp-management")]
#[derive(Clone, Copy, PartialEq)]
struct McpCapabilityOption {
    capability: worklogger_mcp::Capability,
    id: &'static str,
    label_key: &'static str,
    help_key: &'static str,
}

#[cfg(feature = "mcp-management")]
fn organization_allows(capability: worklogger_mcp::Capability) -> bool {
    crate::defaults::product_defaults().allows(capability)
}

#[cfg(feature = "mcp-management")]
fn organization_module_available(module: worklogger_mcp::ModuleId) -> bool {
    let defaults = crate::defaults::product_defaults();
    match module {
        worklogger_mcp::ModuleId::Jira => defaults.modules.jira.is_some(),
        worklogger_mcp::ModuleId::Bitbucket => defaults.modules.bitbucket.is_some(),
    }
}

#[cfg(feature = "mcp-management")]
const JIRA_HOURS_MCP_CAPABILITIES: [McpCapabilityOption; 2] = [
    McpCapabilityOption {
        capability: worklogger_mcp::Capability::ReadOwnTimeEntries,
        id: "mcp-own-hours",
        label_key: "preferences.mcpOwnHoursLabel",
        help_key: "preferences.mcpOwnHoursHelp",
    },
    McpCapabilityOption {
        capability: worklogger_mcp::Capability::WriteOwnTimeEntries,
        id: "mcp-write-own-hours",
        label_key: "preferences.mcpWriteOwnHoursLabel",
        help_key: "preferences.mcpWriteOwnHoursHelp",
    },
];

#[cfg(feature = "mcp-management")]
const JIRA_ISSUES_MCP_CAPABILITIES: [McpCapabilityOption; 4] = [
    McpCapabilityOption {
        capability: worklogger_mcp::Capability::ReadJiraIssues,
        id: "mcp-jira-read",
        label_key: "preferences.mcpJiraReadLabel",
        help_key: "preferences.mcpJiraReadHelp",
    },
    McpCapabilityOption {
        capability: worklogger_mcp::Capability::EditJiraIssues,
        id: "mcp-jira-edit",
        label_key: "preferences.mcpJiraEditLabel",
        help_key: "preferences.mcpJiraEditHelp",
    },
    McpCapabilityOption {
        capability: worklogger_mcp::Capability::CommentJiraIssues,
        id: "mcp-jira-comment",
        label_key: "preferences.mcpJiraCommentLabel",
        help_key: "preferences.mcpJiraCommentHelp",
    },
    McpCapabilityOption {
        capability: worklogger_mcp::Capability::TransitionJiraIssues,
        id: "mcp-jira-transition",
        label_key: "preferences.mcpJiraTransitionLabel",
        help_key: "preferences.mcpJiraTransitionHelp",
    },
];

#[cfg(feature = "mcp-management")]
#[component]
fn CapabilityGroupHeading(title: &'static str) -> Element {
    rsx! { h4 { class: "mcp-capability-group-title", "{title}" } }
}

#[cfg(feature = "mcp-management")]
const BITBUCKET_MCP_CAPABILITIES: [McpCapabilityOption; 7] = [
    McpCapabilityOption {
        capability: worklogger_mcp::Capability::ReadBitbucketPullRequests,
        id: "mcp-bitbucket-read",
        label_key: "preferences.mcpBitbucketReadLabel",
        help_key: "preferences.mcpBitbucketReadHelp",
    },
    McpCapabilityOption {
        capability: worklogger_mcp::Capability::CreateBitbucketPullRequests,
        id: "mcp-bitbucket-create",
        label_key: "preferences.mcpBitbucketCreateLabel",
        help_key: "preferences.mcpBitbucketCreateHelp",
    },
    McpCapabilityOption {
        capability: worklogger_mcp::Capability::EditBitbucketPullRequests,
        id: "mcp-bitbucket-edit",
        label_key: "preferences.mcpBitbucketEditLabel",
        help_key: "preferences.mcpBitbucketEditHelp",
    },
    McpCapabilityOption {
        capability: worklogger_mcp::Capability::CommentBitbucketPullRequests,
        id: "mcp-bitbucket-comment",
        label_key: "preferences.mcpBitbucketCommentLabel",
        help_key: "preferences.mcpBitbucketCommentHelp",
    },
    McpCapabilityOption {
        capability: worklogger_mcp::Capability::ReviewBitbucketPullRequests,
        id: "mcp-bitbucket-review",
        label_key: "preferences.mcpBitbucketReviewLabel",
        help_key: "preferences.mcpBitbucketReviewHelp",
    },
    McpCapabilityOption {
        capability: worklogger_mcp::Capability::MergeBitbucketPullRequests,
        id: "mcp-bitbucket-merge",
        label_key: "preferences.mcpBitbucketMergeLabel",
        help_key: "preferences.mcpBitbucketMergeHelp",
    },
    McpCapabilityOption {
        capability: worklogger_mcp::Capability::DeclineBitbucketPullRequests,
        id: "mcp-bitbucket-decline",
        label_key: "preferences.mcpBitbucketDeclineLabel",
        help_key: "preferences.mcpBitbucketDeclineHelp",
    },
];

#[cfg(feature = "mcp-management")]
#[component]
fn McpCapabilitySwitch(
    configuration: ConnectionConfiguration,
    option: McpCapabilityOption,
    enabled: bool,
    disabled: bool,
    mut status: Signal<crate::mcp_management::McpStatus>,
    mut error: Signal<Option<String>>,
    mut pending_change: Signal<Option<PendingMcpClientChange>>,
) -> Element {
    rsx! { SwitchField {
        id: option.id,
        label: text(option.label_key),
        help: text(option.help_key),
        checked: enabled,
        disabled,
        on_change: move |enabled| configure_mcp_capability(&configuration, option.capability, enabled, status, error, pending_change),
    } }
}

#[cfg(feature = "mcp-management")]
fn configure_mcp_capability(
    configuration: &ConnectionConfiguration,
    capability: worklogger_mcp::Capability,
    enabled: bool,
    mut status: Signal<crate::mcp_management::McpStatus>,
    mut error: Signal<Option<String>>,
    mut pending_change: Signal<Option<PendingMcpClientChange>>,
) {
    pending_change.set(None);
    match crate::mcp_management::configure(configuration, capability, enabled) {
        Ok(next) => {
            status.set(next);
            error.set(None);
            refresh_mcp_status(status, error);
        }
        Err(message) => error.set(Some(message)),
    }
}

#[cfg(feature = "mcp-management")]
#[component]
fn McpClientList(
    status: crate::mcp_management::McpStatus,
    pending_change: Signal<Option<PendingMcpClientChange>>,
) -> Element {
    let clients_loading = status.clients_loading;
    let clients = status
        .clients
        .into_iter()
        .filter(|client| client.state != RegistrationState::Unavailable)
        .collect::<Vec<_>>();
    rsx! { div { class: "mcp-client-section", aria_busy: clients_loading,
        div { class: "mcp-client-heading", strong { {text("preferences.mcpClientsTitle")} } small { {text("preferences.mcpClientsDescription")} } }
        if clients_loading { p { class: "mcp-client-empty", {text("preferences.mcpClientsLoading")} } }
        else if clients.is_empty() { p { class: "mcp-client-empty", {text("preferences.mcpClientsEmpty")} } }
        for client in clients { McpClientRow { client, server_ready: status.binary_available && status.credential_available && !status.enabled_tools.is_empty(), actions_enabled: !clients_loading, pending_change } }
    } }
}

#[cfg(feature = "mcp-management")]
#[component]
fn McpClientRow(
    client: McpClientStatus,
    server_ready: bool,
    actions_enabled: bool,
    mut pending_change: Signal<Option<PendingMcpClientChange>>,
) -> Element {
    let state_label = mcp_client_state_label(client.state);
    let actions = mcp_client_actions(client.state);
    let detail = client.detail.clone();
    rsx! { div { class: "mcp-client-row",
        div { class: "mcp-client-copy", strong { {client.client.display_name()} } span { "{state_label}" } small { {client.target.display().to_string()} } if let Some(detail) = detail { small { class: "danger-text", "{detail}" } } }
        div { class: "mcp-client-actions",
            for action in actions {
                McpClientActionButton { client: client.clone(), action, server_ready, actions_enabled, pending_change }
            }
        }
    } }
}

#[cfg(feature = "mcp-management")]
#[component]
fn McpClientActionButton(
    client: McpClientStatus,
    action: McpClientAction,
    server_ready: bool,
    actions_enabled: bool,
    mut pending_change: Signal<Option<PendingMcpClientChange>>,
) -> Element {
    let change = PendingMcpClientChange {
        client: client.client,
        action,
        target: client.target.display().to_string(),
    };
    rsx! { button {
        class: "button compact secondary",
        r#type: "button",
        disabled: !actions_enabled || (!server_ready && action != McpClientAction::Unregister),
        onclick: move |_| pending_change.set(Some(change.clone())),
        {mcp_client_action_label(action)}
    } }
}

#[cfg(feature = "mcp-management")]
#[component]
fn McpClientConfirmation(
    change: PendingMcpClientChange,
    busy: bool,
    status: Signal<crate::mcp_management::McpStatus>,
    error: Signal<Option<String>>,
    mut pending_change: Signal<Option<PendingMcpClientChange>>,
) -> Element {
    let client_name = change.client.display_name();
    let action_label = mcp_client_confirmation_label(change.action);
    let submitted_change = change.clone();
    rsx! { div { class: "mcp-client-confirmation", role: "group", aria_label: text("preferences.mcpConfirmTitle"),
        strong { "{action_label} {client_name}" }
        p { {text("preferences.mcpConfirmImpact")} }
        code { "{change.target}" }
        div { class: "mcp-client-confirmation-actions",
            button { class: "button compact ghost", r#type: "button", onclick: move |_| pending_change.set(None), {text("action.cancel")} }
            button { class: "button compact primary", r#type: "button", disabled: busy, onclick: move |_| apply_mcp_client_change(&submitted_change, status, error, pending_change), {text("preferences.mcpConfirmAction")} }
        }
    } }
}

#[cfg(feature = "mcp-management")]
fn apply_mcp_client_change(
    change: &PendingMcpClientChange,
    status: Signal<crate::mcp_management::McpStatus>,
    error: Signal<Option<String>>,
    mut pending_change: Signal<Option<PendingMcpClientChange>>,
) {
    let submitted_change = change.clone();
    mark_mcp_clients_loading(status);
    pending_change.set(None);
    spawn(async move {
        let result = tokio::task::spawn_blocking(move || mcp_client_change(&submitted_change))
            .await
            .map_err(|join_error| join_error.to_string())
            .and_then(|change_result| change_result);
        finish_mcp_client_change(result, status, error);
    });
}

#[cfg(feature = "mcp-management")]
fn mcp_client_change(
    change: &PendingMcpClientChange,
) -> Result<crate::mcp_management::McpStatus, String> {
    match change.action {
        McpClientAction::Register | McpClientAction::Update => {
            crate::mcp_management::register_client(change.client)
        }
        McpClientAction::Unregister => crate::mcp_management::unregister_client(change.client),
    }
}

#[cfg(feature = "mcp-management")]
fn finish_mcp_client_change(
    result: Result<crate::mcp_management::McpStatus, String>,
    mut status: Signal<crate::mcp_management::McpStatus>,
    mut error: Signal<Option<String>>,
) {
    match result {
        Ok(next) => {
            status.set(next);
            error.set(None);
        }
        Err(message) => {
            mark_mcp_clients_loaded(status);
            error.set(Some(message));
        }
    }
}

#[cfg(feature = "mcp-management")]
fn mark_mcp_clients_loading(mut status: Signal<crate::mcp_management::McpStatus>) {
    let mut current = status();
    current.clients_loading = true;
    status.set(current);
}

#[cfg(feature = "mcp-management")]
fn mark_mcp_clients_loaded(mut status: Signal<crate::mcp_management::McpStatus>) {
    let mut current = status();
    current.clients_loading = false;
    status.set(current);
}

#[cfg(feature = "mcp-management")]
fn use_mcp_status(status: Signal<crate::mcp_management::McpStatus>, error: Signal<Option<String>>) {
    let mut loaded_status = status;
    let mut loading_error = error;
    use_future(move || async move {
        match tokio::task::spawn_blocking(crate::mcp_management::status).await {
            Ok(next) => loaded_status.set(next),
            Err(join_error) => {
                mark_mcp_clients_loaded(loaded_status);
                loading_error.set(Some(join_error.to_string()));
            }
        }
    });
}

#[cfg(feature = "mcp-management")]
fn refresh_mcp_status(
    status: Signal<crate::mcp_management::McpStatus>,
    error: Signal<Option<String>>,
) {
    mark_mcp_clients_loading(status);
    spawn(async move {
        let result = tokio::task::spawn_blocking(crate::mcp_management::status)
            .await
            .map_err(|join_error| join_error.to_string());
        finish_mcp_client_change(result, status, error);
    });
}

#[cfg(feature = "mcp-management")]
fn mcp_client_actions(state: RegistrationState) -> Vec<McpClientAction> {
    match state {
        RegistrationState::Available => vec![McpClientAction::Register],
        RegistrationState::Registered => vec![McpClientAction::Unregister],
        RegistrationState::BrokenRegistration | RegistrationState::OwnedOutdatedRegistration => {
            vec![McpClientAction::Update, McpClientAction::Unregister]
        }
        RegistrationState::ConflictingRegistration => vec![McpClientAction::Update],
        RegistrationState::Unavailable | RegistrationState::InvalidConfiguration => Vec::new(),
    }
}

#[cfg(feature = "mcp-management")]
fn mcp_client_state_label(state: RegistrationState) -> &'static str {
    match state {
        RegistrationState::Unavailable => text("preferences.mcpClientUnavailable"),
        RegistrationState::Available => text("preferences.mcpClientAvailable"),
        RegistrationState::Registered => text("preferences.mcpClientRegistered"),
        RegistrationState::BrokenRegistration => text("preferences.mcpClientBroken"),
        RegistrationState::OwnedOutdatedRegistration => text("preferences.mcpClientOutdated"),
        RegistrationState::ConflictingRegistration => text("preferences.mcpClientConflict"),
        RegistrationState::InvalidConfiguration => text("preferences.mcpClientInvalid"),
    }
}

#[cfg(feature = "mcp-management")]
fn mcp_client_action_label(action: McpClientAction) -> &'static str {
    match action {
        McpClientAction::Register => text("preferences.mcpInstall"),
        McpClientAction::Update => text("preferences.mcpUpdate"),
        McpClientAction::Unregister => text("preferences.mcpRemove"),
    }
}

#[cfg(feature = "mcp-management")]
fn mcp_client_confirmation_label(action: McpClientAction) -> &'static str {
    match action {
        McpClientAction::Register => text("preferences.mcpConfirmInstall"),
        McpClientAction::Update => text("preferences.mcpConfirmUpdate"),
        McpClientAction::Unregister => text("preferences.mcpConfirmRemove"),
    }
}

#[cfg(feature = "mcp-management")]
#[component]
fn McpRuntimeStatus(status: crate::mcp_management::McpStatus) -> Element {
    let availability = if status.binary_available {
        text("preferences.mcpBinaryReady")
    } else {
        text("preferences.mcpBinaryMissing")
    };
    rsx! { div { class: "mcp-runtime-status",
        strong { "{availability}" }
        code { "{status.command}" }
        if !status.enabled_tools.is_empty() {
            span { class: "mcp-tool-list", {text("preferences.mcpEnabledTools")} " " {status.enabled_tools.join(", ")} }
        }
        if !status.credential_available { small { class: "danger-text", {text("preferences.mcpCredentialMissing")} } }
        small { {text("preferences.mcpCommandHelp")} }
    } }
}

#[component]
fn JiraSettings(
    configuration: ConnectionConfiguration,
    permissions: Option<jira_adapter::ProjectPermissions>,
    draft: Signal<ConfigurationDraft>,
    boards: Signal<BoardOptionsState>,
) -> Element {
    rsx! { div { class: "settings-page",
        div { class: "settings-page-heading", h3 { {text("preferences.jiraTitle")} } p { {text("preferences.jiraDescription")} } }
        AccountSettings { configuration: configuration.clone(), permissions, draft, boards }
        JiraLimits { draft }
        HoursSettings { draft }
    } }
}

#[component]
fn AccountSettings(
    configuration: ConnectionConfiguration,
    permissions: Option<jira_adapter::ProjectPermissions>,
    mut draft: Signal<ConfigurationDraft>,
    boards: Signal<BoardOptionsState>,
) -> Element {
    rsx! { section { class: "settings-section", aria_labelledby: "jira-account-title",
        SettingsHeading { id: "jira-account-title", mark: "J", title: text("preferences.accountTitle"), description: text("preferences.accountDescription"), module: true }
        ReadonlyField { id: "preferences-site", label: text("setup.siteLabel"), value: configuration.jira.site }
        ReadonlyField { id: "preferences-email", label: text("setup.emailLabel"), value: configuration.jira.email }
        BoardPreference { draft, boards }
        label { class: "field", r#for: "preferences-token", span { {text("setup.tokenLabel")} }
            input { id: "preferences-token", r#type: "password", autocomplete: "new-password", placeholder: text("preferences.tokenPlaceholder"), value: draft().replacement_token, oninput: move |event| draft.write().replacement_token = event.value() }
            small { {text("preferences.tokenHelp")} }
        }
        PermissionSummary { permissions }
    } }
}

#[component]
fn BoardPreference(
    mut draft: Signal<ConfigurationDraft>,
    boards: Signal<BoardOptionsState>,
) -> Element {
    rsx! { div { class: "field", label { r#for: "preferences-board", {text("setup.boardLabel")} }
        {board_preference_control(draft, boards())}
        small { id: "preferences-board-help", {text("setup.boardHelp")} }
    } }
}

fn board_preference_control(
    mut draft: Signal<ConfigurationDraft>,
    state: BoardOptionsState,
) -> Element {
    match state {
        BoardOptionsState::Loading => {
            rsx! { div { class: "board-loading", role: "status", {text("preferences.boardsLoading")} } }
        }
        BoardOptionsState::Ready(boards) => {
            rsx! { BoardSelector { id: "preferences-board", boards, selected: draft().jira.board_id, disabled: false, on_select: move |board_id| draft.write().jira.board_id = board_id } }
        }
        BoardOptionsState::Failed(message) => {
            let prefix = text("preferences.boardsFailed");
            rsx! { div { class: "setup-error compact", role: "alert", "{prefix} {message}" } }
        }
    }
}

#[component]
fn PermissionSummary(permissions: Option<jira_adapter::ProjectPermissions>) -> Element {
    let Some(permissions) = permissions else {
        return rsx! { div { class: "permission-summary unavailable", role: "status",
            strong { {text("permissions.title")} }
            span { {text("permissions.unavailable")} }
        } };
    };
    rsx! { div { class: "permission-summary", aria_label: text("permissions.title"),
        strong { {text("permissions.title")} }
        PermissionItem { allowed: permissions.browse_projects, label: text("permissions.browse") }
        PermissionItem { allowed: permissions.own_worklogs.create, label: text("permissions.createOwn") }
        PermissionItem { allowed: permissions.own_worklogs.edit, label: text("permissions.editOwn") }
        PermissionItem { allowed: permissions.own_worklogs.delete, label: text("permissions.deleteOwn") }
        PermissionItem { allowed: permissions.administer_projects, label: text("permissions.administer") }
        PermissionItem { allowed: false, label: text("permissions.changeOthers") }
    } }
}

#[component]
fn PermissionItem(allowed: bool, label: &'static str) -> Element {
    let class = if allowed { "allowed" } else { "denied" };
    let mark = if allowed { "✓" } else { "—" };
    rsx! { span { class: "permission-item {class}", span { aria_hidden: "true", "{mark}" } "{label}" } }
}

#[component]
fn ReadonlyField(id: &'static str, label: &'static str, value: String) -> Element {
    rsx! { label { class: "field", r#for: id, span { "{label}" }
        input { id, value, readonly: true, aria_readonly: "true" }
    } }
}

#[component]
fn JiraLimits(mut draft: Signal<ConfigurationDraft>) -> Element {
    let defaults = product_defaults();
    let limits = defaults.jira();
    rsx! { section { class: "settings-section", aria_labelledby: "jira-limits-title",
        SettingsHeading { id: "jira-limits-title", mark: "↗", title: text("preferences.connectionTitle"), description: text("preferences.connectionDescription"), module: false }
        NumberField { id: "preferences-timeout", label: text("preferences.timeoutLabel"), help: text("preferences.timeoutHelp"), maximum: limits.maximum_allowed_request_timeout_seconds.to_string(), value: draft().jira.request_timeout_seconds, on_input: move |value| draft.write().jira.request_timeout_seconds = value }
        NumberField { id: "preferences-page-size", label: text("preferences.pageSizeLabel"), help: text("preferences.pageSizeHelp"), maximum: limits.maximum_allowed_page_size.to_string(), value: draft().jira.page_size, on_input: move |value| draft.write().jira.page_size = value }
        NumberField { id: "preferences-collection-limit", label: text("preferences.collectionLimitLabel"), help: text("preferences.collectionLimitHelp"), maximum: limits.maximum_allowed_collection_items.to_string(), value: draft().jira.maximum_collection_items, on_input: move |value| draft.write().jira.maximum_collection_items = value }
        NumberField { id: "preferences-search-limit", label: text("preferences.searchLimitLabel"), help: text("preferences.searchLimitHelp"), maximum: limits.maximum_allowed_issue_search_results.to_string(), value: draft().jira.maximum_issue_search_results, on_input: move |value| draft.write().jira.maximum_issue_search_results = value }
        NumberField { id: "preferences-concurrency", label: text("preferences.concurrencyLabel"), help: text("preferences.concurrencyHelp"), maximum: limits.maximum_allowed_concurrent_worklog_requests.to_string(), value: draft().jira.maximum_concurrent_worklog_requests, on_input: move |value| draft.write().jira.maximum_concurrent_worklog_requests = value }
    } }
}

#[component]
fn HoursSettings(mut draft: Signal<ConfigurationDraft>) -> Element {
    let product = product_defaults();
    let limits = product.hours();
    rsx! { section { class: "settings-section", aria_labelledby: "hours-settings-title",
        SettingsHeading { id: "hours-settings-title", mark: "H", title: text("preferences.hoursTitle"), description: text("preferences.hoursDescription"), module: false }
        NumberField { id: "preferences-target", label: text("setup.targetLabel"), help: text("setup.targetHelp"), minimum: limits.minimum_weekly_target_hours.to_string(), maximum: limits.maximum_weekly_target_hours.to_string(), value: draft().hours.weekly_target_hours, on_input: move |value| draft.write().hours.weekly_target_hours = value }
        NumberField { id: "preferences-daily-limit", label: text("preferences.dailyLimitLabel"), help: text("preferences.dailyLimitHelp"), maximum: limits.maximum_daily_hours.to_string(), value: draft().hours.maximum_daily_hours, on_input: move |value| draft.write().hours.maximum_daily_hours = value }
        NumberField { id: "preferences-report-period-limit", label: text("preferences.reportPeriodLimitLabel"), help: text("preferences.reportPeriodLimitHelp"), maximum: limits.maximum_custom_range_days.to_string(), value: draft().hours.maximum_report_period_days, on_input: move |value| draft.write().hours.maximum_report_period_days = value }
        NumberField { id: "preferences-start-hour", label: text("preferences.startHourLabel"), help: text("preferences.startHourHelp"), minimum: "0".to_owned(), maximum: MAXIMUM_CLOCK_HOUR.to_string(), value: draft().hours.default_worklog_start_hour, on_input: move |value| draft.write().hours.default_worklog_start_hour = value }
        NumberField { id: "preferences-start-minute", label: text("preferences.startMinuteLabel"), help: text("preferences.startMinuteHelp"), minimum: "0".to_owned(), maximum: MAXIMUM_CLOCK_MINUTE.to_string(), value: draft().hours.default_worklog_start_minute, on_input: move |value| draft.write().hours.default_worklog_start_minute = value }
    } }
}

#[cfg(feature = "reports")]
#[component]
fn ReportsSettings(
    mut draft: Signal<ConfigurationDraft>,
    permissions: Option<jira_adapter::ProjectPermissions>,
) -> Element {
    let permitted = permissions.is_some_and(team_report_permission_granted);
    let help = if permitted {
        text("preferences.teamReportsHelp")
    } else {
        text("preferences.teamReportsUnavailable")
    };
    rsx! { div { class: "settings-page",
        div { class: "settings-page-heading", h3 { {text("preferences.reportsTitle")} } p { {text("preferences.reportsDescription")} } }
        section { class: "settings-section", aria_labelledby: "team-reports-settings-title",
            SettingsHeading { id: "team-reports-settings-title", mark: "R", title: text("preferences.teamReportsTitle"), description: text("preferences.teamReportsDescription"), module: true }
        SwitchField {
            id: "preferences-team-reports",
            label: text("preferences.teamReportsLabel"),
            help,
            checked: permitted && draft().reports.enable_team_reports,
            disabled: !permitted,
            on_change: move |checked| draft.write().reports.enable_team_reports = checked,
        }
        }
    } }
}

#[component]
fn SettingsHeading(
    id: &'static str,
    mark: &'static str,
    title: &'static str,
    description: &'static str,
    module: bool,
) -> Element {
    let class = if module {
        "settings-section-mark module"
    } else {
        "settings-section-mark"
    };
    rsx! { div { class: "settings-section-heading", span { class, "{mark}" }
        div { h3 { id, "{title}" } p { "{description}" } }
    } }
}

#[component]
fn NumberField(
    id: &'static str,
    label: &'static str,
    help: &'static str,
    value: String,
    on_input: EventHandler<String>,
    minimum: Option<String>,
    maximum: Option<String>,
) -> Element {
    rsx! { label { class: "field", r#for: id, span { "{label}" }
        input { id, r#type: "number", required: true, min: minimum.unwrap_or_else(|| "1".to_owned()), max: maximum, value, oninput: move |event| on_input.call(event.value()) }
        small { "{help}" }
    } }
}

#[component]
fn SwitchField(
    id: &'static str,
    label: &'static str,
    help: &'static str,
    checked: bool,
    disabled: bool,
    on_change: EventHandler<bool>,
) -> Element {
    rsx! { label { class: "switch-setting", r#for: id,
        span { class: "switch-copy", strong { "{label}" } small { "{help}" } }
        span { class: "switch-control",
            input { id, class: "switch-input", r#type: "checkbox", role: "switch", checked, disabled, onchange: move |event| on_change.call(event.checked()) }
            span { class: "switch-track", aria_hidden: "true", span { class: "switch-thumb" } }
        }
    } }
}

fn submit(
    event: &FormEvent,
    saving: bool,
    draft: Signal<ConfigurationDraft>,
    mut error: Signal<Option<String>>,
    on_save: EventHandler<ConfigurationUpdate>,
) {
    event.prevent_default();
    if saving {
        return;
    }
    match parse_configuration(&draft.read()) {
        Ok(configuration) => {
            error.set(None);
            on_save.call(configuration);
        }
        Err(message) => error.set(Some(message)),
    }
}

fn parse_configuration(draft: &ConfigurationDraft) -> Result<ConfigurationUpdate, String> {
    Ok(ConfigurationUpdate {
        replacement_token: replacement_token(&draft.replacement_token),
        jira: parse_jira(draft)?,
        hours: parse_hours(draft)?,
        reports: ReportsConfiguration {
            enable_team_reports: draft.reports.enable_team_reports,
        },
    })
}

fn parse_jira(draft: &ConfigurationDraft) -> Result<JiraConfigurationUpdate, String> {
    let configuration = JiraConfigurationUpdate {
        board_id: parse_positive(&draft.jira.board_id)?,
        request_timeout_seconds: parse_positive(&draft.jira.request_timeout_seconds)?,
        page_size: parse_positive(&draft.jira.page_size)?,
        maximum_collection_items: parse_positive(&draft.jira.maximum_collection_items)?,
        maximum_issue_search_results: parse_positive(&draft.jira.maximum_issue_search_results)?,
        maximum_concurrent_worklog_requests: parse_positive(
            &draft.jira.maximum_concurrent_worklog_requests,
        )?,
    };
    validate_jira_update(&configuration)?;
    Ok(configuration)
}

fn validate_jira_update(configuration: &JiraConfigurationUpdate) -> Result<(), String> {
    let defaults = product_defaults();
    let limits = defaults.jira();
    let valid = configuration.request_timeout_seconds
        <= limits.maximum_allowed_request_timeout_seconds
        && configuration.page_size <= limits.maximum_allowed_page_size
        && configuration.maximum_collection_items <= limits.maximum_allowed_collection_items
        && configuration.maximum_issue_search_results
            <= limits.maximum_allowed_issue_search_results
        && configuration.maximum_concurrent_worklog_requests
            <= limits.maximum_allowed_concurrent_worklog_requests;
    valid
        .then_some(())
        .ok_or_else(|| text("preferences.invalidNumber").to_owned())
}

fn parse_hours(draft: &ConfigurationDraft) -> Result<HoursConfiguration, String> {
    let configuration = HoursConfiguration {
        weekly_target_hours: parse_positive(&draft.hours.weekly_target_hours)?,
        utc_offset_minutes: parse_offset(&draft.hours.utc_offset_minutes)?,
        maximum_daily_hours: parse_positive(&draft.hours.maximum_daily_hours)?,
        maximum_report_period_days: parse_positive(&draft.hours.maximum_report_period_days)?,
        default_worklog_start_hour: parse_number(&draft.hours.default_worklog_start_hour)?,
        default_worklog_start_minute: parse_number(&draft.hours.default_worklog_start_minute)?,
    };
    validate_hours(&configuration)?;
    Ok(configuration)
}

fn validate_hours(configuration: &HoursConfiguration) -> Result<(), String> {
    let product = product_defaults();
    let defaults = product.hours();
    let target = u32::from(configuration.weekly_target_hours);
    let valid_target = (defaults.minimum_weekly_target_hours
        ..=defaults.maximum_weekly_target_hours)
        .contains(&target);
    let valid_daily = configuration.maximum_daily_hours <= defaults.maximum_daily_hours;
    let valid_report_period =
        configuration.maximum_report_period_days <= defaults.maximum_custom_range_days;
    let valid_time = configuration.default_worklog_start_hour <= MAXIMUM_CLOCK_HOUR
        && configuration.default_worklog_start_minute <= MAXIMUM_CLOCK_MINUTE;
    if !valid_target || !valid_daily || !valid_report_period || !valid_time {
        return Err(text("preferences.invalidNumber").to_owned());
    }
    Ok(())
}

fn replacement_token(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn parse_positive<T>(value: &str) -> Result<T, String>
where
    T: std::str::FromStr + Default + PartialOrd,
{
    let parsed = parse_number(value)?;
    if parsed <= T::default() {
        return Err(text("preferences.invalidNumber").to_owned());
    }
    Ok(parsed)
}

fn parse_number<T: std::str::FromStr>(value: &str) -> Result<T, String> {
    value
        .parse()
        .map_err(|_| text("preferences.invalidNumber").to_owned())
}

fn parse_offset(value: &str) -> Result<i16, String> {
    let offset = parse_number(value)?;
    let allowed = product_defaults()
        .hours()
        .utc_offset_options
        .iter()
        .any(|option| option.minutes == offset);
    allowed
        .then_some(offset)
        .ok_or_else(|| text("setup.error.timeZone").to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_token_preserves_the_current_credential() {
        assert_eq!(replacement_token("   "), None);
        assert_eq!(
            replacement_token(" replacement "),
            Some("replacement".to_owned())
        );
    }

    #[test]
    fn rejects_zero_limits() {
        assert_eq!(
            parse_positive::<u16>("0"),
            Err(text("preferences.invalidNumber").to_owned())
        );
    }

    #[test]
    fn rejects_jira_limits_above_the_safe_maxima() {
        let defaults = product_defaults();
        let limits = defaults.jira();
        let configuration = JiraConfigurationUpdate {
            board_id: 42,
            request_timeout_seconds: limits.request_timeout_seconds,
            page_size: limits.page_size,
            maximum_collection_items: limits.maximum_allowed_collection_items.saturating_add(1),
            maximum_issue_search_results: limits.maximum_issue_search_results,
            maximum_concurrent_worklog_requests: limits.maximum_concurrent_worklog_requests,
        };

        assert_eq!(
            validate_jira_update(&configuration),
            Err(text("preferences.invalidNumber").to_owned())
        );
    }
}
