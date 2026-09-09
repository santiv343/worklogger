use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use atomic_write_file::AtomicWriteFile;
#[cfg(feature = "jira")]
use jira_adapter::JiraSiteUrl;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
pub use worklogger_profile::{Capability, IntegrationModuleId as ModuleId};
use worklogger_profile::{OrganizationProfile, ProviderScopeMode};

const CONFIGURATION_SCHEMA_VERSION: u16 = 2;
const LEGACY_CONFIGURATION_SCHEMA_VERSION: u16 = 1;
const MAXIMUM_EMAIL_LENGTH: usize = 254;
const MAXIMUM_WEEKLY_HOURS: u16 = 168;
const MAXIMUM_UTC_OFFSET_MINUTES: i16 = 14 * 60;
const MAXIMUM_CONFIGURATION_BYTES: usize = 1_048_576;
pub const DEFAULT_MAXIMUM_ISSUE_SEARCH_RESULTS: usize = 1_000;
const CONFIGURATION_ENVIRONMENT_VARIABLE: &str = "WORKLOGGER_MCP_CONFIG";
#[cfg(windows)]
const CONFIGURATION_DIRECTORY: &str = "Worklogger";
#[cfg(not(windows))]
const UNIX_CONFIGURATION_DIRECTORY: &str = "worklogger";
const CONFIGURATION_FILE_NAME: &str = "mcp.json";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleConfiguration {
    pub enabled: bool,
    pub capabilities: BTreeSet<Capability>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JiraConfiguration {
    pub base_url: String,
    pub email: String,
    pub board_id: u64,
    pub request_timeout_seconds: u64,
    pub page_size: u16,
    pub maximum_collection_items: usize,
    #[serde(default = "default_maximum_issue_search_results")]
    pub maximum_issue_search_results: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hours: Option<JiraHoursConfiguration>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JiraHoursConfiguration {
    pub weekly_target_hours: u16,
    pub utc_offset_minutes: i16,
    pub maximum_concurrent_worklog_requests: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BitbucketConfiguration {
    pub email: String,
    pub workspaces: BTreeMap<String, BTreeSet<String>>,
    pub request_timeout_seconds: u64,
    pub page_size: u16,
    pub maximum_collection_items: usize,
    #[serde(default)]
    pub pull_request_defaults: BitbucketPullRequestDefaults,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BitbucketPullRequestDefaults {
    pub reviewer_account_ids: BTreeSet<String>,
    pub close_source_branch: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpConfiguration {
    schema_version: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jira: Option<JiraConfiguration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bitbucket: Option<BitbucketConfiguration>,
    pub modules: BTreeMap<ModuleId, ModuleConfiguration>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyMcpConfiguration {
    schema_version: u16,
    jira: LegacyJiraConfiguration,
    modules: BTreeMap<ModuleId, ModuleConfiguration>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyJiraConfiguration {
    base_url: String,
    email: String,
    board_id: u64,
    weekly_target_hours: u16,
    utc_offset_minutes: i16,
    request_timeout_seconds: u64,
    page_size: u16,
    maximum_collection_items: usize,
    maximum_concurrent_worklog_requests: usize,
}

#[derive(Debug, Error)]
pub enum ConfigurationError {
    #[error("schemaVersion is not supported")]
    UnsupportedSchema,
    #[error("the Jira configuration is invalid: {0}")]
    InvalidJira(&'static str),
    #[error("the Bitbucket configuration is invalid: {0}")]
    InvalidBitbucket(&'static str),
    #[error("the module configuration is invalid")]
    InvalidModules,
    #[error("the {capability:?} capability requires {required:?}")]
    MissingRequiredCapability {
        capability: Capability,
        required: Capability,
    },
    #[error("the {0:?} module is enabled but was not included in this binary")]
    ModuleNotBundled(ModuleId),
    #[error("the {0:?} configuration is outside the organization scope")]
    OutsideOrganizationScope(ModuleId),
    #[error("the {0:?} limits exceed the organization profile")]
    OutsideOrganizationLimits(ModuleId),
    #[error("could not access the configuration at {path}: {source}")]
    Storage {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the configuration JSON at {path} is invalid: {source}")]
    Decode {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("could not serialize the configuration: {0}")]
    Encode(#[source] serde_json::Error),
    #[error("the configuration exceeds the maximum allowed size")]
    TooLarge,
    #[error("could not determine the user configuration directory")]
    MissingUserConfigurationDirectory,
}

#[derive(Clone, Debug)]
pub struct ConfigurationStore {
    path: PathBuf,
}

impl McpConfiguration {
    /// Creates and validates a standalone MCP configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when the Jira connection or limits are invalid.
    pub fn new(
        jira: JiraConfiguration,
        modules: BTreeMap<ModuleId, ModuleConfiguration>,
    ) -> Result<Self, ConfigurationError> {
        Self::build(Some(jira), None, modules)
    }

    /// Creates a configuration that includes a Bitbucket Cloud connection.
    ///
    /// # Errors
    ///
    /// Returns an error when a connection, module or capability is invalid.
    pub fn new_with_bitbucket(
        jira: JiraConfiguration,
        bitbucket: BitbucketConfiguration,
        modules: BTreeMap<ModuleId, ModuleConfiguration>,
    ) -> Result<Self, ConfigurationError> {
        Self::build(Some(jira), Some(bitbucket), modules)
    }

    /// Creates a configuration containing only the Bitbucket Cloud addon.
    ///
    /// # Errors
    ///
    /// Returns an error when the connection, module or capability is invalid.
    pub fn new_bitbucket(
        bitbucket: BitbucketConfiguration,
        modules: BTreeMap<ModuleId, ModuleConfiguration>,
    ) -> Result<Self, ConfigurationError> {
        Self::build(None, Some(bitbucket), modules)
    }

    fn build(
        jira: Option<JiraConfiguration>,
        bitbucket: Option<BitbucketConfiguration>,
        modules: BTreeMap<ModuleId, ModuleConfiguration>,
    ) -> Result<Self, ConfigurationError> {
        let configuration = Self {
            schema_version: CONFIGURATION_SCHEMA_VERSION,
            jira,
            bitbucket,
            modules,
        };
        configuration.validate_persisted()?;
        Ok(configuration)
    }

    /// Validates persisted configuration before use.
    ///
    /// # Errors
    ///
    /// Returns an error for incompatible schemas or unsafe values.
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        self.validate_persisted()?;
        validate_bundled_modules(&self.modules)
    }

    fn validate_persisted(&self) -> Result<(), ConfigurationError> {
        if self.schema_version != CONFIGURATION_SCHEMA_VERSION {
            return Err(ConfigurationError::UnsupportedSchema);
        }
        if let Some(jira) = self.jira.as_ref() {
            validate_jira(jira)?;
        }
        validate_bitbucket(self.bitbucket.as_ref())?;
        validate_persisted_modules(&self.modules)?;
        validate_module_connections(self)
    }

    #[must_use]
    pub fn capability_enabled(&self, capability: Capability) -> bool {
        self.modules
            .get(&capability.module())
            .is_some_and(|module| module.enabled && module.capabilities.contains(&capability))
    }

    #[must_use]
    pub fn module_enabled(&self, module_id: ModuleId) -> bool {
        self.modules
            .get(&module_id)
            .is_some_and(|module| module.enabled)
    }

    /// Restricts local state to the shared organization profile.
    ///
    /// # Errors
    ///
    /// Returns an error when a persisted provider scope is outside the profile.
    pub fn apply_organization_profile(
        mut self,
        profile: &OrganizationProfile,
    ) -> Result<Self, ConfigurationError> {
        for (module_id, module) in &mut self.modules {
            module
                .capabilities
                .retain(|capability| module_bundled(*module_id) && profile.allows(*capability));
            module.enabled = module.enabled && !module.capabilities.is_empty();
        }
        validate_organization_scopes(&self, profile)?;
        validate_organization_limits(&self, profile)?;
        self.validate()?;
        Ok(self)
    }

    /// Removes enabled modules that are not present in this binary.
    ///
    /// # Errors
    ///
    /// Returns an error when the remaining runtime configuration is invalid.
    pub fn apply_bundled_modules(mut self) -> Result<Self, ConfigurationError> {
        for (module_id, module) in &mut self.modules {
            if module_bundled(*module_id) {
                continue;
            }
            module.enabled = false;
            module.capabilities.clear();
        }
        self.validate()?;
        Ok(self)
    }
}

fn validate_organization_scopes(
    configuration: &McpConfiguration,
    profile: &OrganizationProfile,
) -> Result<(), ConfigurationError> {
    if configuration.module_enabled(ModuleId::Jira) {
        validate_organization_jira_scope(configuration.jira.as_ref(), profile)?;
    }
    if configuration.module_enabled(ModuleId::Bitbucket) {
        validate_organization_bitbucket_scope(configuration.bitbucket.as_ref(), profile)?;
    }
    Ok(())
}

fn validate_organization_jira_scope(
    jira: Option<&JiraConfiguration>,
    profile: &OrganizationProfile,
) -> Result<(), ConfigurationError> {
    let Some(jira) = jira else {
        return Ok(());
    };
    let Some(policy) = profile.modules.jira.as_ref() else {
        return Ok(());
    };
    if policy.scope_mode == ProviderScopeMode::Unrestricted {
        return Ok(());
    }
    profile
        .allows_jira_board(&jira.base_url, jira.board_id)
        .then_some(())
        .ok_or(ConfigurationError::OutsideOrganizationScope(ModuleId::Jira))
}

fn validate_organization_bitbucket_scope(
    bitbucket: Option<&BitbucketConfiguration>,
    profile: &OrganizationProfile,
) -> Result<(), ConfigurationError> {
    let Some(bitbucket) = bitbucket else {
        return Ok(());
    };
    let restricted = profile
        .modules
        .bitbucket
        .as_ref()
        .is_some_and(|module| module.scope_mode == ProviderScopeMode::Restricted);
    if !restricted {
        return Ok(());
    }
    bitbucket_scope_allowed(bitbucket, profile)
        .then_some(())
        .ok_or(ConfigurationError::OutsideOrganizationScope(
            ModuleId::Bitbucket,
        ))
}

fn bitbucket_scope_allowed(
    bitbucket: &BitbucketConfiguration,
    profile: &OrganizationProfile,
) -> bool {
    bitbucket
        .workspaces
        .iter()
        .all(|(workspace, repositories)| workspace_scope_allowed(workspace, repositories, profile))
}

fn workspace_scope_allowed(
    workspace: &str,
    repositories: &BTreeSet<String>,
    profile: &OrganizationProfile,
) -> bool {
    repositories
        .iter()
        .all(|repository| profile.allows_bitbucket_repository(workspace, repository))
}

fn validate_organization_limits(
    configuration: &McpConfiguration,
    profile: &OrganizationProfile,
) -> Result<(), ConfigurationError> {
    if configuration.module_enabled(ModuleId::Jira) {
        validate_organization_jira_limits(configuration, profile)?;
    }
    if configuration.module_enabled(ModuleId::Bitbucket) {
        validate_organization_bitbucket_limits(configuration.bitbucket.as_ref(), profile)?;
    }
    Ok(())
}

fn validate_organization_jira_limits(
    configuration: &McpConfiguration,
    profile: &OrganizationProfile,
) -> Result<(), ConfigurationError> {
    let Some((jira, policy)) = configuration
        .jira
        .as_ref()
        .zip(profile.modules.jira.as_ref())
    else {
        return Ok(());
    };
    let hours_enabled = configuration.capability_enabled(Capability::ReadOwnTimeEntries);
    if jira_limits_allowed(jira, policy, hours_enabled) {
        return Ok(());
    }
    Err(ConfigurationError::OutsideOrganizationLimits(
        ModuleId::Jira,
    ))
}

fn jira_limits_allowed(
    jira: &JiraConfiguration,
    policy: &worklogger_profile::JiraModuleProfile,
    hours_enabled: bool,
) -> bool {
    jira.request_timeout_seconds <= policy.maximum_allowed_request_timeout_seconds
        && jira.page_size <= policy.maximum_allowed_page_size
        && jira.maximum_collection_items <= policy.maximum_allowed_collection_items
        && jira.maximum_issue_search_results <= policy.maximum_allowed_issue_search_results
        && (!hours_enabled
            || jira
                .hours
                .as_ref()
                .is_some_and(|hours| jira_hours_allowed(hours, policy)))
}

fn jira_hours_allowed(
    hours: &JiraHoursConfiguration,
    policy: &worklogger_profile::JiraModuleProfile,
) -> bool {
    let target = u32::from(hours.weekly_target_hours);
    (policy.hours.minimum_weekly_target_hours..=policy.hours.maximum_weekly_target_hours)
        .contains(&target)
        && policy
            .hours
            .utc_offset_options
            .iter()
            .any(|option| option.minutes == hours.utc_offset_minutes)
        && hours.maximum_concurrent_worklog_requests
            <= policy.maximum_allowed_concurrent_worklog_requests
}

fn validate_organization_bitbucket_limits(
    bitbucket: Option<&BitbucketConfiguration>,
    profile: &OrganizationProfile,
) -> Result<(), ConfigurationError> {
    let Some((bitbucket, policy)) = bitbucket.zip(profile.modules.bitbucket.as_ref()) else {
        return Ok(());
    };
    if bitbucket_limits_allowed(bitbucket, policy) {
        return Ok(());
    }
    Err(ConfigurationError::OutsideOrganizationLimits(
        ModuleId::Bitbucket,
    ))
}

fn bitbucket_limits_allowed(
    bitbucket: &BitbucketConfiguration,
    policy: &worklogger_profile::BitbucketModuleProfile,
) -> bool {
    bitbucket.request_timeout_seconds <= policy.maximum_allowed_request_timeout_seconds
        && bitbucket.page_size <= policy.maximum_allowed_page_size
        && bitbucket.maximum_collection_items <= policy.maximum_allowed_collection_items
}

fn validate_module_connections(configuration: &McpConfiguration) -> Result<(), ConfigurationError> {
    validate_jira_connection(configuration)?;
    validate_hours_connection(configuration)?;
    validate_bitbucket_connection(configuration)
}

fn validate_jira_connection(configuration: &McpConfiguration) -> Result<(), ConfigurationError> {
    if configuration.module_enabled(ModuleId::Jira) && configuration.jira.is_none() {
        return Err(ConfigurationError::InvalidJira("connection"));
    }
    Ok(())
}

fn validate_hours_connection(configuration: &McpConfiguration) -> Result<(), ConfigurationError> {
    if configuration.capability_enabled(Capability::ReadOwnTimeEntries)
        && configuration
            .jira
            .as_ref()
            .is_none_or(|jira| jira.hours.is_none())
    {
        return Err(ConfigurationError::InvalidJira("hours"));
    }
    Ok(())
}

fn validate_bitbucket_connection(
    configuration: &McpConfiguration,
) -> Result<(), ConfigurationError> {
    if configuration.module_enabled(ModuleId::Bitbucket) && configuration.bitbucket.is_none() {
        return Err(ConfigurationError::InvalidBitbucket("connection"));
    }
    Ok(())
}

fn validate_persisted_modules(
    modules: &BTreeMap<ModuleId, ModuleConfiguration>,
) -> Result<(), ConfigurationError> {
    for (module_id, module) in modules {
        validate_module_capabilities(*module_id, module)?;
    }
    Ok(())
}

fn validate_bundled_modules(
    modules: &BTreeMap<ModuleId, ModuleConfiguration>,
) -> Result<(), ConfigurationError> {
    let unavailable = modules
        .iter()
        .find(|(module_id, module)| module.enabled && !module_bundled(**module_id));
    unavailable.map_or(Ok(()), |(module_id, _)| {
        Err(ConfigurationError::ModuleNotBundled(*module_id))
    })
}

impl ModuleConfiguration {
    pub fn set_capability(&mut self, capability: Capability, enabled: bool) {
        if enabled {
            self.capabilities.insert(capability);
            self.capabilities.extend(capability.required_read());
        } else {
            self.capabilities.remove(&capability);
            self.capabilities
                .retain(|candidate| candidate.required_read() != Some(capability));
        }
        self.enabled = !self.capabilities.is_empty();
    }
}

fn validate_module_capabilities(
    module_id: ModuleId,
    module: &ModuleConfiguration,
) -> Result<(), ConfigurationError> {
    if module
        .capabilities
        .iter()
        .all(|capability| capability.module() == module_id)
    {
        return validate_capability_dependencies(module);
    }
    Err(ConfigurationError::InvalidModules)
}

fn validate_capability_dependencies(
    module: &ModuleConfiguration,
) -> Result<(), ConfigurationError> {
    for capability in &module.capabilities {
        let Some(required) = capability.required_read() else {
            continue;
        };
        if !module.capabilities.contains(&required) {
            return Err(ConfigurationError::MissingRequiredCapability {
                capability: *capability,
                required,
            });
        }
    }
    Ok(())
}

const fn module_bundled(module_id: ModuleId) -> bool {
    match module_id {
        ModuleId::Jira => cfg!(feature = "jira"),
        ModuleId::Bitbucket => cfg!(feature = "bitbucket"),
    }
}

impl ConfigurationStore {
    /// Resolves the per-user MCP configuration path.
    ///
    /// `WORKLOGGER_MCP_CONFIG` can override it for isolated automation.
    ///
    /// # Errors
    ///
    /// Returns an error when the operating-system user directory is unavailable.
    pub fn for_current_user() -> Result<Self, ConfigurationError> {
        if let Some(path) = environment_path(CONFIGURATION_ENVIRONMENT_VARIABLE) {
            return Ok(Self::at(path));
        }
        platform_configuration_path()
            .map(Self::at)
            .ok_or(ConfigurationError::MissingUserConfigurationDirectory)
    }

    #[must_use]
    pub const fn at(path: PathBuf) -> Self {
        Self { path }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Loads and validates the persisted configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when the file is inaccessible, malformed, or unsafe.
    pub fn load(&self) -> Result<Option<McpConfiguration>, ConfigurationError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(storage_error(&self.path, source)),
        };
        decode_configuration(&bytes, &self.path).map(Some)
    }

    /// Persists validated configuration atomically.
    ///
    /// # Errors
    ///
    /// Returns an error when validation, serialization, or persistence fails.
    pub fn save(&self, configuration: &McpConfiguration) -> Result<(), ConfigurationError> {
        configuration.validate_persisted()?;
        create_parent(&self.path)?;
        let mut bytes =
            serde_json::to_vec_pretty(configuration).map_err(ConfigurationError::Encode)?;
        bytes.push(b'\n');
        write_atomically(&self.path, &bytes)
    }

    /// Removes persisted configuration. Missing files are accepted.
    ///
    /// # Errors
    ///
    /// Returns an error when the existing file cannot be removed.
    pub fn clear(&self) -> Result<(), ConfigurationError> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(storage_error(&self.path, source)),
        }
    }
}

fn environment_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(windows)]
fn platform_configuration_path() -> Option<PathBuf> {
    environment_path("APPDATA").map(|directory| {
        directory
            .join(CONFIGURATION_DIRECTORY)
            .join(CONFIGURATION_FILE_NAME)
    })
}

#[cfg(not(windows))]
fn platform_configuration_path() -> Option<PathBuf> {
    if let Some(directory) = environment_path("XDG_CONFIG_HOME") {
        return Some(
            directory
                .join(UNIX_CONFIGURATION_DIRECTORY)
                .join(CONFIGURATION_FILE_NAME),
        );
    }
    environment_path("HOME").map(|directory| {
        directory
            .join(".config")
            .join(UNIX_CONFIGURATION_DIRECTORY)
            .join(CONFIGURATION_FILE_NAME)
    })
}

fn decode_configuration(bytes: &[u8], path: &Path) -> Result<McpConfiguration, ConfigurationError> {
    if bytes.len() > MAXIMUM_CONFIGURATION_BYTES {
        return Err(ConfigurationError::TooLarge);
    }
    let value = decode_value(bytes, path)?;
    let schema = value.get("schemaVersion").and_then(Value::as_u64);
    let configuration = if schema == Some(u64::from(LEGACY_CONFIGURATION_SCHEMA_VERSION)) {
        migrate_legacy(value, path)?
    } else {
        decode_current(value, path)?
    };
    configuration.validate_persisted()?;
    Ok(configuration)
}

fn decode_value(bytes: &[u8], path: &Path) -> Result<Value, ConfigurationError> {
    serde_json::from_slice(bytes).map_err(|source| decode_error(path, source))
}

fn decode_current(value: Value, path: &Path) -> Result<McpConfiguration, ConfigurationError> {
    serde_json::from_value(value).map_err(|source| decode_error(path, source))
}

fn migrate_legacy(value: Value, path: &Path) -> Result<McpConfiguration, ConfigurationError> {
    let legacy: LegacyMcpConfiguration =
        serde_json::from_value(value).map_err(|source| decode_error(path, source))?;
    if legacy.schema_version != LEGACY_CONFIGURATION_SCHEMA_VERSION {
        return Err(ConfigurationError::UnsupportedSchema);
    }
    Ok(legacy.into_current())
}

fn decode_error(path: &Path, source: serde_json::Error) -> ConfigurationError {
    ConfigurationError::Decode {
        path: path.to_path_buf(),
        source,
    }
}

impl LegacyMcpConfiguration {
    fn into_current(self) -> McpConfiguration {
        let jira = self.jira.into_current();
        McpConfiguration {
            schema_version: CONFIGURATION_SCHEMA_VERSION,
            jira: Some(jira),
            bitbucket: None,
            modules: self.modules,
        }
    }
}

impl LegacyJiraConfiguration {
    fn into_current(self) -> JiraConfiguration {
        JiraConfiguration {
            base_url: self.base_url,
            email: self.email,
            board_id: self.board_id,
            request_timeout_seconds: self.request_timeout_seconds,
            page_size: self.page_size,
            maximum_collection_items: self.maximum_collection_items,
            maximum_issue_search_results: DEFAULT_MAXIMUM_ISSUE_SEARCH_RESULTS,
            hours: Some(JiraHoursConfiguration {
                weekly_target_hours: self.weekly_target_hours,
                utc_offset_minutes: self.utc_offset_minutes,
                maximum_concurrent_worklog_requests: self.maximum_concurrent_worklog_requests,
            }),
        }
    }
}

fn create_parent(path: &Path) -> Result<(), ConfigurationError> {
    let parent = path.parent().ok_or_else(|| {
        storage_error(
            path,
            std::io::Error::other("the path has no parent directory"),
        )
    })?;
    fs::create_dir_all(parent).map_err(|source| storage_error(path, source))
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), ConfigurationError> {
    let mut file = AtomicWriteFile::options()
        .open(path)
        .map_err(|source| storage_error(path, source))?;
    file.write_all(bytes)
        .map_err(|source| storage_error(path, source))?;
    file.commit().map_err(|source| storage_error(path, source))
}

fn storage_error(path: &Path, source: std::io::Error) -> ConfigurationError {
    ConfigurationError::Storage {
        path: path.to_path_buf(),
        source,
    }
}

fn validate_jira(jira: &JiraConfiguration) -> Result<(), ConfigurationError> {
    #[cfg(feature = "jira")]
    JiraSiteUrl::parse(&jira.base_url).map_err(|_| invalid_jira("baseUrl"))?;
    #[cfg(not(feature = "jira"))]
    validate_unbundled_site(&jira.base_url)?;
    validate_email(&jira.email)?;
    validate_positive_values(jira)?;
    if let Some(hours) = jira.hours.as_ref() {
        validate_jira_hours(hours)?;
    }
    Ok(())
}

fn validate_jira_hours(hours: &JiraHoursConfiguration) -> Result<(), ConfigurationError> {
    if hours.weekly_target_hours == 0 || hours.maximum_concurrent_worklog_requests == 0 {
        return Err(invalid_jira("hours limits"));
    }
    if hours.weekly_target_hours > MAXIMUM_WEEKLY_HOURS {
        return Err(invalid_jira("weeklyTargetHours"));
    }
    if hours.utc_offset_minutes.abs() > MAXIMUM_UTC_OFFSET_MINUTES {
        return Err(invalid_jira("utcOffsetMinutes"));
    }
    Ok(())
}

fn validate_bitbucket(
    bitbucket: Option<&BitbucketConfiguration>,
) -> Result<(), ConfigurationError> {
    let Some(bitbucket) = bitbucket else {
        return Ok(());
    };
    validate_email(&bitbucket.email).map_err(|_| ConfigurationError::InvalidBitbucket("email"))?;
    if bitbucket.request_timeout_seconds == 0
        || bitbucket.page_size == 0
        || bitbucket.maximum_collection_items == 0
    {
        return Err(ConfigurationError::InvalidBitbucket("limits"));
    }
    if !valid_bitbucket_scope(&bitbucket.workspaces) {
        return Err(ConfigurationError::InvalidBitbucket("workspaces"));
    }
    Ok(())
}

fn valid_bitbucket_scope(workspaces: &BTreeMap<String, BTreeSet<String>>) -> bool {
    !workspaces.is_empty()
        && workspaces.iter().all(|(workspace, repositories)| {
            valid_scope(workspace)
                && !repositories.is_empty()
                && repositories
                    .iter()
                    .all(|repository| valid_scope(repository))
        })
}

fn valid_scope(value: &str) -> bool {
    !value.trim().is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
}

#[cfg(not(feature = "jira"))]
fn validate_unbundled_site(site: &str) -> Result<(), ConfigurationError> {
    if site.trim().is_empty() {
        return Err(invalid_jira("baseUrl"));
    }
    Ok(())
}

fn validate_email(email: &str) -> Result<(), ConfigurationError> {
    let email = email.trim();
    let valid = !email.is_empty()
        && email.len() <= MAXIMUM_EMAIL_LENGTH
        && email.contains('@')
        && !email.chars().any(char::is_whitespace);
    if !valid {
        return Err(invalid_jira("email"));
    }
    Ok(())
}

fn validate_positive_values(jira: &JiraConfiguration) -> Result<(), ConfigurationError> {
    let valid = jira.board_id > 0
        && jira.request_timeout_seconds > 0
        && jira.page_size > 0
        && jira.maximum_collection_items > 0
        && jira.maximum_issue_search_results > 0;
    if !valid {
        return Err(invalid_jira("limits must be greater than zero"));
    }
    Ok(())
}

const fn default_maximum_issue_search_results() -> usize {
    DEFAULT_MAXIMUM_ISSUE_SEARCH_RESULTS
}

const fn invalid_jira(field: &'static str) -> ConfigurationError {
    ConfigurationError::InvalidJira(field)
}
