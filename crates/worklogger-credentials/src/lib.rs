//! Secure credentials shared by Worklogger surfaces.

use thiserror::Error;

#[cfg(any(windows, target_os = "linux", test))]
const MAX_EMAIL_LENGTH: usize = 254;
#[cfg(any(windows, target_os = "linux", test))]
const MAX_SITE_LENGTH: usize = 255;
#[cfg(any(windows, target_os = "linux", test))]
const MAX_TOKEN_LENGTH: usize = 4_096;
#[cfg(any(windows, target_os = "linux", test))]
const DESKTOP_SERVICE_NAME: &str = "com.worklogger.jira.desktop";
#[cfg(any(windows, target_os = "linux", test))]
const MCP_SERVICE_NAME: &str = "com.worklogger.mcp";
#[cfg(windows)]
const LEGACY_SERVICE_NAME: &str = "com.worklogger.jira";
#[cfg(windows)]
const LEGACY_MCP_SERVICE_NAME: &str = "com.worklogger.jira.mcp";
#[cfg(any(windows, target_os = "linux", test))]
const ACCOUNT_SEPARATOR: char = '|';

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum CredentialError {
    #[error("el origen de la conexión no tiene un formato válido")]
    InvalidSite,
    #[error("el correo de la cuenta no tiene un formato válido")]
    InvalidEmail,
    #[error("el API token está vacío o supera el tamaño permitido")]
    InvalidToken,
    #[error("el almacén seguro del sistema operativo no está disponible")]
    Unavailable,
    #[error("no se pudo guardar el API token")]
    SaveFailed,
    #[error("no se pudo leer el API token")]
    ReadFailed,
    #[error("no se pudo eliminar el API token")]
    DeleteFailed,
    #[error("otra instancia está actualizando la configuración segura")]
    TransactionBusy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialPurpose {
    Desktop,
    Mcp,
}

impl CredentialPurpose {
    #[cfg(any(windows, target_os = "linux", test))]
    const fn service_name(self) -> &'static str {
        match self {
            Self::Desktop => DESKTOP_SERVICE_NAME,
            Self::Mcp => MCP_SERVICE_NAME,
        }
    }

    #[cfg(windows)]
    const fn legacy_service_name(self) -> &'static str {
        match self {
            Self::Desktop => LEGACY_SERVICE_NAME,
            Self::Mcp => LEGACY_MCP_SERVICE_NAME,
        }
    }
}

#[cfg(windows)]
mod platform {
    use std::fs::{File, OpenOptions, create_dir_all};
    use std::os::windows::fs::OpenOptionsExt;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    use keyring_core::{Entry, Error as KeyringError};
    use windows_native_keyring_store::Store;

    use super::{CredentialError, CredentialPurpose, credential_account, validate_token};

    static CREDENTIAL_LOCK: Mutex<()> = Mutex::new(());
    static INITIALIZED: OnceLock<Result<(), CredentialError>> = OnceLock::new();
    const LOCAL_APP_DATA_VARIABLE: &str = "LOCALAPPDATA";
    const APPLICATION_DIRECTORY: &str = "Worklogger";
    const TRANSACTION_LOCK_FILE: &str = "credential-transaction.lock";
    const EXCLUSIVE_SHARE_MODE: u32 = 0;
    const WINDOWS_SHARING_VIOLATION: i32 = 32;

    #[derive(Debug)]
    pub struct CredentialTransactionGuard {
        _file: File,
    }

    impl CredentialTransactionGuard {
        /// Prevents another Worklogger process from changing related credentials concurrently.
        ///
        /// # Errors
        ///
        /// Returns an error when the per-user exclusive lock cannot be acquired.
        pub fn acquire() -> Result<Self, CredentialError> {
            let directory = transaction_directory()?;
            create_dir_all(&directory).map_err(|_| CredentialError::Unavailable)?;
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .share_mode(EXCLUSIVE_SHARE_MODE)
                .open(directory.join(TRANSACTION_LOCK_FILE))
                .map_err(|error| transaction_error(&error))?;
            Ok(Self { _file: file })
        }
    }

    fn transaction_error(error: &std::io::Error) -> CredentialError {
        if error.raw_os_error() == Some(WINDOWS_SHARING_VIOLATION) {
            return CredentialError::TransactionBusy;
        }
        CredentialError::Unavailable
    }

