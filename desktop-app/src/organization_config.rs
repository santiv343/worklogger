use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use dioxus::prelude::*;

use crate::copy::text;
use crate::defaults::{export_defaults, has_external_defaults, install_defaults, product_defaults};

const CONFIGURATION_FILTER_NAME: &str = "JSON";
const CONFIGURATION_FILE_EXTENSION: &str = "json";
const EXPORTED_CONFIGURATION_FILE_NAME: &str = "worklogger.config.json";

#[derive(Clone, Copy, Default, PartialEq)]
enum ConfigurationOperation {
    #[default]
    Idle,
    Importing,
    Exporting,
}

#[component]
pub(crate) fn OrganizationConfigurationPicker(
    initial_error: Option<String>,
    allow_export: bool,
    on_loaded: EventHandler<()>,
) -> Element {
    let operation = use_signal(ConfigurationOperation::default);
    let error = use_signal(|| initial_error);
    let notice = use_signal(|| None::<String>);
    rsx! { section {
        class: "configuration-choice",
        aria_labelledby: "configuration-choice-title",
        ConfigurationDescription {}
        ConfigurationActions { operation, error, notice, allow_export, on_loaded }
        ConfigurationFeedback { error, notice }
    } }
}

#[component]
fn ConfigurationDescription() -> Element {
    let organization = product_defaults().branding.company_name.clone();
    let status = configuration_status(&organization);
    rsx! { div {
        h2 { id: "configuration-choice-title", {text("setup.configurationTitle")} }
        p { "{status}" }
    } }
}

fn configuration_status(organization: &str) -> String {
    if has_external_defaults() {
        return format!(
            "{}{}",
            text("setup.configurationLoadedPrefix"),
            organization
        );
    }
    text("setup.configurationQuickStatus").to_owned()
}

#[component]
fn ConfigurationActions(
    operation: Signal<ConfigurationOperation>,
    error: Signal<Option<String>>,
    notice: Signal<Option<String>>,
    allow_export: bool,
    on_loaded: EventHandler<()>,
) -> Element {
    let busy = operation() != ConfigurationOperation::Idle;
    rsx! { div { class: "configuration-actions",
        button { class: "button secondary", r#type: "button", disabled: busy,
            onclick: move |_| start_import(operation, error, notice, on_loaded),
            {import_button_label(operation())}
        }
        if allow_export { button { class: "button secondary", r#type: "button", disabled: busy,
            onclick: move |_| start_export(operation, error, notice),
            {export_button_label(operation())}
        } }
    } }
}

fn import_button_label(operation: ConfigurationOperation) -> &'static str {
    if operation == ConfigurationOperation::Importing {
        text("setup.configurationLoading")
    } else {
        text("setup.configurationLoad")
    }
}

fn export_button_label(operation: ConfigurationOperation) -> &'static str {
    if operation == ConfigurationOperation::Exporting {
        text("configuration.exporting")
    } else {
        text("configuration.export")
    }
}

#[component]
fn ConfigurationFeedback(error: Signal<Option<String>>, notice: Signal<Option<String>>) -> Element {
    rsx! {
        if let Some(message) = error() {
            p { class: "configuration-choice-error", role: "alert", "{message}" }
        }
        if let Some(message) = notice() {
            p { class: "configuration-choice-notice", role: "status", "{message}" }
        }
    }
}

fn start_import(
    mut operation: Signal<ConfigurationOperation>,
    error: Signal<Option<String>>,
    notice: Signal<Option<String>>,
    on_loaded: EventHandler<()>,
) {
    begin_operation(operation, error, notice, ConfigurationOperation::Importing);
    spawn(async move {
        let selection = select_configuration_file().await;
        operation.set(ConfigurationOperation::Idle);
        import_selection(selection, error, on_loaded);
    });
}

fn start_export(
    mut operation: Signal<ConfigurationOperation>,
    error: Signal<Option<String>>,
    notice: Signal<Option<String>>,
) {
    begin_operation(operation, error, notice, ConfigurationOperation::Exporting);
    spawn(async move {
        let selection = select_export_path().await;
        operation.set(ConfigurationOperation::Idle);
        export_selection(selection, error, notice);
    });
}

fn begin_operation(
    mut operation: Signal<ConfigurationOperation>,
    mut error: Signal<Option<String>>,
    mut notice: Signal<Option<String>>,
    next: ConfigurationOperation,
) {
    operation.set(next);
    error.set(None);
    notice.set(None);
}

fn import_selection(
    selection: Option<PathBuf>,
    mut error: Signal<Option<String>>,
    on_loaded: EventHandler<()>,
) {
    let Some(path) = selection else {
        return;
    };
    match install_defaults(&path) {
        Ok(()) => on_loaded.call(()),
        Err(message) => error.set(Some(message)),
    }
}

fn export_selection(
    selection: Option<PathBuf>,
    mut error: Signal<Option<String>>,
    mut notice: Signal<Option<String>>,
) {
    let Some(path) = selection else {
        return;
    };
    match export_defaults(&json_path(&path)) {
        Ok(()) => notice.set(Some(text("configuration.exported").to_owned())),
        Err(message) => error.set(Some(message)),
    }
}

fn json_path(path: &Path) -> PathBuf {
    let has_extension = path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case(CONFIGURATION_FILE_EXTENSION));
    if has_extension {
        return path.to_path_buf();
    }
    path.with_extension(CONFIGURATION_FILE_EXTENSION)
}

async fn select_configuration_file() -> Option<PathBuf> {
    file_dialog().pick_file().await.map(|file| file_path(&file))
}

async fn select_export_path() -> Option<PathBuf> {
    file_dialog()
        .set_file_name(EXPORTED_CONFIGURATION_FILE_NAME)
        .save_file()
        .await
        .map(|file| file_path(&file))
}

fn file_dialog() -> rfd::AsyncFileDialog {
    rfd::AsyncFileDialog::new()
        .add_filter(CONFIGURATION_FILTER_NAME, &[CONFIGURATION_FILE_EXTENSION])
}

fn file_path(file: &rfd::FileHandle) -> PathBuf {
    file.path().to_path_buf()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::json_path;

    #[test]
    fn export_path_keeps_or_adds_the_json_extension() {
        assert_eq!(json_path(Path::new("config")), Path::new("config.json"));
        assert_eq!(
            json_path(Path::new("config.JSON")),
            Path::new("config.JSON")
        );
    }
}
