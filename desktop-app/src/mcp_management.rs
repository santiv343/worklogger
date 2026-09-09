#[cfg(feature = "mcp-management")]
use std::collections::{BTreeMap, BTreeSet};
#[cfg(feature = "mcp-management")]
use std::path::PathBuf;

#[cfg(all(feature = "mcp-management", windows))]
use worklogger_credentials::{
    CredentialPurpose, CredentialStore, CredentialTransactionGuard, api_token_coordinates_match,
};
#[cfg(all(not(windows), feature = "mcp-bitbucket"))]
use worklogger_mcp::BITBUCKET_API_TOKEN_ENVIRONMENT_VARIABLE;
#[cfg(all(windows, feature = "mcp-bitbucket"))]
use worklogger_mcp::BITBUCKET_CLOUD_API_ORIGIN;
#[cfg(all(not(windows), feature = "mcp-jira"))]
use worklogger_mcp::JIRA_API_TOKEN_ENVIRONMENT_VARIABLE;
#[cfg(feature = "mcp-management")]
use worklogger_mcp::{
    Capability, ClientRegistrationService, ConfigurationStore, JiraConfiguration,
    JiraHoursConfiguration, MCP_SERVER_SERVE_ARGUMENT, McpClientId, McpClientStatus,
    McpConfiguration, McpServerInstallation, ModuleConfiguration, ModuleId, WorkloggerMcpServer,
};

#[cfg(feature = "mcp-management")]
use crate::connection_model::ConnectionConfiguration;
#[cfg(all(feature = "mcp-management", any(windows, test)))]
use crate::connection_model::ConnectionRequest;
#[cfg(feature = "mcp-management")]
use crate::copy::text;

#[cfg(feature = "mcp-management")]
const WINDOWS_SERVER_EXECUTABLE: &str = "worklogger-mcp.exe";
#[cfg(feature = "mcp-management")]
const UNIX_SERVER_EXECUTABLE: &str = "worklogger-mcp";
#[cfg(feature = "mcp-management")]
const BINARY_ENVIRONMENT_VARIABLE: &str = "WORKLOGGER_MCP_BINARY";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(all(feature = "mcp-management", any(windows, test)))]
enum JiraCredentialAction {
    Delete,
    Preserve,
    Reject,
    Transfer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(all(feature = "mcp-management", any(windows, test)))]
struct JiraCredentialCoordinates<'configuration> {
    site: &'configuration str,
    email: &'configuration str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(feature = "mcp-management")]
pub(crate) struct McpStatus {
    pub enabled_capabilities: BTreeSet<Capability>,
    pub configured_modules: BTreeSet<ModuleId>,
    pub binary_available: bool,
    pub credential_available: bool,
    pub command: String,
    pub enabled_tools: Vec<String>,
    pub clients_loading: bool,
    pub clients: Vec<McpClientStatus>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(not(feature = "mcp-management"))]
pub(crate) struct McpStatus;

#[cfg(feature = "mcp-management")]
pub(crate) fn status() -> McpStatus {
    let mut current = loading_status();
    let executable = installed_server_executable().unwrap_or_else(source_server_executable);
    current.clients = client_statuses(&executable);
    current.clients_loading = false;
    current
}

#[cfg(feature = "mcp-management")]
pub(crate) fn loading_status() -> McpStatus {
    let source = source_server_executable();
    let executable = installed_server_executable().unwrap_or_else(|| source.clone());
    let configuration = effective_saved_configuration();
    let enabled_tools = enabled_tools(configuration.as_ref());
    let enabled_capabilities = configured_capabilities(configuration.as_ref());
    McpStatus {
        enabled_capabilities,
        configured_modules: configured_modules(configuration.as_ref()),
        binary_available: source.exists(),
        credential_available: credentials_available(configuration.as_ref()),
        command: format!("{} {MCP_SERVER_SERVE_ARGUMENT}", executable.display()),
        enabled_tools,
        clients_loading: true,
        clients: Vec::new(),
    }
}

#[cfg(feature = "mcp-management")]
fn effective_saved_configuration() -> Option<McpConfiguration> {
    ConfigurationStore::for_current_user()
        .ok()
        .and_then(|store| store.load().ok().flatten())
        .and_then(|configuration| effective_configuration(configuration).ok())
}

