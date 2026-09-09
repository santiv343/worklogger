#[cfg(any(all(windows, feature = "hours"), feature = "dev-desktop"))]
mod async_request;
#[cfg(any(all(windows, feature = "hours"), feature = "dev-desktop"))]
mod board_selector;
#[cfg(any(all(windows, feature = "hours"), feature = "dev-desktop"))]
mod connection_model;
#[cfg(any(all(feature = "hours", any(windows, test)), feature = "dev-desktop"))]
mod copy;
#[cfg(any(all(windows, feature = "hours"), feature = "dev-desktop"))]
mod credentials;
#[cfg(any(all(feature = "hours", any(windows, test)), feature = "dev-desktop"))]
mod defaults;
#[cfg(all(not(windows), feature = "dev-desktop"))]
mod development_config;
#[cfg(any(all(windows, feature = "hours"), feature = "dev-desktop"))]
mod mcp_management;
#[cfg(any(all(feature = "hours", any(windows, test)), feature = "dev-desktop"))]
mod settings;

#[cfg(any(all(windows, feature = "hours"), feature = "dev-desktop"))]
mod connection;
#[cfg(any(all(windows, feature = "hours"), feature = "dev-desktop"))]
mod demo;
#[cfg(all(
    organization_configuration_mutable,
    any(all(windows, feature = "hours"), feature = "dev-desktop")
))]
mod organization_config;
#[cfg(any(all(windows, feature = "hours"), feature = "dev-desktop"))]
mod preferences;
#[cfg(all(
    feature = "reports",
    any(all(windows, feature = "hours"), feature = "dev-desktop")
))]
mod report_analytics;
#[cfg(all(
    feature = "reports",
    any(all(windows, feature = "hours"), feature = "dev-desktop")
))]
mod report_export;
#[cfg(all(
    feature = "reports",
    any(all(windows, feature = "hours"), feature = "dev-desktop")
))]
mod report_pdf;
#[cfg(all(
    feature = "reports",
    any(all(windows, feature = "hours"), feature = "dev-desktop")
))]
mod reports;
#[cfg(any(all(windows, feature = "hours"), feature = "dev-desktop"))]
mod setup;
#[cfg(all(windows, not(feature = "hours")))]
mod shell;
#[cfg(any(all(windows, feature = "hours"), feature = "dev-desktop"))]
mod ui;

#[cfg(any(windows, feature = "dev-desktop"))]
const APPLICATION_TITLE: &str = "Worklogger";

#[cfg(any(windows, feature = "dev-desktop"))]
fn launch_desktop(app: fn() -> dioxus::prelude::Element) {
    let window = dioxus::desktop::WindowBuilder::new().with_title(APPLICATION_TITLE);
    dioxus::LaunchBuilder::new()
        .with_cfg(dioxus::desktop::Config::new().with_window(window))
        .launch(app);
}

#[cfg(all(windows, feature = "hours"))]
fn main() {
    launch_desktop(ui::App);
}

#[cfg(all(windows, not(feature = "hours")))]
fn main() {
    launch_desktop(shell::App);
}

#[cfg(all(not(windows), feature = "dev-desktop"))]
fn main() {
    launch_desktop(ui::App);
}

#[cfg(all(not(windows), not(feature = "dev-desktop")))]
fn main() {}
