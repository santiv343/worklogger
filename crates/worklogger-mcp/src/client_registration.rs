use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use atomic_write_file::AtomicWriteFile;
use serde::Serialize;
use serde_json::{Map, Value, json};
use thiserror::Error;

use crate::runtime_installation::is_owned_versioned_executable;

pub const MCP_SERVER_REGISTRATION_NAME: &str = "worklogger";
pub const MCP_SERVER_SERVE_ARGUMENT: &str = "serve";

const DEFAULT_MAXIMUM_CLIENT_CONFIGURATION_BYTES: usize = 1_048_576;
const DEFAULT_MAXIMUM_COMMAND_OUTPUT_BYTES: usize = 8_192;
const DEFAULT_CLIENT_COMMAND_TIMEOUT_SECONDS: u64 = 5;
const CLIENT_COMMAND_POLL_INTERVAL_MILLISECONDS: u64 = 20;
const MAXIMUM_CLIENT_CONFIGURATION_ENVIRONMENT_VARIABLE: &str =
    "WORKLOGGER_MCP_MAX_CLIENT_CONFIG_BYTES";
const MAXIMUM_COMMAND_OUTPUT_ENVIRONMENT_VARIABLE: &str =
    "WORKLOGGER_MCP_MAX_CLIENT_COMMAND_OUTPUT_BYTES";
const CLIENT_COMMAND_TIMEOUT_ENVIRONMENT_VARIABLE: &str = "WORKLOGGER_MCP_CLIENT_TIMEOUT_SECONDS";
pub const MCP_SERVERS_PROPERTY: &str = "mcpServers";
const COMMAND_PROPERTY: &str = "command";
const ARGUMENTS_PROPERTY: &str = "args";
const CODEX_CONFIGURATION_DIRECTORY: &str = ".codex";
const CODEX_CONFIGURATION_FILE: &str = "config.toml";
const CLAUDE_CODE_CONFIGURATION_FILE: &str = ".claude.json";
const CURSOR_CONFIGURATION_DIRECTORY: &str = ".cursor";
const CURSOR_CONFIGURATION_FILE: &str = "mcp.json";
const WINDSURF_CONFIGURATION_ROOT: &str = ".codeium";
const WINDSURF_CONFIGURATION_DIRECTORY: &str = "windsurf";
const WINDSURF_CONFIGURATION_FILE: &str = "mcp_config.json";
const CLAUDE_DESKTOP_CONFIGURATION_DIRECTORY: &str = "Claude";
const CLAUDE_DESKTOP_CONFIGURATION_FILE: &str = "claude_desktop_config.json";
const MACOS_APPLICATION_SUPPORT_DIRECTORY: &str = "Library/Application Support";
const HOME_ENVIRONMENT_VARIABLE: &str = "HOME";
const USER_PROFILE_ENVIRONMENT_VARIABLE: &str = "USERPROFILE";
const APP_DATA_ENVIRONMENT_VARIABLE: &str = "APPDATA";
const PATH_ENVIRONMENT_VARIABLE: &str = "PATH";
const CLI_MCP_ARGUMENT: &str = "mcp";
const CLI_LIST_ARGUMENT: &str = "list";
const CLI_ADD_ARGUMENT: &str = "add";
const CLI_REMOVE_ARGUMENT: &str = "remove";
const CLI_JSON_ARGUMENT: &str = "--json";
const CLI_ARGUMENT_SEPARATOR: &str = "--";
const CODEX_NAME_PROPERTY: &str = "name";
const CODEX_TRANSPORT_PROPERTY: &str = "transport";
const CODEX_STATUS_TARGET: &str = "codex mcp list";