    fn transaction_directory() -> Result<PathBuf, CredentialError> {
        std::env::var_os(LOCAL_APP_DATA_VARIABLE)
            .map(PathBuf::from)
            .map(|root| root.join(APPLICATION_DIRECTORY))
            .ok_or(CredentialError::Unavailable)
    }

    #[derive(Clone, Copy, Debug)]
    pub struct CredentialStore {
        service_name: &'static str,
        legacy_service_name: &'static str,
    }

    impl CredentialStore {
        /// Opens the native Windows credential store.
        ///
        /// # Errors
        ///
        /// Returns an error when Windows Credential Manager is unavailable.
        pub fn for_purpose(purpose: CredentialPurpose) -> Result<Self, CredentialError> {
            (*INITIALIZED.get_or_init(configure_store))?;
            Ok(Self {
                service_name: purpose.service_name(),
                legacy_service_name: purpose.legacy_service_name(),
            })
        }

        /// Saves one token under a site-and-email key.
        ///
        /// # Errors
        ///
        /// Returns an error for invalid input or a native store failure.
        pub fn save_api_token(
            self,
            site: &str,
            email: &str,
            token: &str,
        ) -> Result<(), CredentialError> {
            validate_token(token)?;
            let account = credential_account(site, email)?;
            with_entry(self.service_name, &account, |entry| {
                entry
                    .set_password(token)
                    .map_err(|_| CredentialError::SaveFailed)
            })
        }

        /// Loads one token under a site-and-email key.
        ///
        /// # Errors
        ///
        /// Returns an error for invalid input or a native store failure.
        pub fn load_api_token(
            self,
            site: &str,
            email: &str,
        ) -> Result<Option<String>, CredentialError> {
            let account = credential_account(site, email)?;
            let token = with_entry(self.service_name, &account, read_entry)?;
            match token {
                Some(value) => Ok(Some(value)),
                None => with_entry(self.legacy_service_name, &account, read_entry),
            }
        }

        /// Deletes one token. A missing token is accepted.
        ///
        /// # Errors
        ///
        /// Returns an error for invalid input or a native store failure.
        pub fn delete_api_token(self, site: &str, email: &str) -> Result<(), CredentialError> {
            let account = credential_account(site, email)?;
            with_entry(self.service_name, &account, delete_entry)?;
            with_entry(self.legacy_service_name, &account, delete_entry)?;
            Ok(())
        }
    }

