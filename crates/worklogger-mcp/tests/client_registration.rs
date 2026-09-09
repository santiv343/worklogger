use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;
#[cfg(unix)]
use worklogger_mcp::MCP_SERVER_SERVE_ARGUMENT;
use worklogger_mcp::{
    ClientRegistrationService, MCP_SERVER_REGISTRATION_NAME, MCP_SERVERS_PROPERTY, McpClientId,
    RegistrationState,
};

static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);
#[cfg(unix)]
const OWNER_EXECUTABLE_PERMISSIONS: u32 = 0o700;

#[test]
fn cursor_registration_preserves_other_servers_and_is_idempotent() {
    let directory = TestDirectory::new();
    let cursor_directory = directory.path.join(".cursor");
    fs::create_dir_all(&cursor_directory).expect("cursor directory is created");
    let configuration_path = cursor_directory.join("mcp.json");
    fs::write(&configuration_path, existing_configuration()).expect("fixture is written");
    let service = service(&directory);
    let server = directory.path.join("bin/worklogger-mcp.exe");
    create_server(&server);

    service
        .register(McpClientId::Cursor, &server)
        .expect("first registration succeeds");
    service
        .register(McpClientId::Cursor, &server)
        .expect("second registration succeeds");

    let configuration = read_json(&configuration_path);
    assert_eq!(
        configuration[MCP_SERVERS_PROPERTY]["existing"]["command"],
        "other"
    );
    assert_eq!(
        configuration[MCP_SERVERS_PROPERTY][MCP_SERVER_REGISTRATION_NAME]["command"],
        server.display().to_string()
    );
    assert_eq!(
        service.status(McpClientId::Cursor, &server).state,
        RegistrationState::Registered
    );
}

#[test]
fn unregister_only_removes_the_worklogger_entry() {
    let directory = TestDirectory::new();
    fs::create_dir_all(directory.path.join(".cursor")).expect("cursor directory is created");
    let service = service(&directory);
    let server = directory.path.join("worklogger-mcp.exe");
    create_server(&server);
    service
        .register(McpClientId::Cursor, &server)
        .expect("registration succeeds");

    service
        .unregister(McpClientId::Cursor, &server)
        .expect("unregistration succeeds");

    let configuration = read_json(&directory.path.join(".cursor/mcp.json"));
    assert!(
        configuration[MCP_SERVERS_PROPERTY]
            .get(MCP_SERVER_REGISTRATION_NAME)
            .is_none()
    );
}

#[test]
fn outdated_owned_registration_can_be_removed_by_a_new_version() {
    let directory = TestDirectory::new();
    let cursor_directory = directory.path.join(".cursor");
    fs::create_dir_all(&cursor_directory).expect("cursor directory is created");
    let old_server = directory
        .path
        .join("worklogger/MCP/0.1.0/worklogger-mcp.exe");
    let current_server = directory
        .path
        .join("worklogger/MCP/0.2.0/worklogger-mcp.exe");
    create_server(&old_server);
    create_server(&current_server);
    write_registered_server(&cursor_directory.join("mcp.json"), &old_server);
    let service = service(&directory);

    assert_eq!(
        service.status(McpClientId::Cursor, &current_server).state,
        RegistrationState::OwnedOutdatedRegistration
    );
    service
        .unregister(McpClientId::Cursor, &current_server)
        .expect("owned outdated registration is removed");

    let configuration = read_json(&cursor_directory.join("mcp.json"));
    assert!(
        configuration[MCP_SERVERS_PROPERTY]
            .get(MCP_SERVER_REGISTRATION_NAME)
            .is_none()
    );
}

#[test]
fn missing_owned_server_is_reported_as_broken_and_can_be_removed() {
    let directory = TestDirectory::new();
    let cursor_directory = directory.path.join(".cursor");
    fs::create_dir_all(&cursor_directory).expect("cursor directory is created");
    let server = directory
        .path
        .join("worklogger/MCP/0.2.0/worklogger-mcp.exe");
    write_registered_server(&cursor_directory.join("mcp.json"), &server);
    let service = service(&directory);

    assert_eq!(
        service.status(McpClientId::Cursor, &server).state,
        RegistrationState::BrokenRegistration
    );
    service
        .unregister(McpClientId::Cursor, &server)
        .expect("broken owned registration is removed");

    let configuration = read_json(&cursor_directory.join("mcp.json"));
    assert!(
        configuration[MCP_SERVERS_PROPERTY]
            .get(MCP_SERVER_REGISTRATION_NAME)
            .is_none()
    );
}