#[cfg(windows)]
const CODEX_EXECUTABLE_NAMES: &[&str] = &["codex.exe"];
#[cfg(not(windows))]
const CODEX_EXECUTABLE_NAMES: &[&str] = &["codex"];
#[cfg(any(windows, test))]
const CODEX_NPM_COMMAND_SHIM: &str = "codex.cmd";
#[cfg(any(windows, test))]
const CODEX_NPM_SCOPE_DIRECTORY: &str = "@openai";
#[cfg(any(windows, test))]
const CODEX_NPM_PACKAGE_DIRECTORY: &str = "codex";
#[cfg(any(windows, test))]
const CODEX_NPM_ENTRYPOINT: &str = "codex.js";
#[cfg(any(windows, test))]
const NODE_MODULES_DIRECTORY: &str = "node_modules";
#[cfg(any(windows, test))]
const PACKAGE_BINARY_DIRECTORY: &str = "bin";
#[cfg(any(windows, test))]
const NODE_EXECUTABLE_NAMES: &[&str] = &["node.exe"];
#[cfg(windows)]
const CLAUDE_EXECUTABLE_NAMES: &[&str] = &["claude.exe"];
#[cfg(not(windows))]
const CLAUDE_EXECUTABLE_NAMES: &[&str] = &["claude"];

