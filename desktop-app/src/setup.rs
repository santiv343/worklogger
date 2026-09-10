use dioxus::prelude::*;

use crate::board_selector::BoardSelector;
use crate::connection::discover_boards;
use crate::connection_model::{AccessibleBoard, BoardDiscoveryRequest, ConnectionRequest};
use crate::copy::{preferred_language, save_language, text};
use crate::defaults::product_defaults;
#[cfg(organization_configuration_mutable)]
use crate::organization_config::OrganizationConfigurationPicker;
use worklogger_settings::DEFAULT_MAXIMUM_REPORT_PERIOD_DAYS;

#[derive(Clone, PartialEq)]
struct ConnectionDraft {
    site: String,
    email: String,
    token: String,
    board_id: String,
}

impl Default for ConnectionDraft {
    fn default() -> Self {
        let defaults = product_defaults();
        Self {
            site: defaults
                .jira()
                .sites
                .first()
                .map_or_else(String::new, |site| site.url.clone()),
            email: String::new(),
            token: String::new(),
            board_id: String::new(),
        }
    }
}

#[component]
pub fn ConnectionSetup(
    loading: bool,
    error: Option<String>,
    configuration_warning: Option<String>,
    on_connect: EventHandler<ConnectionRequest>,
    on_configuration_loaded: EventHandler<()>,
    on_demo: EventHandler<MouseEvent>,
) -> Element {
    let draft = use_signal(ConnectionDraft::default);
    let validation_error = use_signal(|| None::<String>);
    let boards = use_signal(Vec::<AccessibleBoard>::new);
    let discovering = use_signal(|| false);
    let discovered_identity = use_signal(|| None::<String>);
    rsx! {
        section { class: "setup-shell", aria_labelledby: "setup-title",
            SetupIntro {}
            SetupForm { loading, error, configuration_warning, draft, validation_error, boards, discovering, discovered_identity, on_connect, on_configuration_loaded, on_demo }
        }
    }
}

#[component]
fn SetupIntro() -> Element {
    let kicker = text("setup.kicker");
    let title = text("setup.title");
    let description = text("setup.description");
    let safety = text("setup.safety");
    rsx! {
        header { class: "setup-intro",
            span { class: "setup-kicker", "{kicker}" }
            h1 { id: "setup-title", "{title}" }
            p { "{description}" }
            div { class: "setup-safety", "{safety}" }
        }
    }
}

#[component]
fn SetupForm(
    loading: bool,
    error: Option<String>,
    configuration_warning: Option<String>,
    draft: Signal<ConnectionDraft>,
    validation_error: Signal<Option<String>>,
    boards: Signal<Vec<AccessibleBoard>>,
    discovering: Signal<bool>,
    discovered_identity: Signal<Option<String>>,
    on_connect: EventHandler<ConnectionRequest>,
    on_configuration_loaded: EventHandler<()>,
    on_demo: EventHandler<MouseEvent>,
) -> Element {
    let configuration_blocked = configuration_warning.is_some();
    let busy = loading || discovering();
    let controls_disabled = busy || configuration_blocked;
    rsx! {
        form { class: "setup-form", aria_busy: busy, onsubmit: move |event| submit(&event, controls_disabled, draft, validation_error, on_connect),
            LanguageSelector {}
            {configuration_picker(configuration_warning, on_configuration_loaded)}
            fieldset { disabled: controls_disabled,
                JiraFields { draft, boards, discovered_identity }
                DiscoveryControl { draft, boards, discovering, discovered_identity, validation_error }
                if !boards().is_empty() {
                    BoardField { draft, boards }
                }
            }
            FormError { external: error, validation: validation_error() }
            SubmitArea { loading: busy, ready: !configuration_blocked && !draft().board_id.is_empty(), on_demo }
        }
    }
}

