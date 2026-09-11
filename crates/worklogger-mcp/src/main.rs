#[cfg(any(feature = "jira", feature = "bitbucket"))]
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
#[cfg(any(feature = "jira", feature = "bitbucket"))]
use std::sync::Arc;
#[cfg(any(feature = "jira", feature = "bitbucket"))]
use std::time::Duration;

#[cfg(feature = "bitbucket")]
use bitbucket_adapter::{
    BITBUCKET_CLOUD_API_ORIGIN, BitbucketClient, PageLimits as BitbucketPageLimits, Repository,
};
#[cfg(feature = "jira")]
use jira_adapter::{BoardDto, JiraClient, JiraSiteUrl, PageLimits};
use rmcp::ServiceExt;
use serde::Serialize;
#[cfg(feature = "jira")]
use time::{UtcOffset, format_description::FormatItem, macros::format_description};
#[cfg(all(any(windows, target_os = "linux"), feature = "jira"))]
use worklogger_credentials::api_token_coordinates_match;
#[cfg(any(feature = "jira", feature = "bitbucket"))]
use worklogger_credentials::{CredentialPurpose, CredentialStore, CredentialTransactionGuard};
#[cfg(feature = "bitbucket")]
use worklogger_mcp::{
    BITBUCKET_API_TOKEN_ENVIRONMENT_VARIABLE, BitbucketConfiguration, BitbucketPullRequestDefaults,
    BitbucketPullRequestService,
};
#[cfg(any(feature = "jira", feature = "bitbucket"))]
use worklogger_mcp::{Capability, ModuleConfiguration, ModuleId};
use worklogger_mcp::{
    ClientRegistrationService, ConfigurationStore, McpClientId, McpClientStatus, McpConfiguration,
    McpServerInstallation, RegistrationState, WorkloggerMcpServer,
};
#[cfg(feature = "jira")]
use worklogger_mcp::{
    DEFAULT_MAXIMUM_ISSUE_SEARCH_RESULTS, DEFAULT_MAXIMUM_REPORT_PERIOD_DAYS,
    JIRA_API_TOKEN_ENVIRONMENT_VARIABLE, JiraConfiguration, JiraHoursConfiguration,
    JiraIssueBackend, JiraIssueService, JiraOwnHoursBackend, JiraWorklogService,
    MAXIMUM_REPORT_PERIOD_DAYS, RoutedJiraIssueBackend,
};
#[cfg(feature = "bitbucket")]
use worklogger_profile::BitbucketModuleProfile;
#[cfg(feature = "jira")]
use worklogger_profile::JiraModuleProfile;
use worklogger_profile::OrganizationProfile;
#[cfg(not(feature = "managed-distribution"))]
use worklogger_profile::OrganizationProfileStore;

#[cfg(feature = "bitbucket")]
mod bitbucket_settings;
#[cfg(feature = "jira")]
mod jira_settings;
mod settings_copy;
mod settings_draft;
mod skill_installation;
mod terminal_ui;
mod tui_copy;

use skill_installation::{AgentSkillInstaller, SkillDestinationStatus};
#[cfg(any(feature = "jira", feature = "bitbucket"))]
use terminal_ui::choose_many;
use terminal_ui::{
    Dashboard, TerminalUiSession, choose, choose_dashboard, confirm as tui_confirm,
    notice as tui_notice, present_notices, read_text, show_message, show_progress,
};
#[cfg(all(feature = "jira", feature = "bitbucket"))]
use terminal_ui::{DetailedChoice, choose_detailed};
use tui_copy::tui_copy;

const EMBEDDED_ORGANIZATION_PROFILE: &str = include_str!(concat!(
    env!("OUT_DIR"),
    "/worklogger-mcp-organization-profile.json"
));

#[cfg(any(feature = "jira", feature = "bitbucket"))]
const DEFAULT_REQUEST_TIMEOUT_SECONDS: u64 = 30;
#[cfg(any(feature = "jira", feature = "bitbucket"))]
const DEFAULT_PAGE_SIZE: u16 = 100;
#[cfg(any(feature = "jira", feature = "bitbucket"))]
const DEFAULT_MAXIMUM_COLLECTION_ITEMS: usize = 2_000;
#[cfg(feature = "jira")]
const DEFAULT_MAXIMUM_CONCURRENT_REQUESTS: usize = 8;
#[cfg(feature = "jira")]
const DEFAULT_WEEKLY_TARGET_HOURS: u16 = 40;
#[cfg(feature = "jira")]
const DEFAULT_UTC_OFFSET_MINUTES: i16 = 0;
#[cfg(feature = "jira")]
const SECONDS_PER_MINUTE: i32 = 60;
#[cfg(feature = "jira")]
const UTC_OFFSET_FORMAT: &[FormatItem<'static>] =
    format_description!("[offset_hour sign:mandatory]:[offset_minute]");

#[derive(Clone, Debug, Eq, PartialEq)]
enum Command {
    Menu,
    Serve,
    Setup { profile_path: Option<PathBuf> },
    Install(HeadlessInstall),
    Clients,
    Status,
    Skills,
    Uninstall,
    Help,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HeadlessInstall {
    configuration_path: PathBuf,
    profile_path: Option<PathBuf>,
    clients: ClientSelection,
    install_skills: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ClientSelection {
    All,
    Named(Vec<McpClientId>),
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
#[derive(Clone, Copy)]
struct ProviderRequestLimits {
    timeout_seconds: u64,
    page_size: u16,
    maximum_collection_items: usize,
}

#[cfg(feature = "bitbucket")]
struct BitbucketSetupValues {
    email: String,
    workspace: String,
    repositories: BTreeSet<String>,
    capabilities: BTreeSet<Capability>,
    pull_request_defaults: BitbucketPullRequestDefaults,
    limits: ProviderRequestLimits,
}

#[cfg(all(feature = "jira", feature = "bitbucket"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SetupProvider {
    Jira,
    Bitbucket,
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("operation cancelled")]
    Cancelled,
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Configuration(#[from] worklogger_mcp::ConfigurationError),
    #[error("no MCP configuration found; run `worklogger-mcp setup`")]
    NotConfigured,
    #[error(
        "no API token is available in the secure store or {JIRA_API_TOKEN_ENVIRONMENT_VARIABLE}"
    )]
    #[cfg(feature = "jira")]
    MissingJiraToken,
    #[cfg(feature = "bitbucket")]
    #[error(
        "no Bitbucket API token is available in the secure store or {BITBUCKET_API_TOKEN_ENVIRONMENT_VARIABLE}"
    )]
    MissingBitbucketToken,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MenuAction {
    Configure,
    Clients,
    Refresh,
    Skills,
    Uninstall,
    Exit,
}

#[derive(Clone, Copy)]
enum SettingsAction {
    #[cfg(feature = "jira")]
    Jira,
    #[cfg(feature = "bitbucket")]
    Bitbucket,
    Clients,
    Skills,
    Language,
    Back,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusDocument {
    configured: bool,
    configuration_path: String,
    enabled_modules: Vec<String>,
    enabled_tools: Vec<String>,
    credential_available: bool,
    clients: Vec<ClientStatusDocument>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClientStatusDocument {
    name: String,
    state: RegistrationState,
    target: String,
    detail: Option<String>,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{}: {error}", tui_copy().error_prefix);
        std::process::exit(1);
    }
}

async fn run() -> Result<(), CliError> {
    let command = parse_command(std::env::args().skip(1))?;
    let session = command
        .uses_terminal_ui()
        .then(TerminalUiSession::start)
        .transpose()
        .map_err(|error| message(error.to_string()))?;
    let result = run_command(command).await;
    if let Some(session) = session {
        if let Err(error) = &result {
            show_message(&tui_copy().error_title, &[error.to_string()])
                .map(|_| ())
                .map_err(|io_error| message(io_error.to_string()))?;
        } else {
            present_notices(&tui_copy().result_title)
                .map_err(|io_error| message(io_error.to_string()))?;
        }
        let notices = session
            .finish()
            .map_err(|error| message(error.to_string()))?;
        for notice in notices {
            println!("{notice}");
        }
    }
    result
}

impl Command {
    const fn uses_terminal_ui(&self) -> bool {
        matches!(
            self,
            Self::Menu | Self::Setup { .. } | Self::Clients | Self::Skills | Self::Uninstall
        )
    }
}

async fn run_command(command: Command) -> Result<(), CliError> {
    match command {
        Command::Menu => menu().await,
        Command::Serve => serve().await,
        Command::Setup { profile_path } => setup(profile_path).await,
        Command::Install(options) => install_headless(&options),
        Command::Clients => manage_clients(),
        Command::Status => status(),
        Command::Skills => install_skills(),
        Command::Uninstall => uninstall(),
        Command::Help => {
            print_help();
            Ok(())
        }
    }
}

fn parse_command(mut arguments: impl Iterator<Item = String>) -> Result<Command, CliError> {
    let command = arguments.next().unwrap_or_else(|| "menu".to_owned());
    match command.as_str() {
        "menu" => command_without_arguments(Command::Menu, arguments),
        "setup" => parse_setup_command(arguments),
        "install" => parse_install_command(arguments),
        "serve" => command_without_arguments(Command::Serve, arguments),
        "clients" => command_without_arguments(Command::Clients, arguments),
        "status" => command_without_arguments(Command::Status, arguments),
        "skills" => command_without_arguments(Command::Skills, arguments),
        "uninstall" => command_without_arguments(Command::Uninstall, arguments),
        "help" | "--help" | "-h" => command_without_arguments(Command::Help, arguments),
        _ => Err(message("unknown command; use --help")),
    }
}

fn parse_install_command(mut arguments: impl Iterator<Item = String>) -> Result<Command, CliError> {
    let mut configuration_path = None;
    let mut profile_path = None;
    let mut clients = None;
    let mut install_skills = false;
    let mut confirmed = false;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--config" => set_install_path(&mut configuration_path, arguments.next())?,
            "--profile" => set_install_path(&mut profile_path, arguments.next())?,
            "--clients" => set_install_clients(&mut clients, arguments.next())?,
            "--skills" if !install_skills => install_skills = true,
            "--yes" if !confirmed => confirmed = true,
            _ => return Err(message(&tui_copy().invalid_install_arguments)),
        }
    }
    let configuration_path =
        configuration_path.ok_or_else(|| message(&tui_copy().install_config_path_required))?;
    let clients = clients.ok_or_else(|| message(&tui_copy().install_clients_required))?;
    if !confirmed {
        return Err(message(&tui_copy().install_confirmation_required));
    }
    Ok(Command::Install(HeadlessInstall {
        configuration_path,
        profile_path,
        clients,
        install_skills,
    }))
}

fn set_install_path(
    destination: &mut Option<PathBuf>,
    value: Option<String>,
) -> Result<(), CliError> {
    if destination.is_some() {
        return Err(message(&tui_copy().invalid_install_arguments));
    }
    let value = value.ok_or_else(|| message(&tui_copy().invalid_install_arguments))?;
    *destination = Some(PathBuf::from(value));
    Ok(())
}

fn set_install_clients(
    destination: &mut Option<ClientSelection>,
    value: Option<String>,
) -> Result<(), CliError> {
    if destination.is_some() {
        return Err(message(&tui_copy().invalid_install_arguments));
    }
    let value = value.ok_or_else(|| message(&tui_copy().invalid_install_arguments))?;
    *destination = Some(parse_client_selection(&value)?);
    Ok(())
}

fn parse_client_selection(value: &str) -> Result<ClientSelection, CliError> {
    if value == "all" {
        return Ok(ClientSelection::All);
    }
    let mut clients = Vec::new();
    for name in value.split(',') {
        let client = parse_client_name(name.trim())?;
        if clients.contains(&client) {
            return Err(message(&tui_copy().invalid_install_clients));
        }
        clients.push(client);
    }
    if clients.is_empty() {
        return Err(message(&tui_copy().invalid_install_clients));
    }
    Ok(ClientSelection::Named(clients))
}

fn parse_client_name(value: &str) -> Result<McpClientId, CliError> {
    match value {
        "codex" => Ok(McpClientId::Codex),
        "claude-code" => Ok(McpClientId::ClaudeCode),
        "claude-desktop" => Ok(McpClientId::ClaudeDesktop),
        "cursor" => Ok(McpClientId::Cursor),
        "windsurf" => Ok(McpClientId::Windsurf),
        "qwen-code" => Ok(McpClientId::QwenCode),
        "gemini-cli" => Ok(McpClientId::GeminiCli),
        "kiro" => Ok(McpClientId::Kiro),
        "github-copilot" => Ok(McpClientId::GitHubCopilot),
        "trae-code" => Ok(McpClientId::TraeCode),
        _ => Err(message(&tui_copy().invalid_install_clients)),
    }
}

async fn menu() -> Result<(), CliError> {
    loop {
        let result = match choose_menu_action()? {
            MenuAction::Configure => settings(None).await,
            MenuAction::Clients => manage_clients(),
            MenuAction::Refresh => Ok(()),
            MenuAction::Skills => install_skills(),
            MenuAction::Uninstall => uninstall(),
            MenuAction::Exit => return Ok(()),
        };
        if let Err(error) = result {
            if matches!(error, CliError::Cancelled) {
                continue;
            }
            show_message(&tui_copy().error_title, &[error.to_string()])
                .map(|_| ())
                .map_err(|io_error| message(io_error.to_string()))?;
        }
        present_notices(&tui_copy().result_title)
            .map_err(|io_error| message(io_error.to_string()))?;
    }
}

fn choose_menu_action() -> Result<MenuAction, CliError> {
    let dashboard = menu_dashboard()?;
    let selected = choose_dashboard(&dashboard).map_err(|error| message(error.to_string()))?;
    Ok(menu_action_from_index(selected))
}

fn menu_dashboard() -> Result<Dashboard, CliError> {
    let store = ConfigurationStore::for_current_user()?;
    let configuration = effective_configuration(store.load()?)?;
    let document = status_document(&store, configuration.as_ref());
    let server = installed_server_path()?;
    Ok(Dashboard::new(
        tui_copy().menu_title.clone(),
        menu_overview(&document, &server),
        menu_client_lines(&document.clients),
        menu_actions(),
        tui_copy().tui_navigation_hint.clone(),
        document.configured,
        server_available(&server),
    ))
}

fn menu_overview(document: &StatusDocument, server: &Path) -> Vec<String> {
    let copy = tui_copy();
    let state = if document.configured {
        &copy.state_configured
    } else {
        &copy.state_not_configured
    };
    vec![
        format!("{}: {state}", copy.configuration_label),
        format!("{}: {}", copy.modules_label, menu_modules(document)),
        format!(
            "{}: {}",
            copy.server_state_label,
            yes_no(server_available(server))
        ),
    ]
}

fn menu_modules(document: &StatusDocument) -> String {
    if document.enabled_modules.is_empty() {
        return tui_copy().no_enabled_modules.clone();
    }
    document.enabled_modules.join(", ")
}

fn yes_no(value: bool) -> &'static str {
    if value {
        return &tui_copy().yes;
    }
    &tui_copy().no
}

fn server_available(server: &Path) -> bool {
    server.is_file()
}

fn menu_client_lines(clients: &[ClientStatusDocument]) -> Vec<String> {
    clients
        .iter()
        .map(|client| {
            format!(
                "{} · {}",
                client.name,
                registration_state_label(client.state),
            )
        })
        .collect()
}