const SUPPORTED_CLIENTS: &[McpClientId] = &[
    McpClientId::Codex,
    McpClientId::ClaudeCode,
    McpClientId::ClaudeDesktop,
    McpClientId::Cursor,
    McpClientId::Windsurf,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum McpClientId {
    Codex,
    ClaudeCode,
    ClaudeDesktop,
    Cursor,
    Windsurf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RegistrationState {
    Unavailable,
    Available,
    Registered,
    BrokenRegistration,
    OwnedOutdatedRegistration,
    ConflictingRegistration,
    InvalidConfiguration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct McpClientStatus {
    pub client: McpClientId,
    pub state: RegistrationState,
    pub target: PathBuf,
    pub detail: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ClientRegistrationService {
    home_directory: PathBuf,
    app_data_directory: Option<PathBuf>,
    executable_directories: Vec<PathBuf>,
}

#[derive(Debug, Error)]
pub enum ClientRegistrationError {
    #[error("could not determine the user directory")]
    MissingUserDirectory,
    #[error("{0:?} is not available on this computer")]
    ClientUnavailable(McpClientId),
    #[error("the MCP executable does not exist or is not an absolute path: {0}")]
    InvalidServerExecutable(PathBuf),
    #[error("the {client:?} configuration at {path} is invalid")]
    InvalidConfiguration { client: McpClientId, path: PathBuf },
    #[error("could not access {path}: {source}")]
    Storage {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{client:?} rejected the change: {message}")]
    ClientCommand {
        client: McpClientId,
        message: String,
    },
    #[error("{client:?} did not respond within the allowed time")]
    ClientCommandTimeout { client: McpClientId },
    #[error("the {client:?} configuration changed during the operation: {path}")]
    ConcurrentModification { client: McpClientId, path: PathBuf },
}

#[derive(Clone, Copy)]
enum ClientIntegration {
    CommandLine,
    JsonFile,
}

#[derive(Clone, Debug)]
struct ClientCommand {
    executable: PathBuf,
    leading_arguments: Vec<OsString>,
}

impl ClientCommand {
    fn direct(executable: PathBuf) -> Self {
        Self {
            executable,
            leading_arguments: Vec::new(),
        }
    }

    #[cfg(any(windows, test))]
    fn with_leading_argument(executable: PathBuf, argument: PathBuf) -> Self {
        Self {
            executable,
            leading_arguments: vec![argument.into_os_string()],
        }
    }
}

impl McpClientId {
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::ClaudeCode => "Claude Code",
            Self::ClaudeDesktop => "Claude Desktop",
            Self::Cursor => "Cursor",
            Self::Windsurf => "Windsurf",
        }
    }

    const fn integration(self) -> ClientIntegration {
        match self {
            Self::Codex => ClientIntegration::CommandLine,
            Self::ClaudeCode | Self::ClaudeDesktop | Self::Cursor | Self::Windsurf => {
                ClientIntegration::JsonFile
            }
        }
    }
}

impl ClientRegistrationService {
    /// Resolves client configuration from the current operating-system user.
    ///
    /// # Errors
    ///
    /// Returns an error when the user directory cannot be determined.
    pub fn for_current_user() -> Result<Self, ClientRegistrationError> {
        let home_directory =
            user_directory().ok_or(ClientRegistrationError::MissingUserDirectory)?;
        let app_data_directory =
            env::var_os(APP_DATA_ENVIRONMENT_VARIABLE).filter(|value| !value.is_empty());
        let executable_directories = executable_directories();
        Ok(Self::at(
            home_directory,
            app_data_directory.map(PathBuf::from),
            executable_directories,
        ))
    }

    #[must_use]
    pub const fn at(
        home_directory: PathBuf,
        app_data_directory: Option<PathBuf>,
        executable_directories: Vec<PathBuf>,
    ) -> Self {
        Self {
            home_directory,
            app_data_directory,
            executable_directories,
        }
    }

    #[must_use]
    pub const fn supported_clients() -> &'static [McpClientId] {
        SUPPORTED_CLIENTS
    }

    #[must_use]
    pub fn statuses(&self, server: &Path) -> Vec<McpClientStatus> {
        SUPPORTED_CLIENTS
            .iter()
            .map(|client| self.status(*client, server))
            .collect()
    }

    #[must_use]
    pub fn status(&self, client: McpClientId, server: &Path) -> McpClientStatus {
        let target = self.target(client);
        if !self.available(client, &target) {
            return client_status(client, RegistrationState::Unavailable, target, None);
        }
        match self.registration_state(client, server, &target) {
            Ok(state) => client_status(client, state, target, None),
            Err(error) => client_status(
                client,
                RegistrationState::InvalidConfiguration,
                target,
                Some(error.to_string()),
            ),
        }
    }

    /// Registers or updates Worklogger in the selected MCP client.
    ///
    /// # Errors
    ///
    /// Returns an error without changing unrelated client configuration.
    pub fn register(
        &self,
        client: McpClientId,
        server: &Path,
    ) -> Result<(), ClientRegistrationError> {
        validate_server(server)?;
        let target = self.available_target(client)?;
        match client.integration() {
            ClientIntegration::CommandLine => self.register_with_cli(client, server),
            ClientIntegration::JsonFile => register_in_json(client, &target, server),
        }
    }

    /// Removes only Worklogger from the selected MCP client.
    ///
    /// # Errors
    ///
    /// Returns an error without deleting the client configuration file.
    pub fn unregister(
        &self,
        client: McpClientId,
        server: &Path,
    ) -> Result<(), ClientRegistrationError> {
        let target = self.available_target(client)?;
        let state = self.registration_state(client, server, &target)?;
        if !is_owned_registration(state) {
            return Ok(());
        }
        match client.integration() {
            ClientIntegration::CommandLine => self.unregister_with_cli(client),
            ClientIntegration::JsonFile => unregister_from_json(client, &target, server),
        }
    }

    fn available_target(&self, client: McpClientId) -> Result<PathBuf, ClientRegistrationError> {
        let target = self.target(client);
        self.available(client, &target)
            .then_some(target)
            .ok_or(ClientRegistrationError::ClientUnavailable(client))
    }

    fn registration_state(
        &self,
        client: McpClientId,
        server: &Path,
        target: &Path,
    ) -> Result<RegistrationState, ClientRegistrationError> {
        match client.integration() {
            ClientIntegration::CommandLine => self.cli_registration_state(client, server),
            ClientIntegration::JsonFile => json_registration_state(client, target, server),
        }
    }

    fn cli_registration_state(
        &self,
        client: McpClientId,
        server: &Path,
    ) -> Result<RegistrationState, ClientRegistrationError> {
        let command = self.required_client_command(client)?;
        run_cli_status(client, &command, server)
    }

    fn register_with_cli(
        &self,
        client: McpClientId,
        server: &Path,
    ) -> Result<(), ClientRegistrationError> {
        let command = self.required_client_command(client)?;
        run_client_command(client, &command, &registration_arguments(client, server))
    }

    fn unregister_with_cli(&self, client: McpClientId) -> Result<(), ClientRegistrationError> {
        let command = self.required_client_command(client)?;
        run_client_command(client, &command, &unregistration_arguments(client))
    }

    fn required_client_command(
        &self,
        client: McpClientId,
    ) -> Result<ClientCommand, ClientRegistrationError> {
        self.client_command(client)
            .ok_or(ClientRegistrationError::ClientUnavailable(client))
    }

    fn available(&self, client: McpClientId, target: &Path) -> bool {
        if client == McpClientId::ClaudeCode {
            return self.client_command(client).is_some();
        }
        match client.integration() {
            ClientIntegration::CommandLine => self.client_command(client).is_some(),
            ClientIntegration::JsonFile => target.parent().is_some_and(Path::is_dir),
        }
    }

    fn client_command(&self, client: McpClientId) -> Option<ClientCommand> {
        match client {
            McpClientId::Codex => codex_client_command(&self.executable_directories),
            McpClientId::ClaudeCode => {
                find_executable(&self.executable_directories, CLAUDE_EXECUTABLE_NAMES)
                    .map(ClientCommand::direct)
            }
            _ => None,
        }
    }

    fn target(&self, client: McpClientId) -> PathBuf {
        match client {
            McpClientId::Codex => self
                .home_directory
                .join(CODEX_CONFIGURATION_DIRECTORY)
                .join(CODEX_CONFIGURATION_FILE),
            McpClientId::ClaudeCode => self.home_directory.join(CLAUDE_CODE_CONFIGURATION_FILE),
            McpClientId::ClaudeDesktop => self.claude_desktop_target(),
            McpClientId::Cursor => self
                .home_directory
                .join(CURSOR_CONFIGURATION_DIRECTORY)
                .join(CURSOR_CONFIGURATION_FILE),
            McpClientId::Windsurf => self
                .home_directory
                .join(WINDSURF_CONFIGURATION_ROOT)
                .join(WINDSURF_CONFIGURATION_DIRECTORY)
                .join(WINDSURF_CONFIGURATION_FILE),
        }
    }

    fn claude_desktop_target(&self) -> PathBuf {
        self.app_data_directory.as_ref().map_or_else(
            || {
                self.home_directory
                    .join(MACOS_APPLICATION_SUPPORT_DIRECTORY)
                    .join(CLAUDE_DESKTOP_CONFIGURATION_DIRECTORY)
                    .join(CLAUDE_DESKTOP_CONFIGURATION_FILE)
            },
            |directory| {
                directory
                    .join(CLAUDE_DESKTOP_CONFIGURATION_DIRECTORY)
                    .join(CLAUDE_DESKTOP_CONFIGURATION_FILE)
            },
        )
    }
}

fn user_directory() -> Option<PathBuf> {
    let variable = if cfg!(windows) {
        USER_PROFILE_ENVIRONMENT_VARIABLE
    } else {
        HOME_ENVIRONMENT_VARIABLE
    };
    env::var_os(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn executable_directories() -> Vec<PathBuf> {
    env::var_os(PATH_ENVIRONMENT_VARIABLE)
        .map(|value| env::split_paths(&value).collect())
        .unwrap_or_default()
}

fn find_executable(directories: &[PathBuf], names: &[&str]) -> Option<PathBuf> {
    directories.iter().find_map(|directory| {
        names
            .iter()
            .map(|name| directory.join(name))
            .find(|candidate| candidate.is_file())
    })
}

fn codex_client_command(directories: &[PathBuf]) -> Option<ClientCommand> {
    let direct = find_executable(directories, CODEX_EXECUTABLE_NAMES).map(ClientCommand::direct);
    #[cfg(windows)]
    {
        direct.or_else(|| npm_codex_command(directories))
    }
    #[cfg(not(windows))]
    {
        direct
    }
}

#[cfg(any(windows, test))]
fn npm_codex_command(directories: &[PathBuf]) -> Option<ClientCommand> {
    let entrypoint = directories
        .iter()
        .find_map(|directory| npm_codex_entrypoint(directory))?;
    let node = find_executable(directories, NODE_EXECUTABLE_NAMES)?;
    Some(ClientCommand::with_leading_argument(node, entrypoint))
}

#[cfg(any(windows, test))]
fn npm_codex_entrypoint(directory: &Path) -> Option<PathBuf> {
    if !directory.join(CODEX_NPM_COMMAND_SHIM).is_file() {
        return None;
    }
    let entrypoint = directory
        .join(NODE_MODULES_DIRECTORY)
        .join(CODEX_NPM_SCOPE_DIRECTORY)
        .join(CODEX_NPM_PACKAGE_DIRECTORY)
        .join(PACKAGE_BINARY_DIRECTORY)
        .join(CODEX_NPM_ENTRYPOINT);
    entrypoint.is_file().then_some(entrypoint)
}

fn validate_server(server: &Path) -> Result<(), ClientRegistrationError> {
    if !server.is_absolute() || !server.is_file() {
        return Err(ClientRegistrationError::InvalidServerExecutable(
            server.to_path_buf(),
        ));
    }
    Ok(())
}

fn client_status(
    client: McpClientId,
    state: RegistrationState,
    target: PathBuf,
    detail: Option<String>,
) -> McpClientStatus {
    McpClientStatus {
        client,
        state,
        target,
        detail,
    }
}

fn run_cli_status(
    client: McpClientId,
    command: &ClientCommand,
    server: &Path,
) -> Result<RegistrationState, ClientRegistrationError> {
    let output = command_output(client, command, &status_arguments(client))?;
    command_result(client, &output)?;
    codex_registration_state(client, &output.stdout, server)
}

fn run_client_command(
    client: McpClientId,
    command: &ClientCommand,
    arguments: &[OsString],
) -> Result<(), ClientRegistrationError> {
    let output = command_output(client, command, arguments)?;
    command_result(client, &output)
}

fn command_output(
    client: McpClientId,
    command: &ClientCommand,
    arguments: &[OsString],
) -> Result<Output, ClientRegistrationError> {
    let mut child = Command::new(&command.executable)
        .args(&command.leading_arguments)
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| storage_error(&command.executable, source))?;
    let stdout = child.stdout.take().ok_or_else(|| {
        storage_error(
            &command.executable,
            std::io::Error::other("stdout no disponible"),
        )
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        storage_error(
            &command.executable,
            std::io::Error::other("stderr no disponible"),
        )
    })?;
    let stdout_reader = thread::spawn(move || read_pipe(stdout));
    let stderr_reader = thread::spawn(move || read_pipe(stderr));
    let status = wait_for_exit(client, &command.executable, &mut child)?;
    Ok(Output {
        status,
        stdout: join_pipe(stdout_reader, &command.executable)?,
        stderr: join_pipe(stderr_reader, &command.executable)?,
    })
}

fn wait_for_exit(
    client: McpClientId,
    command: &Path,
    child: &mut Child,
) -> Result<std::process::ExitStatus, ClientRegistrationError> {
    let deadline = Instant::now() + client_command_timeout();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|source| storage_error(command, source))?
        {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _result = child.kill();
            let _result = child.wait();
            return Err(ClientRegistrationError::ClientCommandTimeout { client });
        }
        thread::sleep(Duration::from_millis(
            CLIENT_COMMAND_POLL_INTERVAL_MILLISECONDS,
        ));
    }
}

fn read_pipe(mut pipe: impl Read) -> Result<Vec<u8>, std::io::Error> {
    let mut contents = Vec::new();
    pipe.read_to_end(&mut contents)?;
    Ok(contents)
}

fn join_pipe(
    reader: thread::JoinHandle<Result<Vec<u8>, std::io::Error>>,
    command: &Path,
) -> Result<Vec<u8>, ClientRegistrationError> {
    reader
        .join()
        .map_err(|_| {
            storage_error(
                command,
                std::io::Error::other("lector de salida interrumpido"),
            )
        })?
        .map_err(|source| storage_error(command, source))
}

fn command_result(client: McpClientId, output: &Output) -> Result<(), ClientRegistrationError> {
    if output.status.success() {
        return Ok(());
    }
    let message = bounded_command_error(&output.stderr);
    Err(ClientRegistrationError::ClientCommand { client, message })
}

fn bounded_command_error(bytes: &[u8]) -> String {
    let end = bytes.len().min(maximum_command_output_bytes());
    String::from_utf8_lossy(&bytes[..end]).trim().to_owned()
}

fn status_arguments(client: McpClientId) -> Vec<OsString> {
    match client {
        McpClientId::Codex => strings(&[CLI_MCP_ARGUMENT, CLI_LIST_ARGUMENT, CLI_JSON_ARGUMENT]),
        _ => Vec::new(),
    }
}

fn registration_arguments(client: McpClientId, server: &Path) -> Vec<OsString> {
    match client {
        McpClientId::Codex => command_with_server(&[CLI_MCP_ARGUMENT, CLI_ADD_ARGUMENT], server),
        _ => Vec::new(),
    }
}

fn command_with_server(prefix: &[&str], server: &Path) -> Vec<OsString> {
    let mut arguments = strings(prefix);
    arguments.extend(strings(&[
        MCP_SERVER_REGISTRATION_NAME,
        CLI_ARGUMENT_SEPARATOR,
    ]));
    arguments.push(server.as_os_str().to_owned());
    arguments.push(OsString::from(MCP_SERVER_SERVE_ARGUMENT));
    arguments
}

fn unregistration_arguments(client: McpClientId) -> Vec<OsString> {
    match client {
        McpClientId::Codex => strings(&[
            CLI_MCP_ARGUMENT,
            CLI_REMOVE_ARGUMENT,
            MCP_SERVER_REGISTRATION_NAME,
        ]),
        _ => Vec::new(),
    }
}

fn strings(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

fn codex_registration_state(
    client: McpClientId,
    bytes: &[u8],
    server: &Path,
) -> Result<RegistrationState, ClientRegistrationError> {
    validate_codex_output_size(client, bytes)?;
    let entries = serde_json::from_slice::<Value>(bytes)
        .map_err(|_| invalid_configuration(client, Path::new(CODEX_STATUS_TARGET)))?;
    let servers = entries
        .as_array()
        .ok_or_else(|| invalid_configuration(client, Path::new(CODEX_STATUS_TARGET)))?;
    let entry = servers.iter().find(|entry| {
        entry.get(CODEX_NAME_PROPERTY).and_then(Value::as_str) == Some(MCP_SERVER_REGISTRATION_NAME)
    });
    let transport = entry.and_then(|value| value.get(CODEX_TRANSPORT_PROPERTY));
    Ok(registration_state_for_entry(transport, server))
}

fn validate_codex_output_size(
    client: McpClientId,
    bytes: &[u8],
) -> Result<(), ClientRegistrationError> {
    if bytes.len() <= maximum_command_output_bytes() {
        return Ok(());
    }
    Err(invalid_configuration(
        client,
        Path::new(CODEX_STATUS_TARGET),
    ))
}

fn registration_state_for_entry(entry: Option<&Value>, server: &Path) -> RegistrationState {
    let Some(entry) = entry else {
        return RegistrationState::Available;
    };
    registration_state_for_transport(entry, server)
}

fn json_registration_state(
    client: McpClientId,
    path: &Path,
    server: &Path,
) -> Result<RegistrationState, ClientRegistrationError> {
    let document = read_json(client, path)?;
    let entry = document
        .get(MCP_SERVERS_PROPERTY)
        .and_then(Value::as_object)
        .and_then(|servers| servers.get(MCP_SERVER_REGISTRATION_NAME));
    Ok(registration_state_for_entry(entry, server))
}

fn register_in_json(
    client: McpClientId,
    path: &Path,
    server: &Path,
) -> Result<(), ClientRegistrationError> {
    let original = read_json(client, path)?;
    let mut document = original.clone();
    let servers = server_map(client, path, &mut document)?;
    servers.insert(
        MCP_SERVER_REGISTRATION_NAME.to_owned(),
        server_entry(server),
    );
    write_json(client, path, &original, &document)
}

fn unregister_from_json(
    client: McpClientId,
    path: &Path,
    server: &Path,
) -> Result<(), ClientRegistrationError> {
    let original = read_json(client, path)?;
    let mut document = original.clone();
    let Some(servers) = document
        .get_mut(MCP_SERVERS_PROPERTY)
        .and_then(Value::as_object_mut)
    else {
        return Ok(());
    };
    let owned = servers
        .get(MCP_SERVER_REGISTRATION_NAME)
        .is_some_and(|entry| is_owned_entry(entry, server));
    if !owned {
        return Ok(());
    }
    servers.remove(MCP_SERVER_REGISTRATION_NAME);
    write_json(client, path, &original, &document)
}

fn read_json(client: McpClientId, path: &Path) -> Result<Value, ClientRegistrationError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(json!({})),
        Err(source) => return Err(storage_error(path, source)),
    };
    reject_symlink(client, path)?;
    if bytes.len() > maximum_client_configuration_bytes() {
        return Err(invalid_configuration(client, path));
    }
    let document =
        serde_json::from_slice(&bytes).map_err(|_| invalid_configuration(client, path))?;
    validate_json_shape(client, path, &document)?;
    Ok(document)
}

