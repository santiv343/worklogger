use worklogger_profile::{
    Capability, IntegrationModuleId, OrganizationProfile, OrganizationProfileStore, ProfileError,
    ProviderScopeMode,
};

const LEGACY_PROFILE: &str = include_str!("fixtures/organization-v1.json");
const MODULAR_PROFILE: &str = include_str!("../../../config/example.organization.json");

#[test]
fn reads_a_modular_profile_with_optional_modules() {
    let profile = OrganizationProfile::from_json(MODULAR_PROFILE).expect("profile is valid");

    assert_eq!(profile.schema_version, 2);
    assert!(profile.modules.jira.is_some());
    assert!(profile.modules.bitbucket.is_some());
    assert!(profile.modules.reports.is_some());
    assert!(profile.allows(Capability::ReadOwnTimeEntries));
}

#[test]
fn migrates_the_flat_profile_without_inventing_new_modules() {
    let profile = OrganizationProfile::from_json(LEGACY_PROFILE).expect("legacy profile is valid");
    let jira = profile.modules.jira.expect("Jira is migrated");

    assert_eq!(profile.schema_version, 2);
    assert_eq!(jira.hours.suggested_weekly_target_hours, 40);
    assert!(profile.modules.bitbucket.is_none());
    assert!(profile.modules.reports.is_some());
}

#[test]
fn exported_profile_never_contains_personal_credentials() {
    let profile = OrganizationProfile::from_json(MODULAR_PROFILE).expect("profile is valid");
    let exported = profile.to_pretty_json().expect("profile serializes");

    assert!(!exported.contains("email"));
    assert!(!exported.contains("token"));
    assert!(!exported.contains("password"));
}

#[test]
fn rejects_a_capability_under_the_wrong_module() {
    let invalid = MODULAR_PROFILE.replace("\"jira.hours.read.self\"", "\"bitbucket.pr.read\"");

    assert!(matches!(
        OrganizationProfile::from_json(&invalid),
        Err(ProfileError::CapabilityModuleMismatch {
            expected: IntegrationModuleId::Jira,
            actual: IntegrationModuleId::Bitbucket,
        })
    ));
}

#[test]
fn rejects_unknown_modules_instead_of_partially_applying_them() {
    let invalid = MODULAR_PROFILE.replace("\"reports\": {", "\"calendar\": {}, \"reports\": {");

    assert!(matches!(
        OrganizationProfile::from_json(&invalid),
        Err(ProfileError::Decode(_))
    ));
}

#[test]
fn rejects_write_capabilities_without_their_required_read_capability() {
    let invalid = MODULAR_PROFILE.replace("\"bitbucket.pr.read\",", "");

    assert!(matches!(
        OrganizationProfile::from_json(&invalid),
        Err(ProfileError::MissingRequiredCapability {
            capability: Capability::CreateBitbucketPullRequests,
            required: Capability::ReadBitbucketPullRequests,
        })
    ));
}

#[test]
fn writing_own_hours_requires_reading_own_hours() {
    assert_eq!(
        Capability::WriteOwnTimeEntries.required_read(),
        Some(Capability::ReadOwnTimeEntries)
    );
}

#[test]
fn absent_module_denies_its_capabilities() {
    let mut profile = OrganizationProfile::from_json(MODULAR_PROFILE).expect("profile is valid");
    profile.modules.bitbucket = None;

    assert!(profile.validate().is_ok());
    assert!(!profile.allows(Capability::ReadBitbucketPullRequests));
    assert!(!profile.allows_bitbucket_repository("example-workspace", "example-repository"));
}

#[test]
fn jira_scope_normalizes_site_urls_and_rejects_other_boards() {
    let profile = OrganizationProfile::from_json(MODULAR_PROFILE).expect("profile is valid");

    assert!(profile.allows_jira_board("https://EXAMPLE.atlassian.net/", 42));
    assert!(!profile.allows_jira_board("https://example.atlassian.net", 43));
}

#[test]
fn unrestricted_scope_must_be_explicit_and_cannot_hide_restrictions() {
    let mut invalid = OrganizationProfile::from_json(MODULAR_PROFILE).expect("profile is valid");
    invalid
        .modules
        .jira
        .as_mut()
        .expect("Jira exists")
        .scope_mode = ProviderScopeMode::Unrestricted;

    assert!(matches!(
        invalid.validate(),
        Err(ProfileError::InvalidJira("sites"))
    ));
}

#[test]
fn rejects_logo_urls_with_credentials_or_dynamic_parts() {
    let with_credentials = MODULAR_PROFILE.replace(
        "\"logoUrl\": null",
        "\"logoUrl\": \"https://secret@example.com/logo.svg\"",
    );
    let with_query = MODULAR_PROFILE.replace(
        "\"logoUrl\": null",
        "\"logoUrl\": \"https://example.com/logo.svg?token=secret\"",
    );

    assert!(matches!(
        OrganizationProfile::from_json(&with_credentials),
        Err(ProfileError::InvalidBranding)
    ));
    assert!(matches!(
        OrganizationProfile::from_json(&with_query),
        Err(ProfileError::InvalidBranding)
    ));
}

#[test]
fn installs_a_legacy_profile_as_canonical_modular_json() {
    let directory = temporary_directory("install");
    let source = directory.join("legacy.json");
    let destination = directory.join("private/organization.json");
    std::fs::create_dir_all(&directory).expect("temporary directory is created");
    std::fs::write(&source, LEGACY_PROFILE).expect("legacy fixture is written");

    let store = OrganizationProfileStore::at(destination.clone());
    store.install_from(&source).expect("profile is installed");
    let installed = std::fs::read_to_string(destination).expect("installed profile is readable");

    assert!(installed.contains("\"schemaVersion\": 2"));
    assert!(installed.contains("\"modules\""));
    assert!(!installed.contains("\"email\""));
    std::fs::remove_dir_all(directory).expect("temporary directory is removed");
}

#[test]
fn reads_a_selected_profile_without_installing_it() {
    let directory = temporary_directory("read-only");
    let source = directory.join("selected.json");
    let destination = directory.join("private/organization.json");
    std::fs::create_dir_all(&directory).expect("temporary directory is created");
    std::fs::write(&source, MODULAR_PROFILE).expect("profile fixture is written");

    let profile = OrganizationProfileStore::read_from(&source).expect("profile is valid");
    let store = OrganizationProfileStore::at(destination.clone());

    assert_eq!(profile.schema_version, 2);
    assert_eq!(store.load().expect("destination can be inspected"), None);
    assert!(!destination.exists());
    std::fs::remove_dir_all(directory).expect("temporary directory is removed");
}

fn temporary_directory(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("worklogger-profile-{label}-{}", std::process::id()))
}