fn menu_actions() -> Vec<String> {
    let copy = tui_copy();
    vec![
        copy.menu_configure_action.clone(),
        copy.menu_clients_action.clone(),
        copy.menu_refresh_action.clone(),
        copy.menu_skills_action.clone(),
        copy.menu_uninstall_action.clone(),
        copy.menu_exit_action.clone(),
    ]
}

fn menu_action_from_index(index: usize) -> MenuAction {
    match index {
        0 => MenuAction::Configure,
        1 => MenuAction::Clients,
        2 => MenuAction::Refresh,
        3 => MenuAction::Skills,
        4 => MenuAction::Uninstall,
        _ => MenuAction::Exit,
    }
}

fn parse_setup_command(mut arguments: impl Iterator<Item = String>) -> Result<Command, CliError> {
    let profile_path = match arguments.next().as_deref() {
        None => None,
        Some("--profile") => Some(
            arguments
                .next()
                .map(PathBuf::from)
                .ok_or_else(|| message(&tui_copy().profile_path_required))?,
        ),
        Some(_) => return Err(message(&tui_copy().invalid_setup_arguments)),
    };
    command_without_arguments(Command::Setup { profile_path }, arguments)
}

fn command_without_arguments(
    command: Command,
    mut arguments: impl Iterator<Item = String>,
) -> Result<Command, CliError> {
    if arguments.next().is_some() {
        return Err(message(&tui_copy().unexpected_arguments));
    }
    Ok(command)
}

fn install_skills() -> Result<(), CliError> {
    let copy = tui_copy();
    let installer =
        AgentSkillInstaller::for_current_user().map_err(|error| message(error.to_string()))?;
    let initial_status = installer
        .inspect()
        .map_err(|error| message(error.to_string()))?;
    let continue_installation = terminal_ui::show_skill_status(&initial_status)
        .map_err(|error| message(error.to_string()))?;
    if !continue_installation {
        return Ok(());
    }
    if initial_status
        .iter()
        .any(SkillDestinationStatus::has_conflicts)
    {
        terminal_notice(copy.skills_conflicts.clone());
        return Ok(());
    }
    if initial_status.iter().all(SkillDestinationStatus::is_ready) {
        terminal_notice(copy.skills_ready.clone());
        return Ok(());
    }
    if !confirm(&tui_copy().install_skills_confirmation)? {
        terminal_notice(tui_copy().no_changes.clone());
        return Ok(());
    }
    show_progress(&copy.working_title, &copy.installing_skills)
        .map_err(|error| message(error.to_string()))?;
    installer
        .install()
        .map_err(|error| message(error.to_string()))?;
    let final_status = installer
        .inspect()
        .map_err(|error| message(error.to_string()))?;
    terminal_ui::show_skill_status(&final_status).map_err(|error| message(error.to_string()))?;
    Ok(())
}

fn install_headless(options: &HeadlessInstall) -> Result<(), CliError> {
    let configuration = load_headless_configuration(options)?;
    let registration = ClientRegistrationService::for_current_user()
        .map_err(|error| message(error.to_string()))?;
    let server = installed_server_path()?;
    let clients = selected_headless_clients(registration.statuses(&server), &options.clients)?;
    let skills = options
        .install_skills
        .then(AgentSkillInstaller::for_current_user)
        .transpose()
        .map_err(|error| message(error.to_string()))?;
    validate_headless_skills(skills.as_ref())?;
    save_headless_configuration(&configuration)?;
    save_headless_profile(options.profile_path.as_deref())?;
    let server = install_current_server()?;
    register_headless_clients(&registration, &server, &clients)?;
    install_headless_skills(skills.as_ref())?;
    println!("{}", tui_copy().install_complete);
    Ok(())
}

#[cfg(not(feature = "managed-distribution"))]
fn save_headless_profile(profile_path: Option<&Path>) -> Result<(), CliError> {
    let profile = load_setup_profile(profile_path)?;
    install_selected_profile(profile_path, profile.as_ref())
}

#[cfg(feature = "managed-distribution")]
fn save_headless_profile(profile_path: Option<&Path>) -> Result<(), CliError> {
    let _profile = load_setup_profile(profile_path)?;
    Ok(())
}

fn load_headless_configuration(options: &HeadlessInstall) -> Result<McpConfiguration, CliError> {
    let store = ConfigurationStore::at(options.configuration_path.clone());
    let configuration = store
        .load()?
        .ok_or_else(|| message(&tui_copy().install_config_not_found))?;
    let profile = load_setup_profile(options.profile_path.as_deref())?;
    apply_setup_profile(configuration, profile.as_ref())
}

fn validate_headless_skills(installer: Option<&AgentSkillInstaller>) -> Result<(), CliError> {
    let Some(installer) = installer else {
        return Ok(());
    };
    let statuses = installer
        .inspect()
        .map_err(|error| message(error.to_string()))?;
    if statuses.iter().any(SkillDestinationStatus::has_conflicts) {
        return Err(message(&tui_copy().skills_conflicts));
    }
    Ok(())
}

fn install_headless_skills(installer: Option<&AgentSkillInstaller>) -> Result<(), CliError> {
    let Some(installer) = installer else {
        return Ok(());
    };
    installer
        .install()
        .map_err(|error| message(error.to_string()))?;
    Ok(())
}

fn selected_headless_clients(
    statuses: Vec<McpClientStatus>,
    selection: &ClientSelection,
) -> Result<Vec<McpClientStatus>, CliError> {
    let selected = match selection {
        ClientSelection::All => statuses
            .into_iter()
            .filter(|status| status.state != RegistrationState::Unavailable)
            .collect(),
        ClientSelection::Named(clients) => clients
            .iter()
            .map(|client| headless_client_status(&statuses, *client))
            .collect::<Result<Vec<_>, _>>()?,
    };
    if selected.is_empty() {
        return Err(message(&tui_copy().no_compatible_clients));
    }
    for status in &selected {
        headless_client_is_safe(status)?;
    }
    Ok(selected)
}

fn headless_client_status(
    statuses: &[McpClientStatus],
    client: McpClientId,
) -> Result<McpClientStatus, CliError> {
    statuses
        .iter()
        .find(|status| status.client == client)
        .cloned()
        .ok_or_else(|| message(&tui_copy().invalid_install_clients))
}

fn headless_client_is_safe(status: &McpClientStatus) -> Result<(), CliError> {
    match status.state {
        RegistrationState::Available
        | RegistrationState::Registered
        | RegistrationState::BrokenRegistration
        | RegistrationState::OwnedOutdatedRegistration => Ok(()),
        RegistrationState::Unavailable
        | RegistrationState::ConflictingRegistration
        | RegistrationState::InvalidConfiguration => Err(message(format!(
            "{}: {}",
            status.client.display_name(),
            registration_state_label(status.state)
        ))),
    }
}

fn register_headless_clients(
    registration: &ClientRegistrationService,
    server: &Path,
    clients: &[McpClientStatus],
) -> Result<(), CliError> {
    for status in clients {
        if status.state == RegistrationState::Registered {
            continue;
        }
        registration
            .register(status.client, server)
            .map_err(|error| message(error.to_string()))?;
        println!(
            "{} {}.",
            tui_copy().client_registered,
            status.client.display_name()
        );
    }
    Ok(())
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn save_headless_configuration(configuration: &McpConfiguration) -> Result<(), CliError> {
    #[cfg(feature = "jira")]
    let jira_token = configuration
        .module_enabled(ModuleId::Jira)
        .then(|| required_jira(configuration).and_then(load_token))
        .transpose()?;
    #[cfg(feature = "bitbucket")]
    let bitbucket_token = configuration
        .module_enabled(ModuleId::Bitbucket)
        .then(|| required_bitbucket(configuration).and_then(load_bitbucket_token))
        .transpose()?;
    let mut saved = false;
    #[cfg(feature = "jira")]
    if configuration.module_enabled(ModuleId::Jira) {
        let token = jira_token.as_deref().ok_or(CliError::MissingJiraToken)?;
        save_setup(configuration, token)?;
        saved = true;
    }
    #[cfg(feature = "bitbucket")]
    if configuration.module_enabled(ModuleId::Bitbucket) {
        let token = bitbucket_token
            .as_deref()
            .ok_or(CliError::MissingBitbucketToken)?;
        save_bitbucket_setup(configuration, token)?;
        saved = true;
    }
    if !saved {
        ConfigurationStore::for_current_user()?.save(configuration)?;
    }
    Ok(())
}

#[cfg(not(any(feature = "jira", feature = "bitbucket")))]
fn save_headless_configuration(configuration: &McpConfiguration) -> Result<(), CliError> {
    ConfigurationStore::for_current_user()?.save(configuration)?;
    Ok(())
}

async fn serve() -> Result<(), CliError> {
    let configuration = required_configuration()?;
    let server = WorkloggerMcpServer::new(configuration.clone());
    #[cfg(feature = "jira")]
    let server = with_jira_backends(server, &configuration)?;
    #[cfg(feature = "bitbucket")]
    let server = with_bitbucket_backend(server, &configuration)?;
    let service = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|error| message(error.to_string()))?;
    service
        .waiting()
        .await
        .map_err(|error| message(error.to_string()))?;
    Ok(())
}

#[cfg(feature = "jira")]
fn with_jira_backends(
    server: WorkloggerMcpServer,
    configuration: &McpConfiguration,
) -> Result<WorkloggerMcpServer, CliError> {
    if !configuration.module_enabled(ModuleId::Jira) {
        return Ok(server);
    }
    let jira = required_jira(configuration)?;
    let token = load_token(jira)?;
    let issues =
        JiraIssueService::new(jira, token.clone()).map_err(|error| message(error.to_string()))?;
    let server = server.with_jira_issues(issue_backend(configuration, Arc::new(issues)));
    let server = with_jira_worklog_backend(server, configuration, jira, token.clone())?;
    with_jira_hours_backend(server, configuration, jira, token)
}

/// Serves reads from the additional connections that already hold a credential.
///
/// A connection without a stored credential is skipped rather than refusing to
/// start the server: the primary connection has to keep working even when a
/// secondary one was configured on another machine.
#[cfg(feature = "jira")]
fn issue_backend(
    configuration: &McpConfiguration,
    primary: Arc<dyn JiraIssueBackend>,
) -> Arc<dyn JiraIssueBackend> {
    let named: BTreeMap<String, Arc<dyn JiraIssueBackend>> = configuration
        .jira_connections
        .iter()
        .filter_map(|(name, connection)| {
            let token = stored_token(connection)?;
            let service = JiraIssueService::new(connection, token).ok()?;
            Some((name.clone(), Arc::new(service) as Arc<dyn JiraIssueBackend>))
        })
        .collect();
    if named.is_empty() {
        primary
    } else {
        Arc::new(RoutedJiraIssueBackend::new(primary, named))
    }
}

/// Reads one connection's token from the credential store.
///
/// Unlike `load_token`, the environment variable is deliberately not consulted:
/// it carries a single token, and sending it to a second Jira site would hand a
/// credential to a host the user never authorised for it.
#[cfg(feature = "jira")]
fn stored_token(configuration: &JiraConfiguration) -> Option<String> {
    CredentialStore::for_purpose(CredentialPurpose::Mcp)
        .ok()
        .and_then(|store| {
            store
                .load_api_token(&configuration.base_url, &configuration.email)
                .ok()
                .flatten()
        })
}

#[cfg(feature = "jira")]
fn with_jira_worklog_backend(
    server: WorkloggerMcpServer,
    configuration: &McpConfiguration,
    jira: &JiraConfiguration,
    token: String,
) -> Result<WorkloggerMcpServer, CliError> {
    if !configuration.capability_enabled(Capability::WriteOwnTimeEntries) {
        return Ok(server);
    }
    let worklogs =
        JiraWorklogService::new(jira, token).map_err(|error| message(error.to_string()))?;
    Ok(server.with_jira_worklogs(Arc::new(worklogs)))
}

#[cfg(feature = "jira")]
fn with_jira_hours_backend(
    server: WorkloggerMcpServer,
    configuration: &McpConfiguration,
    jira: &JiraConfiguration,
    token: String,
) -> Result<WorkloggerMcpServer, CliError> {
    if !configuration.capability_enabled(Capability::ReadOwnTimeEntries) {
        return Ok(server);
    }
    let hours =
        JiraOwnHoursBackend::new(jira, token).map_err(|error| message(error.to_string()))?;
    Ok(server.with_own_hours(Arc::new(hours)))
}

#[cfg(feature = "bitbucket")]
fn with_bitbucket_backend(
    server: WorkloggerMcpServer,
    configuration: &McpConfiguration,
) -> Result<WorkloggerMcpServer, CliError> {
    if !configuration.module_enabled(ModuleId::Bitbucket) {
        return Ok(server);
    }
    let Some(bitbucket) = configuration.bitbucket.as_ref() else {
        return Ok(server);
    };
    let token = load_bitbucket_token(bitbucket)?;
    let backend = BitbucketPullRequestService::new(bitbucket, token)
        .map_err(|error| message(error.to_string()))?;
    Ok(server.with_bitbucket(Arc::new(backend)))
}

async fn setup(profile_path: Option<PathBuf>) -> Result<(), CliError> {
    let profile_path = profile_path.as_deref();
    let profile = load_setup_profile(profile_path)?;
    run_setup_provider(profile.as_ref()).await?;
    #[cfg(not(feature = "managed-distribution"))]
    install_selected_profile(profile_path, profile.as_ref())?;
    #[cfg(any(feature = "jira", feature = "bitbucket"))]
    configure_clients()?;
    Ok(())
}

async fn settings(profile_path: Option<PathBuf>) -> Result<(), CliError> {
    loop {
        let copy = settings_copy::settings_copy();
        let actions = settings_actions();
        let options = actions
            .iter()
            .map(|action| settings_action_label(*action, copy))
            .collect::<Vec<_>>();
        let selected = choose(&copy.title, &options).map_err(|error| terminal_error(&error));
        let action = match selected {
            Ok(index) => actions.get(index).copied().unwrap_or(SettingsAction::Back),
            Err(CliError::Cancelled) => SettingsAction::Back,
            Err(error) => return Err(error),
        };
        let result = match action {
            #[cfg(feature = "jira")]
            SettingsAction::Jira => {
                let profile = load_setup_profile(profile_path.as_deref())?;
                jira_settings::run(profile.as_ref()).await
            }
            #[cfg(feature = "bitbucket")]
            SettingsAction::Bitbucket => {
                let profile = load_setup_profile(profile_path.as_deref())?;
                bitbucket_settings::run(profile.as_ref()).await
            }
            SettingsAction::Clients => manage_clients(),
            SettingsAction::Skills => install_skills(),
            SettingsAction::Language => configure_language(),
            SettingsAction::Back => return Ok(()),
        };
        match result {
            Ok(()) | Err(CliError::Cancelled) => {}
            Err(error) => return Err(error),
        }
        present_notices(&tui_copy().result_title).map_err(|error| message(error.to_string()))?;
    }
}

fn settings_actions() -> Vec<SettingsAction> {
    let mut actions = vec![SettingsAction::Language];
    #[cfg(feature = "jira")]
    actions.push(SettingsAction::Jira);
    #[cfg(feature = "bitbucket")]
    actions.push(SettingsAction::Bitbucket);
    actions.extend([
        SettingsAction::Clients,
        SettingsAction::Skills,
        SettingsAction::Back,
    ]);
    actions
}