#[test]
fn conflicting_json_registration_is_reported_and_never_removed() {
    let directory = TestDirectory::new();
    let cursor_directory = directory.path.join(".cursor");
    fs::create_dir_all(&cursor_directory).expect("cursor directory is created");
    let configuration_path = cursor_directory.join("mcp.json");
    fs::write(&configuration_path, conflicting_configuration()).expect("fixture is written");
    let service = service(&directory);
    let server = directory.path.join("worklogger-mcp.exe");
    create_server(&server);

    assert_eq!(
        service.status(McpClientId::Cursor, &server).state,
        RegistrationState::ConflictingRegistration
    );
    service
        .unregister(McpClientId::Cursor, &server)
        .expect("conflict is preserved");
    assert_eq!(
        read_json(&configuration_path)[MCP_SERVERS_PROPERTY][MCP_SERVER_REGISTRATION_NAME]["command"],
        "other"
    );
}

#[test]
fn malformed_client_configuration_is_never_overwritten() {
    let directory = TestDirectory::new();
    let cursor_directory = directory.path.join(".cursor");
    fs::create_dir_all(&cursor_directory).expect("cursor directory is created");
    let configuration_path = cursor_directory.join("mcp.json");
    fs::write(&configuration_path, "not-json").expect("fixture is written");
    let service = service(&directory);
    let server = directory.path.join("worklogger-mcp.exe");
    create_server(&server);

    let result = service.register(McpClientId::Cursor, &server);

    assert!(result.is_err());
    assert_eq!(
        fs::read_to_string(configuration_path).expect("fixture remains readable"),
        "not-json"
    );
}

#[cfg(unix)]
#[test]
fn symbolic_linked_client_configuration_is_rejected() {
    use std::os::unix::fs::symlink;

    let directory = TestDirectory::new();
    let cursor_directory = directory.path.join(".cursor");
    fs::create_dir_all(&cursor_directory).expect("cursor directory is created");
    let real_configuration = directory.path.join("real.json");
    fs::write(&real_configuration, existing_configuration()).expect("fixture is written");
    symlink(&real_configuration, cursor_directory.join("mcp.json")).expect("link is created");
    let service = service(&directory);
    let server = directory.path.join("worklogger-mcp.exe");
    create_server(&server);

    assert!(service.register(McpClientId::Cursor, &server).is_err());
    assert_eq!(
        fs::read_to_string(real_configuration).expect("target remains readable"),
        existing_configuration()
    );
}

#[test]
fn unavailable_clients_are_reported_without_creating_directories() {
    let directory = TestDirectory::new();
    let service = service(&directory);
    let server = directory.path.join("worklogger-mcp.exe");

    let status = service.status(McpClientId::Windsurf, &server);

    assert_eq!(status.state, RegistrationState::Unavailable);
    assert!(!directory.path.join(".codeium").exists());
}

