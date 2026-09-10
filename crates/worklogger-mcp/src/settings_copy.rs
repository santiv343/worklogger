use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SettingsCopy {
    pub title: String,
    pub jira: String,
    pub bitbucket: String,
    pub mcp_clients: String,
    pub assistant_skills: String,
    pub language: String,
    pub language_saved: String,
    pub connection: String,
    pub site: String,
    pub email: String,
    pub token: String,
    pub verify: String,
    pub board: String,
    pub hours: String,
    pub issues: String,
    pub settings: String,
    pub permissions: String,
    pub search_limit: String,
    pub advanced: String,
    pub workspaces: String,
    pub pull_requests: String,
    pub defaults: String,
    pub timeout: String,
    pub page_size: String,
    pub collection_limit: String,
    pub concurrency: String,
    pub report_period_limit: String,
    pub back: String,
    pub verified: String,
    pub saved: String,
    pub needs_setup: String,
    pub blocked: String,
    pub disabled: String,
    pub ready: String,
    pub managed: String,
    pub pending: String,
    pub applied: String,
    pub connection_required: String,
    pub number_required: String,
    pub connection_changed: String,
    pub managed_notice: String,
    pub no_capabilities: String,
}

pub(crate) fn settings_copy() -> &'static SettingsCopy {
    copy_for(super::preferred_language())
}

fn copy_for(language: worklogger_settings::Language) -> &'static SettingsCopy {
    static ENGLISH: OnceLock<SettingsCopy> = OnceLock::new();
    static SPANISH: OnceLock<SettingsCopy> = OnceLock::new();
    match language {
        worklogger_settings::Language::English => {
            ENGLISH.get_or_init(|| parse(include_str!("../resources/settings.en.json")))
        }
        worklogger_settings::Language::Spanish => {
            SPANISH.get_or_init(|| parse(include_str!("../resources/settings.es.json")))
        }
    }
}

fn parse(copy: &str) -> SettingsCopy {
    serde_json::from_str(copy).expect("bundled settings copy must be valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_changes_with_the_selected_language() {
        let english = copy_for(worklogger_settings::Language::English);
        let spanish = copy_for(worklogger_settings::Language::Spanish);
        assert_ne!(english.title, spanish.title);
    }
}