fn settings_action_label(action: SettingsAction, copy: &settings_copy::SettingsCopy) -> String {
    match action {
        #[cfg(feature = "jira")]
        SettingsAction::Jira => copy.jira.clone(),
        #[cfg(feature = "bitbucket")]
        SettingsAction::Bitbucket => copy.bitbucket.clone(),
        SettingsAction::Clients => copy.mcp_clients.clone(),
        SettingsAction::Skills => copy.assistant_skills.clone(),
        SettingsAction::Language => copy.language.clone(),
        SettingsAction::Back => copy.back.clone(),
    }
}

fn configure_language() -> Result<(), CliError> {
    let options = ["English".to_owned(), "Español".to_owned()];
    let selected = choose(&settings_copy::settings_copy().language, &options)
        .map_err(|error| terminal_error(&error))?;
    let language = match selected {
        0 => worklogger_settings::Language::English,
        1 => worklogger_settings::Language::Spanish,
        _ => return Err(message("the language selection is invalid")),
    };
    save_language(language)?;
    terminal_notice(settings_copy::settings_copy().language_saved.clone());
    Ok(())
}

pub(crate) fn preferred_language() -> worklogger_settings::Language {
    worklogger_settings::SettingsStore::for_current_user()
        .ok()
        .and_then(|store| store.load().ok().flatten())
        .and_then(|settings| settings.language)
        .unwrap_or_default()
}

fn save_language(language: worklogger_settings::Language) -> Result<(), CliError> {
    let store = worklogger_settings::SettingsStore::for_current_user()
        .map_err(|error| message(error.to_string()))?;
    let mut settings = store
        .load()
        .map_err(|error| message(error.to_string()))?
        .unwrap_or_default();
    settings.language = Some(language);
    store
        .save(&settings, settings.revision)
        .map_err(|error| message(error.to_string()))?;
    Ok(())
}

#[cfg_attr(
    not(any(feature = "jira", feature = "bitbucket")),
    expect(
        clippy::unused_async,
        reason = "the provider setup contract is asynchronous in addon builds"
    )
)]
async fn run_setup_provider(profile: Option<&OrganizationProfile>) -> Result<(), CliError> {
    #[cfg(all(feature = "jira", feature = "bitbucket"))]
    return match choose_setup_provider(profile)? {
        SetupProvider::Jira => setup_jira(profile).await,
        SetupProvider::Bitbucket => setup_bitbucket(profile).await,
    };
    #[cfg(all(feature = "jira", not(feature = "bitbucket")))]
    return setup_jira(profile).await;
    #[cfg(all(feature = "bitbucket", not(feature = "jira")))]
    return setup_bitbucket(profile).await;
    #[cfg(not(any(feature = "jira", feature = "bitbucket")))]
    {
        let _ = profile;
        Err(message(&tui_copy().profile_has_no_bundled_modules))
    }
}

#[cfg(not(feature = "managed-distribution"))]
fn install_selected_profile(
    profile_path: Option<&Path>,
    profile: Option<&OrganizationProfile>,
) -> Result<(), CliError> {
    let Some((_, profile)) = profile_path.zip(profile) else {
        return Ok(());
    };
    OrganizationProfileStore::for_current_user()
        .map_err(|error| message(error.to_string()))?
        .save(profile)
        .map_err(|error| message(error.to_string()))
}

#[cfg(feature = "managed-distribution")]
fn load_setup_profile(
    profile_path: Option<&Path>,
) -> Result<Option<OrganizationProfile>, CliError> {
    if profile_path.is_some() {
        return Err(message(&tui_copy().managed_profile_immutable));
    }
    embedded_organization_profile().map(Some)
}

#[cfg(not(feature = "managed-distribution"))]
fn load_setup_profile(
    profile_path: Option<&Path>,
) -> Result<Option<OrganizationProfile>, CliError> {
    if let Some(path) = profile_path {
        return OrganizationProfileStore::read_from(path)
            .map(Some)
            .map_err(|error| message(error.to_string()));
    }
    let profile = OrganizationProfileStore::for_current_user()
        .map_err(|error| message(error.to_string()))?
        .load()
        .map_err(|error| message(error.to_string()))?;
    profile.map_or_else(
        || embedded_organization_profile().map(Some),
        |value| Ok(Some(value)),
    )
}

#[cfg(feature = "jira")]
struct JiraSetupValues {
    site: String,
    email: String,
    board_id: u64,
    capabilities: BTreeSet<Capability>,
    hours: Option<JiraHoursConfiguration>,
    limits: ProviderRequestLimits,
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn credential_transaction() -> Result<CredentialTransactionGuard, CliError> {
    CredentialTransactionGuard::acquire().map_err(|error| message(error.to_string()))
}

#[cfg(feature = "jira")]
async fn setup_jira(profile: Option<&OrganizationProfile>) -> Result<(), CliError> {
    let copy = tui_copy();
    let jira_profile = jira_setup_profile(profile)?;
    let site = choose_jira_site(jira_profile)?;
    let email = prompt(&copy.jira_email_label, None)?;
    let token = read_token()?;
    let limits = jira_setup_limits(jira_profile)?;
    show_progress(&copy.working_title, &copy.validating_account)
        .map_err(|error| message(error.to_string()))?;
    let (identity, boards) = discover(&site, &email, &token, limits).await?;
    terminal_notice(format!("{}: {identity}", copy.account_verified));
    let boards = scoped_jira_boards(jira_profile, &site, boards);
    let board_id = choose_board(&boards)?;
    let capabilities = jira_setup_capabilities(jira_profile)?;
    let hours = configure_jira_hours(&capabilities, jira_profile)?;
    let configuration = persist_jira_setup(
        JiraSetupValues {
            site,
            email,
            board_id,
            capabilities,
            hours,
            limits,
        },
        &token,
        profile,
    )?;
    let tools = WorkloggerMcpServer::configured_tool_names(&configuration).join(", ");
    terminal_notice(format!("{}: {tools}", copy.setup_saved));
    print_platform_secret_notice(JIRA_API_TOKEN_ENVIRONMENT_VARIABLE);
    Ok(())
}

#[cfg(feature = "jira")]
fn persist_jira_setup(
    values: JiraSetupValues,
    token: &str,
    profile: Option<&OrganizationProfile>,
) -> Result<McpConfiguration, CliError> {
    let _transaction_guard = credential_transaction()?;
    let current = ConfigurationStore::for_current_user()?.load()?;
    let jira_profile = profile.and_then(|organization| organization.modules.jira.as_ref());
    let configuration = configured_jira(values, current.as_ref(), jira_profile)?;
    let configuration = apply_setup_profile(configuration, profile)?;
    save_setup(&configuration, token)?;
    Ok(configuration)
}

#[cfg(feature = "bitbucket")]
async fn setup_bitbucket(profile: Option<&OrganizationProfile>) -> Result<(), CliError> {
    let copy = tui_copy();
    let bitbucket_profile = bitbucket_setup_profile(profile)?;
    let email = prompt(&copy.bitbucket_email_label, None)?;
    let workspace = choose_bitbucket_workspace(bitbucket_profile)?;
    let token = read_bitbucket_setup_token()?;
    let limits = bitbucket_setup_limits(bitbucket_profile)?;
    show_progress(&copy.working_title, &copy.validating_account)
        .map_err(|error| message(error.to_string()))?;
    let (identity, repositories) = discover_bitbucket(&email, &token, &workspace, limits).await?;
    terminal_notice(format!("{}: {identity}", copy.account_verified));
    let repositories = scoped_bitbucket_repositories(bitbucket_profile, &workspace, repositories);
    let repositories = select_bitbucket_repositories(bitbucket_profile, &repositories)?;
    let capabilities = bitbucket_setup_capabilities(bitbucket_profile)?;
    let pull_request_defaults = bitbucket_pull_request_defaults()?;
    complete_bitbucket_setup(
        BitbucketSetupValues {
            email,
            workspace,
            repositories,
            capabilities,
            pull_request_defaults,
            limits,
        },
        &token,
        profile,
    )
}

#[cfg(feature = "bitbucket")]
fn complete_bitbucket_setup(
    values: BitbucketSetupValues,
    token: &str,
    profile: Option<&OrganizationProfile>,
) -> Result<(), CliError> {
    let copy = tui_copy();
    let configuration = persist_bitbucket_setup(values, token, profile)?;
    let tools = WorkloggerMcpServer::configured_tool_names(&configuration).join(", ");
    terminal_notice(format!("{}: {tools}", copy.setup_saved));
    print_platform_secret_notice(BITBUCKET_API_TOKEN_ENVIRONMENT_VARIABLE);
    Ok(())
}

#[cfg(feature = "bitbucket")]
fn persist_bitbucket_setup(
    values: BitbucketSetupValues,
    token: &str,
    profile: Option<&OrganizationProfile>,
) -> Result<McpConfiguration, CliError> {
    let _transaction_guard = credential_transaction()?;
    let current = ConfigurationStore::for_current_user()?.load()?;
    let configuration = configured_bitbucket(
        values,
        current.as_ref(),
        profile.and_then(|value| value.modules.bitbucket.as_ref()),
    )?;
    let configuration = apply_setup_profile(configuration, profile)?;
    save_bitbucket_setup(&configuration, token)?;
    Ok(configuration)
}

fn apply_setup_profile(
    configuration: McpConfiguration,
    profile: Option<&OrganizationProfile>,
) -> Result<McpConfiguration, CliError> {
    match profile {
        Some(profile) => configuration
            .apply_organization_profile(profile)
            .map_err(CliError::from),
        None => Ok(configuration),
    }
}

#[cfg(all(feature = "jira", feature = "bitbucket"))]
fn choose_setup_provider(profile: Option<&OrganizationProfile>) -> Result<SetupProvider, CliError> {
    match available_setup_providers(profile).as_slice() {
        [provider] => return Ok(*provider),
        [] => return Err(message(&tui_copy().profile_has_no_bundled_modules)),
        _ => {}
    }
    let copy = tui_copy();
    let options = vec![
        DetailedChoice::new(
            copy.provider_jira.clone(),
            copy.provider_jira_description.clone(),
        ),
        DetailedChoice::new(
            copy.provider_bitbucket.clone(),
            copy.provider_bitbucket_description.clone(),
        ),
    ];
    match choose_detailed(&copy.provider_title, &options).map_err(|error| terminal_error(&error))? {
        0 => Ok(SetupProvider::Jira),
        1 => Ok(SetupProvider::Bitbucket),
        _ => Err(message(&copy.invalid_provider_selection)),
    }
}

#[cfg(all(feature = "jira", feature = "bitbucket"))]
fn available_setup_providers(profile: Option<&OrganizationProfile>) -> Vec<SetupProvider> {
    let jira_available = profile.is_none_or(|value| value.modules.jira.is_some());
    let bitbucket_available = profile.is_none_or(|value| value.modules.bitbucket.is_some());
    let mut providers = Vec::new();
    if jira_available {
        providers.push(SetupProvider::Jira);
    }
    if bitbucket_available {
        providers.push(SetupProvider::Bitbucket);
    }
    providers
}

#[cfg(feature = "jira")]
fn jira_setup_profile(
    profile: Option<&OrganizationProfile>,
) -> Result<Option<&JiraModuleProfile>, CliError> {
    required_profile_module(
        profile,
        |value| value.modules.jira.as_ref(),
        &tui_copy().provider_jira,
    )
}

#[cfg(feature = "bitbucket")]
fn bitbucket_setup_profile(
    profile: Option<&OrganizationProfile>,
) -> Result<Option<&BitbucketModuleProfile>, CliError> {
    required_profile_module(
        profile,
        |value| value.modules.bitbucket.as_ref(),
        &tui_copy().provider_bitbucket,
    )
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn required_profile_module<'profile, Module>(
    profile: Option<&'profile OrganizationProfile>,
    select: impl FnOnce(&'profile OrganizationProfile) -> Option<&'profile Module>,
    module_name: &str,
) -> Result<Option<&'profile Module>, CliError> {
    let Some(profile) = profile else {
        return Ok(None);
    };
    select(profile).map(Some).ok_or_else(|| {
        message(format!(
            "{} {module_name}",
            tui_copy().profile_module_unavailable
        ))
    })
}

fn status() -> Result<(), CliError> {
    let store = ConfigurationStore::for_current_user()?;
    let configuration = effective_configuration(store.load()?)?;
    let document = status_document(&store, configuration.as_ref());
    let json =
        serde_json::to_string_pretty(&document).map_err(|error| message(error.to_string()))?;
    println!("{json}");
    Ok(())
}

fn uninstall() -> Result<(), CliError> {
    let store = ConfigurationStore::for_current_user()?;
    let registration = ClientRegistrationService::for_current_user()
        .map_err(|error| message(error.to_string()))?;
    let server = installed_server_path()?;
    let clients = registration.statuses(&server);
    if !uninstall_confirmed(&clients)? {
        terminal_notice(tui_copy().no_changes.clone());
        return Ok(());
    }
    #[cfg(any(feature = "jira", feature = "bitbucket"))]
    let _transaction_guard = credential_transaction()?;
    #[cfg(all(
        any(windows, target_os = "linux"),
        any(feature = "jira", feature = "bitbucket")
    ))]
    let snapshot = uninstall_snapshot(&store)?;
    unregister_clients(&registration, &server, &clients)?;
    #[cfg(all(
        any(windows, target_os = "linux"),
        any(feature = "jira", feature = "bitbucket")
    ))]
    clear_secure_installation(&store, &snapshot)?;
    #[cfg(not(all(
        any(windows, target_os = "linux"),
        any(feature = "jira", feature = "bitbucket")
    )))]
    store.clear()?;
    terminal_notice(tui_copy().uninstall_complete.clone());
    Ok(())
}

fn uninstall_confirmed(clients: &[McpClientStatus]) -> Result<bool, CliError> {
    ensure_clients_are_readable(clients)?;
    print_registered_clients(clients);
    confirm(&tui_copy().uninstall_confirmation)
}

fn ensure_clients_are_readable(clients: &[McpClientStatus]) -> Result<(), CliError> {
    let unreadable = clients
        .iter()
        .find(|status| status.state == RegistrationState::InvalidConfiguration);
    let Some(status) = unreadable else {
        return Ok(());
    };
    let detail = status
        .detail
        .as_deref()
        .unwrap_or(&tui_copy().unreadable_configuration);
    Err(message(format!(
        "{}: {}: {detail}",
        tui_copy().unsafe_uninstall,
        status.client.display_name()
    )))
}

fn required_configuration() -> Result<McpConfiguration, CliError> {
    let configuration = ConfigurationStore::for_current_user()?.load()?;
    effective_configuration(configuration)?.ok_or(CliError::NotConfigured)
}

fn effective_configuration(
    configuration: Option<McpConfiguration>,
) -> Result<Option<McpConfiguration>, CliError> {
    let Some(configuration) = configuration else {
        return Ok(None);
    };
    let profile = runtime_organization_profile()?;
    match profile {
        Some(profile) => configuration
            .apply_organization_profile(&profile)
            .map(Some)
            .map_err(CliError::from),
        None => configuration
            .apply_bundled_modules()
            .map(Some)
            .map_err(CliError::from),
    }
}