#[component]
fn LanguageSelector() -> Element {
    let mut language = use_signal(preferred_language);
    rsx! {
        label { class: "field", r#for: "setup-language",
            span { {text("preferences.languageLabel")} }
            select { id: "setup-language", value: language_value(language()), oninput: move |event| {
                let selected = parse_language(&event.value());
                if save_language(selected).is_ok() {
                    language.set(selected);
                }
            },
                option { value: "english", {text("preferences.languageEnglish")} }
                option { value: "spanish", {text("preferences.languageSpanish")} }
            }
            small { {text("preferences.languageRestart")} }
        }
    }
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
fn configuration_picker(error: Option<String>, on_loaded: EventHandler<()>) -> Element {
    rsx! { div { class: "configuration-choice-compact",
        OrganizationConfigurationPicker { initial_error: error, allow_export: false, on_loaded }
    } }
}

#[cfg(not(organization_configuration_mutable))]
fn configuration_picker(_error: Option<String>, _on_loaded: EventHandler<()>) -> Element {
    rsx! {}
}

#[component]
fn JiraFields(
    mut draft: Signal<ConnectionDraft>,
    mut boards: Signal<Vec<AccessibleBoard>>,
    mut discovered_identity: Signal<Option<String>>,
) -> Element {
    let section_title = text("setup.jiraSection");
    rsx! {
        div { class: "setup-section", h2 { "{section_title}" }
            ConfiguredSiteField { draft, boards, discovered_identity }
            SetupField { id: "jira-email", label: text("setup.emailLabel"), help: text("setup.emailHelp"), control: rsx! { input { id: "jira-email", name: "jira-email", aria_describedby: "jira-email-help", r#type: "email", required: true, autocomplete: "email", spellcheck: "false", placeholder: text("setup.emailPlaceholder"), value: draft().email, oninput: move |event| { draft.write().email = event.value(); reset_discovery(draft, boards, discovered_identity); } } } }
            SetupField { id: "jira-token", label: text("setup.tokenLabel"), help: text("setup.tokenHelp"), control: rsx! { input { id: "jira-token", name: "jira-token", aria_describedby: "jira-token-help", r#type: "password", required: true, autocomplete: "new-password", spellcheck: "false", placeholder: text("setup.tokenPlaceholder"), value: draft().token, oninput: move |event| { draft.write().token = event.value(); reset_discovery(draft, boards, discovered_identity); } } } }
        }
    }
}

fn reset_discovery(
    mut draft: Signal<ConnectionDraft>,
    mut boards: Signal<Vec<AccessibleBoard>>,
    mut discovered_identity: Signal<Option<String>>,
) {
    draft.write().board_id.clear();
    boards.set(Vec::new());
    discovered_identity.set(None);
}

#[component]
fn DiscoveryControl(
    draft: Signal<ConnectionDraft>,
    boards: Signal<Vec<AccessibleBoard>>,
    discovering: Signal<bool>,
    discovered_identity: Signal<Option<String>>,
    validation_error: Signal<Option<String>>,
) -> Element {
    let label = if discovering() {
        text("setup.boardDiscovering")
    } else {
        text("setup.boardDiscover")
    };
    let identity_label = discovered_identity()
        .map(|identity| format!("{}{identity}", text("setup.boardDiscoveredPrefix")));
    rsx! {
        div { class: "board-discovery",
            button { class: "button secondary", r#type: "button", disabled: discovering(), onclick: move |_| start_discovery(draft, boards, discovering, discovered_identity, validation_error), "{label}" }
            if let Some(identity) = identity_label {
                span { role: "status", "{identity}" }
            }
        }
    }
}

fn start_discovery(
    draft: Signal<ConnectionDraft>,
    boards: Signal<Vec<AccessibleBoard>>,
    mut discovering: Signal<bool>,
    discovered_identity: Signal<Option<String>>,
    mut validation_error: Signal<Option<String>>,
) {
    let request = match build_discovery_request(&draft.read()) {
        Ok(request) => request,
        Err(error) => return validation_error.set(Some(error)),
    };
    discovering.set(true);
    validation_error.set(None);
    spawn(async move {
        let result = discover_boards(request).await;
        discovering.set(false);
        match result {
            Ok(discovery) => apply_discovery(
                discovery.boards,
                discovery.identity,
                draft,
                boards,
                discovered_identity,
                validation_error,
            ),
            Err(error) => validation_error.set(Some(error)),
        }
    });
}

fn apply_discovery(
    discovered_boards: Vec<AccessibleBoard>,
    identity: String,
    mut draft: Signal<ConnectionDraft>,
    mut boards: Signal<Vec<AccessibleBoard>>,
    mut discovered_identity: Signal<Option<String>>,
    mut validation_error: Signal<Option<String>>,
) {
    if discovered_boards.is_empty() {
        return validation_error.set(Some(text("setup.error.noBoards").to_owned()));
    }
    draft.write().board_id = single_board_id(&discovered_boards);
    boards.set(discovered_boards);
    discovered_identity.set(Some(identity));
    validation_error.set(None);
}

fn single_board_id(boards: &[AccessibleBoard]) -> String {
    if boards.len() != 1 {
        return String::new();
    }
    boards
        .first()
        .map_or_else(String::new, |board| board.id.to_string())
}

#[component]
fn ConfiguredSiteField(
    mut draft: Signal<ConnectionDraft>,
    mut boards: Signal<Vec<AccessibleBoard>>,
    mut discovered_identity: Signal<Option<String>>,
) -> Element {
    let sites = product_defaults().jira().sites.clone();
    if sites.is_empty() {
        return rsx! { SetupField { id: "jira-site", label: text("setup.siteLabel"), help: text("setup.siteHelp"), control: rsx! {
            input { id: "jira-site", name: "jira-site", aria_describedby: "jira-site-help", r#type: "url", required: true, autocomplete: "url", spellcheck: "false", placeholder: text("setup.sitePlaceholder"), value: draft().site, oninput: move |event| {
                draft.write().site = event.value();
                reset_discovery(draft, boards, discovered_identity);
            } }
        } } };
    }
    if sites.len() == 1 {
        return rsx! { SetupField { id: "jira-site", label: text("setup.siteLabel"), help: text("setup.siteConfiguredHelp"), control: rsx! {
            input { class: "setup-readonly", id: "jira-site", name: "jira-site", value: "{sites[0].name} · {sites[0].url}", readonly: true, aria_readonly: "true" }
        } } };
    }
    rsx! { SetupField { id: "jira-site", label: text("setup.siteLabel"), help: text("setup.siteConfiguredHelp"), control: rsx! {
        select { id: "jira-site", name: "jira-site", required: true, value: draft().site, oninput: move |event| {
            draft.write().site = event.value();
            reset_discovery(draft, boards, discovered_identity);
        },
            for site in sites { option { value: site.url, "{site.name}" } }
        }
    } } }
}

#[component]
fn BoardField(mut draft: Signal<ConnectionDraft>, boards: Signal<Vec<AccessibleBoard>>) -> Element {
    rsx! {
        div { class: "setup-section preferences", h2 { {text("setup.boardSection")} }
            SetupField { id: "board-id", label: text("setup.boardLabel"), help: text("setup.boardHelp"), control: rsx! {
                BoardSelector { id: "board-id", boards: boards(), selected: draft().board_id, disabled: false, on_select: move |board_id| draft.write().board_id = board_id }
            } }
        }
    }
}

#[component]
fn SetupField(
    id: &'static str,
    label: &'static str,
    help: &'static str,
    control: Element,
) -> Element {
    rsx! {
        div { class: "setup-field", label { r#for: id, "{label}" } {control}
            small { id: "{id}-help", "{help}" }
        }
    }
}

#[component]
fn FormError(external: Option<String>, validation: Option<String>) -> Element {
    let message = validation.or(external);
    let prefix = text("setup.errorPrefix");
    rsx! {
        if let Some(message) = message {
            div { class: "setup-error", role: "alert", strong { "{prefix}" } "{message}" }
        }
    }
}

#[component]
fn SubmitArea(loading: bool, ready: bool, on_demo: EventHandler<MouseEvent>) -> Element {
    let label = if loading {
        text("setup.verifying")
    } else {
        text("setup.submit")
    };
    let help = if ready {
        text("setup.readyHelp")
    } else {
        text("setup.verifyHelp")
    };
    let demo = text("setup.demo");
    rsx! {
        footer { class: "setup-actions",
            p { "{help}" }
            button { class: "button ghost", r#type: "button", disabled: loading, onclick: on_demo, "{demo}" }
            if ready {
                button { class: "button primary", r#type: "submit", disabled: loading, "{label}" }
            }
        }
    }
}

fn submit(
    event: &FormEvent,
    loading: bool,
    draft: Signal<ConnectionDraft>,
    mut validation_error: Signal<Option<String>>,
    on_connect: EventHandler<ConnectionRequest>,
) {
    event.prevent_default();
    if loading {
        return;
    }
    match build_request(&draft.read()) {
        Ok(request) => on_connect.call(request),
        Err(message) => validation_error.set(Some(message)),
    }
}

fn build_request(draft: &ConnectionDraft) -> Result<ConnectionRequest, String> {
    validate_identity(draft)?;
    let defaults = product_defaults();
    Ok(ConnectionRequest {
        site: draft.site.trim().trim_end_matches('/').to_owned(),
        email: draft.email.trim().to_owned(),
        token: draft.token.clone(),
        board_id: parse_board_id(&draft.board_id)?,
        weekly_target_hours: defaults.hours().suggested_weekly_target_hours,
        utc_offset_minutes: defaults.hours().suggested_utc_offset_minutes,
        request_timeout_seconds: defaults.jira().request_timeout_seconds,
        page_size: defaults.jira().page_size,
        maximum_collection_items: defaults.jira().maximum_collection_items,
        maximum_issue_search_results: defaults.jira().maximum_issue_search_results,
        maximum_concurrent_worklog_requests: defaults.jira().maximum_concurrent_worklog_requests,
        maximum_daily_hours: defaults.hours().maximum_daily_hours,
        maximum_report_period_days: DEFAULT_MAXIMUM_REPORT_PERIOD_DAYS
            .min(defaults.hours().maximum_custom_range_days),
        default_worklog_start_hour: defaults.hours().default_worklog_start_hour,
        default_worklog_start_minute: defaults.hours().default_worklog_start_minute,
        enable_team_reports: false,
    })
}

fn build_discovery_request(draft: &ConnectionDraft) -> Result<BoardDiscoveryRequest, String> {
    validate_identity(draft)?;
    let defaults = product_defaults();
    Ok(BoardDiscoveryRequest {
        site: draft.site.trim().trim_end_matches('/').to_owned(),
        email: draft.email.trim().to_owned(),
        token: draft.token.clone(),
        request_timeout_seconds: defaults.jira().request_timeout_seconds,
        page_size: defaults.jira().page_size,
        maximum_items: defaults.jira().maximum_collection_items,
    })
}

fn validate_identity(draft: &ConnectionDraft) -> Result<(), String> {
    if !draft.site.trim().starts_with("https://") {
        return Err(text("setup.error.secureUrl").to_owned());
    }
    if !draft.email.contains('@') {
        return Err(text("setup.error.email").to_owned());
    }
    if draft.token.trim().is_empty() {
        return Err(text("setup.error.token").to_owned());
    }
    Ok(())
}

fn parse_board_id(value: &str) -> Result<u64, String> {
    let board_id = value
        .parse()
        .map_err(|_| text("setup.error.board").to_owned())?;
    if board_id == 0 {
        return Err(text("setup.error.boardPositive").to_owned());
    }
    Ok(board_id)
}
