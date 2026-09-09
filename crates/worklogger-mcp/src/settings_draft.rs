//! Incomplete settings are kept separately from the executable MCP configuration.
//! Tokens and provider verification results intentionally have no representation here.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use worklogger_mcp::{BitbucketPullRequestDefaults, Capability, JiraHoursConfiguration};

const DRAFT_SUFFIX: &str = ".settings-draft.json";
const MAXIMUM_DRAFT_BYTES: usize = 1_048_576;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SettingsDraft {
    pub jira: Option<JiraSettingsDraft>,
    pub bitbucket: Option<BitbucketSettingsDraft>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct JiraSettingsDraft {
    pub base_url: Option<String>,
    pub email: Option<String>,
    pub board_id: Option<u64>,
    pub capabilities: BTreeSet<Capability>,
    pub hours: Option<JiraHoursConfiguration>,
    pub request_timeout_seconds: Option<u64>,
    pub page_size: Option<u16>,
    pub maximum_collection_items: Option<usize>,
    pub maximum_issue_search_results: Option<usize>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BitbucketSettingsDraft {
    pub email: Option<String>,
    pub workspaces: BTreeMap<String, BTreeSet<String>>,
    pub capabilities: BTreeSet<Capability>,
    pub pull_request_defaults: BitbucketPullRequestDefaults,
    pub request_timeout_seconds: Option<u64>,
    pub page_size: Option<u16>,
    pub maximum_collection_items: Option<usize>,
}

#[derive(Debug, Error)]
pub(crate) enum SettingsDraftError {
    #[error("the MCP configuration path must name a file")]
    InvalidPath,
    #[error("the settings draft exceeds the maximum allowed size")]
    TooLarge,
    #[error("the draft Jira site must be an HTTPS origin without credentials or parameters")]
    UnsafeSite,
    #[error("could not access the settings draft at {path}: {source}")]
    Storage {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("the settings draft JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
}

pub(crate) struct SettingsDraftStore {
    path: PathBuf,
}

impl SettingsDraftStore {
    pub fn for_configuration(configuration: &Path) -> Result<Self, SettingsDraftError> {
        let mut filename = configuration
            .file_name()
            .ok_or(SettingsDraftError::InvalidPath)?
            .to_os_string();
        filename.push(DRAFT_SUFFIX);
        Ok(Self {
            path: configuration.with_file_name(filename),
        })
    }

    #[cfg(test)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Option<SettingsDraft>, SettingsDraftError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(storage_error(&self.path, source)),
        };
        check_size(&bytes)?;
        let draft = serde_json::from_slice(&bytes)?;
        validate_draft(&draft)?;
        Ok(Some(draft))
    }

    pub fn save(&self, draft: &SettingsDraft) -> Result<(), SettingsDraftError> {
        validate_draft(draft)?;
        let bytes = serde_json::to_vec_pretty(draft)?;
        check_size(&bytes)?;
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|source| storage_error(&self.path, source))?;
        write_draft(&self.path, &bytes)
    }

    pub fn clear(&self) -> Result<(), SettingsDraftError> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(storage_error(&self.path, source)),
        }
    }
}

fn check_size(bytes: &[u8]) -> Result<(), SettingsDraftError> {
    if bytes.len() > MAXIMUM_DRAFT_BYTES {
        return Err(SettingsDraftError::TooLarge);
    }
    Ok(())
}

fn validate_draft(draft: &SettingsDraft) -> Result<(), SettingsDraftError> {
    let Some(site) = draft
        .jira
        .as_ref()
        .and_then(|jira| jira.base_url.as_deref())
    else {
        return Ok(());
    };
    let host = site
        .strip_prefix("https://")
        .unwrap_or_default()
        .trim_end_matches('/');
    if host.is_empty()
        || !host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-:".contains(&byte))
    {
        return Err(SettingsDraftError::UnsafeSite);
    }
    Ok(())
}

fn write_draft(path: &Path, bytes: &[u8]) -> Result<(), SettingsDraftError> {
    let mut file = AtomicWriteFile::open(path).map_err(|source| storage_error(path, source))?;
    restrict_permissions(&file).map_err(|source| storage_error(path, source))?;
    file.write_all(bytes)
        .map_err(|source| storage_error(path, source))?;
    file.commit().map_err(|source| storage_error(path, source))
}