#[cfg(feature = "managed-distribution")]
fn runtime_organization_profile() -> Result<Option<OrganizationProfile>, CliError> {
    embedded_organization_profile().map(Some)
}

#[cfg(not(feature = "managed-distribution"))]
fn runtime_organization_profile() -> Result<Option<OrganizationProfile>, CliError> {
    let profile = OrganizationProfileStore::for_current_user()
        .map_err(|error| message(error.to_string()))?
        .load()
        .map_err(|error| message(error.to_string()))?;
    profile.map_or_else(
        || embedded_organization_profile().map(Some),
        |value| Ok(Some(value)),
    )
}

fn embedded_organization_profile() -> Result<OrganizationProfile, CliError> {
    OrganizationProfile::from_json(EMBEDDED_ORGANIZATION_PROFILE)
        .map_err(|error| message(error.to_string()))
}

#[cfg(feature = "jira")]
fn required_jira(configuration: &McpConfiguration) -> Result<&JiraConfiguration, CliError> {
    configuration
        .jira
        .as_ref()
        .ok_or_else(|| message("the Jira connection is not enabled"))
}

#[cfg(all(any(windows, target_os = "linux"), feature = "bitbucket"))]
fn required_bitbucket(
    configuration: &McpConfiguration,
) -> Result<&BitbucketConfiguration, CliError> {
    configuration
        .bitbucket
        .as_ref()
        .ok_or_else(|| message("the Bitbucket connection is not enabled"))
}

#[cfg(feature = "jira")]
fn load_token(configuration: &JiraConfiguration) -> Result<String, CliError> {
    if let Some(token) = environment_token() {
        return Ok(token);
    }
    CredentialStore::for_purpose(CredentialPurpose::Mcp)
        .ok()
        .and_then(|store| {
            store
                .load_api_token(&configuration.base_url, &configuration.email)
                .ok()
                .flatten()
        })
        .ok_or(CliError::MissingJiraToken)
}

#[cfg(feature = "jira")]
fn environment_token() -> Option<String> {
    std::env::var(JIRA_API_TOKEN_ENVIRONMENT_VARIABLE)
        .ok()
        .filter(|token| !token.trim().is_empty())
}

#[cfg(feature = "bitbucket")]
fn load_bitbucket_token(configuration: &BitbucketConfiguration) -> Result<String, CliError> {
    if let Some(token) = bitbucket_environment_token() {
        return Ok(token);
    }
    CredentialStore::for_purpose(CredentialPurpose::Mcp)
        .ok()
        .and_then(|store| {
            store
                .load_api_token(BITBUCKET_CLOUD_API_ORIGIN, &configuration.email)
                .ok()
                .flatten()
        })
        .ok_or(CliError::MissingBitbucketToken)
}

#[cfg(feature = "bitbucket")]
fn bitbucket_environment_token() -> Option<String> {
    std::env::var(BITBUCKET_API_TOKEN_ENVIRONMENT_VARIABLE)
        .ok()
        .filter(|token| !token.trim().is_empty())
}

#[cfg(feature = "jira")]
async fn discover(
    site: &str,
    email: &str,
    token: &str,
    limits: ProviderRequestLimits,
) -> Result<(String, Vec<BoardDto>), CliError> {
    let url = JiraSiteUrl::parse(site).map_err(|error| message(error.to_string()))?;
    let timeout = Duration::from_secs(limits.timeout_seconds);
    let client =
        JiraClient::new(url, email, token, timeout).map_err(|error| message(error.to_string()))?;
    let identity = client
        .current_user()
        .await
        .map_err(|error| message(error.to_string()))?;
    let boards = discover_boards(&client, limits).await?;
    Ok((identity.display_name, boards))
}

#[cfg(feature = "jira")]
async fn discover_boards(
    client: &JiraClient,
    configured: ProviderRequestLimits,
) -> Result<Vec<BoardDto>, CliError> {
    let limits = PageLimits::new(configured.page_size, configured.maximum_collection_items)
        .map_err(|error| message(error.to_string()))?;
    client
        .list_boards(limits)
        .await
        .map_err(|error| message(error.to_string()))
}

#[cfg(feature = "bitbucket")]
async fn discover_bitbucket(
    email: &str,
    token: &str,
    workspace: &str,
    limits: ProviderRequestLimits,
) -> Result<(String, Vec<Repository>), CliError> {
    let timeout = Duration::from_secs(limits.timeout_seconds);
    let client =
        BitbucketClient::new(email, token, timeout).map_err(|error| message(error.to_string()))?;
    let identity = client
        .current_user()
        .await
        .map_err(|error| message(error.to_string()))?;
    let limits = BitbucketPageLimits::new(limits.page_size, limits.maximum_collection_items)
        .map_err(|error| message(error.to_string()))?;
    let repositories = client
        .list_repositories(workspace, limits)
        .await
        .map_err(|error| message(error.to_string()))?;
    Ok((identity.display_name, repositories))
}

#[cfg(feature = "jira")]
fn choose_jira_site(profile: Option<&JiraModuleProfile>) -> Result<String, CliError> {
    let sites = profile
        .map(|value| value.sites.as_slice())
        .unwrap_or_default();
    match sites {
        [] => prompt(
            &tui_copy().jira_site_label,
            Some(&tui_copy().jira_site_example),
        ),
        [site] => Ok(site.url.clone()),
        _ => choose_jira_profile_site(sites),
    }
}

#[cfg(feature = "jira")]
fn choose_jira_profile_site(
    sites: &[worklogger_profile::JiraSiteProfile],
) -> Result<String, CliError> {
    let options = sites
        .iter()
        .map(|site| format!("{} ({})", site.name, site.url))
        .collect::<Vec<_>>();
    let index = choose(&tui_copy().profile_sites_title, &options)
        .map_err(|error| terminal_error(&error))?;
    Ok(sites[index].url.clone())
}

#[cfg(feature = "bitbucket")]
fn choose_bitbucket_workspace(
    profile: Option<&BitbucketModuleProfile>,
) -> Result<String, CliError> {
    let Some(workspaces) = profile.and_then(|value| value.workspaces.as_ref()) else {
        return prompt(&tui_copy().bitbucket_workspace_label, None);
    };
    if workspaces.len() == 1 {
        return workspaces
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| message(&tui_copy().invalid_provider_selection));
    }
    choose_profile_workspace(workspaces)
}

#[cfg(feature = "bitbucket")]
fn choose_profile_workspace(
    workspaces: &BTreeMap<String, BTreeSet<String>>,
) -> Result<String, CliError> {
    let options = workspaces.keys().cloned().collect::<Vec<_>>();
    let index = choose(&tui_copy().profile_workspaces_title, &options)
        .map_err(|error| terminal_error(&error))?;
    workspaces
        .keys()
        .nth(index)
        .cloned()
        .ok_or_else(|| message(&tui_copy().invalid_provider_selection))
}

#[cfg(feature = "jira")]
fn scoped_jira_boards(
    profile: Option<&JiraModuleProfile>,
    site_url: &str,
    boards: Vec<BoardDto>,
) -> Vec<BoardDto> {
    match profile {
        Some(policy) => boards
            .into_iter()
            .filter(|board| policy.allows_board(site_url, board.id))
            .collect(),
        None => boards,
    }
}

#[cfg(feature = "bitbucket")]
fn scoped_bitbucket_repositories(
    profile: Option<&BitbucketModuleProfile>,
    workspace: &str,
    repositories: Vec<Repository>,
) -> Vec<Repository> {
    match profile {
        Some(policy) => repositories
            .into_iter()
            .filter(|repository| policy.allows_repository(workspace, &repository.slug))
            .collect(),
        None => repositories,
    }
}

#[cfg(feature = "bitbucket")]
fn select_bitbucket_repositories(
    profile: Option<&BitbucketModuleProfile>,
    repositories: &[Repository],
) -> Result<BTreeSet<String>, CliError> {
    if profile
        .and_then(|value| value.workspaces.as_ref())
        .is_some()
    {
        if repositories.is_empty() {
            return Err(message(&tui_copy().no_repositories));
        }
        return Ok(repositories
            .iter()
            .map(|repository| repository.slug.clone())
            .collect());
    }
    choose_repositories(repositories)
}

#[cfg(feature = "jira")]
fn choose_board(boards: &[BoardDto]) -> Result<u64, CliError> {
    if boards.is_empty() {
        return Err(message(&tui_copy().no_boards));
    }
    let options = boards
        .iter()
        .map(|board| format!("{} ({})", board.name, board.board_type))
        .collect::<Vec<_>>();
    let selected =
        choose(&tui_copy().boards_title, &options).map_err(|error| terminal_error(&error))?;
    boards
        .get(selected)
        .map(|board| board.id)
        .ok_or_else(|| message(&tui_copy().invalid_board_selection))
}

#[cfg(feature = "bitbucket")]
fn choose_repositories(repositories: &[Repository]) -> Result<BTreeSet<String>, CliError> {
    if repositories.is_empty() {
        return Err(message(&tui_copy().no_repositories));
    }
    let options = repositories
        .iter()
        .map(|repository| format!("{} ({})", repository.name, repository.slug))
        .collect::<Vec<_>>();
    let selected = choose_many(
        &tui_copy().repositories_title,
        &options,
        vec![false; repositories.len()],
    )
    .map_err(|error| terminal_error(&error))?;
    if !selected.iter().any(|is_selected| *is_selected) {
        return Err(message(&tui_copy().invalid_repository_selection));
    }
    Ok(repositories
        .iter()
        .zip(selected)
        .filter_map(|(repository, is_selected)| is_selected.then_some(repository.slug.clone()))
        .collect())
}

#[cfg(feature = "jira")]
fn configured_jira(
    values: JiraSetupValues,
    current: Option<&McpConfiguration>,
    profile: Option<&JiraModuleProfile>,
) -> Result<McpConfiguration, CliError> {
    let jira = JiraConfiguration {
        base_url: values.site,
        email: values.email,
        board_id: values.board_id,
        request_timeout_seconds: values.limits.timeout_seconds,
        page_size: values.limits.page_size,
        maximum_collection_items: values.limits.maximum_collection_items,
        maximum_issue_search_results: selected_jira_issue_search_limit(current, profile),
        hours: values.hours,
    };
    let modules = enabled_jira_module(values.capabilities, current);
    match current.and_then(|value| value.bitbucket.clone()) {
        Some(bitbucket) => McpConfiguration::new_with_bitbucket(jira, bitbucket, modules),
        None => McpConfiguration::new(jira, modules),
    }
    .map_err(CliError::from)
}

#[cfg(feature = "jira")]
fn selected_jira_issue_search_limit(
    current: Option<&McpConfiguration>,
    profile: Option<&JiraModuleProfile>,
) -> usize {
    let current = current
        .and_then(|configuration| configuration.jira.as_ref())
        .map(|jira| jira.maximum_issue_search_results);
    match current {
        Some(limit)
            if profile
                .is_none_or(|policy| limit <= policy.maximum_allowed_issue_search_results) =>
        {
            limit
        }
        Some(_) | None => profile.map_or(DEFAULT_MAXIMUM_ISSUE_SEARCH_RESULTS, |policy| {
            policy.maximum_issue_search_results
        }),
    }
}

#[cfg(feature = "bitbucket")]
fn configured_bitbucket(
    values: BitbucketSetupValues,
    current: Option<&McpConfiguration>,
    profile: Option<&BitbucketModuleProfile>,
) -> Result<McpConfiguration, CliError> {
    let BitbucketSetupValues {
        email,
        workspace,
        repositories,
        capabilities,
        pull_request_defaults,
        limits,
    } = values;
    let workspaces =
        configured_bitbucket_workspaces(current, &email, workspace, repositories, profile);
    let bitbucket = BitbucketConfiguration {
        email,
        workspaces,
        request_timeout_seconds: limits.timeout_seconds,
        page_size: limits.page_size,
        maximum_collection_items: limits.maximum_collection_items,
        pull_request_defaults,
    };
    let modules = enabled_bitbucket_module(capabilities, current);
    match current.and_then(|value| value.jira.clone()) {
        Some(jira) => McpConfiguration::new_with_bitbucket(jira, bitbucket, modules),
        None => McpConfiguration::new_bitbucket(bitbucket, modules),
    }
    .map_err(CliError::from)
}

#[cfg(feature = "bitbucket")]
fn configured_bitbucket_workspaces(
    current: Option<&McpConfiguration>,
    email: &str,
    workspace: String,
    repositories: BTreeSet<String>,
    profile: Option<&BitbucketModuleProfile>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut workspaces = current
        .and_then(|value| value.bitbucket.as_ref())
        .filter(|value| value.email.eq_ignore_ascii_case(email))
        .map_or_else(BTreeMap::new, |value| value.workspaces.clone());
    retain_profile_workspaces(&mut workspaces, profile);
    workspaces.insert(workspace, repositories);
    workspaces
}

#[cfg(feature = "bitbucket")]
fn retain_profile_workspaces(
    workspaces: &mut BTreeMap<String, BTreeSet<String>>,
    profile: Option<&BitbucketModuleProfile>,
) {
    let Some(profile) = profile else {
        return;
    };
    workspaces.retain(|workspace, repositories| {
        repositories.retain(|repository| profile.allows_repository(workspace, repository));
        !repositories.is_empty()
    });
}

#[cfg(feature = "jira")]
fn jira_setup_limits(
    profile: Option<&JiraModuleProfile>,
) -> Result<ProviderRequestLimits, CliError> {
    let current = ConfigurationStore::for_current_user()?.load()?;
    let jira = current
        .as_ref()
        .and_then(|configuration| configuration.jira.as_ref());
    Ok(selected_jira_setup_limits(jira, profile))
}

#[cfg(feature = "jira")]
fn selected_jira_setup_limits(
    current: Option<&JiraConfiguration>,
    profile: Option<&JiraModuleProfile>,
) -> ProviderRequestLimits {
    selected_provider_setup_limits(
        current.map(jira_limits),
        profile.map(jira_profile_limits),
        |limits| profile.is_none_or(|policy| jira_request_limits_allowed(limits, policy)),
    )
}

#[cfg(feature = "jira")]
const fn jira_profile_limits(profile: &JiraModuleProfile) -> ProviderRequestLimits {
    ProviderRequestLimits {
        timeout_seconds: profile.request_timeout_seconds,
        page_size: profile.page_size,
        maximum_collection_items: profile.maximum_collection_items,
    }
}

#[cfg(feature = "jira")]
const fn jira_request_limits_allowed(
    limits: ProviderRequestLimits,
    profile: &JiraModuleProfile,
) -> bool {
    limits.timeout_seconds <= profile.maximum_allowed_request_timeout_seconds
        && limits.page_size <= profile.maximum_allowed_page_size
        && limits.maximum_collection_items <= profile.maximum_allowed_collection_items
}

#[cfg(feature = "jira")]
const fn jira_limits(jira: &JiraConfiguration) -> ProviderRequestLimits {
    ProviderRequestLimits {
        timeout_seconds: jira.request_timeout_seconds,
        page_size: jira.page_size,
        maximum_collection_items: jira.maximum_collection_items,
    }
}