#[cfg(feature = "mcp-management")]
fn effective_configuration(configuration: McpConfiguration) -> Result<McpConfiguration, String> {
    effective_configuration_for_profile(configuration, &crate::defaults::product_defaults())
}

#[cfg(feature = "mcp-management")]
pub(crate) fn register_client(client: McpClientId) -> Result<McpStatus, String> {
    ensure_server_ready()?;
    let executable = install_server()?;
    ClientRegistrationService::for_current_user()
        .and_then(|registration| registration.register(client, &executable))
        .map_err(|error| error.to_string())?;
    Ok(status())
}

#[cfg(feature = "mcp-management")]
pub(crate) fn unregister_client(client: McpClientId) -> Result<McpStatus, String> {
    let executable = installed_server_executable()
        .ok_or_else(|| text("preferences.mcpInstallLocationMissing").to_owned())?;
    ClientRegistrationService::for_current_user()
        .and_then(|registration| registration.unregister(client, &executable))
        .map_err(|error| error.to_string())?;
    Ok(status())
}

#[cfg(feature = "mcp-management")]
fn client_statuses(executable: &std::path::Path) -> Vec<McpClientStatus> {
    ClientRegistrationService::for_current_user()
        .map(|registration| registration.statuses(executable))
        .unwrap_or_default()
}

#[cfg(not(feature = "mcp-management"))]
pub(crate) fn status() -> McpStatus {
    McpStatus
}

#[cfg(not(feature = "mcp-management"))]
pub(crate) fn loading_status() -> McpStatus {
    status()
}

#[cfg(feature = "mcp-management")]
fn ensure_server_ready() -> Result<(), String> {
    let status = loading_status();
    if status.enabled_tools.is_empty() {
        return Err(text("preferences.mcpNoToolsEnabled").to_owned());
    }
    if !status.credential_available {
        return Err(text("preferences.mcpCredentialMissing").to_owned());
    }
    Ok(())
}

#[cfg(all(feature = "mcp-management", windows))]
fn credentials_available(configuration: Option<&McpConfiguration>) -> bool {
    let Some(configuration) = configuration else {
        return false;
    };
    let Ok(store) = CredentialStore::for_purpose(CredentialPurpose::Mcp) else {
        return false;
    };
    jira_credential_available(store, configuration)
        && bitbucket_credential_available(store, configuration)
}

#[cfg(all(feature = "mcp-management", not(windows)))]
fn credentials_available(configuration: Option<&McpConfiguration>) -> bool {
    let Some(configuration) = configuration else {
        return false;
    };
    jira_environment_credential_available(configuration)
        && bitbucket_environment_credential_available(configuration)
}

#[cfg(all(windows, feature = "mcp-jira"))]
fn jira_credential_available(store: CredentialStore, configuration: &McpConfiguration) -> bool {
    !configuration.module_enabled(ModuleId::Jira)
        || configuration.jira.as_ref().is_some_and(|jira| {
            store
                .load_api_token(&jira.base_url, &jira.email)
                .is_ok_and(|token| token.is_some())
        })
}

#[cfg(all(feature = "mcp-management", windows, not(feature = "mcp-jira")))]
const fn jira_credential_available(
    _store: CredentialStore,
    _configuration: &McpConfiguration,
) -> bool {
    true
}

#[cfg(all(windows, feature = "mcp-bitbucket"))]
fn bitbucket_credential_available(
    store: CredentialStore,
    configuration: &McpConfiguration,
) -> bool {
    !configuration.module_enabled(ModuleId::Bitbucket)
        || configuration.bitbucket.as_ref().is_some_and(|bitbucket| {
            store
                .load_api_token(BITBUCKET_CLOUD_API_ORIGIN, &bitbucket.email)
                .is_ok_and(|token| token.is_some())
        })
}

#[cfg(all(feature = "mcp-management", windows, not(feature = "mcp-bitbucket")))]
const fn bitbucket_credential_available(
    _store: CredentialStore,
    _configuration: &McpConfiguration,
) -> bool {
    true
}

#[cfg(all(not(windows), feature = "mcp-jira"))]
fn jira_environment_credential_available(configuration: &McpConfiguration) -> bool {
    !configuration.module_enabled(ModuleId::Jira)
        || environment_credential_available(JIRA_API_TOKEN_ENVIRONMENT_VARIABLE)
}

