use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "jira")]
use serde_json::json;
#[cfg(all(feature = "jira", feature = "bitbucket"))]
use worklogger_mcp::BitbucketConfiguration;
use worklogger_mcp::{
    Capability, ConfigurationStore, JiraConfiguration, JiraHoursConfiguration, McpConfiguration,
    ModuleConfiguration, ModuleId,
};
use worklogger_profile::OrganizationProfile;

static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
#[cfg(feature = "jira")]
fn enabled_tools_follow_the_configured_capabilities() {
    let mut capabilities = BTreeSet::new();
    capabilities.insert(Capability::ReadOwnTimeEntries);
    let configuration = configuration(ModuleConfiguration {
        enabled: true,
        capabilities,
    });

    assert!(configuration.capability_enabled(Capability::ReadOwnTimeEntries));
}

#[test]
fn disabling_the_module_exposes_no_tools() {
    let configuration = configuration(ModuleConfiguration {
        enabled: false,
        capabilities: BTreeSet::from([Capability::ReadOwnTimeEntries]),
    });

    assert!(!configuration.capability_enabled(Capability::ReadOwnTimeEntries));
}

#[test]
#[cfg(all(feature = "jira", feature = "bitbucket"))]
fn jira_and_bitbucket_capabilities_belong_to_their_own_modules() {
    let jira_capabilities = all_jira_issue_capabilities();
    let bitbucket_capabilities = all_bitbucket_capabilities();
    let modules = provider_modules(&jira_capabilities, &bitbucket_capabilities);
    let configuration = McpConfiguration::new_with_bitbucket(
        jira_configuration(),
        bitbucket_configuration(),
        modules,
    )
    .expect("provider modules are valid");

    for capability in jira_capabilities.into_iter().chain(bitbucket_capabilities) {
        assert!(configuration.capability_enabled(capability));
    }
}

#[cfg(all(feature = "jira", feature = "bitbucket"))]
fn all_jira_issue_capabilities() -> BTreeSet<Capability> {
    BTreeSet::from([
        Capability::ReadJiraIssues,
        Capability::EditJiraIssues,
        Capability::CommentJiraIssues,
        Capability::TransitionJiraIssues,
    ])
}

#[cfg(all(feature = "jira", feature = "bitbucket"))]
fn all_bitbucket_capabilities() -> BTreeSet<Capability> {
    BTreeSet::from([
        Capability::ReadBitbucketPullRequests,
        Capability::CreateBitbucketPullRequests,
        Capability::EditBitbucketPullRequests,
        Capability::CommentBitbucketPullRequests,
        Capability::ReviewBitbucketPullRequests,
        Capability::MergeBitbucketPullRequests,
        Capability::DeclineBitbucketPullRequests,
    ])
}

#[cfg(all(feature = "jira", feature = "bitbucket"))]
fn provider_modules(
    jira: &BTreeSet<Capability>,
    bitbucket: &BTreeSet<Capability>,
) -> BTreeMap<ModuleId, ModuleConfiguration> {
    BTreeMap::from([
        (ModuleId::Jira, enabled_module(jira.clone())),
        (ModuleId::Bitbucket, enabled_module(bitbucket.clone())),
    ])
}

#[cfg(all(feature = "jira", feature = "bitbucket"))]
fn enabled_module(capabilities: BTreeSet<Capability>) -> ModuleConfiguration {
    ModuleConfiguration {
        enabled: true,
        capabilities,
    }
}

#[test]
#[cfg(feature = "jira")]
fn jira_issue_capabilities_do_not_require_the_hours_capability() {
    let mut jira = jira_configuration();
    jira.hours = None;
    let module = ModuleConfiguration {
        enabled: true,
        capabilities: BTreeSet::from([Capability::ReadJiraIssues]),
    };

    let configuration = McpConfiguration::new(jira, BTreeMap::from([(ModuleId::Jira, module)]));

    assert!(configuration.is_ok());
}

#[test]
#[cfg(feature = "jira")]
fn jira_mutations_require_issue_read_capability() {
    let module = ModuleConfiguration {
        enabled: true,
        capabilities: BTreeSet::from([Capability::EditJiraIssues]),
    };

    let result = McpConfiguration::new(
        jira_configuration(),
        BTreeMap::from([(ModuleId::Jira, module)]),
    );

    assert!(matches!(
        result,
        Err(
            worklogger_mcp::ConfigurationError::MissingRequiredCapability {
                capability: Capability::EditJiraIssues,
                required: Capability::ReadJiraIssues,
            }
        )
    ));
}