#[cfg(feature = "bitbucket")]
fn bitbucket_setup_limits(
    profile: Option<&BitbucketModuleProfile>,
) -> Result<ProviderRequestLimits, CliError> {
    let current = ConfigurationStore::for_current_user()?.load()?;
    let bitbucket = current
        .as_ref()
        .and_then(|configuration| configuration.bitbucket.as_ref());
    Ok(selected_bitbucket_setup_limits(bitbucket, profile))
}

#[cfg(feature = "bitbucket")]
fn selected_bitbucket_setup_limits(
    current: Option<&BitbucketConfiguration>,
    profile: Option<&BitbucketModuleProfile>,
) -> ProviderRequestLimits {
    selected_provider_setup_limits(
        current.map(bitbucket_limits),
        profile.map(bitbucket_profile_limits),
        |limits| profile.is_none_or(|policy| bitbucket_request_limits_allowed(limits, policy)),
    )
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn selected_provider_setup_limits(
    current: Option<ProviderRequestLimits>,
    profile_defaults: Option<ProviderRequestLimits>,
    current_allowed: impl FnOnce(ProviderRequestLimits) -> bool,
) -> ProviderRequestLimits {
    match current {
        Some(limits) if current_allowed(limits) => limits,
        Some(_) | None => profile_defaults.unwrap_or_else(default_provider_request_limits),
    }
}

#[cfg(feature = "bitbucket")]
const fn bitbucket_profile_limits(profile: &BitbucketModuleProfile) -> ProviderRequestLimits {
    ProviderRequestLimits {
        timeout_seconds: profile.request_timeout_seconds,
        page_size: profile.page_size,
        maximum_collection_items: profile.maximum_collection_items,
    }
}

#[cfg(feature = "bitbucket")]
const fn bitbucket_request_limits_allowed(
    limits: ProviderRequestLimits,
    profile: &BitbucketModuleProfile,
) -> bool {
    limits.timeout_seconds <= profile.maximum_allowed_request_timeout_seconds
        && limits.page_size <= profile.maximum_allowed_page_size
        && limits.maximum_collection_items <= profile.maximum_allowed_collection_items
}

#[cfg(feature = "bitbucket")]
const fn bitbucket_limits(bitbucket: &BitbucketConfiguration) -> ProviderRequestLimits {
    ProviderRequestLimits {
        timeout_seconds: bitbucket.request_timeout_seconds,
        page_size: bitbucket.page_size,
        maximum_collection_items: bitbucket.maximum_collection_items,
    }
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
const fn default_provider_request_limits() -> ProviderRequestLimits {
    ProviderRequestLimits {
        timeout_seconds: DEFAULT_REQUEST_TIMEOUT_SECONDS,
        page_size: DEFAULT_PAGE_SIZE,
        maximum_collection_items: DEFAULT_MAXIMUM_COLLECTION_ITEMS,
    }
}

#[cfg(feature = "jira")]
const fn default_jira_hours() -> JiraHoursConfiguration {
    JiraHoursConfiguration {
        weekly_target_hours: DEFAULT_WEEKLY_TARGET_HOURS,
        utc_offset_minutes: DEFAULT_UTC_OFFSET_MINUTES,
        maximum_daily_hours: 24,
        maximum_report_period_days: DEFAULT_MAXIMUM_REPORT_PERIOD_DAYS,
        maximum_concurrent_worklog_requests: DEFAULT_MAXIMUM_CONCURRENT_REQUESTS,
    }
}

#[cfg(feature = "jira")]
fn configure_jira_hours(
    capabilities: &BTreeSet<Capability>,
    profile: Option<&JiraModuleProfile>,
) -> Result<Option<JiraHoursConfiguration>, CliError> {
    if !capabilities.contains(&Capability::ReadOwnTimeEntries) {
        return Ok(None);
    }
    let current = ConfigurationStore::for_current_user()?.load()?;
    let defaults = jira_hours_defaults(current.as_ref(), profile)?;
    prompt_jira_hours(&defaults, profile).map(Some)
}

#[cfg(feature = "jira")]
fn prompt_jira_hours(
    defaults: &JiraHoursConfiguration,
    profile: Option<&JiraModuleProfile>,
) -> Result<JiraHoursConfiguration, CliError> {
    let weekly_target_hours = prompt_weekly_target(defaults.weekly_target_hours)?;
    let utc_offset_minutes = prompt_utc_offset(defaults.utc_offset_minutes)?;
    let hours = JiraHoursConfiguration {
        weekly_target_hours,
        utc_offset_minutes,
        maximum_daily_hours: defaults.maximum_daily_hours,
        maximum_report_period_days: defaults.maximum_report_period_days,
        maximum_concurrent_worklog_requests: defaults.maximum_concurrent_worklog_requests,
    };
    validate_prompted_hours(&hours, profile)?;
    Ok(hours)
}

#[cfg(feature = "jira")]
fn validate_prompted_hours(
    hours: &JiraHoursConfiguration,
    profile: Option<&JiraModuleProfile>,
) -> Result<(), CliError> {
    let allowed = profile.is_none_or(|policy| {
        policy
            .hours
            .allows_weekly_target(u32::from(hours.weekly_target_hours))
            && policy.hours.allows_utc_offset(hours.utc_offset_minutes)
            && hours.maximum_report_period_days <= policy.hours.maximum_custom_range_days
    });
    allowed
        .then_some(())
        .ok_or_else(|| message(&tui_copy().hours_outside_profile))
}

#[cfg(feature = "jira")]
fn prompt_weekly_target(default: u16) -> Result<u16, CliError> {
    let default = default.to_string();
    prompt(&tui_copy().weekly_target_hours, Some(&default))?
        .parse()
        .map_err(|_| message(&tui_copy().invalid_weekly_target_hours))
}

#[cfg(feature = "jira")]
fn prompt_utc_offset(default: i16) -> Result<i16, CliError> {
    let default = format_utc_offset(default)?;
    let value = prompt(&tui_copy().utc_offset, Some(&default))?;
    parse_utc_offset(&value)
}

#[cfg(feature = "jira")]
fn jira_hours_defaults(
    current: Option<&McpConfiguration>,
    profile: Option<&JiraModuleProfile>,
) -> Result<JiraHoursConfiguration, CliError> {
    let current = current
        .and_then(|configuration| configuration.jira.as_ref())
        .and_then(|jira| jira.hours.clone());
    match (current, profile) {
        (Some(hours), Some(profile)) => constrained_jira_hours(hours, profile),
        (Some(hours), None) => Ok(hours),
        (None, Some(profile)) => profile_jira_hours(profile),
        (None, None) => Ok(default_jira_hours()),
    }
}

#[cfg(feature = "jira")]
fn constrained_jira_hours(
    mut hours: JiraHoursConfiguration,
    profile: &JiraModuleProfile,
) -> Result<JiraHoursConfiguration, CliError> {
    if !profile
        .hours
        .allows_weekly_target(u32::from(hours.weekly_target_hours))
        || !profile.hours.allows_utc_offset(hours.utc_offset_minutes)
    {
        return profile_jira_hours(profile);
    }
    hours.maximum_concurrent_worklog_requests = hours
        .maximum_concurrent_worklog_requests
        .min(profile.maximum_allowed_concurrent_worklog_requests);
    hours.maximum_report_period_days = hours
        .maximum_report_period_days
        .min(profile.hours.maximum_custom_range_days);
    Ok(hours)
}

#[cfg(feature = "jira")]
fn profile_jira_hours(profile: &JiraModuleProfile) -> Result<JiraHoursConfiguration, CliError> {
    let weekly_target_hours = u16::try_from(profile.hours.suggested_weekly_target_hours)
        .map_err(|error| message(error.to_string()))?;
    Ok(JiraHoursConfiguration {
        weekly_target_hours,
        utc_offset_minutes: profile.hours.suggested_utc_offset_minutes,
        maximum_daily_hours: 24,
        maximum_report_period_days: DEFAULT_MAXIMUM_REPORT_PERIOD_DAYS
            .min(profile.hours.maximum_custom_range_days),
        maximum_concurrent_worklog_requests: profile.maximum_concurrent_worklog_requests,
    })
}

#[cfg(feature = "jira")]
fn parse_utc_offset(value: &str) -> Result<i16, CliError> {
    UtcOffset::parse(value, UTC_OFFSET_FORMAT)
        .map(UtcOffset::whole_minutes)
        .map_err(|_| message(&tui_copy().invalid_utc_offset))
}

#[cfg(feature = "jira")]
fn format_utc_offset(minutes: i16) -> Result<String, CliError> {
    let seconds = i32::from(minutes) * SECONDS_PER_MINUTE;
    let offset = UtcOffset::from_whole_seconds(seconds)
        .map_err(|_| message(&tui_copy().invalid_utc_offset))?;
    offset
        .format(UTC_OFFSET_FORMAT)
        .map_err(|_| message(&tui_copy().invalid_utc_offset))
}

#[cfg(feature = "jira")]
fn enabled_jira_module(
    capabilities: BTreeSet<Capability>,
    current: Option<&McpConfiguration>,
) -> BTreeMap<ModuleId, ModuleConfiguration> {
    let mut modules = current.map_or_else(BTreeMap::new, |value| value.modules.clone());
    modules.insert(ModuleId::Jira, configured_module(capabilities));
    modules
}

#[cfg(feature = "bitbucket")]
fn enabled_bitbucket_module(
    capabilities: BTreeSet<Capability>,
    current: Option<&McpConfiguration>,
) -> BTreeMap<ModuleId, ModuleConfiguration> {
    let mut modules = current.map_or_else(BTreeMap::new, |value| value.modules.clone());
    modules.insert(ModuleId::Bitbucket, configured_module(capabilities));
    modules
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn configured_module(capabilities: BTreeSet<Capability>) -> ModuleConfiguration {
    let mut module = ModuleConfiguration {
        enabled: false,
        capabilities: BTreeSet::new(),
    };
    for capability in capabilities {
        module.set_capability(capability, true);
    }
    module
}

#[cfg(feature = "jira")]
fn jira_setup_capabilities(
    profile: Option<&JiraModuleProfile>,
) -> Result<BTreeSet<Capability>, CliError> {
    choose_jira_capabilities(profile.map(|value| &value.mcp_capabilities))
}

#[cfg(feature = "bitbucket")]
fn bitbucket_setup_capabilities(
    profile: Option<&BitbucketModuleProfile>,
) -> Result<BTreeSet<Capability>, CliError> {
    choose_bitbucket_capabilities(profile.map(|value| &value.mcp_capabilities))
}

#[cfg(feature = "bitbucket")]
fn bitbucket_pull_request_defaults() -> Result<BitbucketPullRequestDefaults, CliError> {
    let current = ConfigurationStore::for_current_user()?
        .load()?
        .and_then(|configuration| configuration.bitbucket)
        .map_or_else(BitbucketPullRequestDefaults::default, |value| {
            value.pull_request_defaults
        });
    if !confirm(&tui_copy().configure_pull_request_defaults)? {
        return Ok(current);
    }
    let reviewer_account_ids = if confirm(&tui_copy().configure_default_reviewers)? {
        prompt_reviewer_account_ids(&current)?
    } else {
        current.reviewer_account_ids.clone()
    };
    let close_source_branch = confirm_with_default(
        &tui_copy().default_close_source_branch,
        current.close_source_branch,
    )?;
    Ok(BitbucketPullRequestDefaults {
        reviewer_account_ids,
        close_source_branch,
    })
}

#[cfg(feature = "bitbucket")]
fn prompt_reviewer_account_ids(
    current: &BitbucketPullRequestDefaults,
) -> Result<BTreeSet<String>, CliError> {
    let default = current
        .reviewer_account_ids
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join(",");
    let value = prompt(&tui_copy().default_reviewer_account_ids, Some(&default))?;
    Ok(value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect())
}

#[cfg(feature = "jira")]
fn choose_jira_capabilities(
    allowed: Option<&BTreeSet<Capability>>,
) -> Result<BTreeSet<Capability>, CliError> {
    let copy = tui_copy();
    let mut capabilities = select_capabilities(
        &copy.jira_capabilities_title,
        allowed,
        vec![
            (
                Capability::ReadOwnTimeEntries,
                copy.enable_hours.clone(),
                true,
            ),
            (
                Capability::WriteOwnTimeEntries,
                copy.enable_hours_write.clone(),
                false,
            ),
            (
                Capability::ReadJiraIssues,
                copy.enable_issue_read.clone(),
                true,
            ),
            (
                Capability::EditJiraIssues,
                copy.enable_issue_edit.clone(),
                false,
            ),
            (
                Capability::CommentJiraIssues,
                copy.enable_issue_comment.clone(),
                false,
            ),
            (
                Capability::TransitionJiraIssues,
                copy.enable_issue_transition.clone(),
                false,
            ),
        ],
    )?;
    disable_dependent_jira_capabilities(&mut capabilities);
    Ok(capabilities)
}

#[cfg(feature = "bitbucket")]
fn choose_bitbucket_capabilities(
    allowed: Option<&BTreeSet<Capability>>,
) -> Result<BTreeSet<Capability>, CliError> {
    let copy = tui_copy();
    let mut capabilities = select_capabilities(
        &copy.bitbucket_capabilities_title,
        allowed,
        vec![
            (
                Capability::ReadBitbucketPullRequests,
                copy.enable_bitbucket_read.clone(),
                true,
            ),
            (
                Capability::CreateBitbucketPullRequests,
                copy.enable_bitbucket_create.clone(),
                false,
            ),
            (
                Capability::EditBitbucketPullRequests,
                copy.enable_bitbucket_edit.clone(),
                false,
            ),
            (
                Capability::CommentBitbucketPullRequests,
                copy.enable_bitbucket_comment.clone(),
                false,
            ),
            (
                Capability::ReviewBitbucketPullRequests,
                copy.enable_bitbucket_review.clone(),
                false,
            ),
            (
                Capability::MergeBitbucketPullRequests,
                copy.enable_bitbucket_merge.clone(),
                false,
            ),
            (
                Capability::DeclineBitbucketPullRequests,
                copy.enable_bitbucket_decline.clone(),
                false,
            ),
        ],
    )?;
    disable_dependent_bitbucket_capabilities(&mut capabilities);
    Ok(capabilities)
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn select_capabilities(
    title: &str,
    allowed: Option<&BTreeSet<Capability>>,
    options: Vec<(Capability, String, bool)>,
) -> Result<BTreeSet<Capability>, CliError> {
    let available = options
        .into_iter()
        .filter(|(capability, _, _)| capability_available(allowed, *capability))
        .collect::<Vec<_>>();
    let labels = available
        .iter()
        .map(|(_, label, _)| label.clone())
        .collect::<Vec<_>>();
    let defaults = available
        .iter()
        .map(|(_, _, enabled)| *enabled)
        .collect::<Vec<_>>();
    let selected = choose_many(title, &labels, defaults).map_err(|error| terminal_error(&error))?;
    Ok(available
        .into_iter()
        .zip(selected)
        .filter_map(|((capability, _, _), enabled)| enabled.then_some(capability))
        .collect())
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn capability_available(allowed: Option<&BTreeSet<Capability>>, capability: Capability) -> bool {
    allowed.is_none_or(|capabilities| capabilities.contains(&capability))
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn disable_dependent_jira_capabilities(capabilities: &mut BTreeSet<Capability>) {
    if !capabilities.contains(&Capability::ReadOwnTimeEntries) {
        capabilities.remove(&Capability::WriteOwnTimeEntries);
    }
    if !capabilities.contains(&Capability::ReadJiraIssues) {
        capabilities.remove(&Capability::EditJiraIssues);
        capabilities.remove(&Capability::CommentJiraIssues);
        capabilities.remove(&Capability::TransitionJiraIssues);
    }
}

#[cfg(feature = "bitbucket")]
fn disable_dependent_bitbucket_capabilities(capabilities: &mut BTreeSet<Capability>) {
    if capabilities.contains(&Capability::ReadBitbucketPullRequests) {
        return;
    }
    capabilities.remove(&Capability::CreateBitbucketPullRequests);
    capabilities.remove(&Capability::EditBitbucketPullRequests);
    capabilities.remove(&Capability::CommentBitbucketPullRequests);
    capabilities.remove(&Capability::ReviewBitbucketPullRequests);
    capabilities.remove(&Capability::MergeBitbucketPullRequests);
    capabilities.remove(&Capability::DeclineBitbucketPullRequests);
}

#[cfg(all(any(windows, target_os = "linux"), feature = "jira"))]
struct SetupSnapshot {
    configuration: Option<McpConfiguration>,
    current_token: Option<String>,
    replaced_token: Option<String>,
}

#[cfg(all(any(windows, target_os = "linux"), feature = "bitbucket"))]
struct BitbucketSetupSnapshot {
    configuration: Option<McpConfiguration>,
    current_token: Option<String>,
    replaced_token: Option<String>,
}

#[cfg(all(any(windows, target_os = "linux"), feature = "jira"))]
fn save_setup(configuration: &McpConfiguration, token: &str) -> Result<(), CliError> {
    let credentials = CredentialStore::for_purpose(CredentialPurpose::Mcp)
        .map_err(|error| message(error.to_string()))?;
    let store = ConfigurationStore::for_current_user()?;
    let snapshot = setup_snapshot(&store, credentials, configuration)?;
    save_token(credentials, configuration, token)?;
    if let Err(error) = store.save(configuration) {
        let rollback = restore_token(credentials, configuration, snapshot.current_token);
        return Err(error_with_setup_rollback(error.into(), rollback));
    }
    if let Err(error) =
        delete_replaced_credential(credentials, snapshot.configuration.as_ref(), configuration)
    {
        let rollback = rollback_setup(&store, credentials, configuration, snapshot);
        return Err(error_with_setup_rollback(error, rollback));
    }
    Ok(())
}

#[cfg(all(any(windows, target_os = "linux"), feature = "bitbucket"))]
fn save_bitbucket_setup(configuration: &McpConfiguration, token: &str) -> Result<(), CliError> {
    let bitbucket = required_bitbucket(configuration)?;
    let credentials = CredentialStore::for_purpose(CredentialPurpose::Mcp)
        .map_err(|error| message(error.to_string()))?;
    let store = ConfigurationStore::for_current_user()?;
    let snapshot = bitbucket_setup_snapshot(&store, credentials, bitbucket)?;
    credentials
        .save_api_token(BITBUCKET_CLOUD_API_ORIGIN, &bitbucket.email, token)
        .map_err(|error| message(error.to_string()))?;
    if let Err(error) = store.save(configuration) {
        let rollback =
            restore_bitbucket_setup_token(credentials, bitbucket, snapshot.current_token);
        return Err(error_with_setup_rollback(error.into(), rollback));
    }
    if let Err(error) = delete_replaced_bitbucket_credential(credentials, &snapshot, bitbucket) {
        let rollback = rollback_bitbucket_setup(&store, credentials, bitbucket, snapshot);
        return Err(error_with_setup_rollback(error, rollback));
    }
    Ok(())
}

#[cfg(all(any(windows, target_os = "linux"), feature = "bitbucket"))]
fn bitbucket_setup_snapshot(
    store: &ConfigurationStore,
    credentials: CredentialStore,
    current: &BitbucketConfiguration,
) -> Result<BitbucketSetupSnapshot, CliError> {
    let configuration = store.load()?;
    let current_token = load_bitbucket_setup_token(credentials, &current.email)?;
    let replaced_token = configuration
        .as_ref()
        .and_then(|value| value.bitbucket.as_ref())
        .filter(|previous| !bitbucket_coordinates_match(previous, current))
        .map(|previous| load_bitbucket_setup_token(credentials, &previous.email))
        .transpose()?
        .flatten();
    Ok(BitbucketSetupSnapshot {
        configuration,
        current_token,
        replaced_token,
    })
}

#[cfg(all(any(windows, target_os = "linux"), feature = "bitbucket"))]
fn load_bitbucket_setup_token(
    credentials: CredentialStore,
    email: &str,
) -> Result<Option<String>, CliError> {
    credentials
        .load_api_token(BITBUCKET_CLOUD_API_ORIGIN, email)
        .map_err(|error| message(error.to_string()))
}

#[cfg(all(any(windows, target_os = "linux"), feature = "bitbucket"))]
fn bitbucket_coordinates_match(
    previous: &BitbucketConfiguration,
    current: &BitbucketConfiguration,
) -> bool {
    previous.email.eq_ignore_ascii_case(&current.email)
}

#[cfg(all(any(windows, target_os = "linux"), feature = "bitbucket"))]
fn delete_replaced_bitbucket_credential(
    credentials: CredentialStore,
    snapshot: &BitbucketSetupSnapshot,
    current: &BitbucketConfiguration,
) -> Result<(), CliError> {
    let Some(previous) = snapshot
        .configuration
        .as_ref()
        .and_then(|value| value.bitbucket.as_ref())
    else {
        return Ok(());
    };
    if bitbucket_coordinates_match(previous, current) {
        return Ok(());
    }
    credentials
        .delete_api_token(BITBUCKET_CLOUD_API_ORIGIN, &previous.email)
        .map_err(|error| message(error.to_string()))
}

#[cfg(all(any(windows, target_os = "linux"), feature = "bitbucket"))]
fn rollback_bitbucket_setup(
    store: &ConfigurationStore,
    credentials: CredentialStore,
    current: &BitbucketConfiguration,
    snapshot: BitbucketSetupSnapshot,
) -> Result<(), CliError> {
    let replaced_result = restore_replaced_bitbucket_setup_token(credentials, &snapshot, current);
    let current_result =
        restore_bitbucket_setup_token(credentials, current, snapshot.current_token);
    let configuration_result = restore_setup_configuration(store, snapshot.configuration);
    combine_setup_recovery_results(configuration_result, current_result, replaced_result)
}

#[cfg(all(any(windows, target_os = "linux"), feature = "bitbucket"))]
fn restore_replaced_bitbucket_setup_token(
    credentials: CredentialStore,
    snapshot: &BitbucketSetupSnapshot,
    current: &BitbucketConfiguration,
) -> Result<(), CliError> {
    let Some(previous) = snapshot
        .configuration
        .as_ref()
        .and_then(|value| value.bitbucket.as_ref())
    else {
        return Ok(());
    };
    if bitbucket_coordinates_match(previous, current) {
        return Ok(());
    }
    restore_bitbucket_setup_token(credentials, previous, snapshot.replaced_token.clone())
}

#[cfg(all(any(windows, target_os = "linux"), feature = "bitbucket"))]
fn restore_bitbucket_setup_token(
    credentials: CredentialStore,
    configuration: &BitbucketConfiguration,
    previous: Option<String>,
) -> Result<(), CliError> {
    let result = match previous {
        Some(token) => {
            credentials.save_api_token(BITBUCKET_CLOUD_API_ORIGIN, &configuration.email, &token)
        }
        None => credentials.delete_api_token(BITBUCKET_CLOUD_API_ORIGIN, &configuration.email),
    };
    result.map_err(|error| message(error.to_string()))
}

#[cfg(all(any(windows, target_os = "linux"), feature = "jira"))]
fn setup_snapshot(
    store: &ConfigurationStore,
    credentials: CredentialStore,
    current: &McpConfiguration,
) -> Result<SetupSnapshot, CliError> {
    let configuration = store.load()?;
    let current_token = load_setup_token(credentials, required_jira(current)?)?;
    let replaced_token = load_replaced_token(credentials, configuration.as_ref(), current)?;
    Ok(SetupSnapshot {
        configuration,
        current_token,
        replaced_token,
    })
}

#[cfg(all(any(windows, target_os = "linux"), feature = "jira"))]
fn load_setup_token(
    credentials: CredentialStore,
    jira: &JiraConfiguration,
) -> Result<Option<String>, CliError> {
    credentials
        .load_api_token(&jira.base_url, &jira.email)
        .map_err(|error| message(error.to_string()))
}

#[cfg(all(any(windows, target_os = "linux"), feature = "jira"))]
fn load_replaced_token(
    credentials: CredentialStore,
    previous: Option<&McpConfiguration>,
    current: &McpConfiguration,
) -> Result<Option<String>, CliError> {
    let Some(previous) = previous else {
        return Ok(None);
    };
    let Some(previous_jira) = previous.jira.as_ref() else {
        return Ok(None);
    };
    if setup_coordinates_match(previous, current)? {
        return Ok(None);
    }
    load_setup_token(credentials, previous_jira)
}

#[cfg(all(any(windows, target_os = "linux"), feature = "jira"))]
fn save_token(
    credentials: CredentialStore,
    configuration: &McpConfiguration,
    token: &str,
) -> Result<(), CliError> {
    let jira = required_jira(configuration)?;
    credentials
        .save_api_token(&jira.base_url, &jira.email, token)
        .map_err(|error| message(error.to_string()))
}

#[cfg(all(any(windows, target_os = "linux"), feature = "jira"))]
fn delete_replaced_credential(
    credentials: CredentialStore,
    previous: Option<&McpConfiguration>,
    current: &McpConfiguration,
) -> Result<(), CliError> {
    let Some(previous) = previous else {
        return Ok(());
    };
    let Some(jira) = previous.jira.as_ref() else {
        return Ok(());
    };
    if setup_coordinates_match(previous, current)? {
        return Ok(());
    }
    credentials
        .delete_api_token(&jira.base_url, &jira.email)
        .map_err(|error| message(error.to_string()))
}

#[cfg(all(any(windows, target_os = "linux"), feature = "jira"))]
fn setup_coordinates_match(
    previous: &McpConfiguration,
    current: &McpConfiguration,
) -> Result<bool, CliError> {
    let Some(previous) = previous.jira.as_ref() else {
        return Ok(false);
    };
    let current = required_jira(current)?;
    api_token_coordinates_match(
        &previous.base_url,
        &previous.email,
        &current.base_url,
        &current.email,
    )
    .map_err(|error| message(error.to_string()))
}

#[cfg(all(not(any(windows, target_os = "linux")), feature = "jira"))]
fn save_setup(configuration: &McpConfiguration, _token: &str) -> Result<(), CliError> {
    ConfigurationStore::for_current_user()?.save(configuration)?;
    Ok(())
}

#[cfg(all(not(any(windows, target_os = "linux")), feature = "bitbucket"))]
fn save_bitbucket_setup(configuration: &McpConfiguration, _token: &str) -> Result<(), CliError> {
    ConfigurationStore::for_current_user()?.save(configuration)?;
    Ok(())
}

#[cfg(all(any(windows, target_os = "linux"), feature = "jira"))]
fn restore_token(
    credentials: CredentialStore,
    configuration: &McpConfiguration,
    previous: Option<String>,
) -> Result<(), CliError> {
    let jira = required_jira(configuration)?;
    let result = match previous {
        Some(token) => credentials.save_api_token(&jira.base_url, &jira.email, &token),
        None => credentials.delete_api_token(&jira.base_url, &jira.email),
    };
    result.map_err(|error| message(error.to_string()))
}

#[cfg(all(any(windows, target_os = "linux"), feature = "jira"))]
fn rollback_setup(
    store: &ConfigurationStore,
    credentials: CredentialStore,
    current: &McpConfiguration,
    snapshot: SetupSnapshot,
) -> Result<(), CliError> {
    let replaced_result = restore_replaced_token(
        credentials,
        snapshot.configuration.as_ref(),
        current,
        snapshot.replaced_token,
    );
    let current_result = restore_token(credentials, current, snapshot.current_token);
    let configuration_result = restore_setup_configuration(store, snapshot.configuration);
    combine_setup_recovery_results(configuration_result, current_result, replaced_result)
}

#[cfg(all(any(windows, target_os = "linux"), feature = "jira"))]
fn restore_replaced_token(
    credentials: CredentialStore,
    previous: Option<&McpConfiguration>,
    current: &McpConfiguration,
    token: Option<String>,
) -> Result<(), CliError> {
    let Some(previous) = previous else {
        return Ok(());
    };
    if previous.jira.is_none() {
        return Ok(());
    }
    if setup_coordinates_match(previous, current)? {
        return Ok(());
    }
    restore_token(credentials, previous, token)
}

#[cfg(all(
    any(windows, target_os = "linux"),
    any(feature = "jira", feature = "bitbucket")
))]
fn restore_setup_configuration(
    store: &ConfigurationStore,
    previous: Option<McpConfiguration>,
) -> Result<(), CliError> {
    match previous {
        Some(configuration) => store.save(&configuration),
        None => store.clear(),
    }
    .map_err(CliError::from)
}

#[cfg(all(
    any(windows, target_os = "linux"),
    any(feature = "jira", feature = "bitbucket")
))]
fn combine_setup_recovery_results(
    first: Result<(), CliError>,
    second: Result<(), CliError>,
    third: Result<(), CliError>,
) -> Result<(), CliError> {
    let errors = [first, second, third]
        .into_iter()
        .filter_map(Result::err)
        .map(|error| error.to_string())
        .collect::<Vec<_>>();
    if errors.is_empty() {
        return Ok(());
    }
    Err(message(errors.join("; ")))
}

