use std::sync::OnceLock;

use serde::Deserialize;

const ENGLISH_COPY: &str = include_str!("../resources/en.json");
const SPANISH_COPY: &str = include_str!("../resources/es.json");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TuiCopy {
    pub error_prefix: String,
    pub result_title: String,
    pub error_title: String,
    pub working_title: String,
    pub validating_account: String,
    pub installing_skills: String,
    pub skills_status_title: String,
    pub skills_status_subtitle: String,
    pub skills_metric_labels: [String; 4],
    pub input_description: String,
    pub input_value_label: String,
    pub progress_description: String,
    pub skills_ready: String,
    pub skills_conflicts: String,
    pub menu_title: String,
    pub menu_subtitle: String,
    pub menu_navigation_title: String,
    pub menu_overview_title: String,
    pub menu_action_descriptions: Vec<String>,
    pub configuration_label: String,
    pub modules_label: String,
    pub server_state_label: String,
    pub clients_label: String,
    pub more_clients_label: String,
    pub state_configured: String,
    pub state_not_configured: String,
    pub no_enabled_modules: String,
    pub yes: String,
    pub no: String,
    pub menu_configure_action: String,
    pub menu_clients_action: String,
    pub menu_refresh_action: String,
    pub menu_skills_action: String,
    pub menu_uninstall_action: String,
    pub menu_exit_action: String,
    pub tui_navigation_hint: String,
    pub single_select_help: String,
    pub multi_select_help: String,
    pub input_help: String,
    pub acknowledge_help: String,
    pub confirm_accept: String,
    pub confirm_cancel: String,
    pub profile_path_required: String,
    pub invalid_setup_arguments: String,
    pub install_config_path_required: String,
    pub install_clients_required: String,
    pub install_confirmation_required: String,
    pub invalid_install_arguments: String,
    pub invalid_install_clients: String,
    pub install_config_not_found: String,
    pub install_complete: String,
    pub unexpected_arguments: String,
    #[cfg(feature = "managed-distribution")]
    pub managed_profile_immutable: String,
    #[cfg(any(
        all(feature = "jira", feature = "bitbucket"),
        not(any(feature = "jira", feature = "bitbucket"))
    ))]
    pub profile_has_no_bundled_modules: String,
    #[cfg(any(feature = "jira", feature = "bitbucket"))]
    pub profile_module_unavailable: String,
    #[cfg(feature = "jira")]
    pub profile_sites_title: String,
    #[cfg(feature = "bitbucket")]
    pub profile_workspaces_title: String,
    #[cfg(all(feature = "jira", feature = "bitbucket"))]
    pub provider_title: String,
    #[cfg(feature = "jira")]
    pub provider_jira: String,
    #[cfg(all(feature = "jira", feature = "bitbucket"))]
    pub provider_jira_description: String,
    #[cfg(feature = "bitbucket")]
    pub provider_bitbucket: String,
    #[cfg(all(feature = "jira", feature = "bitbucket"))]
    pub provider_bitbucket_description: String,
    #[cfg(any(feature = "jira", feature = "bitbucket"))]
    pub invalid_provider_selection: String,
    #[cfg(feature = "jira")]
    pub jira_site_label: String,
    #[cfg(feature = "jira")]
    pub jira_site_example: String,
    #[cfg(feature = "jira")]
    pub jira_email_label: String,
    #[cfg(feature = "bitbucket")]
    pub bitbucket_email_label: String,
    #[cfg(feature = "bitbucket")]
    pub bitbucket_workspace_label: String,
    #[cfg(any(feature = "jira", feature = "bitbucket"))]
    pub account_verified: String,
    #[cfg(any(feature = "jira", feature = "bitbucket"))]
    pub setup_saved: String,
    pub uninstall_confirmation: String,
    pub install_skills_confirmation: String,
    pub no_changes: String,
    pub uninstall_complete: String,
    #[cfg(feature = "jira")]
    pub no_boards: String,
    #[cfg(feature = "jira")]
    pub boards_title: String,
    #[cfg(feature = "jira")]
    pub invalid_board_selection: String,
    #[cfg(feature = "bitbucket")]
    pub no_repositories: String,
    #[cfg(feature = "bitbucket")]
    pub repositories_title: String,
    #[cfg(feature = "bitbucket")]
    #[cfg(feature = "bitbucket")]
    pub invalid_repository_selection: String,
    #[cfg(any(feature = "jira", feature = "bitbucket"))]
    pub no_pending_clients: String,
    pub no_compatible_clients: String,
    pub clients_title: String,
    pub remove_action: String,
    pub update_action: String,
    pub install_action: String,
    pub invalid_client_selection: String,
    pub invalid_client_state: String,
    pub client_updated: String,
    #[cfg(any(feature = "jira", feature = "bitbucket"))]
    pub detected_clients_title: String,
    pub client_registered: String,
    pub state_unavailable: String,
    pub state_available: String,
    pub state_registered: String,
    pub state_broken: String,
    pub state_outdated: String,
    pub state_conflict: String,
    pub state_invalid: String,
    pub removals_title: String,
    pub field_required: String,
    #[cfg(any(feature = "jira", feature = "bitbucket"))]
    pub token_environment_notice: String,
    #[cfg(any(feature = "jira", feature = "bitbucket"))]
    pub token_prompt: String,
    #[cfg(feature = "jira")]
    pub enable_hours: String,
    #[cfg(feature = "jira")]
    pub jira_capabilities_title: String,
    #[cfg(feature = "jira")]
    pub enable_hours_write: String,
    #[cfg(feature = "jira")]
    pub weekly_target_hours: String,
    #[cfg(feature = "jira")]
    pub invalid_weekly_target_hours: String,
    #[cfg(feature = "jira")]
    pub utc_offset: String,
    #[cfg(feature = "jira")]
    pub invalid_utc_offset: String,
    #[cfg(feature = "jira")]
    pub hours_outside_profile: String,
    #[cfg(feature = "jira")]
    pub enable_issue_read: String,
    #[cfg(feature = "jira")]
    pub enable_issue_edit: String,
    #[cfg(feature = "jira")]
    pub enable_issue_comment: String,
    #[cfg(feature = "jira")]
    pub enable_issue_transition: String,
    #[cfg(feature = "bitbucket")]
    pub enable_bitbucket_read: String,
    #[cfg(feature = "bitbucket")]
    pub bitbucket_capabilities_title: String,
    #[cfg(feature = "bitbucket")]
    pub enable_bitbucket_create: String,
    #[cfg(feature = "bitbucket")]
    pub enable_bitbucket_edit: String,
    #[cfg(feature = "bitbucket")]
    pub enable_bitbucket_comment: String,
    #[cfg(feature = "bitbucket")]
    pub enable_bitbucket_review: String,
    #[cfg(feature = "bitbucket")]
    pub enable_bitbucket_merge: String,
    #[cfg(feature = "bitbucket")]
    pub enable_bitbucket_decline: String,
    #[cfg(feature = "bitbucket")]
    pub configure_pull_request_defaults: String,
    #[cfg(feature = "bitbucket")]
    pub configure_default_reviewers: String,
    #[cfg(feature = "bitbucket")]
    pub default_reviewer_account_ids: String,
    #[cfg(feature = "bitbucket")]
    pub default_close_source_branch: String,
    #[cfg(any(feature = "jira", feature = "bitbucket"))]
    pub protected_secret_notice: String,
    #[cfg(any(feature = "jira", feature = "bitbucket"))]
    pub environment_secret_notice: String,
    pub help_usage: String,
    pub unsafe_uninstall: String,
    pub unreadable_configuration: String,
    #[cfg(all(
        any(windows, target_os = "linux"),
        any(feature = "jira", feature = "bitbucket")
    ))]
    pub rollback_failed: String,
    #[cfg(not(all(
        any(windows, target_os = "linux"),
        any(feature = "jira", feature = "bitbucket")
    )))]
    #[serde(rename = "rollbackFailed")]
    _rollback_failed: String,
}

