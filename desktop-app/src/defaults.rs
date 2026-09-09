use std::{
    ops::Deref,
    sync::{Arc, OnceLock, RwLock, RwLockReadGuard},
};

#[cfg(organization_configuration_mutable)]
use std::path::{Path, PathBuf};

#[cfg(organization_configuration_mutable)]
use worklogger_profile::OrganizationProfileStore;
pub(crate) use worklogger_profile::{
    JiraModuleProfile as JiraDefaults, OrganizationProfile as ProductDefaults,
};

const EMBEDDED_DEFAULTS: &str = include_str!(concat!(
    env!("OUT_DIR"),
    "/worklogger-distribution-profile.json"
));
#[cfg(organization_configuration_mutable)]
const CONFIGURATION_FILE_NAME: &str = "worklogger.config.json";

struct DefaultsState {
    defaults: Arc<ProductDefaults>,
    error: Option<String>,
    #[cfg(all(
        organization_configuration_mutable,
        any(windows, feature = "dev-desktop")
    ))]
    external: bool,
}

pub(crate) struct ProductDefaultsGuard {
    guard: RwLockReadGuard<'static, DefaultsState>,
}

impl Deref for ProductDefaultsGuard {
    type Target = ProductDefaults;

    fn deref(&self) -> &Self::Target {
        self.guard.defaults.as_ref()
    }
}

impl ProductDefaultsGuard {
    pub(crate) fn jira(&self) -> &JiraDefaults {
        self.modules
            .jira
            .as_ref()
            .expect("el perfil Desktop validado debe incluir Jira")
    }

    pub(crate) fn hours(&self) -> &worklogger_profile::HoursProfile {
        &self.jira().hours
    }

    #[cfg(any(windows, feature = "dev-desktop"))]
    pub(crate) fn reports(&self) -> &worklogger_profile::ReportsModuleProfile {
        self.modules
            .reports
            .as_ref()
            .expect("el perfil Desktop validado debe incluir Reportes")
    }
}

pub(crate) fn product_defaults() -> ProductDefaultsGuard {
    ProductDefaultsGuard {
        guard: read_state(),
    }
}

#[cfg(any(windows, feature = "dev-desktop"))]
pub(crate) fn defaults_error() -> Option<String> {
    read_state().error.clone()
}

#[cfg(any(windows, feature = "dev-desktop"))]
pub(crate) fn ensure_defaults_valid() -> Result<(), String> {
    validate_defaults_state(&read_state())
}

fn validate_defaults_state(state: &DefaultsState) -> Result<(), String> {
    state.error.clone().map_or(Ok(()), Err)
}

#[cfg(all(
    organization_configuration_mutable,
    any(windows, feature = "dev-desktop")
))]
pub(crate) fn has_external_defaults() -> bool {
    read_state().external
}

#[cfg(any(windows, feature = "dev-desktop"))]
pub(crate) fn branding_css() -> String {
    let defaults = product_defaults();
    let branding = &defaults.branding;
    format!(
        ":root {{--color-primary:{};--color-text:{};--color-text-muted:{};--color-accent-subtle:{};--color-border:{};--color-chart-1:{};--color-chart-2:{};--color-chart-3:{};--color-chart-4:{};--color-chart-5:{};--color-chart-6:{}}}",
        branding.primary_color,
        branding.text_color,
        branding.muted_color,
        branding.surface_color,
        branding.border_color,
        branding.chart_palette[0],
        branding.chart_palette[1],
        branding.chart_palette[2],
        branding.chart_palette[3],
        branding.chart_palette[4],
        branding.chart_palette[5],
    )
}

#[cfg(all(
    organization_configuration_mutable,
    any(windows, feature = "dev-desktop")
))]
pub(crate) fn install_defaults(path: &Path) -> Result<(), String> {
    let defaults = read_defaults(path)?;
    persist_defaults(&defaults)?;
    let mut state_guard = defaults_lock().write().map_err(|error| error.to_string())?;
    *state_guard = state(Arc::new(defaults), None, true);
    Ok(())
}

#[cfg(organization_configuration_mutable)]
pub(crate) fn export_defaults(path: &Path) -> Result<(), String> {
    let defaults = product_defaults();
    OrganizationProfileStore::at(path.to_path_buf())
        .save(&defaults)
        .map_err(|error| error.to_string())
}

fn defaults_lock() -> &'static RwLock<DefaultsState> {
    static DEFAULTS: OnceLock<RwLock<DefaultsState>> = OnceLock::new();
    DEFAULTS.get_or_init(|| RwLock::new(load_defaults()))
}

