use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use atomic_write_file::AtomicWriteFile;
use thiserror::Error;

use crate::{OrganizationProfile, ProfileError};

const PROFILE_ENVIRONMENT_VARIABLE: &str = "WORKLOGGER_CONFIG";
const PROFILE_FILE_NAME: &str = "organization.json";
#[cfg(windows)]
const APPLICATION_DIRECTORY: &str = "Worklogger";
#[cfg(not(windows))]
const UNIX_APPLICATION_DIRECTORY: &str = "worklogger";
const MAXIMUM_PROFILE_BYTES: usize = 1_048_576;

#[derive(Clone, Debug)]
pub struct OrganizationProfileStore {
    path: PathBuf,
}

#[derive(Debug, Error)]
pub enum ProfileStorageError {
    #[error("could not determine the user configuration directory")]
    MissingUserConfigurationDirectory,
    #[error("the profile exceeds the maximum allowed size")]
    TooLarge,
    #[error("no se pudo acceder al perfil en {path}: {source}")]
    Storage {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Profile(#[from] ProfileError),
}

impl OrganizationProfileStore {
    /// Resolves the shared per-user organization profile path.
    ///
    /// # Errors
    ///
    /// Returns an error when the operating-system configuration directory is unavailable.
    pub fn for_current_user() -> Result<Self, ProfileStorageError> {
        if let Some(path) = environment_path() {
            return Ok(Self::at(path));
        }
        let directory = user_configuration_directory()
            .ok_or(ProfileStorageError::MissingUserConfigurationDirectory)?;
        Ok(Self::at(directory.join(PROFILE_FILE_NAME)))
    }

    #[must_use]
    pub const fn at(path: PathBuf) -> Self {
        Self { path }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Loads and validates the profile when it exists.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be read or validated.
    pub fn load(&self) -> Result<Option<OrganizationProfile>, ProfileStorageError> {
        if !self.path.exists() {
            return Ok(None);
        }
        read_profile(&self.path).map(Some)
    }

    /// Reads and validates a selected profile without installing it.
    ///
    /// # Errors
    ///
    /// Returns an error when the selected file cannot be read or validated.
    pub fn read_from(source: &Path) -> Result<OrganizationProfile, ProfileStorageError> {
        read_profile(source)
    }

    /// Validates and atomically installs a selected profile.
    ///
    /// # Errors
    ///
    /// Returns an error without changing the destination when validation fails.
    pub fn install_from(&self, source: &Path) -> Result<OrganizationProfile, ProfileStorageError> {
        let profile = read_profile(source)?;
        self.save(&profile)?;
        Ok(profile)
    }

    /// Atomically stores the canonical modular representation.
    ///
    /// # Errors
    ///
    /// Returns an error when the destination cannot be created or written.
    pub fn save(&self, profile: &OrganizationProfile) -> Result<(), ProfileStorageError> {
        profile.validate()?;
        let document = profile.to_pretty_json()?;
        create_parent(&self.path)?;
        write_atomically(&self.path, document.as_bytes())
    }
}

fn environment_path() -> Option<PathBuf> {
    env::var_os(PROFILE_ENVIRONMENT_VARIABLE)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(windows)]
fn user_configuration_directory() -> Option<PathBuf> {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|directory| directory.join(APPLICATION_DIRECTORY))
}

#[cfg(not(windows))]
fn user_configuration_directory() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|value| PathBuf::from(value).join(".config")))
        .map(|directory| directory.join(UNIX_APPLICATION_DIRECTORY))
}

fn read_profile(path: &Path) -> Result<OrganizationProfile, ProfileStorageError> {
    let bytes = fs::read(path).map_err(|source| storage_error(path, source))?;
    if bytes.len() > MAXIMUM_PROFILE_BYTES {
        return Err(ProfileStorageError::TooLarge);
    }
    let contents = String::from_utf8(bytes).map_err(|error| invalid_encoding(path, error))?;
    OrganizationProfile::from_json(&contents).map_err(ProfileStorageError::from)
}

fn invalid_encoding(path: &Path, error: std::string::FromUtf8Error) -> ProfileStorageError {
    let source = std::io::Error::new(std::io::ErrorKind::InvalidData, error);
    storage_error(path, source)
}

fn create_parent(path: &Path) -> Result<(), ProfileStorageError> {
    let parent = path.parent().ok_or_else(|| invalid_parent_error(path))?;
    fs::create_dir_all(parent).map_err(|source| storage_error(path, source))
}

fn invalid_parent_error(path: &Path) -> ProfileStorageError {
    let source = std::io::Error::new(std::io::ErrorKind::InvalidInput, "ruta sin directorio");
    storage_error(path, source)
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), ProfileStorageError> {
    let mut file = AtomicWriteFile::options()
        .open(path)
        .map_err(|source| storage_error(path, source))?;
    file.write_all(bytes)
        .map_err(|source| storage_error(path, source))?;
    file.commit().map_err(|source| storage_error(path, source))
}

fn storage_error(path: &Path, source: std::io::Error) -> ProfileStorageError {
    ProfileStorageError::Storage {
        path: path.to_path_buf(),
        source,
    }
}