fn validate_json_shape(
    client: McpClientId,
    path: &Path,
    document: &Value,
) -> Result<(), ClientRegistrationError> {
    let root = document
        .as_object()
        .ok_or_else(|| invalid_configuration(client, path))?;
    if root
        .get(MCP_SERVERS_PROPERTY)
        .is_some_and(|servers| !servers.is_object())
    {
        return Err(invalid_configuration(client, path));
    }
    Ok(())
}

fn server_map<'document>(
    client: McpClientId,
    path: &Path,
    document: &'document mut Value,
) -> Result<&'document mut Map<String, Value>, ClientRegistrationError> {
    let root = document
        .as_object_mut()
        .ok_or_else(|| invalid_configuration(client, path))?;
    let servers = root
        .entry(MCP_SERVERS_PROPERTY)
        .or_insert_with(|| Value::Object(Map::new()));
    servers
        .as_object_mut()
        .ok_or_else(|| invalid_configuration(client, path))
}

fn server_entry(server: &Path) -> Value {
    json!({
        COMMAND_PROPERTY: server.display().to_string(),
        ARGUMENTS_PROPERTY: [MCP_SERVER_SERVE_ARGUMENT]
    })
}

fn registration_state_for_transport(entry: &Value, server: &Path) -> RegistrationState {
    if !entry_has_serve_arguments(entry) {
        return RegistrationState::ConflictingRegistration;
    }
    let Some(command) = entry_command(entry) else {
        return RegistrationState::ConflictingRegistration;
    };
    if command == server {
        return if server.is_file() {
            RegistrationState::Registered
        } else {
            RegistrationState::BrokenRegistration
        };
    }
    if is_versioned_worklogger_command(command, server) {
        return RegistrationState::OwnedOutdatedRegistration;
    }
    RegistrationState::ConflictingRegistration
}