#[cfg(all(feature = "mcp-management", not(windows), not(feature = "mcp-jira")))]
const fn jira_environment_credential_available(_configuration: &McpConfiguration) -> bool {
    true
}

#[cfg(all(not(windows), feature = "mcp-bitbucket"))]
fn bitbucket_environment_credential_available(configuration: &McpConfiguration) -> bool {
    !configuration.module_enabled(ModuleId::Bitbucket)
        || environment_credential_available(BITBUCKET_API_TOKEN_ENVIRONMENT_VARIABLE)
}

#[cfg(all(
    feature = "mcp-management",
    not(windows),
    not(feature = "mcp-bitbucket")
))]
const fn bitbucket_environment_credential_available(_configuration: &McpConfiguration) -> bool {
    true
}

#[cfg(all(not(windows), any(feature = "mcp-jira", feature = "mcp-bitbucket")))]
fn environment_credential_available(variable: &str) -> bool {
    std::env::var(variable).is_ok_and(|value| !value.trim().is_empty())
}

#[cfg(feature = "mcp-management")]
pub(crate) fn configure(
    application: &ConnectionConfiguration,
    capability: Capability,
    enabled: bool,
) -> Result<McpStatus, String> {
    #[cfg(windows)]
    let _transaction_guard =
        CredentialTransactionGuard::acquire().map_err(|error| error.to_string())?;
    let store = ConfigurationStore::for_current_user().map_err(|error| error.to_string())?;
    let previous = store.load().map_err(|error| error.to_string())?;
    let configuration = mcp_configuration(application, capability, enabled, previous.as_ref())?;
    persist_configuration(application, &store, previous, &configuration)?;
    Ok(loading_status())
}

#[cfg(all(feature = "mcp-management", not(windows)))]
fn persist_configuration(
    _application: &ConnectionConfiguration,
    store: &ConfigurationStore,
    _previous: Option<McpConfiguration>,
    configuration: &McpConfiguration,
) -> Result<(), String> {
    store.save(configuration).map_err(|error| error.to_string())
}

#[cfg(all(feature = "mcp-management", windows))]
fn persist_configuration(
    application: &ConnectionConfiguration,
    store: &ConfigurationStore,
    previous: Option<McpConfiguration>,
    configuration: &McpConfiguration,
) -> Result<(), String> {
    let previous_token = configured_mcp_token(configuration)?;
    store
        .save(configuration)
        .map_err(|error| error.to_string())?;
    apply_configured_credential_or_rollback(
        application,
        configuration,
        store,
        previous,
        previous_token,
    )
}

#[cfg(all(feature = "mcp-management", windows))]
fn apply_configured_credential_or_rollback(
    application: &ConnectionConfiguration,
    configuration: &McpConfiguration,
    store: &ConfigurationStore,
    previous: Option<McpConfiguration>,
    previous_token: Option<String>,
) -> Result<(), String> {
    if let Err(error) = synchronize_credential(application, configuration) {
        let credential_result = restore_configured_mcp_token(configuration, previous_token);
        let configuration_result = restore_configuration(store, previous);
        let rollback_result = combine_recovery_results(credential_result, configuration_result);
        return Err(error_with_rollback(error, rollback_result));
    }
    Ok(())
}

#[cfg(all(feature = "mcp-management", windows))]
fn configured_mcp_token(configuration: &McpConfiguration) -> Result<Option<String>, String> {
    let Some(coordinates) = configured_jira_coordinates(configuration) else {
        return Ok(None);
    };
    CredentialStore::for_purpose(CredentialPurpose::Mcp)
        .and_then(|store| store.load_api_token(coordinates.site, coordinates.email))
        .map_err(|error| error.to_string())
}

#[cfg(all(feature = "mcp-management", windows))]
fn restore_configured_mcp_token(
    configuration: &McpConfiguration,
    token: Option<String>,
) -> Result<(), String> {
    let Some(coordinates) = configured_jira_coordinates(configuration) else {
        return Ok(());
    };
    restore_mcp_token(coordinates.site, coordinates.email, token)
}

#[cfg(all(feature = "mcp-management", windows))]
fn restore_configuration(
    store: &ConfigurationStore,
    previous: Option<McpConfiguration>,
) -> Result<(), String> {
    match previous {
        Some(configuration) => store.save(&configuration),
        None => store.clear(),
    }
    .map_err(|error| error.to_string())
}