#[test]
#[cfg(all(feature = "jira", feature = "bitbucket"))]
fn bitbucket_mutations_require_pull_request_read_capability() {
    let module = ModuleConfiguration {
        enabled: true,
        capabilities: BTreeSet::from([Capability::MergeBitbucketPullRequests]),
    };

    let result = McpConfiguration::new_with_bitbucket(
        jira_configuration(),
        bitbucket_configuration(),
        BTreeMap::from([(ModuleId::Bitbucket, module)]),
    );

    assert!(matches!(
        result,
        Err(
            worklogger_mcp::ConfigurationError::MissingRequiredCapability {
                capability: Capability::MergeBitbucketPullRequests,
                required: Capability::ReadBitbucketPullRequests,
            }
        )
    ));
}

#[test]
#[cfg(feature = "jira")]
fn disabling_read_also_disables_dependent_mutations() {
    let mut module = ModuleConfiguration {
        enabled: false,
        capabilities: BTreeSet::new(),
    };
    module.set_capability(Capability::EditJiraIssues, true);
    module.set_capability(Capability::ReadJiraIssues, false);

    assert!(module.capabilities.is_empty());
    assert!(!module.enabled);
}

#[test]
#[cfg(feature = "jira")]
fn saving_configuration_is_idempotent_and_never_contains_a_token() {
    let directory = TestDirectory::new();
    let store = ConfigurationStore::at(directory.path.join("mcp.json"));
    let configuration = configuration(ModuleConfiguration {
        enabled: true,
        capabilities: BTreeSet::from([Capability::ReadOwnTimeEntries]),
    });

    store.save(&configuration).expect("first save succeeds");
    store.save(&configuration).expect("second save succeeds");

    assert_eq!(store.load().expect("load succeeds"), Some(configuration));
    let contents = fs::read_to_string(store.path()).expect("configuration is readable");
    assert!(!contents.to_ascii_lowercase().contains("token"));
}

#[test]
fn clearing_configuration_is_idempotent() {
    let directory = TestDirectory::new();
    let store = ConfigurationStore::at(directory.path.join("mcp.json"));
    store.clear().expect("missing configuration is harmless");
    store
        .save(&configuration(ModuleConfiguration {
            enabled: false,
            capabilities: BTreeSet::new(),
        }))
        .expect("configuration is saved");
    store.clear().expect("configuration is removed");
    store.clear().expect("second clear is harmless");
    assert_eq!(store.load().expect("load succeeds"), None);
}

#[test]
#[cfg(all(feature = "jira", feature = "bitbucket"))]
fn documented_provider_configuration_is_valid_and_secret_free() {
    let contents = include_str!("../../../config/example.mcp.json");
    let configuration: McpConfiguration =
        serde_json::from_str(contents).expect("example configuration decodes");

    configuration
        .validate()
        .expect("example configuration validates");
    assert!(configuration.module_enabled(ModuleId::Jira));
    assert!(configuration.module_enabled(ModuleId::Bitbucket));
    assert!(!contents.to_ascii_lowercase().contains("token"));
}

#[test]
#[cfg(feature = "jira")]
fn documented_read_only_jira_configuration_is_valid_and_secret_free() {
    let contents = include_str!("../../../config/example.jira-readonly.mcp.json");
    let configuration: McpConfiguration =
        serde_json::from_str(contents).expect("read-only example configuration decodes");

    configuration
        .validate()
        .expect("read-only example configuration validates");
    assert!(configuration.capability_enabled(Capability::ReadJiraIssues));
    assert!(!contents.to_ascii_lowercase().contains("token"));
}

#[test]
fn complete_configuration_is_portable_across_addon_builds() {
    let directory = TestDirectory::new();
    let store = ConfigurationStore::at(directory.path.join("mcp.json"));
    fs::write(
        store.path(),
        include_str!("../../../config/example.mcp.json"),
    )
    .expect("full configuration fixture is written");

    let configuration = store
        .load()
        .expect("portable configuration loads")
        .expect("configuration exists")
        .apply_organization_profile(&organization_profile())
        .expect("compiled addons are selected");

    assert_eq!(
        configuration.module_enabled(ModuleId::Jira),
        cfg!(feature = "jira")
    );
    assert_eq!(
        configuration.module_enabled(ModuleId::Bitbucket),
        cfg!(feature = "bitbucket")
    );
}

#[test]
#[cfg(feature = "jira")]
fn organization_profile_removes_capabilities_that_are_not_allowed() {
    let mut profile = organization_profile();
    let jira = profile.modules.jira.as_mut().expect("Jira is configured");
    jira.mcp_capabilities.remove(&Capability::EditJiraIssues);
    let module = ModuleConfiguration {
        enabled: true,
        capabilities: BTreeSet::from([Capability::ReadJiraIssues, Capability::EditJiraIssues]),
    };
    let configuration = configuration(module)
        .apply_organization_profile(&profile)
        .expect("scope is valid");

    assert!(configuration.capability_enabled(Capability::ReadJiraIssues));
    assert!(!configuration.capability_enabled(Capability::EditJiraIssues));
}

