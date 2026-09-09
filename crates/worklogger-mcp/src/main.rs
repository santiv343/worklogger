#[cfg(any(feature = "jira", feature = "bitbucket"))]
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
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
    BITBUCKET_API_TOKEN_ENVIRONMENT_VARIABLE, BitbucketConfiguration, BitbucketPullRequestService,
};
#[cfg(any(feature = "jira", feature = "bitbucket"))]
use worklogger_mcp::{Capability, ModuleConfiguration, ModuleId};
use worklogger_mcp::{
    ClientRegistrationService, ConfigurationStore, McpClientStatus, McpConfiguration,
    McpServerInstallation, RegistrationState, WorkloggerMcpServer,
};
#[cfg(feature = "jira")]
use worklogger_mcp::{
    DEFAULT_MAXIMUM_ISSUE_SEARCH_RESULTS, JIRA_API_TOKEN_ENVIRONMENT_VARIABLE, JiraConfiguration,
    JiraHoursConfiguration, JiraIssueService, JiraOwnHoursBackend, JiraWorklogService,
};
#[cfg(feature = "bitbucket")]
use worklogger_profile::BitbucketModuleProfile;
#[cfg(feature = "jira")]
use worklogger_profile::JiraModuleProfile;
use worklogger_profile::OrganizationProfile;
#[cfg(not(feature = "managed-distribution"))]
use worklogger_profile::OrganizationProfileStore;

mod skill_installation;
mod tui_copy;

use skill_installation::AgentSkillInstaller;
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
    Clients,
    Status,
    Skills,
    Uninstall,
    Help,
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
#[derive(Clone, Copy)]
struct ProviderRequestLimits {
    timeout_seconds: u64,
    page_size: u16,
    maximum_collection_items: usize,
}

#[cfg(all(feature = "jira", feature = "bitbucket"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SetupProvider {
    Jira,
    Bitbucket,
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Configuration(#[from] worklogger_mcp::ConfigurationError),
    #[error("no hay configuración MCP; ejecutá `worklogger-mcp setup`")]
    NotConfigured,
    #[error(
        "no hay un API token disponible en el almacén seguro ni en {JIRA_API_TOKEN_ENVIRONMENT_VARIABLE}"
    )]
    #[cfg(feature = "jira")]
    MissingJiraToken,
    #[cfg(feature = "bitbucket")]
    #[error(
        "no hay un API token de Bitbucket disponible en el almacén seguro ni en {BITBUCKET_API_TOKEN_ENVIRONMENT_VARIABLE}"
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

const MENU_CONFIGURE_SELECTION: &str = "1";
const MENU_CLIENTS_SELECTION: &str = "2";
const MENU_REFRESH_SELECTION: &str = "3";
const MENU_SKILLS_SELECTION: &str = "4";
const MENU_UNINSTALL_SELECTION: &str = "5";
const MENU_EXIT_SELECTION: &str = "0";

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
    match command {
        Command::Menu => menu().await,
        Command::Serve => serve().await,
        Command::Setup { profile_path } => setup(profile_path).await,
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
        "serve" => command_without_arguments(Command::Serve, arguments),
        "clients" => command_without_arguments(Command::Clients, arguments),
        "status" => command_without_arguments(Command::Status, arguments),
        "skills" => command_without_arguments(Command::Skills, arguments),
        "uninstall" => command_without_arguments(Command::Uninstall, arguments),
        "help" | "--help" | "-h" => command_without_arguments(Command::Help, arguments),
        _ => Err(message("comando desconocido; usá --help")),
    }
}

async fn menu() -> Result<(), CliError> {
    loop {
        print_menu()?;
        match choose_menu_action()? {
            MenuAction::Configure => setup(None).await?,
            MenuAction::Clients => manage_clients()?,
            MenuAction::Refresh => {}
            MenuAction::Skills => install_skills()?,
            MenuAction::Uninstall => uninstall()?,
            MenuAction::Exit => return Ok(()),
        }
    }
}