fn read_state() -> RwLockReadGuard<'static, DefaultsState> {
    defaults_lock()
        .read()
        .expect("el estado de configuración no debe quedar bloqueado")
}

fn load_defaults() -> DefaultsState {
    let embedded = parse_defaults(EMBEDDED_DEFAULTS)
        .expect("el perfil embebido debe respetar el schema de ProductDefaults");
    #[cfg(not(organization_configuration_mutable))]
    return state(Arc::new(embedded), None, false);
    #[cfg(organization_configuration_mutable)]
    let external = match configured_defaults() {
        Ok(Some(defaults)) => defaults,
        Ok(None) => return state(Arc::new(embedded), None, false),
        Err(error) => return state(Arc::new(embedded), Some(error), false),
    };
    #[cfg(organization_configuration_mutable)]
    state(Arc::new(external), None, true)
}

fn state(defaults: Arc<ProductDefaults>, error: Option<String>, external: bool) -> DefaultsState {
    #[cfg(not(any(windows, feature = "dev-desktop")))]
    let _ = external;
    #[cfg(all(
        not(organization_configuration_mutable),
        any(windows, feature = "dev-desktop")
    ))]
    let _ = external;
    DefaultsState {
        defaults,
        error,
        #[cfg(all(
            organization_configuration_mutable,
            any(windows, feature = "dev-desktop")
        ))]
        external,
    }
}

#[cfg(organization_configuration_mutable)]
fn configured_defaults() -> Result<Option<ProductDefaults>, String> {
    let store = OrganizationProfileStore::for_current_user().map_err(|error| error.to_string())?;
    if let Some(defaults) = store.load().map_err(|error| error.to_string())? {
        validate_desktop_modules(&defaults)?;
        return Ok(Some(defaults));
    }
    let path = legacy_sibling_path()?;
    path.map(|value| read_defaults(&value)).transpose()
}

#[cfg(organization_configuration_mutable)]
fn legacy_sibling_path() -> Result<Option<PathBuf>, String> {
    let executable_path = std::env::current_exe().map_err(|error| error.to_string())?;
    let sibling = executable_path.with_file_name(CONFIGURATION_FILE_NAME);
    Ok(sibling.is_file().then_some(sibling))
}

#[cfg(organization_configuration_mutable)]
fn read_defaults(path: &Path) -> Result<ProductDefaults, String> {
    let store = OrganizationProfileStore::at(path.to_path_buf());
    let defaults = store
        .load()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| path.display().to_string())?;
    validate_desktop_modules(&defaults)?;
    Ok(defaults)
}

#[cfg(all(
    organization_configuration_mutable,
    any(windows, feature = "dev-desktop")
))]
fn persist_defaults(defaults: &ProductDefaults) -> Result<(), String> {
    OrganizationProfileStore::for_current_user()
        .map_err(|error| error.to_string())?
        .save(defaults)
        .map_err(|error| error.to_string())
}

#[cfg(all(organization_configuration_mutable, test))]
fn persist_defaults_to(defaults: &ProductDefaults, destination: &Path) -> Result<(), String> {
    OrganizationProfileStore::at(destination.to_path_buf())
        .save(defaults)
        .map_err(|error| error.to_string())
}

fn parse_defaults(contents: &str) -> Result<ProductDefaults, String> {
    let defaults = ProductDefaults::from_json(contents).map_err(|error| error.to_string())?;
    validate_desktop_modules(&defaults)?;
    Ok(defaults)
}