#[cfg(all(
    any(windows, target_os = "linux"),
    any(feature = "jira", feature = "bitbucket")
))]
fn error_with_setup_rollback(error: CliError, rollback: Result<(), CliError>) -> CliError {
    match rollback {
        Ok(()) => error,
        Err(rollback_error) => message(format!(
            "{error}; {}: {rollback_error}",
            tui_copy().rollback_failed
        )),
    }
}

fn status_document(
    store: &ConfigurationStore,
    configuration: Option<&McpConfiguration>,
) -> StatusDocument {
    let enabled_tools =
        configuration.map_or_else(Vec::new, WorkloggerMcpServer::configured_tool_names);
    StatusDocument {
        configured: configuration.is_some(),
        configuration_path: store.path().display().to_string(),
        enabled_modules: enabled_modules(configuration),
        enabled_tools,
        credential_available: configuration.is_some_and(has_credential),
        clients: client_status_documents(),
    }
}

fn client_status_documents() -> Vec<ClientStatusDocument> {
    let Ok(registration) = ClientRegistrationService::for_current_user() else {
        return Vec::new();
    };
    let Ok(server) = installed_server_path() else {
        return Vec::new();
    };
    registration
        .statuses(&server)
        .iter()
        .map(client_status_document)
        .collect()
}

fn client_status_document(status: &McpClientStatus) -> ClientStatusDocument {
    ClientStatusDocument {
        name: status.client.display_name().to_owned(),
        state: status.state,
        target: status.target.display().to_string(),
        detail: status.detail.clone(),
    }
}

