use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

use worklogger_mcp::{
    BitbucketConfiguration, BitbucketPullRequestDefaults, Capability, ConfigurationStore,
    JiraConfiguration, JiraHoursConfiguration, McpConfiguration, ModuleConfiguration, ModuleId,
};
use worklogger_settings::{HoursSettings, ReportsSettings, SettingsDocument, SettingsStore};

fn runtime() -> McpConfiguration {
    McpConfiguration::new(
        JiraConfiguration {
            base_url: "https://example.atlassian.net".into(),
            email: "person@example.com".into(),
            board_id: 17,
            request_timeout_seconds: 30,
            page_size: 50,
            maximum_collection_items: 1_000,
            maximum_issue_search_results: 1_000,
            hours: Some(JiraHoursConfiguration {
                weekly_target_hours: 40,
                utc_offset_minutes: 0,
                maximum_daily_hours: 12,
                maximum_report_period_days: 7,
                maximum_concurrent_worklog_requests: 4,
            }),
        },
        BTreeMap::from([(
            ModuleId::Jira,
            ModuleConfiguration {
                enabled: true,
                capabilities: BTreeSet::from([Capability::ReadOwnTimeEntries]),
            },
        )]),
    )
    .unwrap()
}

#[test]
fn shared_projection_preserves_explicit_consent_and_desktop_preferences() {
    let mut document = SettingsDocument {
        hours: Some(HoursSettings {
            maximum_daily_hours: Some(12),
            default_worklog_start_hour: Some(9),
            default_worklog_start_minute: Some(30),
            ..HoursSettings::default()
        }),
        reports: Some(ReportsSettings {
            enable_team_reports: Some(true),
        }),
        ..SettingsDocument::default()
    };
    runtime().update_shared(&mut document).unwrap();
    assert_eq!(
        McpConfiguration::from_shared(&document).unwrap(),
        Some(runtime())
    );
    assert_eq!(
        document.hours.as_ref().unwrap().maximum_daily_hours,
        Some(12)
    );
    assert_eq!(
        document.hours.as_ref().unwrap().default_worklog_start_hour,
        Some(9)
    );
    assert_eq!(
        document.reports.as_ref().unwrap().enable_team_reports,
        Some(true)
    );
    document.mcp = None;
    assert_eq!(McpConfiguration::from_shared(&document).unwrap(), None);
}

#[test]
fn incomplete_enabled_provider_never_becomes_runtime_configuration() {
    let mut document = SettingsDocument::default();
    runtime().update_shared(&mut document).unwrap();
    document.jira.as_mut().unwrap().board_id = None;
    assert_eq!(McpConfiguration::from_shared(&document).unwrap(), None);
    document.jira.as_mut().unwrap().board_id = Some(17);
    document.hours.as_mut().unwrap().utc_offset_minutes = None;
    assert_eq!(McpConfiguration::from_shared(&document).unwrap(), None);
}

#[test]
fn bitbucket_defaults_and_scope_round_trip_without_enabling_jira() {
    let bitbucket = BitbucketConfiguration {
        email: "reviewer@example.com".into(),
        workspaces: BTreeMap::from([("example".into(), BTreeSet::from(["project".into()]))]),
        request_timeout_seconds: 30,
        page_size: 50,
        maximum_collection_items: 500,
        pull_request_defaults: BitbucketPullRequestDefaults {
            reviewer_account_ids: BTreeSet::from(["reviewer-id".into()]),
            close_source_branch: true,
        },
    };
    let modules = BTreeMap::from([(
        ModuleId::Bitbucket,
        ModuleConfiguration {
            enabled: true,
            capabilities: BTreeSet::from([Capability::ReadBitbucketPullRequests]),
        },
    )]);
    let configuration = McpConfiguration::new_bitbucket(bitbucket, modules).unwrap();
    let mut document = SettingsDocument::default();
    configuration.update_shared(&mut document).unwrap();
    assert_eq!(
        McpConfiguration::from_shared(&document).unwrap(),
        Some(configuration)
    );
    assert!(document.jira.is_none());
    assert!(
        document
            .bitbucket
            .unwrap()
            .pull_request_defaults
            .unwrap()
            .close_source_branch
    );
}