fn validate_desktop_modules(defaults: &ProductDefaults) -> Result<(), String> {
    if defaults.modules.jira.is_none() {
        return Err("esta edición de Desktop requiere el módulo Jira".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(organization_configuration_mutable)]
    use std::fs;
    #[cfg(organization_configuration_mutable)]
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{
        DefaultsState, EMBEDDED_DEFAULTS, parse_defaults, product_defaults, validate_defaults_state,
    };
    #[cfg(organization_configuration_mutable)]
    use super::{export_defaults, persist_defaults_to};

    #[cfg(organization_configuration_mutable)]
    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn embedded_defaults_are_typed_and_non_zero() {
        let defaults = product_defaults();
        assert!(defaults.jira().request_timeout_seconds > 0);
        assert!(!defaults.branding.company_name.is_empty());
        assert!(defaults.branding.primary_color.starts_with('#'));
        assert!(defaults.branding.text_color.starts_with('#'));
        assert!(defaults.branding.muted_color.starts_with('#'));
        assert!(defaults.branding.surface_color.starts_with('#'));
        assert!(defaults.branding.border_color.starts_with('#'));
        assert!(!defaults.branding.chart_palette.is_empty());
        assert!(
            defaults
                .branding
                .chart_palette
                .iter()
                .all(|color| color.starts_with('#'))
        );
        assert!(defaults.jira().page_size > 0);
        assert!(defaults.jira().maximum_collection_items > 0);
        assert!(defaults.jira().maximum_issue_search_results > 0);
        assert!(defaults.jira().maximum_concurrent_worklog_requests > 0);
        assert!(
            defaults.jira().maximum_allowed_request_timeout_seconds
                >= defaults.jira().request_timeout_seconds
        );
        assert!(defaults.jira().maximum_allowed_page_size >= defaults.jira().page_size);
        assert!(
            defaults.jira().maximum_allowed_collection_items
                >= defaults.jira().maximum_collection_items
        );
        assert!(
            defaults.jira().maximum_allowed_issue_search_results
                >= defaults.jira().maximum_issue_search_results
        );
        assert!(
            defaults.jira().maximum_allowed_concurrent_worklog_requests
                >= defaults.jira().maximum_concurrent_worklog_requests
        );
        assert!(defaults.hours().maximum_custom_range_days > 0);
        assert!(!defaults.hours().utc_offset_options.is_empty());
        if let Some(reports) = defaults.modules.reports.as_ref() {
            assert!(reports.maximum_task_slices > 1);
            assert!(reports.maximum_trend_labels > 1);
            assert!(reports.maximum_team_chart_members > 0);
            assert!(reports.table_page_size > 0);
        }
    }

    #[test]
    fn invalid_installed_profile_is_a_blocking_state() {
        let defaults = parse_defaults(EMBEDDED_DEFAULTS).expect("embedded profile is valid");
        let state = DefaultsState {
            defaults: std::sync::Arc::new(defaults),
            error: Some("invalid installed profile".to_owned()),
            #[cfg(all(
                organization_configuration_mutable,
                any(windows, feature = "dev-desktop")
            ))]
            external: false,
        };

        assert_eq!(
            validate_defaults_state(&state),
            Err("invalid installed profile".to_owned())
        );
    }

    #[cfg(organization_configuration_mutable)]
    #[test]
    fn selected_configuration_is_validated_and_copied() {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "worklogger-defaults-{}-{sequence}",
            std::process::id()
        ));
        let source = directory.join("selected.json");
        let destination = directory.join("private/organization.json");
        fs::create_dir_all(&directory).expect("temporary directory is created");
        fs::write(&source, EMBEDDED_DEFAULTS).expect("source configuration is created");
        let defaults = parse_defaults(EMBEDDED_DEFAULTS).expect("embedded configuration is valid");
        persist_defaults_to(&defaults, &destination).expect("configuration is copied");
        let copied = fs::read_to_string(&destination).expect("copied configuration is readable");
        assert_eq!(parse_defaults(&copied), parse_defaults(EMBEDDED_DEFAULTS));
        fs::remove_dir_all(directory).expect("temporary directory is removed");
    }

    #[cfg(organization_configuration_mutable)]
    #[test]
    fn exported_configuration_is_valid_json_without_credentials() {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "worklogger-export-{}-{sequence}.json",
            std::process::id()
        ));
        export_defaults(&path).expect("configuration is exported");
        let exported = fs::read_to_string(&path).expect("export is readable");
        assert!(parse_defaults(&exported).is_ok());
        assert!(exported.contains("\"schemaVersion\": 2"));
        assert!(exported.contains("\"modules\""));
        assert!(!exported.contains("token"));
        assert!(!exported.contains("email"));
        fs::remove_file(path).expect("temporary export is removed");
    }

    #[test]
    fn rejects_branding_that_makes_text_unreadable() {
        let mut unreadable = parse_defaults(EMBEDDED_DEFAULTS).expect("embedded profile is valid");
        unreadable.branding.text_color = unreadable.branding.surface_color.clone();
        let document = serde_json::to_string(&unreadable).expect("profile serializes");

        assert!(parse_defaults(&document).is_err());
    }

    #[cfg(feature = "reports")]
    #[test]
    fn accepts_a_profile_that_does_not_offer_reports() {
        let mut profile = parse_defaults(EMBEDDED_DEFAULTS).expect("embedded profile is valid");
        profile.modules.reports = None;
        let document = serde_json::to_string(&profile).expect("profile serializes");

        assert!(parse_defaults(&document).is_ok());
    }
}