#[cfg(feature = "mcp-management")]
fn enabled_tools(configuration: Option<&McpConfiguration>) -> Vec<String> {
    configuration.map_or_else(Vec::new, WorkloggerMcpServer::configured_tool_names)
}

#[cfg(feature = "mcp-management")]
fn mcp_configuration(
    application: &ConnectionConfiguration,
    capability: Capability,
    enabled: bool,
    current: Option<&McpConfiguration>,
) -> Result<McpConfiguration, String> {
    mcp_configuration_for_profile(
        application,
        capability,
        enabled,
        current,
        &crate::defaults::product_defaults(),
    )
}

#[cfg(feature = "mcp-management")]
fn mcp_configuration_for_profile(
    application: &ConnectionConfiguration,
    capability: Capability,
    enabled: bool,
    current: Option<&McpConfiguration>,
    profile: &worklogger_profile::OrganizationProfile,
) -> Result<McpConfiguration, String> {
    if enabled && !profile.allows(capability) {
        return Err(text("preferences.mcpCapabilityBlocked").to_owned());
    }
    let modules = module_configuration(current, capability, enabled);
    let bitbucket = current.and_then(|configuration| configuration.bitbucket.clone());
    let jira = mcp_jira_configuration(application, capability, enabled, current);
    rebuild_configuration(jira, bitbucket, modules)
        .and_then(|configuration| effective_configuration_for_profile(configuration, profile))
}

#[cfg(feature = "mcp-management")]
fn mcp_jira_configuration(
    application: &ConnectionConfiguration,
    capability: Capability,
    enabled: bool,
    current: Option<&McpConfiguration>,
) -> JiraConfiguration {
    let mut jira = current
        .and_then(|configuration| configuration.jira.clone())
        .unwrap_or_else(|| jira_configuration(application));
    if enabled && capability == Capability::ReadOwnTimeEntries && jira.hours.is_none() {
        jira.hours = Some(jira_hours_configuration(application));
    }
    jira
}

#[cfg(feature = "mcp-management")]
fn rebuild_configuration(
    jira: JiraConfiguration,
    bitbucket: Option<worklogger_mcp::BitbucketConfiguration>,
    modules: BTreeMap<ModuleId, ModuleConfiguration>,
) -> Result<McpConfiguration, String> {
    let configuration = match bitbucket {
        Some(bitbucket) => McpConfiguration::new_with_bitbucket(jira, bitbucket, modules),
        None => McpConfiguration::new(jira, modules),
    };
    configuration.map_err(|error| error.to_string())
}

#[cfg(feature = "mcp-management")]
fn effective_configuration_for_profile(
    configuration: McpConfiguration,
    profile: &worklogger_profile::OrganizationProfile,
) -> Result<McpConfiguration, String> {
    configuration
        .apply_organization_profile(profile)
        .map_err(|error| error.to_string())
}

#[cfg(feature = "mcp-management")]
fn jira_configuration(application: &ConnectionConfiguration) -> JiraConfiguration {
    JiraConfiguration {
        base_url: application.jira.site.clone(),
        email: application.jira.email.clone(),
        board_id: application.jira.board_id,
        request_timeout_seconds: application.jira.request_timeout_seconds,
        page_size: application.jira.page_size,
        maximum_collection_items: application.jira.maximum_collection_items,
        maximum_issue_search_results: application.jira.maximum_issue_search_results,
        hours: Some(jira_hours_configuration(application)),
    }
}

#[cfg(feature = "mcp-management")]
fn jira_hours_configuration(application: &ConnectionConfiguration) -> JiraHoursConfiguration {
    JiraHoursConfiguration {
        weekly_target_hours: application.hours.weekly_target_hours,
        utc_offset_minutes: application.hours.utc_offset_minutes,
        maximum_concurrent_worklog_requests: application.jira.maximum_concurrent_worklog_requests,
    }
}