pub(crate) fn tui_copy() -> &'static TuiCopy {
    copy_for(super::preferred_language())
}

fn copy_for(language: worklogger_settings::Language) -> &'static TuiCopy {
    static ENGLISH: OnceLock<TuiCopy> = OnceLock::new();
    static SPANISH: OnceLock<TuiCopy> = OnceLock::new();
    match language {
        worklogger_settings::Language::English => ENGLISH.get_or_init(|| parse(ENGLISH_COPY)),
        worklogger_settings::Language::Spanish => SPANISH.get_or_init(|| parse(SPANISH_COPY)),
    }
}

fn parse(copy: &str) -> TuiCopy {
    serde_json::from_str(copy).expect("the embedded TUI resource must be valid JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_copy_is_complete() {
        let copy = tui_copy();
        assert!(!copy.menu_title.trim().is_empty());
        assert!(!copy.help_usage.trim().is_empty());
    }

    #[test]
    fn both_language_resources_match_the_tui_schema() {
        let _: TuiCopy = serde_json::from_str(ENGLISH_COPY).expect("English TUI copy is valid");
        let _: TuiCopy = serde_json::from_str(SPANISH_COPY).expect("Spanish TUI copy is valid");
    }

    #[test]
    fn copy_changes_with_the_selected_language() {
        let english = copy_for(worklogger_settings::Language::English);
        let spanish = copy_for(worklogger_settings::Language::Spanish);
        assert_ne!(english.menu_title, spanish.menu_title);
    }
}