fn entry_command(entry: &Value) -> Option<&Path> {
    entry
        .get(COMMAND_PROPERTY)
        .and_then(Value::as_str)
        .map(Path::new)
}

fn entry_has_serve_arguments(entry: &Value) -> bool {
    entry.get(ARGUMENTS_PROPERTY) == Some(&json!([MCP_SERVER_SERVE_ARGUMENT]))
}

fn is_versioned_worklogger_command(command: &Path, current: &Path) -> bool {
    is_owned_versioned_executable(command, current)
}

fn is_owned_entry(entry: &Value, server: &Path) -> bool {
    is_owned_registration(registration_state_for_transport(entry, server))
}

const fn is_owned_registration(state: RegistrationState) -> bool {
    matches!(
        state,
        RegistrationState::Registered
            | RegistrationState::BrokenRegistration
            | RegistrationState::OwnedOutdatedRegistration
    )
}

fn write_json(
    client: McpClientId,
    path: &Path,
    original: &Value,
    document: &Value,
) -> Result<(), ClientRegistrationError> {
    ensure_unchanged(client, path, original)?;
    let mut bytes =
        serde_json::to_vec_pretty(document).map_err(|_| invalid_configuration(client, path))?;
    bytes.push(b'\n');
    if bytes.len() > maximum_client_configuration_bytes() {
        return Err(invalid_configuration(client, path));
    }
    let mut file = AtomicWriteFile::options()
        .open(path)
        .map_err(|source| storage_error(path, source))?;
    file.write_all(&bytes)
        .map_err(|source| storage_error(path, source))?;
    file.commit().map_err(|source| storage_error(path, source))
}