fn enabled_modules(configuration: Option<&McpConfiguration>) -> Vec<String> {
    configuration
        .into_iter()
        .flat_map(|configuration| configuration.modules.iter())
        .filter(|(_, module)| module.enabled)
        .map(|(module, _)| format!("{module:?}").to_ascii_lowercase())
        .collect()
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn has_credential(configuration: &McpConfiguration) -> bool {
    #[cfg(feature = "jira")]
    let jira_available = !configuration.module_enabled(ModuleId::Jira)
        || configuration
            .jira
            .as_ref()
            .is_some_and(|jira| load_token(jira).is_ok());
    #[cfg(not(feature = "jira"))]
    let jira_available = true;
    #[cfg(feature = "bitbucket")]
    let bitbucket_available = !configuration.module_enabled(ModuleId::Bitbucket)
        || configuration
            .bitbucket
            .as_ref()
            .is_some_and(|value| load_bitbucket_token(value).is_ok());
    #[cfg(not(feature = "bitbucket"))]
    let bitbucket_available = true;
    jira_available && bitbucket_available
}

#[cfg(not(any(feature = "jira", feature = "bitbucket")))]
const fn has_credential(_configuration: &McpConfiguration) -> bool {
    true
}

#[cfg(all(
    any(windows, target_os = "linux"),
    any(feature = "jira", feature = "bitbucket")
))]
struct CredentialBackup {
    site: String,
    email: String,
    token: Option<String>,
}

#[cfg(all(
    any(windows, target_os = "linux"),
    any(feature = "jira", feature = "bitbucket")
))]
struct UninstallSnapshot {
    configuration: Option<McpConfiguration>,
    credentials: Vec<CredentialBackup>,
}

#[cfg(all(
    any(windows, target_os = "linux"),
    any(feature = "jira", feature = "bitbucket")
))]
fn uninstall_snapshot(store: &ConfigurationStore) -> Result<UninstallSnapshot, CliError> {
    let configuration = store.load()?;
    let credentials = configured_credential_backups(configuration.as_ref())?;
    Ok(UninstallSnapshot {
        configuration,
        credentials,
    })
}

#[cfg(all(
    any(windows, target_os = "linux"),
    any(feature = "jira", feature = "bitbucket")
))]
fn configured_credential_backups(
    configuration: Option<&McpConfiguration>,
) -> Result<Vec<CredentialBackup>, CliError> {
    let Some(configuration) = configuration else {
        return Ok(Vec::new());
    };
    let store = CredentialStore::for_purpose(CredentialPurpose::Mcp)
        .map_err(|error| message(error.to_string()))?;
    let mut backups = Vec::new();
    #[cfg(feature = "jira")]
    append_jira_backup(store, configuration, &mut backups)?;
    #[cfg(feature = "bitbucket")]
    append_bitbucket_backup(store, configuration, &mut backups)?;
    Ok(backups)
}

#[cfg(all(
    any(windows, target_os = "linux"),
    any(feature = "jira", feature = "bitbucket")
))]
fn credential_backup(
    store: CredentialStore,
    site: &str,
    email: &str,
) -> Result<CredentialBackup, CliError> {
    let token = store
        .load_api_token(site, email)
        .map_err(|error| message(error.to_string()))?;
    Ok(CredentialBackup {
        site: site.to_owned(),
        email: email.to_owned(),
        token,
    })
}

#[cfg(all(any(windows, target_os = "linux"), feature = "jira"))]
fn append_jira_backup(
    store: CredentialStore,
    configuration: &McpConfiguration,
    backups: &mut Vec<CredentialBackup>,
) -> Result<(), CliError> {
    let Some(jira) = configuration.jira.as_ref() else {
        return Ok(());
    };
    backups.push(credential_backup(store, &jira.base_url, &jira.email)?);
    Ok(())
}

#[cfg(all(any(windows, target_os = "linux"), feature = "bitbucket"))]
fn append_bitbucket_backup(
    store: CredentialStore,
    configuration: &McpConfiguration,
    backups: &mut Vec<CredentialBackup>,
) -> Result<(), CliError> {
    let Some(bitbucket) = configuration.bitbucket.as_ref() else {
        return Ok(());
    };
    backups.push(credential_backup(
        store,
        BITBUCKET_CLOUD_API_ORIGIN,
        &bitbucket.email,
    )?);
    Ok(())
}

#[cfg(all(
    any(windows, target_os = "linux"),
    any(feature = "jira", feature = "bitbucket")
))]
fn clear_secure_installation(
    store: &ConfigurationStore,
    snapshot: &UninstallSnapshot,
) -> Result<(), CliError> {
    store.clear()?;
    if let Err(error) = delete_credential(snapshot.configuration.as_ref()) {
        let rollback = restore_uninstall_snapshot(store, snapshot);
        return Err(error_with_setup_rollback(error, rollback));
    }
    Ok(())
}

#[cfg(all(
    any(windows, target_os = "linux"),
    any(feature = "jira", feature = "bitbucket")
))]
fn restore_uninstall_snapshot(
    store: &ConfigurationStore,
    snapshot: &UninstallSnapshot,
) -> Result<(), CliError> {
    let credential_result = restore_credential_backups(&snapshot.credentials);
    let configuration_result = restore_setup_configuration(store, snapshot.configuration.clone());
    combine_setup_recovery_results(configuration_result, credential_result, Ok(()))
}

#[cfg(all(
    any(windows, target_os = "linux"),
    any(feature = "jira", feature = "bitbucket")
))]
fn restore_credential_backups(backups: &[CredentialBackup]) -> Result<(), CliError> {
    let store = CredentialStore::for_purpose(CredentialPurpose::Mcp)
        .map_err(|error| message(error.to_string()))?;
    let results = backups
        .iter()
        .map(|backup| restore_credential_backup(store, backup))
        .collect::<Vec<_>>();
    combine_credential_results(results)
}

#[cfg(all(
    any(windows, target_os = "linux"),
    any(feature = "jira", feature = "bitbucket")
))]
fn restore_credential_backup(
    store: CredentialStore,
    backup: &CredentialBackup,
) -> Result<(), CliError> {
    let result = match backup.token.as_deref() {
        Some(token) => store.save_api_token(&backup.site, &backup.email, token),
        None => store.delete_api_token(&backup.site, &backup.email),
    };
    result.map_err(|error| message(error.to_string()))
}

#[cfg(all(
    any(windows, target_os = "linux"),
    any(feature = "jira", feature = "bitbucket")
))]
fn combine_credential_results(results: Vec<Result<(), CliError>>) -> Result<(), CliError> {
    let errors = results
        .into_iter()
        .filter_map(Result::err)
        .map(|error| error.to_string())
        .collect::<Vec<_>>();
    if errors.is_empty() {
        return Ok(());
    }
    Err(message(errors.join("; ")))
}

#[cfg(all(
    any(windows, target_os = "linux"),
    any(feature = "jira", feature = "bitbucket")
))]
fn delete_credential(configuration: Option<&McpConfiguration>) -> Result<(), CliError> {
    let Some(configuration) = configuration else {
        return Ok(());
    };
    let store = CredentialStore::for_purpose(CredentialPurpose::Mcp)
        .map_err(|error| message(error.to_string()))?;
    if let Some(jira) = configuration.jira.as_ref() {
        store
            .delete_api_token(&jira.base_url, &jira.email)
            .map_err(|error| message(error.to_string()))?;
    }
    #[cfg(feature = "bitbucket")]
    return delete_bitbucket_credential(store, configuration);
    #[cfg(not(feature = "bitbucket"))]
    Ok(())
}

#[cfg(all(any(windows, target_os = "linux"), feature = "bitbucket"))]
fn delete_bitbucket_credential(
    store: CredentialStore,
    configuration: &McpConfiguration,
) -> Result<(), CliError> {
    let Some(bitbucket) = configuration.bitbucket.as_ref() else {
        return Ok(());
    };
    store
        .delete_api_token(BITBUCKET_CLOUD_API_ORIGIN, &bitbucket.email)
        .map_err(|error| message(error.to_string()))
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn configure_clients() -> Result<(), CliError> {
    let registration = ClientRegistrationService::for_current_user()
        .map_err(|error| message(error.to_string()))?;
    let server = install_current_server()?;
    let clients = registration.statuses(&server);
    let candidates = clients
        .iter()
        .filter(|status| setup_candidate(status.state))
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        terminal_notice(tui_copy().no_pending_clients.clone());
        return Ok(());
    }
    let options = candidates
        .iter()
        .map(|status| client_status_option(status))
        .collect::<Vec<_>>();
    let selected = choose_many(
        &tui_copy().detected_clients_title,
        &options,
        vec![false; candidates.len()],
    )
    .map_err(|error| terminal_error(&error))?;
    register_selected_clients(&registration, &server, &candidates, &selected)
}

fn manage_clients() -> Result<(), CliError> {
    let registration = ClientRegistrationService::for_current_user()
        .map_err(|error| message(error.to_string()))?;
    let server = install_current_server()?;
    let clients = actionable_clients(registration.statuses(&server));
    if clients.is_empty() {
        terminal_notice(tui_copy().no_compatible_clients.clone());
        return Ok(());
    }
    apply_selected_client(&registration, &server, &clients)
}

fn actionable_clients(clients: Vec<McpClientStatus>) -> Vec<McpClientStatus> {
    clients
        .into_iter()
        .filter(|status| {
            matches!(
                status.state,
                RegistrationState::Available
                    | RegistrationState::BrokenRegistration
                    | RegistrationState::OwnedOutdatedRegistration
                    | RegistrationState::Registered
                    | RegistrationState::ConflictingRegistration
            )
        })
        .collect()
}

fn apply_selected_client(
    registration: &ClientRegistrationService,
    server: &std::path::Path,
    clients: &[McpClientStatus],
) -> Result<(), CliError> {
    let options = clients.iter().map(client_status_option).collect::<Vec<_>>();
    let selected =
        choose(&tui_copy().clients_title, &options).map_err(|error| terminal_error(&error))?;
    let status = clients
        .get(selected)
        .ok_or_else(|| message(tui_copy().invalid_client_selection.clone()))?;
    apply_client_action(registration, server, status)
}

fn apply_client_action(
    registration: &ClientRegistrationService,
    server: &std::path::Path,
    status: &McpClientStatus,
) -> Result<(), CliError> {
    let action = match status.state {
        RegistrationState::Registered => &tui_copy().remove_action,
        RegistrationState::BrokenRegistration
        | RegistrationState::OwnedOutdatedRegistration
        | RegistrationState::ConflictingRegistration => &tui_copy().update_action,
        _ => &tui_copy().install_action,
    };
    if !confirm(&format!("{action} {}?", status.client.display_name()))? {
        terminal_notice(tui_copy().no_changes.clone());
        return Ok(());
    }
    change_client_registration(registration, server, status)
}

fn change_client_registration(
    registration: &ClientRegistrationService,
    server: &std::path::Path,
    status: &McpClientStatus,
) -> Result<(), CliError> {
    let result = match status.state {
        RegistrationState::Registered => registration.unregister(status.client, server),
        RegistrationState::Available
        | RegistrationState::BrokenRegistration
        | RegistrationState::OwnedOutdatedRegistration
        | RegistrationState::ConflictingRegistration => {
            registration.register(status.client, server)
        }
        _ => return Err(message(tui_copy().invalid_client_state.clone())),
    };
    result.map_err(|error| message(error.to_string()))?;
    terminal_notice(format!(
        "{} {}.",
        tui_copy().client_updated,
        status.client.display_name()
    ));
    Ok(())
}