    fn read_entry(entry: &Entry) -> Result<Option<String>, CredentialError> {
        match entry.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(_) => Err(CredentialError::ReadFailed),
        }
    }

    fn delete_entry(entry: &Entry) -> Result<(), CredentialError> {
        match entry.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(_) => Err(CredentialError::DeleteFailed),
        }
    }

    fn with_entry<T>(
        service_name: &str,
        account: &str,
        operation: impl FnOnce(&Entry) -> Result<T, CredentialError>,
    ) -> Result<T, CredentialError> {
        let _guard = CREDENTIAL_LOCK
            .lock()
            .map_err(|_| CredentialError::Unavailable)?;
        let entry = Entry::new(service_name, account).map_err(|_| CredentialError::Unavailable)?;
        operation(&entry)
    }

    fn configure_store() -> Result<(), CredentialError> {
        let store = Store::new().map_err(|_| CredentialError::Unavailable)?;
        keyring_core::set_default_store(store);
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;

    use atomic_write_file::AtomicWriteFile;
    use atomic_write_file::unix::OpenOptionsExt as AtomicOpenOptionsExt;
    use nix::fcntl::{Flock, FlockArg, OFlag};
    use sha2::{Digest, Sha256};

    use super::{CredentialError, CredentialPurpose, credential_account, validate_token};

    const DIRECTORY_MODE: u32 = 0o700;
    const FILE_MODE: u32 = 0o600;
    const PRIVATE_PERMISSION_MASK: u32 = 0o077;
    const XDG_DATA_HOME_ENVIRONMENT_VARIABLE: &str = "XDG_DATA_HOME";
    const HOME_ENVIRONMENT_VARIABLE: &str = "HOME";
    const LOCAL_DATA_DIRECTORY: &str = ".local/share";
    const PRODUCT_DIRECTORY: &str = "worklogger";
    const CREDENTIAL_DIRECTORY: &str = "credentials";
    const TRANSACTION_LOCK_FILE: &str = "transaction.lock";
    const UNSAFE_CREDENTIAL_PATH: &str = "unsafe credential path";
    static CURRENT_USER_DIRECTORY: OnceLock<Option<PathBuf>> = OnceLock::new();

    #[derive(Clone, Copy, Debug)]
    pub struct CredentialStore {
        service_name: &'static str,
        directory: &'static Path,
    }

    #[derive(Debug)]
    pub struct CredentialTransactionGuard {
        _lock: Flock<fs::File>,
    }

    impl CredentialTransactionGuard {
        /// Locks credential and configuration changes for the current Linux user.
        ///
        /// # Errors
        ///
        /// Returns an error when the lock is unavailable or already held.
        pub fn acquire() -> Result<Self, CredentialError> {
            let directory = current_user_directory().ok_or(CredentialError::Unavailable)?;
            Self::acquire_at(directory)
        }

        fn acquire_at(directory: &Path) -> Result<Self, CredentialError> {
            ensure_directory(directory).map_err(|_| CredentialError::Unavailable)?;
            let lock_path = directory.join(TRANSACTION_LOCK_FILE);
            let file = open_transaction_lock(&lock_path)?;
            let lock = Flock::lock(file, FlockArg::LockExclusiveNonblock)
                .map_err(|_| CredentialError::TransactionBusy)?;
            Ok(Self { _lock: lock })
        }
    }

    fn open_transaction_lock(path: &Path) -> Result<fs::File, CredentialError> {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(FILE_MODE)
            .custom_flags(OFlag::O_NOFOLLOW.bits())
            .open(path)
            .map_err(|_| CredentialError::Unavailable)?;
        protect_open_file(&file)?;
        Ok(file)
    }

    fn protect_open_file(file: &fs::File) -> Result<(), CredentialError> {
        file.set_permissions(fs::Permissions::from_mode(FILE_MODE))
            .map_err(|_| CredentialError::Unavailable)?;
        let metadata = file.metadata().map_err(|_| CredentialError::Unavailable)?;
        if metadata.file_type().is_file() && is_private(&metadata) {
            return Ok(());
        }
        Err(CredentialError::Unavailable)
    }

    impl CredentialStore {
        /// Opens the protected credential store for the current Linux user.
        ///
        /// # Errors
        ///
        /// Returns an error when no per-user data directory can be resolved.
        pub fn for_purpose(purpose: CredentialPurpose) -> Result<Self, CredentialError> {
            let directory = current_user_directory().ok_or(CredentialError::Unavailable)?;
            Ok(Self::at(purpose, directory))
        }

        #[must_use]
        const fn at(purpose: CredentialPurpose, directory: &'static Path) -> Self {
            Self {
                service_name: purpose.service_name(),
                directory,
            }
        }

        /// Saves a token atomically with owner-only permissions.
        ///
        /// # Errors
        ///
        /// Returns an error for invalid input or protected-store failures.
        pub fn save_api_token(
            self,
            site: &str,
            email: &str,
            token: &str,
        ) -> Result<(), CredentialError> {
            validate_token(token)?;
            let account = credential_account(site, email)?;
            ensure_directory(self.directory)?;
            write_token(&self.path(&account), token)
        }

        /// Loads a token from the owner-only credential store.
        ///
        /// # Errors
        ///
        /// Returns an error for invalid input or an unsafe credential path.
        pub fn load_api_token(
            self,
            site: &str,
            email: &str,
        ) -> Result<Option<String>, CredentialError> {
            let account = credential_account(site, email)?;
            if !safe_directory_exists(self.directory).map_err(|_| CredentialError::ReadFailed)? {
                return Ok(None);
            }
            read_token(&self.path(&account))
        }

        /// Deletes one token. A missing token is accepted.
        ///
        /// # Errors
        ///
        /// Returns an error for invalid input or an unsafe credential path.
        pub fn delete_api_token(self, site: &str, email: &str) -> Result<(), CredentialError> {
            let account = credential_account(site, email)?;
            if !safe_directory_exists(self.directory).map_err(|_| CredentialError::DeleteFailed)? {
                return Ok(());
            }
            delete_token(&self.path(&account))
        }

        fn path(self, account: &str) -> PathBuf {
            self.directory
                .join(credential_file_name(self.service_name, account))
        }
    }

    fn current_user_directory() -> Option<&'static Path> {
        CURRENT_USER_DIRECTORY
            .get_or_init(resolve_current_user_directory)
            .as_deref()
    }

    fn resolve_current_user_directory() -> Option<PathBuf> {
        environment_path(XDG_DATA_HOME_ENVIRONMENT_VARIABLE)
            .or_else(default_data_directory)
            .map(|root| root.join(PRODUCT_DIRECTORY).join(CREDENTIAL_DIRECTORY))
    }

    fn default_data_directory() -> Option<PathBuf> {
        environment_path(HOME_ENVIRONMENT_VARIABLE).map(|home| home.join(LOCAL_DATA_DIRECTORY))
    }

    fn environment_path(name: &str) -> Option<PathBuf> {
        std::env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    }

    fn credential_file_name(service_name: &str, account: &str) -> String {
        let digest = Sha256::digest(format!("{service_name}|{account}").as_bytes());
        format!("{digest:x}")
    }

    fn ensure_directory(directory: &Path) -> Result<(), CredentialError> {
        match fs::symlink_metadata(directory) {
            Ok(metadata) => {
                require_directory(&metadata).map_err(|_| CredentialError::SaveFailed)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                create_private_directory(directory)?;
            }
            Err(_) => return Err(CredentialError::SaveFailed),
        }
        set_mode(directory, DIRECTORY_MODE).map_err(|_| CredentialError::SaveFailed)?;
        require_private_directory(directory)
    }

    fn create_private_directory(directory: &Path) -> Result<(), CredentialError> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(DIRECTORY_MODE)
            .create(directory)
            .map_err(|_| CredentialError::SaveFailed)
    }

    fn require_directory(metadata: &fs::Metadata) -> Result<(), std::io::Error> {
        metadata
            .file_type()
            .is_dir()
            .then_some(())
            .ok_or_else(unsafe_path_error)
    }

    fn require_private_directory(directory: &Path) -> Result<(), CredentialError> {
        match safe_directory_exists(directory) {
            Ok(true) => Ok(()),
            Ok(false) | Err(_) => Err(CredentialError::SaveFailed),
        }
    }

    fn write_token(path: &Path, token: &str) -> Result<(), CredentialError> {
        let mut options = AtomicWriteFile::options();
        options.mode(FILE_MODE).preserve_mode(false);
        let mut file = options
            .open(path)
            .map_err(|_| CredentialError::SaveFailed)?;
        file.write_all(token.as_bytes())
            .map_err(|_| CredentialError::SaveFailed)?;
        file.commit().map_err(|_| CredentialError::SaveFailed)?;
        protect_written_token(path)
    }

    fn protect_written_token(path: &Path) -> Result<(), CredentialError> {
        let mode_set = set_mode(path, FILE_MODE).is_ok();
        if mode_set && matches!(safe_file_exists(path), Ok(true)) {
            return Ok(());
        }
        drop(fs::remove_file(path));
        Err(CredentialError::SaveFailed)
    }

    fn read_token(path: &Path) -> Result<Option<String>, CredentialError> {
        if !safe_file_exists(path).map_err(|_| CredentialError::ReadFailed)? {
            return Ok(None);
        }
        let token = fs::read_to_string(path).map_err(|_| CredentialError::ReadFailed)?;
        validate_token(&token).map_err(|_| CredentialError::ReadFailed)?;
        Ok(Some(token))
    }

    fn delete_token(path: &Path) -> Result<(), CredentialError> {
        if !safe_file_exists(path).map_err(|_| CredentialError::DeleteFailed)? {
            return Ok(());
        }
        fs::remove_file(path).map_err(|_| CredentialError::DeleteFailed)
    }

    fn safe_file_exists(path: &Path) -> Result<bool, std::io::Error> {
        let Some(metadata) = metadata_if_exists(path)? else {
            return Ok(false);
        };
        if metadata.file_type().is_file() && is_private(&metadata) {
            return Ok(true);
        }
        Err(unsafe_path_error())
    }

    fn safe_directory_exists(path: &Path) -> Result<bool, std::io::Error> {
        let Some(metadata) = metadata_if_exists(path)? else {
            return Ok(false);
        };
        if metadata.file_type().is_dir() && is_private(&metadata) {
            return Ok(true);
        }
        Err(unsafe_path_error())
    }

    fn metadata_if_exists(path: &Path) -> Result<Option<fs::Metadata>, std::io::Error> {
        match fs::symlink_metadata(path) {
            Ok(metadata) => Ok(Some(metadata)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn is_private(metadata: &fs::Metadata) -> bool {
        metadata.permissions().mode() & PRIVATE_PERMISSION_MASK == 0
    }

    fn set_mode(path: &Path, mode: u32) -> Result<(), std::io::Error> {
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    }

    fn unsafe_path_error() -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::PermissionDenied, UNSAFE_CREDENTIAL_PATH)
    }

    #[cfg(test)]
    mod tests {
        use std::fs;
        use std::os::unix::fs::{PermissionsExt, symlink};
        use std::path::{Path, PathBuf};
        use std::sync::atomic::{AtomicU64, Ordering};

        use super::*;

        const DIRECTORY_MODE: u32 = 0o700;
        const FILE_MODE: u32 = 0o600;
        static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

        #[test]
        fn protected_store_round_trips_and_deletes_a_token() {
            let directory = temporary_directory();
            let store = CredentialStore::at(CredentialPurpose::Mcp, directory);

            store.save_api_token(site(), email(), "secret").unwrap();
            assert_eq!(
                store.load_api_token(site(), email()).unwrap(),
                Some("secret".into())
            );
            store.delete_api_token(site(), email()).unwrap();
            assert_eq!(store.load_api_token(site(), email()).unwrap(), None);

            fs::remove_dir_all(directory).unwrap();
        }

        #[test]
        fn protected_store_restricts_directory_and_file_permissions() {
            let directory = temporary_directory();
            let store = CredentialStore::at(CredentialPurpose::Mcp, directory);
            store.save_api_token(site(), email(), "secret").unwrap();

            assert_mode(directory, DIRECTORY_MODE);
            assert_mode(&only_file(directory), FILE_MODE);

            fs::remove_dir_all(directory).unwrap();
        }

        #[test]
        fn protected_store_rejects_a_world_readable_token() {
            let directory = temporary_directory();
            let store = saved_store(directory);
            let token_path = only_file(directory);
            fs::set_permissions(&token_path, fs::Permissions::from_mode(0o644)).unwrap();

            assert_eq!(
                store.load_api_token(site(), email()),
                Err(CredentialError::ReadFailed)
            );

            fs::remove_dir_all(directory).unwrap();
        }

        #[test]
        fn protected_store_rejects_a_symlinked_directory() {
            let parent = temporary_directory();
            fs::create_dir(parent).unwrap();
            let target = parent.join("target");
            let link = parent.join("credentials");
            fs::create_dir(&target).unwrap();
            symlink(&target, &link).unwrap();
            let store = CredentialStore::at(CredentialPurpose::Mcp, leaked_path(link));

            assert_eq!(
                store.save_api_token(site(), email(), "secret"),
                Err(CredentialError::SaveFailed)
            );
            assert!(fs::read_dir(target).unwrap().next().is_none());

            fs::remove_dir_all(parent).unwrap();
        }

        #[test]
        fn protected_store_never_follows_a_token_symlink() {
            let directory = temporary_directory();
            let store = saved_store(directory);
            let token_path = only_file(directory);
            let external_path = directory.with_extension("external-token");
            fs::write(&external_path, "external-secret").unwrap();
            fs::remove_file(&token_path).unwrap();
            symlink(&external_path, &token_path).unwrap();

            assert_symlink_rejected(store, &external_path);

            fs::remove_dir_all(directory).unwrap();
            fs::remove_file(external_path).unwrap();
        }

        fn assert_symlink_rejected(store: CredentialStore, external_path: &Path) {
            assert_eq!(
                store.load_api_token(site(), email()),
                Err(CredentialError::ReadFailed)
            );
            assert_eq!(
                store.delete_api_token(site(), email()),
                Err(CredentialError::DeleteFailed)
            );
            assert_eq!(
                fs::read_to_string(external_path).unwrap(),
                "external-secret"
            );
        }

        #[test]
        fn transaction_guard_is_exclusive_and_recoverable() {
            let directory = temporary_directory();
            let first = CredentialTransactionGuard::acquire_at(directory).unwrap();
            assert!(matches!(
                CredentialTransactionGuard::acquire_at(directory),
                Err(CredentialError::TransactionBusy)
            ));
            drop(first);
            CredentialTransactionGuard::acquire_at(directory).unwrap();
            fs::remove_dir_all(directory).unwrap();
        }

        fn saved_store(directory: &'static Path) -> CredentialStore {
            let store = CredentialStore::at(CredentialPurpose::Mcp, directory);
            store.save_api_token(site(), email(), "secret").unwrap();
            store
        }

        fn temporary_directory() -> &'static Path {
            let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "worklogger-credentials-{}-{sequence}",
                std::process::id()
            ));
            Box::leak(path.into_boxed_path())
        }

        fn leaked_path(path: PathBuf) -> &'static Path {
            Box::leak(path.into_boxed_path())
        }

        fn only_file(directory: &Path) -> PathBuf {
            fs::read_dir(directory)
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path()
        }

        fn assert_mode(path: &Path, expected: u32) {
            let actual = fs::metadata(path).unwrap().permissions().mode() & DIRECTORY_MODE;
            assert_eq!(actual, expected);
        }

        const fn site() -> &'static str {
            "https://example.atlassian.net"
        }

        const fn email() -> &'static str {
            "person@example.com"
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use super::{CredentialError, CredentialPurpose};

    #[derive(Clone, Copy, Debug)]
    pub struct CredentialStore;

    #[derive(Debug)]
    pub struct CredentialTransactionGuard;

    impl CredentialTransactionGuard {
        pub const fn acquire() -> Result<Self, CredentialError> {
            Ok(Self)
        }
    }

    impl CredentialStore {
        pub const fn for_purpose(_purpose: CredentialPurpose) -> Result<Self, CredentialError> {
            Err(CredentialError::Unavailable)
        }

        pub fn save_api_token(
            self,
            _site: &str,
            _email: &str,
            _token: &str,
        ) -> Result<(), CredentialError> {
            Err(CredentialError::Unavailable)
        }

        pub fn load_api_token(
            self,
            _site: &str,
            _email: &str,
        ) -> Result<Option<String>, CredentialError> {
            Err(CredentialError::Unavailable)
        }

        pub fn delete_api_token(self, _site: &str, _email: &str) -> Result<(), CredentialError> {
            Err(CredentialError::Unavailable)
        }
    }
}