fn ensure_unchanged(
    client: McpClientId,
    path: &Path,
    original: &Value,
) -> Result<(), ClientRegistrationError> {
    if read_json(client, path)? == *original {
        return Ok(());
    }
    Err(ClientRegistrationError::ConcurrentModification {
        client,
        path: path.to_path_buf(),
    })
}

fn reject_symlink(client: McpClientId, path: &Path) -> Result<(), ClientRegistrationError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(invalid_configuration(client, path))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(storage_error(path, source)),
    }
}

fn maximum_client_configuration_bytes() -> usize {
    environment_number(
        MAXIMUM_CLIENT_CONFIGURATION_ENVIRONMENT_VARIABLE,
        DEFAULT_MAXIMUM_CLIENT_CONFIGURATION_BYTES,
    )
}

fn maximum_command_output_bytes() -> usize {
    environment_number(
        MAXIMUM_COMMAND_OUTPUT_ENVIRONMENT_VARIABLE,
        DEFAULT_MAXIMUM_COMMAND_OUTPUT_BYTES,
    )
}

fn client_command_timeout() -> Duration {
    Duration::from_secs(environment_number(
        CLIENT_COMMAND_TIMEOUT_ENVIRONMENT_VARIABLE,
        DEFAULT_CLIENT_COMMAND_TIMEOUT_SECONDS,
    ))
}