#[cfg_attr(
    not(unix),
    expect(
        clippy::unnecessary_wraps,
        reason = "the Windows implementation matches the fallible Unix permission contract"
    )
)]
fn restrict_permissions(file: &AtomicWriteFile) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    let _ = file;
    Ok(())
}

fn storage_error(path: &Path, source: std::io::Error) -> SettingsDraftError {
    SettingsDraftError::Storage {
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn round_trip_incomplete_settings_without_touching_runtime_configuration() {
        let directory = TestDirectory::new();
        let configuration = directory.path.join("mcp.json");
        fs::write(&configuration, "existing runtime configuration").unwrap();
        let store = SettingsDraftStore::for_configuration(&configuration).unwrap();
        let mut draft = sample_draft();
        store.save(&draft).unwrap();
        assert_eq!(store.load().unwrap(), Some(draft.clone()));
        draft.jira.as_mut().unwrap().board_id = Some(17);
        store.save(&draft).unwrap();
        assert_eq!(store.load().unwrap(), Some(draft));
        assert_eq!(
            fs::read_to_string(configuration).unwrap(),
            "existing runtime configuration"
        );
        assert_eq!(fs::read_dir(&directory.path).unwrap().count(), 2);
    }

    #[test]
    fn missing_draft_loads_and_clears_without_creating_files() {
        let directory = TestDirectory::new();
        let store =
            SettingsDraftStore::for_configuration(&directory.path.join("mcp.json")).unwrap();
        assert_eq!(store.load().unwrap(), None);
        store.clear().unwrap();
        store.save(&sample_draft()).unwrap();
        store.clear().unwrap();
        store.clear().unwrap();
        assert_eq!(store.load().unwrap(), None);
        assert_eq!(fs::read_dir(directory.path.as_path()).unwrap().count(), 0);
    }

    #[test]
    fn serialized_fields_exclude_tokens_credentials_and_verification() {
        let serialized = serde_json::to_string(&sample_draft()).unwrap();
        for forbidden in ["token", "password", "credential", "secret", "verified"] {
            assert!(!serialized.to_lowercase().contains(forbidden));
        }
        let mut value = serde_json::to_value(sample_draft()).unwrap();
        value["jira"]["apiToken"] = serde_json::json!("must not be accepted");
        assert!(serde_json::from_value::<SettingsDraft>(value).is_err());
    }

    #[test]
    fn site_credentials_and_parameters_are_never_saved() {
        let directory = TestDirectory::new();
        let store =
            SettingsDraftStore::for_configuration(&directory.path.join("mcp.json")).unwrap();
        for site in [
            "https://user:secret@example.atlassian.net",
            "https://example.atlassian.net/?token=secret",
        ] {
            let mut draft = sample_draft();
            draft.jira.as_mut().unwrap().base_url = Some(site.into());
            assert!(matches!(
                store.save(&draft),
                Err(SettingsDraftError::UnsafeSite)
            ));
        }
        assert!(!store.path().exists());
    }

    #[test]
    fn different_configuration_filenames_have_separate_drafts() {
        let first = SettingsDraftStore::for_configuration(Path::new("/example/mcp.json")).unwrap();
        let second = SettingsDraftStore::for_configuration(Path::new("/example/mcp.toml")).unwrap();
        assert_ne!(first.path(), second.path());
        assert_eq!(
            first.path(),
            Path::new("/example/mcp.json.settings-draft.json")
        );
    }

    #[test]
    #[cfg(unix)]
    fn persisted_draft_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;
        let directory = TestDirectory::new();
        let store =
            SettingsDraftStore::for_configuration(&directory.path.join("mcp.json")).unwrap();
        store.save(&sample_draft()).unwrap();
        assert_eq!(
            fs::metadata(store.path()).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    fn sample_draft() -> SettingsDraft {
        SettingsDraft {
            jira: Some(JiraSettingsDraft {
                base_url: Some("https://example.atlassian.net".into()),
                email: Some("person@example.com".into()),
                capabilities: BTreeSet::from([Capability::ReadOwnTimeEntries]),
                ..JiraSettingsDraft::default()
            }),
            bitbucket: Some(BitbucketSettingsDraft::default()),
        }
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
            let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "worklogger-settings-draft-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.path).unwrap();
        }
    }
}