#[test]
#[cfg(feature = "jira")]
fn absent_organization_module_disables_the_local_module() {
    let mut profile = organization_profile();
    profile.modules.jira = None;
    let mut jira = jira_configuration();
    jira.board_id = 999;
    let module = ModuleConfiguration {
        enabled: true,
        capabilities: BTreeSet::from([Capability::ReadJiraIssues]),
    };
    let configuration = McpConfiguration::new(jira, BTreeMap::from([(ModuleId::Jira, module)]))
        .expect("local configuration is valid")
        .apply_organization_profile(&profile)
        .expect("a disabled provider does not enforce its dormant scope");

    assert!(!configuration.module_enabled(ModuleId::Jira));
    assert!(!configuration.capability_enabled(Capability::ReadJiraIssues));
}

#[test]
#[cfg(feature = "jira")]
fn issue_only_configuration_ignores_dormant_hours_outside_profile() {
    let mut jira = jira_configuration();
    let hours = jira.hours.as_mut().expect("hours exist");
    hours.weekly_target_hours = 100;
    let module = ModuleConfiguration {
        enabled: true,
        capabilities: BTreeSet::from([Capability::ReadJiraIssues]),
    };
    let configuration = McpConfiguration::new(jira, BTreeMap::from([(ModuleId::Jira, module)]))
        .expect("local configuration is valid")
        .apply_organization_profile(&organization_profile())
        .expect("disabled hours do not constrain Jira issues");

    assert!(configuration.capability_enabled(Capability::ReadJiraIssues));
    assert!(!configuration.capability_enabled(Capability::ReadOwnTimeEntries));
}

#[test]
#[cfg(feature = "jira")]
fn organization_profile_rejects_a_jira_board_outside_its_scope() {
    let mut jira = jira_configuration();
    jira.board_id = 43;
    let module = ModuleConfiguration {
        enabled: true,
        capabilities: BTreeSet::from([Capability::ReadJiraIssues]),
    };
    let configuration = McpConfiguration::new(jira, BTreeMap::from([(ModuleId::Jira, module)]))
        .expect("configuration is valid");

    assert!(matches!(
        configuration.apply_organization_profile(&organization_profile()),
        Err(worklogger_mcp::ConfigurationError::OutsideOrganizationScope(ModuleId::Jira))
    ));
}

#[test]
#[cfg(feature = "jira")]
fn organization_profile_rejects_jira_limits_above_its_maxima() {
    let mut jira = jira_configuration();
    jira.request_timeout_seconds = 121;
    let module = ModuleConfiguration {
        enabled: true,
        capabilities: BTreeSet::from([Capability::ReadJiraIssues]),
    };
    let configuration = McpConfiguration::new(jira, BTreeMap::from([(ModuleId::Jira, module)]))
        .expect("configuration is locally valid");

    assert!(matches!(
        configuration.apply_organization_profile(&organization_profile()),
        Err(worklogger_mcp::ConfigurationError::OutsideOrganizationLimits(ModuleId::Jira))
    ));
}

#[test]
#[cfg(feature = "jira")]
fn organization_profile_rejects_jira_search_results_above_its_maximum() {
    let mut jira = jira_configuration();
    jira.maximum_issue_search_results = 1_001;
    let module = ModuleConfiguration {
        enabled: true,
        capabilities: BTreeSet::from([Capability::ReadJiraIssues]),
    };
    let configuration = McpConfiguration::new(jira, BTreeMap::from([(ModuleId::Jira, module)]))
        .expect("configuration is locally valid");

    assert!(matches!(
        configuration.apply_organization_profile(&organization_profile()),
        Err(worklogger_mcp::ConfigurationError::OutsideOrganizationLimits(ModuleId::Jira))
    ));
}

#[test]
#[cfg(all(feature = "jira", feature = "bitbucket"))]
fn organization_profile_rejects_a_repository_outside_its_scope() {
    let configuration = McpConfiguration::new_with_bitbucket(
        jira_configuration(),
        bitbucket_configuration(),
        provider_modules(
            &all_jira_issue_capabilities(),
            &all_bitbucket_capabilities(),
        ),
    )
    .expect("configuration is valid");

    assert!(matches!(
        configuration.apply_organization_profile(&organization_profile()),
        Err(worklogger_mcp::ConfigurationError::OutsideOrganizationScope(ModuleId::Bitbucket))
    ));
}