fn environment_number<T>(name: &str, default: T) -> T
where
    T: std::str::FromStr + Copy,
{
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<T>().ok())
        .unwrap_or(default)
}

fn invalid_configuration(client: McpClientId, path: &Path) -> ClientRegistrationError {
    ClientRegistrationError::InvalidConfiguration {
        client,
        path: path.to_path_buf(),
    }
}

fn storage_error(path: &Path, source: std::io::Error) -> ClientRegistrationError {
    ClientRegistrationError::Storage {
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_codex_shim_resolves_to_node_and_javascript_without_a_shell() {
        let directory = npm_test_directory();
        let entrypoint = directory
            .join(NODE_MODULES_DIRECTORY)
            .join(CODEX_NPM_SCOPE_DIRECTORY)
            .join(CODEX_NPM_PACKAGE_DIRECTORY)
            .join(PACKAGE_BINARY_DIRECTORY)
            .join(CODEX_NPM_ENTRYPOINT);
        fs::create_dir_all(entrypoint.parent().expect("entrypoint has a parent"))
            .expect("fixture directory is created");
        fs::write(directory.join(CODEX_NPM_COMMAND_SHIM), []).expect("shim is created");
        fs::write(directory.join(NODE_EXECUTABLE_NAMES[0]), []).expect("node is created");
        fs::write(&entrypoint, []).expect("entrypoint is created");

        let command = npm_codex_command(std::slice::from_ref(&directory))
            .expect("standard npm installation is detected");

        assert_eq!(command.executable, directory.join(NODE_EXECUTABLE_NAMES[0]));
        assert_eq!(command.leading_arguments, vec![entrypoint.into_os_string()]);
        fs::remove_dir_all(directory).expect("fixture is removed");
    }

    fn npm_test_directory() -> PathBuf {
        env::temp_dir().join(format!("worklogger-codex-npm-test-{}", std::process::id()))
    }
}