#[test]
fn shared_store_observes_other_frontend_and_uninstall_preserves_shared_preferences() {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let directory = std::env::temp_dir().join(format!(
        "worklogger-shared-adapter-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let shared = SettingsStore::at(directory.join("settings.json"));
    let adapter = ConfigurationStore::shared(shared.clone());
    adapter.save(&runtime()).unwrap();
    let mut desktop_edit = shared.load().unwrap().unwrap();
    desktop_edit.hours.as_mut().unwrap().weekly_target_hours = Some(35);
    shared.save(&desktop_edit, desktop_edit.revision).unwrap();
    let observed = adapter.load().unwrap().unwrap();
    assert_eq!(
        observed.jira.unwrap().hours.unwrap().weekly_target_hours,
        35
    );
    adapter.clear().unwrap();
    assert_eq!(adapter.load().unwrap(), None);
    let persisted = shared.load().unwrap().unwrap();
    assert_eq!(persisted.hours.unwrap().weekly_target_hours, Some(35));
    assert!(persisted.jira.is_some());
    assert!(persisted.mcp.is_none());
    fs::remove_dir_all(directory).unwrap();
}

fn connection(board_id: Option<u64>) -> worklogger_settings::JiraSettings {
    worklogger_settings::JiraSettings {
        base_url: Some("https://legacy.atlassian.net".into()),
        email: Some("person@example.com".into()),
        board_id,
        request_timeout_seconds: Some(30),
        page_size: Some(50),
        maximum_collection_items: Some(1_000),
        maximum_issue_search_results: Some(1_000),
        maximum_concurrent_worklog_requests: Some(4),
    }
}

fn document_with(connection_settings: worklogger_settings::JiraSettings) -> SettingsDocument {
    let mut document = SettingsDocument::default();
    runtime().update_shared(&mut document).unwrap();
    document
        .jira_connections
        .insert("legacy".into(), connection_settings);
    document
}

#[test]
fn a_named_connection_reaches_the_runtime_without_hours() {
    let document = document_with(connection(Some(99)));

    let configuration = McpConfiguration::from_shared(&document).unwrap().unwrap();
    let legacy = configuration.resolve_jira(Some("legacy")).unwrap();

    assert_eq!(configuration.jira_connection_names(), vec!["legacy"]);
    assert_eq!(legacy.board_id, 99);
    assert!(
        legacy.hours.is_none(),
        "hours describe the person and the primary site, not a secondary one"
    );
}

#[test]
fn an_incomplete_connection_is_skipped_instead_of_failing_the_projection() {
    let document = document_with(connection(None));

    let configuration = McpConfiguration::from_shared(&document).unwrap().unwrap();

    assert!(configuration.jira_connection_names().is_empty());
    assert_eq!(configuration.resolve_jira(None), runtime().jira.as_ref());
}

#[test]
fn writing_back_keeps_a_connection_the_runtime_skipped() {
    let mut document = document_with(connection(None));
    let configuration = McpConfiguration::from_shared(&document).unwrap().unwrap();
    assert!(configuration.jira_connection_names().is_empty());

    configuration.update_shared(&mut document).unwrap();

    assert!(
        document.jira_connections.contains_key("legacy"),
        "a draft the runtime could not serve must survive a write-back"
    );
    assert_eq!(document.jira_connections["legacy"].board_id, None);
}

#[test]
fn writing_back_leaves_the_desktop_active_connection_alone() {
    let mut document = document_with(connection(Some(99)));
    document.active_connection = Some("legacy".into());

    let configuration = McpConfiguration::from_shared(&document).unwrap().unwrap();
    configuration.update_shared(&mut document).unwrap();

    assert_eq!(document.active_connection.as_deref(), Some("legacy"));
}

#[test]
fn a_connection_round_trips_through_the_shared_document() {
    let mut document = document_with(connection(Some(99)));
    let configuration = McpConfiguration::from_shared(&document).unwrap().unwrap();

    configuration.update_shared(&mut document).unwrap();

    assert_eq!(
        McpConfiguration::from_shared(&document).unwrap(),
        Some(configuration)
    );
}