#[cfg(all(test, feature = "mcp-management"))]
fn jira_configuration_from_request(
    request: &ConnectionRequest,
) -> Result<JiraConfiguration, String> {
    let weekly_target_hours =
        u16::try_from(request.weekly_target_hours).map_err(|error| error.to_string())?;
    Ok(JiraConfiguration {
        base_url: request.site.clone(),
        email: request.email.clone(),
        board_id: request.board_id,
        request_timeout_seconds: request.request_timeout_seconds,
        page_size: request.page_size,
        maximum_collection_items: request.maximum_collection_items,
        maximum_issue_search_results: request.maximum_issue_search_results,
        hours: Some(JiraHoursConfiguration {
            weekly_target_hours,
            utc_offset_minutes: request.utc_offset_minutes,
            maximum_concurrent_worklog_requests: request.maximum_concurrent_worklog_requests,
        }),
    })
}

#[cfg(feature = "mcp-management")]
fn module_configuration(
    current: Option<&McpConfiguration>,
    capability: Capability,
    enabled: bool,
) -> BTreeMap<ModuleId, ModuleConfiguration> {
    let mut modules = current.map_or_else(BTreeMap::new, |value| value.modules.clone());
    let module = modules
        .entry(capability.module())
        .or_insert(ModuleConfiguration {
            enabled: false,
            capabilities: BTreeSet::new(),
        });
    module.set_capability(capability, enabled);
    modules
}

#[cfg(feature = "mcp-management")]
fn configured_capabilities(configuration: Option<&McpConfiguration>) -> BTreeSet<Capability> {
    let Some(configuration) = configuration else {
        return BTreeSet::new();
    };
    configuration
        .modules
        .values()
        .filter(|module| module.enabled)
        .flat_map(|module| module.capabilities.iter().copied())
        .filter(|capability| crate::defaults::product_defaults().allows(*capability))
        .collect()
}

#[cfg(feature = "mcp-management")]
fn configured_modules(configuration: Option<&McpConfiguration>) -> BTreeSet<ModuleId> {
    let Some(configuration) = configuration else {
        return BTreeSet::new();
    };
    let mut modules = BTreeSet::new();
    let defaults = crate::defaults::product_defaults();
    if configuration.jira.is_some() && defaults.modules.jira.is_some() {
        modules.insert(ModuleId::Jira);
    }
    if configuration.bitbucket.is_some() && defaults.modules.bitbucket.is_some() {
        modules.insert(ModuleId::Bitbucket);
    }
    modules
}

#[cfg(feature = "mcp-management")]
fn source_server_executable() -> PathBuf {
    if let Some(path) = configured_binary() {
        return path;
    }
    executable_directory().map_or_else(
        || PathBuf::from(server_file_name()),
        |directory| directory.join(server_file_name()),
    )
}

#[cfg(feature = "mcp-management")]
fn installed_server_executable() -> Option<PathBuf> {
    McpServerInstallation::for_current_user()
        .ok()
        .map(|installation| installation.executable().to_path_buf())
}

#[cfg(feature = "mcp-management")]
fn install_server() -> Result<PathBuf, String> {
    let source = source_server_executable();
    McpServerInstallation::for_current_user()
        .and_then(|installation| installation.install(&source))
        .map_err(|error| error.to_string())
}

#[cfg(all(feature = "mcp-management", windows))]
fn synchronize_credential(
    application: &ConnectionConfiguration,
    configuration: &McpConfiguration,
) -> Result<(), String> {
    let Some(jira) = configuration.jira.as_ref() else {
        return Ok(());
    };
    let enabled = configuration.module_enabled(ModuleId::Jira);
    let token_exists = configured_mcp_token(configuration)?.is_some();
    let coordinates_match =
        jira_coordinates_match_if_required(application, jira, enabled && !token_exists)?;
    apply_jira_credential_action(
        jira,
        jira_credential_action(enabled, token_exists, coordinates_match),
    )
}

#[cfg(all(feature = "mcp-management", windows))]
fn jira_coordinates_match_if_required(
    application: &ConnectionConfiguration,
    jira: &JiraConfiguration,
    required: bool,
) -> Result<bool, String> {
    if !required {
        return Ok(false);
    }
    api_token_coordinates_match(
        &application.jira.site,
        &application.jira.email,
        &jira.base_url,
        &jira.email,
    )
    .map_err(|error| error.to_string())
}