#[test]
#[cfg(all(feature = "jira", feature = "bitbucket"))]
fn organization_profile_rejects_bitbucket_limits_above_its_maxima() {
    let mut bitbucket = bitbucket_configuration();
    bitbucket.workspaces = BTreeMap::from([(
        "example-workspace".to_owned(),
        BTreeSet::from(["example-repository".to_owned()]),
    )]);
    bitbucket.page_size = 101;
    let configuration = McpConfiguration::new_with_bitbucket(
        jira_configuration(),
        bitbucket,
        provider_modules(
            &all_jira_issue_capabilities(),
            &all_bitbucket_capabilities(),
        ),
    )
    .expect("configuration is locally valid");

    assert!(matches!(
        configuration.apply_organization_profile(&organization_profile()),
        Err(worklogger_mcp::ConfigurationError::OutsideOrganizationLimits(ModuleId::Bitbucket))
    ));
}

#[test]
#[cfg(all(feature = "jira", feature = "bitbucket"))]
fn organization_profile_accepts_bitbucket_limits_above_defaults_within_maxima() {
    let mut bitbucket = bitbucket_configuration();
    bitbucket.workspaces = BTreeMap::from([(
        "example-workspace".to_owned(),
        BTreeSet::from(["example-repository".to_owned()]),
    )]);
    bitbucket.page_size = 100;
    bitbucket.maximum_collection_items = 2_000;
    let configuration = McpConfiguration::new_with_bitbucket(
        jira_configuration(),
        bitbucket,
        provider_modules(
            &all_jira_issue_capabilities(),
            &all_bitbucket_capabilities(),
        ),
    )
    .expect("configuration is locally valid");

    assert!(
        configuration
            .apply_organization_profile(&organization_profile())
            .is_ok()
    );
}

#[test]
#[cfg(feature = "jira")]
fn legacy_hours_capability_migrates_under_the_jira_module() {
    let directory = TestDirectory::new();
    let store = ConfigurationStore::at(directory.path.join("mcp.json"));
    fs::write(store.path(), legacy_configuration()).expect("legacy fixture is written");

    let configuration = store.load().expect("legacy configuration migrates");
    let configuration = configuration.expect("configuration exists");

    assert!(configuration.capability_enabled(Capability::ReadOwnTimeEntries));
    let jira = configuration.jira.expect("Jira configuration exists");
    assert_eq!(jira.maximum_issue_search_results, 1_000);
    assert!(jira.hours.is_some());
}

#[cfg(feature = "jira")]
fn legacy_configuration() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schemaVersion": 1,
        "jira": {
            "baseUrl": "https://example.atlassian.net", "email": "person@example.com",
            "boardId": 42, "weeklyTargetHours": 40, "utcOffsetMinutes": 0,
            "requestTimeoutSeconds": 30, "pageSize": 100, "maximumCollectionItems": 2000,
            "maximumConcurrentWorklogRequests": 8
        },
        "modules": {"jira": {"enabled": true, "capabilities": ["time-entry.read.self"]}}
    }))
    .expect("legacy fixture serializes")
}

fn organization_profile() -> OrganizationProfile {
    OrganizationProfile::from_json(include_str!("../../../config/example.organization.json"))
        .expect("organization profile is valid")
}

fn configuration(jira_module: ModuleConfiguration) -> McpConfiguration {
    McpConfiguration::new(
        jira_configuration(),
        BTreeMap::from([(ModuleId::Jira, jira_module)]),
    )
    .expect("fixture is valid")
}

fn jira_configuration() -> JiraConfiguration {
    JiraConfiguration {
        base_url: "https://example.atlassian.net".to_owned(),
        email: "person@example.com".to_owned(),
        board_id: 42,
        request_timeout_seconds: 30,
        page_size: 100,
        maximum_collection_items: 2_000,
        maximum_issue_search_results: 1_000,
        hours: Some(JiraHoursConfiguration {
            weekly_target_hours: 40,
            utc_offset_minutes: 0,
            maximum_concurrent_worklog_requests: 8,
        }),
    }
}

#[cfg(all(feature = "jira", feature = "bitbucket"))]
fn bitbucket_configuration() -> BitbucketConfiguration {
    BitbucketConfiguration {
        email: "person@example.com".to_owned(),
        workspaces: BTreeMap::from([(
            "workspace".to_owned(),
            BTreeSet::from(["repository".to_owned()]),
        )]),
        request_timeout_seconds: 30,
        page_size: 50,
        maximum_collection_items: 1_000,
        pull_request_defaults: worklogger_mcp::BitbucketPullRequestDefaults::default(),
    }
}

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn new() -> Self {
        let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let name = format!("worklogger-mcp-config-{}-{sequence}", std::process::id());
        let path = std::env::temp_dir().join(name);
        fs::create_dir_all(&path).expect("temporary directory is created");
        Self { path }
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _result = fs::remove_dir_all(&self.path);
    }
}
