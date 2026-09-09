use std::sync::OnceLock;

use dioxus::prelude::*;
use serde::Deserialize;

const ENGLISH_SHELL_COPY: &str = include_str!("../resources/shell.en.json");
const SPANISH_SHELL_COPY: &str = include_str!("../resources/shell.es.json");
const CSS: Asset = asset!("/assets/main.css");
const BRAND_LOGO: Asset = asset!("/assets/brand-logo.svg");

#[derive(Deserialize)]
struct ShellCopy {
    title: String,
    company_name: String,
    heading: String,
    description: String,
}

#[allow(non_snake_case)]
pub(crate) fn App() -> Element {
    let copy = copy();
    rsx! {
        document::Title { "{copy.title}" }
        document::Stylesheet { href: CSS }
        main { class: "loading-screen empty-shell",
            img { class: "loading-logo", src: BRAND_LOGO, alt: "{copy.company_name}", width: "132", height: "42" }
            h1 { "{copy.heading}" }
            p { "{copy.description}" }
        }
    }
}

fn copy() -> &'static ShellCopy {
    static COPY: OnceLock<ShellCopy> = OnceLock::new();
    COPY.get_or_init(|| {
        serde_json::from_str(selected_copy())
            .expect("the selected shell resource must match the ShellCopy schema")
    })
}

fn selected_copy() -> &'static str {
    match crate::copy::preferred_language() {
        worklogger_settings::Language::English => ENGLISH_SHELL_COPY,
        worklogger_settings::Language::Spanish => SPANISH_SHELL_COPY,
    }
}