#[cfg(all(feature = "mcp-management", windows))]
fn apply_jira_credential_action(
    jira: &JiraConfiguration,
    action: JiraCredentialAction,
) -> Result<(), String> {
    match action {
        JiraCredentialAction::Delete => transfer_credential(&jira.base_url, &jira.email, false),
        JiraCredentialAction::Preserve => Ok(()),
        JiraCredentialAction::Reject => {
            Err(text("preferences.mcpScopeCredentialMissing").to_owned())
        }
        JiraCredentialAction::Transfer => transfer_credential(&jira.base_url, &jira.email, true),
    }
}

#[cfg(all(feature = "mcp-management", any(windows, test)))]
const fn jira_credential_action(
    enabled: bool,
    token_exists: bool,
    coordinates_match: bool,
) -> JiraCredentialAction {
    if !enabled {
        return JiraCredentialAction::Delete;
    }
    if token_exists {
        return JiraCredentialAction::Preserve;
    }
    if coordinates_match {
        return JiraCredentialAction::Transfer;
    }
    JiraCredentialAction::Reject
}

#[cfg(all(feature = "mcp-management", any(windows, test)))]
fn configured_jira_coordinates(
    configuration: &McpConfiguration,
) -> Option<JiraCredentialCoordinates<'_>> {
    configuration
        .jira
        .as_ref()
        .map(|jira| JiraCredentialCoordinates {
            site: &jira.base_url,
            email: &jira.email,
        })
}

#[cfg(all(feature = "mcp-management", windows))]
fn transfer_credential(site: &str, email: &str, enabled: bool) -> Result<(), String> {
    let desktop = CredentialStore::for_purpose(CredentialPurpose::Desktop)
        .map_err(|error| error.to_string())?;
    let mcp =
        CredentialStore::for_purpose(CredentialPurpose::Mcp).map_err(|error| error.to_string())?;
    if !enabled {
        return mcp
            .delete_api_token(site, email)
            .map_err(|error| error.to_string());
    }
    let token = desktop
        .load_api_token(site, email)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| text("preferences.mcpDesktopCredentialMissing").to_owned())?;
    mcp.save_api_token(site, email, &token)
        .map_err(|error| error.to_string())
}

#[cfg(feature = "mcp-management")]
fn configured_binary() -> Option<PathBuf> {
    std::env::var_os(BINARY_ENVIRONMENT_VARIABLE)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(feature = "mcp-management")]
fn executable_directory() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(PathBuf::from))
}

#[cfg(feature = "mcp-management")]
const fn server_file_name() -> &'static str {
    if cfg!(windows) {
        WINDOWS_SERVER_EXECUTABLE
    } else {
        UNIX_SERVER_EXECUTABLE
    }
}

#[cfg(all(test, feature = "mcp-jira"))]
mod tests {
    use super::*;

    #[test]
    fn capability_toggles_preserve_sibling_jira_capabilities() {
        let modules = module_configuration(None, Capability::ReadOwnTimeEntries, true);
        let current = McpConfiguration::new(jira_fixture(), modules).expect("fixture is valid");

        let modules = module_configuration(Some(&current), Capability::ReadJiraIssues, true);
        let updated = McpConfiguration::new(jira_fixture(), modules).expect("update is valid");

        assert!(updated.capability_enabled(Capability::ReadOwnTimeEntries));
        assert!(updated.capability_enabled(Capability::ReadJiraIssues));
    }

    #[test]
    fn capability_toggle_preserves_the_mcp_scope() {
        let application = connection_configuration(10);
        let modules = module_configuration(None, Capability::ReadOwnTimeEntries, true);
        let current =
            McpConfiguration::new(jira_fixture_for_board(20), modules).expect("fixture is valid");

        let updated = mcp_configuration_for_profile(
            &application,
            Capability::ReadJiraIssues,
            true,
            Some(&current),
            &test_profile(),
        )
        .expect("capability update succeeds");

        assert_eq!(updated.jira.expect("Jira remains configured").board_id, 20);
    }

    #[test]
    fn enabling_hours_completes_only_missing_hours_configuration() {
        let application = connection_configuration(10);
        let modules = module_configuration(None, Capability::ReadJiraIssues, true);
        let mut jira = jira_fixture_for_board(20);
        jira.hours = None;
        let current = McpConfiguration::new(jira, modules).expect("fixture is valid");

        let updated = mcp_configuration_for_profile(
            &application,
            Capability::ReadOwnTimeEntries,
            true,
            Some(&current),
            &test_profile(),
        )
        .expect("hours capability update succeeds");
        let jira = updated.jira.expect("Jira remains configured");

        assert_eq!(jira.board_id, 20);
        assert_eq!(
            jira.hours.expect("hours are completed").weekly_target_hours,
            40
        );
    }