fn client_status_option(status: &McpClientStatus) -> String {
    format!(
        "{} · {} · {}",
        status.client.display_name(),
        registration_state_label(status.state),
        status.target.display()
    )
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn register_selected_clients(
    registration: &ClientRegistrationService,
    server: &std::path::Path,
    candidates: &[&McpClientStatus],
    selected: &[bool],
) -> Result<(), CliError> {
    for (status, selected) in candidates.iter().zip(selected) {
        if *selected {
            register_client(registration, server, status)?;
        }
    }
    Ok(())
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn register_client(
    registration: &ClientRegistrationService,
    server: &std::path::Path,
    status: &McpClientStatus,
) -> Result<(), CliError> {
    registration
        .register(status.client, server)
        .map_err(|error| message(error.to_string()))?;
    terminal_notice(format!(
        "{} {}.",
        tui_copy().client_registered,
        status.client.display_name()
    ));
    Ok(())
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
const fn setup_candidate(state: RegistrationState) -> bool {
    matches!(
        state,
        RegistrationState::Available
            | RegistrationState::BrokenRegistration
            | RegistrationState::OwnedOutdatedRegistration
            | RegistrationState::ConflictingRegistration
    )
}

fn registration_state_label(state: RegistrationState) -> &'static str {
    match state {
        RegistrationState::Unavailable => tui_copy().state_unavailable.as_str(),
        RegistrationState::Available => tui_copy().state_available.as_str(),
        RegistrationState::Registered => tui_copy().state_registered.as_str(),
        RegistrationState::BrokenRegistration => tui_copy().state_broken.as_str(),
        RegistrationState::OwnedOutdatedRegistration => tui_copy().state_outdated.as_str(),
        RegistrationState::ConflictingRegistration => tui_copy().state_conflict.as_str(),
        RegistrationState::InvalidConfiguration => tui_copy().state_invalid.as_str(),
    }
}

fn print_registered_clients(clients: &[McpClientStatus]) {
    let registered = clients
        .iter()
        .filter(|status| owned_registration(status.state));
    terminal_notice(tui_copy().removals_title.clone());
    for status in registered {
        terminal_notice(format!(
            "  {} · {}",
            status.client.display_name(),
            status.target.display()
        ));
    }
}

fn unregister_clients(
    registration: &ClientRegistrationService,
    server: &std::path::Path,
    clients: &[McpClientStatus],
) -> Result<(), CliError> {
    for status in clients
        .iter()
        .filter(|status| owned_registration(status.state))
    {
        registration
            .unregister(status.client, server)
            .map_err(|error| message(error.to_string()))?;
    }
    Ok(())
}

const fn owned_registration(state: RegistrationState) -> bool {
    matches!(
        state,
        RegistrationState::Registered
            | RegistrationState::BrokenRegistration
            | RegistrationState::OwnedOutdatedRegistration
    )
}

fn current_executable() -> Result<std::path::PathBuf, CliError> {
    std::env::current_exe().map_err(|error| message(error.to_string()))
}

fn installed_server_path() -> Result<std::path::PathBuf, CliError> {
    McpServerInstallation::for_current_user()
        .map(|installation| installation.executable().to_path_buf())
        .map_err(|error| message(error.to_string()))
}

fn install_current_server() -> Result<std::path::PathBuf, CliError> {
    let source = current_executable()?;
    McpServerInstallation::for_current_user()
        .and_then(|installation| installation.install(&source))
        .map_err(|error| message(error.to_string()))
}

fn prompt(label: &str, default: Option<&str>) -> Result<String, CliError> {
    let input = read_text(label, default, false).map_err(|error| terminal_error(&error))?;
    let value = input.trim();
    match (value.is_empty(), default) {
        (true, Some(value)) => Ok(value.to_owned()),
        (true, None) => Err(message(format!("{label} {}", tui_copy().field_required))),
        (false, _) => Ok(value.to_owned()),
    }
}

#[cfg(feature = "jira")]
fn read_token() -> Result<String, CliError> {
    read_provider_token(JIRA_API_TOKEN_ENVIRONMENT_VARIABLE, environment_token())
}

#[cfg(feature = "bitbucket")]
fn read_bitbucket_setup_token() -> Result<String, CliError> {
    read_provider_token(
        BITBUCKET_API_TOKEN_ENVIRONMENT_VARIABLE,
        bitbucket_environment_token(),
    )
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn read_provider_token(
    environment_variable: &str,
    token: Option<String>,
) -> Result<String, CliError> {
    if let Some(token) = token {
        terminal_notice(format!(
            "{} {environment_variable}",
            tui_copy().token_environment_notice
        ));
        return Ok(token);
    }
    read_text(&tui_copy().token_prompt, None, true).map_err(|error| terminal_error(&error))
}

fn confirm(label: &str) -> Result<bool, CliError> {
    tui_confirm(label, false).map_err(|error| terminal_error(&error))
}

fn confirm_with_default(label: &str, default_yes: bool) -> Result<bool, CliError> {
    tui_confirm(label, default_yes).map_err(|error| terminal_error(&error))
}

fn terminal_notice(message: String) {
    tui_notice(message);
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn print_platform_secret_notice(environment_variable: &str) {
    if cfg!(any(windows, target_os = "linux")) {
        terminal_notice(tui_copy().protected_secret_notice.clone());
        return;
    }
    terminal_notice(format!(
        "{}: {environment_variable}.",
        tui_copy().environment_secret_notice
    ));
}

fn print_help() {
    println!("Worklogger MCP {}", env!("CARGO_PKG_VERSION"));
    println!("{}", tui_copy().help_usage);
}

fn message(value: impl Into<String>) -> CliError {
    CliError::Message(value.into())
}

fn terminal_error(error: &std::io::Error) -> CliError {
    if error.kind() == std::io::ErrorKind::Interrupted {
        return CliError::Cancelled;
    }
    message(error.to_string())
}

#[cfg(all(test, any(feature = "jira", feature = "bitbucket")))]
fn organization_profile_fixture() -> OrganizationProfile {
    OrganizationProfile::from_json(include_str!("../../../config/example.organization.json"))
        .expect("example organization profile is valid")
}

#[cfg(test)]
mod command_tests {
    use super::*;

    #[test]
    fn no_arguments_open_the_interactive_menu() {
        let command = parse_command(std::iter::empty()).expect("empty arguments are valid");

        assert_eq!(command, Command::Menu);
    }

    #[test]
    fn menu_selection_wraps_at_both_ends() {
        assert_eq!(terminal_ui::move_selection(0, -1, 6), 5);
        assert_eq!(terminal_ui::move_selection(5, 1, 6), 0);
    }

    #[test]
    fn terminal_cancellation_is_preserved_for_the_menu() {
        let interrupted = std::io::Error::from(std::io::ErrorKind::Interrupted);
        let error = terminal_error(&interrupted);

        assert!(matches!(error, CliError::Cancelled));
    }

    #[test]
    fn setup_accepts_an_optional_shared_profile() {
        let command = parse_command(
            ["setup", "--profile", "organization.json"]
                .into_iter()
                .map(str::to_owned),
        )
        .expect("setup arguments are valid");

        assert_eq!(
            command,
            Command::Setup {
                profile_path: Some(PathBuf::from("organization.json")),
            }
        );
    }

    #[test]
    fn setup_rejects_a_missing_profile_path() {
        let command = parse_command(["setup", "--profile"].into_iter().map(str::to_owned));

        assert!(command.is_err());
    }

    #[test]
    fn install_requires_explicit_configuration_clients_and_confirmation() {
        let command = parse_command(
            [
                "install",
                "--config",
                "mcp.json",
                "--clients",
                "codex,claude-code",
                "--skills",
                "--yes",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .expect("install arguments are valid");

        assert_eq!(
            command,
            Command::Install(HeadlessInstall {
                configuration_path: PathBuf::from("mcp.json"),
                profile_path: None,
                clients: ClientSelection::Named(vec![McpClientId::Codex, McpClientId::ClaudeCode]),
                install_skills: true,
            })
        );
    }

    #[test]
    fn install_rejects_changes_without_explicit_confirmation() {
        let command = parse_command(
            ["install", "--config", "mcp.json", "--clients", "all"]
                .into_iter()
                .map(str::to_owned),
        );

        assert!(command.is_err());
    }

    #[test]
    fn install_rejects_unknown_or_duplicated_clients() {
        assert!(parse_client_selection("unknown").is_err());
        assert!(parse_client_selection("codex,codex").is_err());
    }

    #[test]
    fn install_accepts_common_and_special_client_adapters() {
        let clients = parse_client_selection("qwen-code,gemini-cli,kiro,github-copilot,trae-code")
            .expect("supported clients are accepted");

        assert_eq!(
            clients,
            ClientSelection::Named(vec![
                McpClientId::QwenCode,
                McpClientId::GeminiCli,
                McpClientId::Kiro,
                McpClientId::GitHubCopilot,
                McpClientId::TraeCode,
            ])
        );
    }

    #[test]
    fn skills_command_is_available() {
        let command = parse_command(["skills"].into_iter().map(str::to_owned))
            .expect("skills command is valid");

        assert_eq!(command, Command::Skills);
    }

    #[cfg(feature = "managed-distribution")]
    #[test]
    fn managed_setup_rejects_a_runtime_profile_replacement() {
        let profile_path = Some(PathBuf::from("replacement.json"));

        assert!(load_setup_profile(profile_path.as_deref()).is_err());
    }
}

#[cfg(all(test, feature = "jira"))]
mod jira_setup_tests {
    use super::*;

    #[test]
    fn jira_setup_preserves_existing_hours_defaults() {
        let expected = JiraHoursConfiguration {
            weekly_target_hours: 30,
            utc_offset_minutes: -180,
            maximum_daily_hours: 24,
            maximum_report_period_days: 7,
            maximum_concurrent_worklog_requests: 4,
        };
        let current = jira_setup_configuration(expected.clone());

        assert_eq!(
            jira_hours_defaults(Some(&current), None).expect("defaults resolve"),
            expected
        );
    }

    #[test]
    fn jira_setup_replaces_defaults_outside_the_organization_profile() {
        let profile = organization_profile_fixture();
        let jira_profile = profile.modules.jira.as_ref().expect("Jira is configured");
        let current = jira_setup_configuration(JiraHoursConfiguration {
            weekly_target_hours: 30,
            utc_offset_minutes: 120,
            maximum_daily_hours: 24,
            maximum_report_period_days: 7,
            maximum_concurrent_worklog_requests: 40,
        });

        let defaults = jira_hours_defaults(Some(&current), Some(jira_profile))
            .expect("profile defaults resolve");

        assert_eq!(defaults.weekly_target_hours, 40);
        assert_eq!(defaults.utc_offset_minutes, 0);
        assert_eq!(defaults.maximum_concurrent_worklog_requests, 8);
    }

    #[test]
    fn jira_setup_parses_explicit_utc_offset() {
        assert_eq!(parse_utc_offset("-03:00").expect("valid offset"), -180);
        assert!(parse_utc_offset("America/Argentina/Buenos_Aires").is_err());
    }

    #[test]
    fn jira_setup_uses_profile_scope_capabilities_and_defaults() {
        let profile = organization_profile_fixture();
        let jira = profile.modules.jira.as_ref().expect("Jira is configured");
        let boards = vec![board(42), board(99)];

        assert_eq!(
            scoped_jira_boards(Some(jira), &jira.sites[0].url, boards)[0].id,
            42
        );
        assert!(capability_available(
            Some(&jira.mcp_capabilities),
            Capability::ReadOwnTimeEntries
        ));
        assert_eq!(
            jira_hours_defaults(None, Some(jira))
                .expect("hours resolve")
                .weekly_target_hours,
            40
        );
    }

    #[test]
    fn jira_setup_does_not_enable_capabilities_outside_the_local_selection() {
        let selected = BTreeSet::from([Capability::ReadJiraIssues]);

        assert!(!capability_available(
            Some(&selected),
            Capability::ReadOwnTimeEntries
        ));
    }

    #[test]
    fn jira_setup_preserves_existing_request_limits() {
        let hours = default_jira_hours();
        let current = jira_setup_configuration_with_custom_limits(hours.clone());
        let jira = current.jira.as_ref().expect("Jira is configured");
        let profile = organization_profile_fixture();
        let jira_profile = profile.modules.jira.as_ref().expect("Jira is configured");
        let limits = selected_jira_setup_limits(Some(jira), Some(jira_profile));

        assert_eq!(limits.timeout_seconds, 12);
        assert_eq!(limits.page_size, 5);
        assert_eq!(limits.maximum_collection_items, 9);
        assert_eq!(
            selected_jira_issue_search_limit(Some(&current), Some(jira_profile)),
            jira.maximum_issue_search_results
        );
    }

    #[test]
    fn jira_first_setup_uses_profile_request_defaults() {
        let profile = organization_profile_fixture();
        let jira = profile.modules.jira.as_ref().expect("Jira is configured");
        let limits = selected_jira_setup_limits(None, Some(jira));

        assert_eq!(limits.timeout_seconds, jira.request_timeout_seconds);
        assert_eq!(limits.page_size, jira.page_size);
        assert_eq!(
            limits.maximum_collection_items,
            jira.maximum_collection_items
        );
        assert_eq!(
            selected_jira_issue_search_limit(None, Some(jira)),
            jira.maximum_issue_search_results
        );
    }

    fn jira_setup_configuration_with_custom_limits(
        hours: JiraHoursConfiguration,
    ) -> McpConfiguration {
        let mut current = jira_setup_configuration(hours);
        let jira = current.jira.as_mut().expect("Jira is configured");
        jira.request_timeout_seconds = 12;
        jira.page_size = 5;
        jira.maximum_collection_items = 9;
        current
    }

    fn jira_setup_configuration(hours: JiraHoursConfiguration) -> McpConfiguration {
        let jira = JiraConfiguration {
            base_url: "https://example.atlassian.net".to_owned(),
            email: "person@example.com".to_owned(),
            board_id: 42,
            request_timeout_seconds: DEFAULT_REQUEST_TIMEOUT_SECONDS,
            page_size: DEFAULT_PAGE_SIZE,
            maximum_collection_items: DEFAULT_MAXIMUM_COLLECTION_ITEMS,
            maximum_issue_search_results: DEFAULT_MAXIMUM_ISSUE_SEARCH_RESULTS,
            hours: Some(hours),
        };
        let module = ModuleConfiguration {
            enabled: true,
            capabilities: BTreeSet::from([Capability::ReadOwnTimeEntries]),
        };
        McpConfiguration::new(jira, BTreeMap::from([(ModuleId::Jira, module)]))
            .expect("fixture configuration is valid")
    }

    fn board(id: u64) -> BoardDto {
        BoardDto {
            id,
            name: format!("Board {id}"),
            board_type: "scrum".to_owned(),
            self_url: format!("https://example.atlassian.net/board/{id}"),
            location: None,
        }
    }
}

#[cfg(all(test, feature = "bitbucket"))]
mod tests {
    use super::*;

    #[test]
    fn bitbucket_setup_preserves_workspaces_for_the_same_account() {
        let current = bitbucket_configuration("person@example.com", "first");

        let workspaces = configured_bitbucket_workspaces(
            Some(&current),
            "PERSON@example.com",
            "second".to_owned(),
            BTreeSet::from(["repository-two".to_owned()]),
            None,
        );

        assert_eq!(workspaces.len(), 2);
        assert!(workspaces.contains_key("first"));
        assert!(workspaces.contains_key("second"));
    }

    #[test]
    fn bitbucket_setup_uses_profile_scope_and_capabilities() {
        let profile = organization_profile_fixture();
        let bitbucket = profile
            .modules
            .bitbucket
            .as_ref()
            .expect("Bitbucket is configured");
        let repositories = vec![repository("example-repository"), repository("outside")];
        let scoped =
            scoped_bitbucket_repositories(Some(bitbucket), "example-workspace", repositories);

        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].slug, "example-repository");
        assert!(capability_available(
            Some(&bitbucket.mcp_capabilities),
            Capability::ReadBitbucketPullRequests
        ));
    }

    #[test]
    fn bitbucket_setup_preserves_existing_request_limits() {
        let mut current = bitbucket_configuration("person@example.com", "first");
        let bitbucket = current.bitbucket.as_mut().expect("Bitbucket is configured");
        bitbucket.request_timeout_seconds = 12;
        bitbucket.page_size = 5;
        bitbucket.maximum_collection_items = 9;
        let profile = organization_profile_fixture();
        let bitbucket_profile = profile
            .modules
            .bitbucket
            .as_ref()
            .expect("Bitbucket is configured");
        let limits = selected_bitbucket_setup_limits(Some(bitbucket), Some(bitbucket_profile));

        assert_eq!(limits.timeout_seconds, 12);
        assert_eq!(limits.page_size, 5);
        assert_eq!(limits.maximum_collection_items, 9);
    }

    #[test]
    fn bitbucket_first_setup_uses_profile_request_defaults() {
        let profile = organization_profile_fixture();
        let bitbucket = profile
            .modules
            .bitbucket
            .as_ref()
            .expect("Bitbucket is configured");
        let limits = selected_bitbucket_setup_limits(None, Some(bitbucket));

        assert_eq!(limits.timeout_seconds, bitbucket.request_timeout_seconds);
        assert_eq!(limits.page_size, bitbucket.page_size);
        assert_eq!(
            limits.maximum_collection_items,
            bitbucket.maximum_collection_items
        );
    }

    #[test]
    fn bitbucket_setup_does_not_mix_accounts() {
        let current = bitbucket_configuration("first@example.com", "first");

        let workspaces = configured_bitbucket_workspaces(
            Some(&current),
            "second@example.com",
            "second".to_owned(),
            BTreeSet::from(["repository-two".to_owned()]),
            None,
        );

        assert_eq!(workspaces.len(), 1);
        assert!(workspaces.contains_key("second"));
    }

    fn bitbucket_configuration(email: &str, workspace: &str) -> McpConfiguration {
        let bitbucket = BitbucketConfiguration {
            email: email.to_owned(),
            workspaces: BTreeMap::from([(
                workspace.to_owned(),
                BTreeSet::from(["repository-one".to_owned()]),
            )]),
            request_timeout_seconds: DEFAULT_REQUEST_TIMEOUT_SECONDS,
            page_size: DEFAULT_PAGE_SIZE,
            maximum_collection_items: DEFAULT_MAXIMUM_COLLECTION_ITEMS,
            pull_request_defaults: BitbucketPullRequestDefaults::default(),
        };
        McpConfiguration::new_bitbucket(bitbucket, BTreeMap::new()).expect("fixture is valid")
    }

    fn repository(slug: &str) -> Repository {
        serde_json::from_value(serde_json::json!({
            "uuid": format!("{{{slug}}}"),
            "name": slug,
            "slug": slug,
            "full_name": format!("example-workspace/{slug}"),
            "links": { "html": { "href": format!("https://example.test/{slug}") } }
        }))
        .expect("repository fixture is valid")
    }
}
