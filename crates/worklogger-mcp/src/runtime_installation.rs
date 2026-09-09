use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use atomic_write_file::AtomicWriteFile;
use thiserror::Error;

const LOCAL_APP_DATA_ENVIRONMENT_VARIABLE: &str = "LOCALAPPDATA";
const XDG_DATA_HOME_ENVIRONMENT_VARIABLE: &str = "XDG_DATA_HOME";
const HOME_ENVIRONMENT_VARIABLE: &str = "HOME";
const INSTALLATION_PATH_ENVIRONMENT_VARIABLE: &str = "WORKLOGGER_MCP_INSTALL_PATH";
const WINDOWS_PRODUCT_DIRECTORY: &str = "Worklogger";
const UNIX_PRODUCT_DIRECTORY: &str = "worklogger";
const MCP_DIRECTORY: &str = "MCP";
const UNIX_LOCAL_DATA_DIRECTORY: &str = ".local/share";
const WINDOWS_EXECUTABLE_NAME: &str = "worklogger-mcp.exe";
const UNIX_EXECUTABLE_NAME: &str = "worklogger-mcp";

pub(crate) fn is_owned_versioned_executable(candidate: &Path, current: &Path) -> bool {
    candidate.file_name() == current.file_name()
        && installation_root(candidate) == installation_root(current)
        && installation_root(current).is_some()
}

fn installation_root(executable: &Path) -> Option<&Path> {
    let root = executable.parent()?.parent()?;
    (root.file_name()? == MCP_DIRECTORY).then_some(root)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct McpServerInstallation {
    executable: PathBuf,
}

#[derive(Debug, Error)]
pub enum RuntimeInstallationError {
    #[error("no se pudo determinar el directorio de datos del usuario")]
    MissingUserDataDirectory,
    #[error("el ejecutable MCP de origen no existe o no es una ruta absoluta: {0}")]
    InvalidSource(PathBuf),
    #[error("la ruta de instalación MCP no es absoluta: {0}")]
    InvalidDestination(PathBuf),
    #[error("no se pudo instalar el ejecutable MCP en {path}: {source}")]
    Storage {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl McpServerInstallation {
    /// Resolves the versioned per-user installation used by every Worklogger surface.
    ///
    /// # Errors
    ///
    /// Returns an error when the operating system does not expose a user data directory.
    pub fn for_current_user() -> Result<Self, RuntimeInstallationError> {
        if let Some(executable) = environment_path(INSTALLATION_PATH_ENVIRONMENT_VARIABLE) {
            if !executable.is_absolute() {
                return Err(RuntimeInstallationError::InvalidDestination(executable));
            }
            return Ok(Self::at(executable));
        }
        user_data_directory()
            .map(|directory| Self::at(versioned_executable(&directory)))
            .ok_or(RuntimeInstallationError::MissingUserDataDirectory)
    }

    #[must_use]
    pub const fn at(executable: PathBuf) -> Self {
        Self { executable }
    }

    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Copies a bundled or downloaded server into the stable versioned location.
    ///
    /// # Errors
    ///
    /// Returns an error when the source is invalid or the destination cannot be updated safely.
    pub fn install(&self, source: &Path) -> Result<PathBuf, RuntimeInstallationError> {
        validate_source(source)?;
        if source == self.executable {
            return Ok(self.executable.clone());
        }
        if files_match(source, &self.executable)? {
            copy_permissions(source, &self.executable)?;
            return Ok(self.executable.clone());
        }
        let bytes = fs::read(source).map_err(|source_error| storage(source, source_error))?;
        write_executable(&self.executable, &bytes)?;
        copy_permissions(source, &self.executable)?;
        Ok(self.executable.clone())
    }
}

fn user_data_directory() -> Option<PathBuf> {
    if cfg!(windows) {
        return environment_path(LOCAL_APP_DATA_ENVIRONMENT_VARIABLE)
            .map(|directory| directory.join(WINDOWS_PRODUCT_DIRECTORY));
    }
    environment_path(XDG_DATA_HOME_ENVIRONMENT_VARIABLE)
        .or_else(unix_default_data_directory)
        .map(|directory| directory.join(UNIX_PRODUCT_DIRECTORY))
}

fn unix_default_data_directory() -> Option<PathBuf> {
    environment_path(HOME_ENVIRONMENT_VARIABLE)
        .map(|directory| directory.join(UNIX_LOCAL_DATA_DIRECTORY))
}

fn environment_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn versioned_executable(directory: &Path) -> PathBuf {
    directory
        .join(MCP_DIRECTORY)
        .join(env!("CARGO_PKG_VERSION"))
        .join(executable_name())
}

const fn executable_name() -> &'static str {
    if cfg!(windows) {
        WINDOWS_EXECUTABLE_NAME
    } else {
        UNIX_EXECUTABLE_NAME
    }
}

fn validate_source(source: &Path) -> Result<(), RuntimeInstallationError> {
    if source.is_absolute() && source.is_file() {
        return Ok(());
    }
    Err(RuntimeInstallationError::InvalidSource(
        source.to_path_buf(),
    ))
}

fn files_match(left: &Path, right: &Path) -> Result<bool, RuntimeInstallationError> {
    let left_metadata = fs::metadata(left).map_err(|source| storage(left, source))?;
    let right_metadata = match fs::metadata(right) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(source) => return Err(storage(right, source)),
    };
    if left_metadata.len() != right_metadata.len() {
        return Ok(false);
    }
    let left_bytes = fs::read(left).map_err(|source| storage(left, source))?;
    let right_bytes = fs::read(right).map_err(|source| storage(right, source))?;
    Ok(left_bytes == right_bytes)
}

fn write_executable(path: &Path, bytes: &[u8]) -> Result<(), RuntimeInstallationError> {
    let parent = path
        .parent()
        .ok_or_else(|| RuntimeInstallationError::InvalidSource(path.to_path_buf()))?;
    fs::create_dir_all(parent).map_err(|source| storage(path, source))?;
    let mut file = AtomicWriteFile::options()
        .open(path)
        .map_err(|source| storage(path, source))?;
    file.write_all(bytes)
        .map_err(|source| storage(path, source))?;
    file.commit().map_err(|source| storage(path, source))
}

fn copy_permissions(source: &Path, destination: &Path) -> Result<(), RuntimeInstallationError> {
    let permissions = fs::metadata(source)
        .map_err(|source_error| storage(source, source_error))?
        .permissions();
    fs::set_permissions(destination, permissions)
        .map_err(|source_error| storage(destination, source_error))
}

fn storage(path: &Path, source: std::io::Error) -> RuntimeInstallationError {
    RuntimeInstallationError::Storage {
        path: path.to_path_buf(),
        source,
    }
}