pub use platform::{CredentialStore, CredentialTransactionGuard};

#[cfg(any(windows, target_os = "linux", test))]
/// Compares two Jira site-and-email pairs using the credential-store canonical form.
///
/// # Errors
///
/// Returns an error when either coordinate pair is invalid.
pub fn api_token_coordinates_match(
    first_site: &str,
    first_email: &str,
    second_site: &str,
    second_email: &str,
) -> Result<bool, CredentialError> {
    Ok(credential_account(first_site, first_email)?
        == credential_account(second_site, second_email)?)
}

#[cfg(any(windows, target_os = "linux", test))]
fn credential_account(site: &str, email: &str) -> Result<String, CredentialError> {
    validate_site(site)?;
    validate_email(email)?;
    Ok(format!(
        "{}{}{}",
        site.trim().to_ascii_lowercase(),
        ACCOUNT_SEPARATOR,
        email.trim().to_ascii_lowercase()
    ))
}

#[cfg(any(windows, target_os = "linux", test))]
fn validate_site(site: &str) -> Result<(), CredentialError> {
    let value = site.trim();
    let valid =
        !value.is_empty() && value.len() <= MAX_SITE_LENGTH && !value.chars().any(char::is_control);
    valid.then_some(()).ok_or(CredentialError::InvalidSite)
}