#[test]
fn claude_code_uses_its_detected_cli_and_preserves_user_configuration() {
    let directory = TestDirectory::new();
    let executable_directory = directory.path.join("bin");
    fs::create_dir_all(&executable_directory).expect("binary directory is created");
    fs::write(executable_directory.join(claude_executable_name()), []).expect("client is created");
    fs::write(directory.path.join(".claude.json"), r#"{"theme":"dark"}"#)
        .expect("configuration is created");
    let service =
        ClientRegistrationService::at(directory.path.clone(), None, vec![executable_directory]);
    let server = directory.path.join("worklogger-mcp.exe");
    create_server(&server);

    service
        .register(McpClientId::ClaudeCode, &server)
        .expect("registration succeeds");

    let configuration = read_json(&directory.path.join(".claude.json"));
    assert_eq!(configuration["theme"], "dark");
    assert_eq!(
        service.status(McpClientId::ClaudeCode, &server).state,
        RegistrationState::Registered
    );
}

#[cfg(unix)]
#[test]
fn codex_registration_passes_the_server_path_as_one_argument() {
    use std::os::unix::fs::PermissionsExt;

    let directory = TestDirectory::new();
    let executable_directory = directory.path.join("client bin");
    fs::create_dir_all(&executable_directory).expect("binary directory is created");
    let command_log = directory.path.join("command.log");
    let codex = executable_directory.join("codex");
    create_fake_client(&codex, &command_log);
    fs::set_permissions(
        &codex,
        fs::Permissions::from_mode(OWNER_EXECUTABLE_PERMISSIONS),
    )
    .expect("client is executable");
    let service =
        ClientRegistrationService::at(directory.path.clone(), None, vec![executable_directory]);
    let server = directory.path.join("server bin/worklogger mcp");
    create_server(&server);

    service
        .register(McpClientId::Codex, &server)
        .expect("registration succeeds");

    let arguments = fs::read_to_string(command_log).expect("arguments are recorded");
    let expected = format!(
        "mcp\nadd\n{MCP_SERVER_REGISTRATION_NAME}\n--\n{}\n{MCP_SERVER_SERVE_ARGUMENT}\n",
        server.display()
    );
    assert_eq!(arguments, expected);
}

#[cfg(unix)]
#[test]
fn codex_status_compares_the_registered_command_and_arguments() {
    use std::os::unix::fs::PermissionsExt;

    let directory = TestDirectory::new();
    let executable_directory = directory.path.join("bin");
    fs::create_dir_all(&executable_directory).expect("binary directory is created");
    let codex = executable_directory.join("codex");
    let server = directory.path.join("worklogger-mcp");
    create_server(&server);
    create_fake_codex_status(&codex, &server);
    fs::set_permissions(
        &codex,
        fs::Permissions::from_mode(OWNER_EXECUTABLE_PERMISSIONS),
    )
    .expect("client is executable");
    let service =
        ClientRegistrationService::at(directory.path.clone(), None, vec![executable_directory]);

    assert_eq!(
        service.status(McpClientId::Codex, &server).state,
        RegistrationState::Registered
    );
    assert_eq!(
        service
            .status(McpClientId::Codex, &directory.path.join("other"))
            .state,
        RegistrationState::ConflictingRegistration
    );
}

fn service(directory: &TestDirectory) -> ClientRegistrationService {
    ClientRegistrationService::at(directory.path.clone(), None, Vec::new())
}

fn existing_configuration() -> String {
    serde_json::json!({(MCP_SERVERS_PROPERTY): {"existing": {"command": "other", "args": []}}})
        .to_string()
}

fn conflicting_configuration() -> String {
    serde_json::json!({
        (MCP_SERVERS_PROPERTY): {
            (MCP_SERVER_REGISTRATION_NAME): {"command": "other", "args": []}
        }
    })
    .to_string()
}

fn write_registered_server(path: &Path, server: &Path) {
    let configuration = serde_json::json!({
        (MCP_SERVERS_PROPERTY): {
            (MCP_SERVER_REGISTRATION_NAME): {
                "command": server.display().to_string(),
                "args": ["serve"]
            }
        }
    });
    fs::write(path, configuration.to_string()).expect("registration fixture is written");
}

fn read_json(path: &Path) -> Value {
    let bytes = fs::read(path).expect("configuration is readable");
    serde_json::from_slice(&bytes).expect("configuration remains valid JSON")
}

fn create_server(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("server directory is created");
    }
    fs::write(path, []).expect("server fixture is created");
}

const fn claude_executable_name() -> &'static str {
    if cfg!(windows) {
        "claude.exe"
    } else {
        "claude"
    }
}

#[cfg(unix)]
fn create_fake_client(path: &Path, command_log: &Path) {
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
        command_log.display()
    );
    fs::write(path, script).expect("fake client is created");
}

#[cfg(unix)]
fn create_fake_codex_status(path: &Path, server: &Path) {
    let output = serde_json::json!([{
        "name": MCP_SERVER_REGISTRATION_NAME,
        "transport": {
            "type": "stdio",
            "command": server.display().to_string(),
            "args": [MCP_SERVER_SERVE_ARGUMENT]
        }
    }]);
    let script = format!("#!/bin/sh\ncat <<'JSON'\n{output}\nJSON\n");
    fs::write(path, script).expect("fake client is created");
}

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn new() -> Self {
        let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let name = format!(
            "worklogger-client-registration-{}-{sequence}",
            std::process::id()
        );
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