    #[test]
    fn disabled_jira_deletes_its_mcp_credential() {
        assert_eq!(
            jira_credential_action(false, true, true),
            JiraCredentialAction::Delete
        );
    }

    #[test]
    fn existing_mcp_credential_is_preserved() {
        assert_eq!(
            jira_credential_action(true, true, false),
            JiraCredentialAction::Preserve
        );
    }

    #[test]
    fn matching_desktop_credential_is_transferred() {
        assert_eq!(
            jira_credential_action(true, false, true),
            JiraCredentialAction::Transfer
        );
    }

    #[test]
    fn mismatched_desktop_credential_is_rejected() {
        assert_eq!(
            jira_credential_action(true, false, false),
            JiraCredentialAction::Reject
        );
    }

    #[test]
    fn rollback_coordinates_follow_the_mcp_scope() {
        let modules = module_configuration(None, Capability::ReadJiraIssues, true);
        let configuration =
            McpConfiguration::new(jira_fixture_for_board(20), modules).expect("fixture is valid");

        let coordinates = configured_jira_coordinates(&configuration).expect("Jira is configured");

        assert_eq!(coordinates.site, "https://example.atlassian.net");
        assert_eq!(coordinates.email, "person@example.com");
    }

    fn jira_fixture() -> JiraConfiguration {
        jira_fixture_for_board(10)
    }

    fn test_profile() -> worklogger_profile::OrganizationProfile {
        worklogger_profile::OrganizationProfile::from_json(include_str!(
            "../resources/defaults.json"
        ))
        .expect("neutral test profile is valid")
    }

    fn jira_fixture_for_board(board_id: u64) -> JiraConfiguration {
        let request = request(
            "https://example.atlassian.net",
            "person@example.com",
            board_id,
        );
        jira_configuration_from_request(&request).expect("fixture is valid")
    }

    fn connection_configuration(board_id: u64) -> ConnectionConfiguration {
        let request = request(
            "https://example.atlassian.net",
            "person@example.com",
            board_id,
        );
        ConnectionConfiguration {
            jira: jira_connection_configuration(&request),
            hours: hours_connection_configuration(&request),
            reports: crate::connection_model::ReportsConfiguration {
                enable_team_reports: request.enable_team_reports,
            },
        }
    }

    fn jira_connection_configuration(
        request: &ConnectionRequest,
    ) -> crate::connection_model::JiraConfiguration {
        crate::connection_model::JiraConfiguration {
            site: request.site.clone(),
            email: request.email.clone(),
            board_id: request.board_id,
            request_timeout_seconds: request.request_timeout_seconds,
            page_size: request.page_size,
            maximum_collection_items: request.maximum_collection_items,
            maximum_issue_search_results: request.maximum_issue_search_results,
            maximum_concurrent_worklog_requests: request.maximum_concurrent_worklog_requests,
        }
    }

    fn hours_connection_configuration(
        request: &ConnectionRequest,
    ) -> crate::connection_model::HoursConfiguration {
        crate::connection_model::HoursConfiguration {
            weekly_target_hours: 40,
            utc_offset_minutes: request.utc_offset_minutes,
            maximum_daily_hours: request.maximum_daily_hours,
            default_worklog_start_hour: request.default_worklog_start_hour,
            default_worklog_start_minute: request.default_worklog_start_minute,
        }
    }

    fn request(site: &str, email: &str, board_id: u64) -> ConnectionRequest {
        ConnectionRequest {
            site: site.to_owned(),
            email: email.to_owned(),
            token: "fixture-token".to_owned(),
            board_id,
            weekly_target_hours: 40,
            utc_offset_minutes: 0,
            request_timeout_seconds: 30,
            page_size: 100,
            maximum_collection_items: 2_000,
            maximum_issue_search_results: 20,
            maximum_concurrent_worklog_requests: 8,
            maximum_daily_hours: 24,
            default_worklog_start_hour: 9,
            default_worklog_start_minute: 0,
            enable_team_reports: false,
        }
    }
}
