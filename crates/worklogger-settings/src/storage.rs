use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use atomic_write_file::AtomicWriteFile;
use thiserror::Error;

use crate::SettingsDocument;

const MAXIMUM_BYTES: u64 = 1_048_576;
const SETTINGS_FILE: &str = "settings.json";

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("invalid shared settings field: {0}")]
    Invalid(&'static str),
    #[error("shared settings exceed the size limit")]
    TooLarge,
    #[error("shared settings JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("could not access shared settings: {0}")]
    Io(#[from] std::io::Error),
    #[error("shared settings are being edited; retry after the other operation completes")]
    Busy,
    #[error(
        "shared settings changed (expected revision {expected}, found {actual}); reload before saving"
    )]
    Conflict { expected: u64, actual: u64 },
    #[error("could not determine the user configuration directory")]
    MissingDirectory,
}

#[derive(Clone, Debug)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    #[must_use]
    pub const fn at(path: PathBuf) -> Self {
        Self { path }
    }

    /// Resolves the neutral per-user location; isolated installs may override it.
    ///
    /// # Errors
    /// Fails when no user configuration directory is available.
    pub fn for_current_user() -> Result<Self, SettingsError> {
        let path = environment_path("WORKLOGGER_SETTINGS_CONFIG")
            .or_else(platform_directory)
            .ok_or(SettingsError::MissingDirectory)?;
        Ok(Self::at(path))
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads a bounded, typed document. Missing settings are distinct from a draft.
    ///
    /// # Errors
    /// Rejects unreadable, malformed, oversized or invalid documents.
    pub fn load(&self) -> Result<Option<SettingsDocument>, SettingsError> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let mut bytes = Vec::new();
        file.take(MAXIMUM_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAXIMUM_BYTES {
            return Err(SettingsError::TooLarge);
        }
        let document: SettingsDocument = serde_json::from_slice(&bytes)?;
        document.validate()?;
        Ok(Some(document))
    }

    /// Atomically saves if no other frontend has changed the observed revision.
    /// Returns the committed document with its incremented revision.
    ///
    /// # Errors
    /// Rejects invalid data, concurrent edits, lock contention, or storage failure.
    pub fn save(
        &self,
        document: &SettingsDocument,
        expected_revision: u64,
    ) -> Result<SettingsDocument, SettingsError> {
        document.validate()?;
        create_parent(&self.path)?;
        let _lock = WriteLock::acquire(self.path.with_extension("json.lock"))?;
        let actual = self.load()?.map_or(0, |saved| saved.revision);
        if actual != expected_revision {
            return Err(SettingsError::Conflict {
                expected: expected_revision,
                actual,
            });
        }
        let mut committed = document.clone();
        committed.revision = actual
            .checked_add(1)
            .ok_or(SettingsError::Invalid("revision"))?;
        write_document(&self.path, &committed)?;
        Ok(committed)
    }
}

struct WriteLock {
    path: PathBuf,
    _file: File,
}

impl WriteLock {
    fn acquire(path: PathBuf) -> Result<Self, SettingsError> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    SettingsError::Busy
                } else {
                    error.into()
                }
            })?;
        restrict_file_permissions(&file)?;
        Ok(Self { path, _file: file })
    }
}

impl Drop for WriteLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn write_document(path: &Path, document: &SettingsDocument) -> Result<(), SettingsError> {
    let mut bytes = serde_json::to_vec_pretty(document)?;
    bytes.push(b'\n');
    if bytes.len() as u64 > MAXIMUM_BYTES {
        return Err(SettingsError::TooLarge);
    }
    let mut file = AtomicWriteFile::options().open(path)?;
    restrict_atomic_permissions(&file)?;
    file.write_all(&bytes)?;
    file.commit()?;
    Ok(())
}

fn restrict_file_permissions(file: &File) -> Result<(), SettingsError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    let _ = file;
    Ok(())
}

fn restrict_atomic_permissions(file: &AtomicWriteFile) -> Result<(), SettingsError> {
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

fn create_parent(path: &Path) -> Result<(), SettingsError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    Ok(())
}

fn environment_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(windows)]
fn platform_directory() -> Option<PathBuf> {
    environment_path("APPDATA").map(|path| path.join("Worklogger").join(SETTINGS_FILE))
}

#[cfg(not(windows))]
fn platform_directory() -> Option<PathBuf> {
    environment_path("XDG_CONFIG_HOME")
        .or_else(|| environment_path("HOME").map(|path| path.join(".config")))
        .map(|path| path.join("worklogger").join(SETTINGS_FILE))
}