fn print_menu() -> Result<(), CliError> {
    let copy = tui_copy();
    println!("\n{}", copy.menu_title);
    println!("{}", copy.menu_separator);
    print_menu_overview()?;
    print_menu_options();
    Ok(())
}

fn print_menu_overview() -> Result<(), CliError> {
    let store = ConfigurationStore::for_current_user()?;
    let configuration = effective_configuration(store.load()?)?;
    let document = status_document(&store, configuration.as_ref());
    let server = installed_server_path()?;
    print_menu_configuration(&document, &server);
    print_menu_clients(&document.clients);
    Ok(())
}

fn print_menu_configuration(document: &StatusDocument, server: &Path) {
    let copy = tui_copy();
    let state = if document.configured {
        &copy.state_configured
    } else {
        &copy.state_not_configured
    };
    println!("{}: {state}", copy.configuration_label);
    println!("{}: {}", copy.modules_label, menu_modules(document));
    println!("{}: {}", copy.server_label, server.display());
    println!("{}: {}", copy.server_state_label, yes_no(server.exists()));
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

fn print_menu_clients(clients: &[ClientStatusDocument]) {
    println!("{}:", tui_copy().clients_label);
    for client in clients {
        println!(
            "  • {} · {} · {}",
            client.name,
            registration_state_label(client.state),
            client.target
        );
    }
}

fn print_menu_options() {
    let copy = tui_copy();
    println!("\n{}", copy.menu_actions_title);
    println!(
        "  {MENU_CONFIGURE_SELECTION}) {}",
        copy.menu_configure_action
    );
    println!("  {MENU_CLIENTS_SELECTION}) {}", copy.menu_clients_action);
    println!("  {MENU_REFRESH_SELECTION}) {}", copy.menu_refresh_action);
    println!("  {MENU_SKILLS_SELECTION}) {}", copy.menu_skills_action);
    println!(
        "  {MENU_UNINSTALL_SELECTION}) {}",
        copy.menu_uninstall_action
    );
    println!("  {MENU_EXIT_SELECTION}) {}", copy.menu_exit_action);
}

fn choose_menu_action() -> Result<MenuAction, CliError> {
    let selection = prompt(&tui_copy().choose_number, Some(MENU_EXIT_SELECTION))?;
    match selection.as_str() {
        MENU_CONFIGURE_SELECTION => Ok(MenuAction::Configure),
        MENU_CLIENTS_SELECTION => Ok(MenuAction::Clients),
        MENU_REFRESH_SELECTION => Ok(MenuAction::Refresh),
        MENU_SKILLS_SELECTION => Ok(MenuAction::Skills),
        MENU_UNINSTALL_SELECTION => Ok(MenuAction::Uninstall),
        MENU_EXIT_SELECTION => Ok(MenuAction::Exit),
        _ => Err(message(&tui_copy().invalid_menu_selection)),
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
    if !confirm(&tui_copy().install_skills_confirmation)? {
        println!("{}", tui_copy().no_changes);
        return Ok(());
    }
    let destinations = AgentSkillInstaller::for_current_user()
        .and_then(|installer| installer.install())
        .map_err(|error| message(error.to_string()))?;
    println!(
        "{}: {}.",
        tui_copy().skills_installed,
        destinations.join(", ")
    );
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
    let server = server.with_jira_issues(Arc::new(issues));
    let server = with_jira_worklog_backend(server, configuration, jira, token.clone())?;
    with_jira_hours_backend(server, configuration, jira, token)
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
    print_setup_header();
    let profile_path = profile_path.as_deref();
    let profile = load_setup_profile(profile_path)?;
    run_setup_provider(profile.as_ref()).await?;
    #[cfg(not(feature = "managed-distribution"))]
    install_selected_profile(profile_path, profile.as_ref())?;
    #[cfg(any(feature = "jira", feature = "bitbucket"))]
    configure_clients()?;
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
    let (identity, boards) = discover(&site, &email, &token, limits).await?;
    println!("\n{}: {identity}", copy.account_verified);
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
    println!("\n{}: {tools}", copy.setup_saved);
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
    let (identity, repositories) = discover_bitbucket(&email, &token, &workspace, limits).await?;
    println!("\n{}: {identity}", copy.account_verified);
    let repositories = scoped_bitbucket_repositories(bitbucket_profile, &workspace, repositories);
    let repositories = select_bitbucket_repositories(bitbucket_profile, &repositories)?;
    let capabilities = bitbucket_setup_capabilities(bitbucket_profile)?;
    complete_bitbucket_setup(
        email,
        workspace,
        &token,
        repositories,
        capabilities,
        limits,
        profile,
    )
}

#[cfg(feature = "bitbucket")]
fn complete_bitbucket_setup(
    email: String,
    workspace: String,
    token: &str,
    repositories: BTreeSet<String>,
    capabilities: BTreeSet<Capability>,
    limits: ProviderRequestLimits,
    profile: Option<&OrganizationProfile>,
) -> Result<(), CliError> {
    let copy = tui_copy();
    let configuration = persist_bitbucket_setup(
        email,
        workspace,
        repositories,
        capabilities,
        limits,
        token,
        profile,
    )?;
    let tools = WorkloggerMcpServer::configured_tool_names(&configuration).join(", ");
    println!("\n{}: {tools}", copy.setup_saved);
    print_platform_secret_notice(BITBUCKET_API_TOKEN_ENVIRONMENT_VARIABLE);
    Ok(())
}

#[cfg(feature = "bitbucket")]
fn persist_bitbucket_setup(
    email: String,
    workspace: String,
    repositories: BTreeSet<String>,
    capabilities: BTreeSet<Capability>,
    limits: ProviderRequestLimits,
    token: &str,
    profile: Option<&OrganizationProfile>,
) -> Result<McpConfiguration, CliError> {
    let _transaction_guard = credential_transaction()?;
    let current = ConfigurationStore::for_current_user()?.load()?;
    let configuration = configured_bitbucket(
        email,
        workspace,
        repositories,
        capabilities,
        limits,
        current.as_ref(),
        profile.and_then(|value| value.modules.bitbucket.as_ref()),
    )?;
    let configuration = apply_setup_profile(configuration, profile)?;
    save_bitbucket_setup(&configuration, token)?;
    Ok(configuration)
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
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
    println!("\n{}", copy.provider_title);
    println!("  1) {}", copy.provider_jira);
    println!("  2) {}", copy.provider_bitbucket);
    match prompt(&copy.choose_number, Some(&copy.default_selection))?.as_str() {
        "1" => Ok(SetupProvider::Jira),
        "2" => Ok(SetupProvider::Bitbucket),
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
        println!("{}", tui_copy().no_changes);
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
    println!("{}", tui_copy().uninstall_complete);
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
        .ok_or_else(|| message("falta la conexión Jira habilitada"))
}

#[cfg(all(any(windows, target_os = "linux"), feature = "bitbucket"))]
fn required_bitbucket(
    configuration: &McpConfiguration,
) -> Result<&BitbucketConfiguration, CliError> {
    configuration
        .bitbucket
        .as_ref()
        .ok_or_else(|| message("falta la conexión Bitbucket habilitada"))
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
    println!("\n{}", tui_copy().profile_sites_title);
    for (index, site) in sites.iter().enumerate() {
        println!("  {}) {} ({})", index + 1, site.name, site.url);
    }
    let index = choose_index(sites.len())?;
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
    println!("\n{}", tui_copy().profile_workspaces_title);
    for (index, workspace) in workspaces.keys().enumerate() {
        println!("  {}) {workspace}", index + 1);
    }
    let index = choose_index(workspaces.len())?;
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

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn choose_index(item_count: usize) -> Result<usize, CliError> {
    let selected = prompt(
        &tui_copy().choose_number,
        Some(&tui_copy().default_selection),
    )?
    .parse::<usize>()
    .map_err(|_| message(&tui_copy().selection_not_number))?;
    selected
        .checked_sub(1)
        .filter(|index| *index < item_count)
        .ok_or_else(|| message(&tui_copy().invalid_provider_selection))
}

#[cfg(feature = "jira")]
fn choose_board(boards: &[BoardDto]) -> Result<u64, CliError> {
    if boards.is_empty() {
        return Err(message(&tui_copy().no_boards));
    }
    println!("\n{}", tui_copy().boards_title);
    for (index, board) in boards.iter().enumerate() {
        println!("  {}) {} ({})", index + 1, board.name, board.board_type);
    }
    let selected = prompt(
        &tui_copy().choose_number,
        Some(&tui_copy().default_selection),
    )?
    .parse::<usize>()
    .map_err(|_| message(&tui_copy().selection_not_number))?;
    boards
        .get(selected.saturating_sub(1))
        .map(|board| board.id)
        .ok_or_else(|| message(&tui_copy().invalid_board_selection))
}

#[cfg(feature = "bitbucket")]
fn choose_repositories(repositories: &[Repository]) -> Result<BTreeSet<String>, CliError> {
    if repositories.is_empty() {
        return Err(message(&tui_copy().no_repositories));
    }
    println!("\n{}", tui_copy().repositories_title);
    for (index, repository) in repositories.iter().enumerate() {
        println!("  {}) {} ({})", index + 1, repository.name, repository.slug);
    }
    let selection = prompt(
        &tui_copy().choose_repositories,
        Some(&tui_copy().default_selection),
    )?;
    selected_repository_slugs(repositories, &selection)
}

#[cfg(feature = "bitbucket")]
fn selected_repository_slugs(
    repositories: &[Repository],
    selection: &str,
) -> Result<BTreeSet<String>, CliError> {
    selection
        .split(',')
        .map(str::trim)
        .map(|value| selected_repository_slug(repositories, value))
        .collect()
}

#[cfg(feature = "bitbucket")]
fn selected_repository_slug(
    repositories: &[Repository],
    selection: &str,
) -> Result<String, CliError> {
    let index = selection
        .parse::<usize>()
        .map_err(|_| message(&tui_copy().selection_not_number))?;
    repositories
        .get(index.saturating_sub(1))
        .map(|repository| repository.slug.clone())
        .ok_or_else(|| message(&tui_copy().invalid_repository_selection))
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
    email: String,
    workspace: String,
    repositories: BTreeSet<String>,
    capabilities: BTreeSet<Capability>,
    limits: ProviderRequestLimits,
    current: Option<&McpConfiguration>,
    profile: Option<&BitbucketModuleProfile>,
) -> Result<McpConfiguration, CliError> {
    let workspaces =
        configured_bitbucket_workspaces(current, &email, workspace, repositories, profile);
    let bitbucket = BitbucketConfiguration {
        email,
        workspaces,
        request_timeout_seconds: limits.timeout_seconds,
        page_size: limits.page_size,
        maximum_collection_items: limits.maximum_collection_items,
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
    Ok(hours)
}

#[cfg(feature = "jira")]
fn profile_jira_hours(profile: &JiraModuleProfile) -> Result<JiraHoursConfiguration, CliError> {
    let weekly_target_hours = u16::try_from(profile.hours.suggested_weekly_target_hours)
        .map_err(|error| message(error.to_string()))?;
    Ok(JiraHoursConfiguration {
        weekly_target_hours,
        utc_offset_minutes: profile.hours.suggested_utc_offset_minutes,
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

#[cfg(feature = "jira")]
fn choose_jira_capabilities(
    allowed: Option<&BTreeSet<Capability>>,
) -> Result<BTreeSet<Capability>, CliError> {
    let copy = tui_copy();
    let mut capabilities = BTreeSet::new();
    choose_jira_read_capabilities(copy, allowed, &mut capabilities)?;
    choose_jira_write_capabilities(copy, allowed, &mut capabilities)?;
    Ok(capabilities)
}

#[cfg(feature = "bitbucket")]
fn choose_bitbucket_capabilities(
    allowed: Option<&BTreeSet<Capability>>,
) -> Result<BTreeSet<Capability>, CliError> {
    let copy = tui_copy();
    let mut capabilities = BTreeSet::new();
    choose_bitbucket_content_capabilities(copy, allowed, &mut capabilities)?;
    choose_bitbucket_review_capabilities(copy, allowed, &mut capabilities)?;
    Ok(capabilities)
}

#[cfg(feature = "jira")]
fn choose_jira_read_capabilities(
    copy: &tui_copy::TuiCopy,
    allowed: Option<&BTreeSet<Capability>>,
    capabilities: &mut BTreeSet<Capability>,
) -> Result<(), CliError> {
    choose_and_record(
        capabilities,
        &copy.enable_hours,
        Capability::ReadOwnTimeEntries,
        allowed,
        true,
    )?;
    choose_and_record(
        capabilities,
        &copy.enable_issue_read,
        Capability::ReadJiraIssues,
        allowed,
        true,
    )
}

#[cfg(feature = "jira")]
fn choose_jira_write_capabilities(
    copy: &tui_copy::TuiCopy,
    allowed: Option<&BTreeSet<Capability>>,
    capabilities: &mut BTreeSet<Capability>,
) -> Result<(), CliError> {
    choose_hours_write_capability(copy, allowed, capabilities)?;
    choose_jira_issue_write_capabilities(copy, allowed, capabilities)
}

#[cfg(feature = "jira")]
fn choose_hours_write_capability(
    copy: &tui_copy::TuiCopy,
    allowed: Option<&BTreeSet<Capability>>,
    capabilities: &mut BTreeSet<Capability>,
) -> Result<(), CliError> {
    if capabilities.contains(&Capability::ReadOwnTimeEntries) {
        return choose_and_record(
            capabilities,
            &copy.enable_hours_write,
            Capability::WriteOwnTimeEntries,
            allowed,
            false,
        );
    }
    Ok(())
}

#[cfg(feature = "jira")]
fn choose_jira_issue_write_capabilities(
    copy: &tui_copy::TuiCopy,
    allowed: Option<&BTreeSet<Capability>>,
    capabilities: &mut BTreeSet<Capability>,
) -> Result<(), CliError> {
    let options = [
        (&copy.enable_issue_edit, Capability::EditJiraIssues),
        (&copy.enable_issue_comment, Capability::CommentJiraIssues),
        (
            &copy.enable_issue_transition,
            Capability::TransitionJiraIssues,
        ),
    ];
    for (label, capability) in options {
        choose_and_record(capabilities, label, capability, allowed, false)?;
    }
    Ok(())
}

#[cfg(feature = "bitbucket")]
fn choose_bitbucket_content_capabilities(
    copy: &tui_copy::TuiCopy,
    allowed: Option<&BTreeSet<Capability>>,
    capabilities: &mut BTreeSet<Capability>,
) -> Result<(), CliError> {
    choose_bitbucket_read_capability(copy, allowed, capabilities)?;
    choose_bitbucket_change_capabilities(copy, allowed, capabilities)
}

#[cfg(feature = "bitbucket")]
fn choose_bitbucket_read_capability(
    copy: &tui_copy::TuiCopy,
    allowed: Option<&BTreeSet<Capability>>,
    capabilities: &mut BTreeSet<Capability>,
) -> Result<(), CliError> {
    choose_and_record(
        capabilities,
        &copy.enable_bitbucket_read,
        Capability::ReadBitbucketPullRequests,
        allowed,
        true,
    )
}

#[cfg(feature = "bitbucket")]
fn choose_bitbucket_change_capabilities(
    copy: &tui_copy::TuiCopy,
    allowed: Option<&BTreeSet<Capability>>,
    capabilities: &mut BTreeSet<Capability>,
) -> Result<(), CliError> {
    choose_and_record(
        capabilities,
        &copy.enable_bitbucket_create,
        Capability::CreateBitbucketPullRequests,
        allowed,
        false,
    )?;
    choose_and_record(
        capabilities,
        &copy.enable_bitbucket_edit,
        Capability::EditBitbucketPullRequests,
        allowed,
        false,
    )?;
    choose_and_record(
        capabilities,
        &copy.enable_bitbucket_comment,
        Capability::CommentBitbucketPullRequests,
        allowed,
        false,
    )
}

#[cfg(feature = "bitbucket")]
fn choose_bitbucket_review_capabilities(
    copy: &tui_copy::TuiCopy,
    allowed: Option<&BTreeSet<Capability>>,
    capabilities: &mut BTreeSet<Capability>,
) -> Result<(), CliError> {
    choose_and_record(
        capabilities,
        &copy.enable_bitbucket_review,
        Capability::ReviewBitbucketPullRequests,
        allowed,
        false,
    )?;
    choose_and_record(
        capabilities,
        &copy.enable_bitbucket_merge,
        Capability::MergeBitbucketPullRequests,
        allowed,
        false,
    )?;
    choose_and_record(
        capabilities,
        &copy.enable_bitbucket_decline,
        Capability::DeclineBitbucketPullRequests,
        allowed,
        false,
    )
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn choose_and_record(
    capabilities: &mut BTreeSet<Capability>,
    label: &str,
    capability: Capability,
    allowed: Option<&BTreeSet<Capability>>,
    enabled_by_default: bool,
) -> Result<(), CliError> {
    let enabled = choose_capability(label, capability, allowed, enabled_by_default)?;
    record_capability(capabilities, capability, enabled);
    Ok(())
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn choose_capability(
    label: &str,
    capability: Capability,
    allowed: Option<&BTreeSet<Capability>>,
    enabled_by_default: bool,
) -> Result<bool, CliError> {
    if !capability_available(allowed, capability) {
        return Ok(false);
    }
    if enabled_by_default {
        return confirm_yes(label);
    }
    confirm(label)
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn capability_available(allowed: Option<&BTreeSet<Capability>>, capability: Capability) -> bool {
    allowed.is_none_or(|capabilities| capabilities.contains(&capability))
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn record_capability(
    capabilities: &mut BTreeSet<Capability>,
    capability: Capability,
    enabled: bool,
) {
    if enabled {
        capabilities.insert(capability);
    }
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
    let selectable = print_client_options(&clients);
    if selectable == 0 {
        println!("\n{}", tui_copy().no_pending_clients);
        return Ok(());
    }
    register_selected_clients(&registration, &server, &clients)
}

fn manage_clients() -> Result<(), CliError> {
    let registration = ClientRegistrationService::for_current_user()
        .map_err(|error| message(error.to_string()))?;
    let server = install_current_server()?;
    let clients = actionable_clients(registration.statuses(&server));
    if clients.is_empty() {
        println!("{}", tui_copy().no_compatible_clients);
        return Ok(());
    }
    print_numbered_clients(&clients);
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

fn print_numbered_clients(clients: &[McpClientStatus]) {
    println!("{}", tui_copy().clients_title);
    for (index, status) in clients.iter().enumerate() {
        println!(
            "  {}) {} · {} · {}",
            index + 1,
            status.client.display_name(),
            registration_state_label(status.state),
            status.target.display()
        );
    }
}

fn apply_selected_client(
    registration: &ClientRegistrationService,
    server: &std::path::Path,
    clients: &[McpClientStatus],
) -> Result<(), CliError> {
    let selected = prompt(
        &tui_copy().choose_number,
        Some(&tui_copy().default_selection),
    )?
    .parse::<usize>()
    .map_err(|_| message(tui_copy().selection_not_number.clone()))?;
    let status = clients
        .get(selected.saturating_sub(1))
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
    if !confirm(&format!("¿{action} {}?", status.client.display_name()))? {
        println!("{}", tui_copy().no_changes);
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
    println!(
        "{} {}.",
        tui_copy().client_updated,
        status.client.display_name()
    );
    Ok(())
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn print_client_options(clients: &[McpClientStatus]) -> usize {
    println!("\n{}", tui_copy().detected_clients_title);
    for status in clients
        .iter()
        .filter(|status| status.state != RegistrationState::Unavailable)
    {
        print_client_status(status);
    }
    clients
        .iter()
        .filter(|status| setup_candidate(status.state))
        .count()
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn register_selected_clients(
    registration: &ClientRegistrationService,
    server: &std::path::Path,
    clients: &[McpClientStatus],
) -> Result<(), CliError> {
    for status in clients
        .iter()
        .filter(|status| setup_candidate(status.state))
    {
        register_client_if_confirmed(registration, server, status)?;
    }
    Ok(())
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn register_client_if_confirmed(
    registration: &ClientRegistrationService,
    server: &std::path::Path,
    status: &McpClientStatus,
) -> Result<(), CliError> {
    let action = setup_action(status.state);
    let label = format!("¿{action} {}?", status.client.display_name());
    if !confirm(&label)? {
        return Ok(());
    }
    registration
        .register(status.client, server)
        .map_err(|error| message(error.to_string()))?;
    println!(
        "{} {}.",
        tui_copy().client_registered,
        status.client.display_name()
    );
    Ok(())
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn print_client_status(status: &McpClientStatus) {
    println!(
        "  {} · {} · {}",
        status.client.display_name(),
        registration_state_label(status.state),
        status.target.display()
    );
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn setup_action(state: RegistrationState) -> &'static str {
    if state == RegistrationState::Available {
        return &tui_copy().register_action;
    }
    &tui_copy().update_action
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
    println!("\n{}", tui_copy().removals_title);
    for status in registered {
        println!(
            "  {} · {}",
            status.client.display_name(),
            status.target.display()
        );
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
    let suffix = default.map_or_else(String::new, |value| format!(" [{value}]"));
    print!("{label}{suffix}: ");
    flush_output()?;
    let input = read_line()?;
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
        println!(
            "{} {environment_variable}",
            tui_copy().token_environment_notice
        );
        return Ok(token);
    }
    rpassword::prompt_password(&tui_copy().token_prompt).map_err(|error| message(error.to_string()))
}

fn confirm(label: &str) -> Result<bool, CliError> {
    print!("{label} {}", tui_copy().confirmation_suffix);
    flush_output()?;
    let response = read_line()?;
    Ok(matches!(
        response.trim().to_ascii_lowercase().as_str(),
        "s" | "si" | "sí"
    ))
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn confirm_yes(label: &str) -> Result<bool, CliError> {
    print!("{label} {}", tui_copy().confirmation_yes_suffix);
    flush_output()?;
    let response = read_line()?;
    let value = response.trim().to_ascii_lowercase();
    Ok(value.is_empty() || matches!(value.as_str(), "s" | "si" | "sí"))
}

fn flush_output() -> Result<(), CliError> {
    io::stdout()
        .flush()
        .map_err(|error| message(error.to_string()))
}

fn read_line() -> Result<String, CliError> {
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| message(error.to_string()))?;
    Ok(input)
}

fn print_setup_header() {
    println!("{}", tui_copy().setup_title);
    println!("{}", tui_copy().setup_secret_notice);
}

#[cfg(any(feature = "jira", feature = "bitbucket"))]
fn print_platform_secret_notice(environment_variable: &str) {
    if cfg!(any(windows, target_os = "linux")) {
        println!("{}", tui_copy().protected_secret_notice);
        return;
    }
    println!(
        "{}: {environment_variable}.",
        tui_copy().environment_secret_notice
    );
}

fn print_help() {
    println!("Worklogger MCP {}", env!("CARGO_PKG_VERSION"));
    println!("{}", tui_copy().help_usage);
}

fn message(value: impl Into<String>) -> CliError {
    CliError::Message(value.into())
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