#[cfg(any(windows, target_os = "linux", test))]
fn validate_email(email: &str) -> Result<(), CredentialError> {
    let value = email.trim();
    let valid = !value.is_empty()
        && value.len() <= MAX_EMAIL_LENGTH
        && value.contains('@')
        && !value.chars().any(char::is_whitespace);
    valid.then_some(()).ok_or(CredentialError::InvalidEmail)
}

#[cfg(any(windows, target_os = "linux", test))]
fn validate_token(token: &str) -> Result<(), CredentialError> {
    let valid = !token.trim().is_empty()
        && token.len() <= MAX_TOKEN_LENGTH
        && !token.chars().any(char::is_control);
    valid.then_some(()).ok_or(CredentialError::InvalidToken)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_credential_coordinates_before_platform_access() {
        assert_eq!(
            credential_account("", "person@example.com"),
            Err(CredentialError::InvalidSite)
        );
        assert_eq!(
            credential_account("https://example.atlassian.net", "invalid"),
            Err(CredentialError::InvalidEmail)
        );
        assert_eq!(validate_token("  "), Err(CredentialError::InvalidToken));
        assert_ne!(
            CredentialPurpose::Desktop.service_name(),
            CredentialPurpose::Mcp.service_name()
        );
        assert_eq!(
            api_token_coordinates_match(
                " HTTPS://EXAMPLE.ATLASSIAN.NET ",
                " Person@Example.com ",
                "https://example.atlassian.net",
                "person@example.com",
            ),
            Ok(true)
        );
    }
}
