use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use worklogger_mcp::McpServerInstallation;

static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn installation_copies_the_server_and_is_idempotent() {
    let directory = temporary_directory();
    let source = directory.join("source/worklogger-mcp");
    let destination = directory.join("installed/0.2.0/worklogger-mcp");
    fs::create_dir_all(source.parent().expect("source has a parent"))
        .expect("source directory is created");
    fs::write(&source, b"server-v1").expect("source is written");
    let installation = McpServerInstallation::at(destination.clone());

    assert_eq!(
        installation.install(&source).expect("install succeeds"),
        destination
    );
    installation
        .install(&source)
        .expect("repeated install succeeds");
    assert_eq!(
        fs::read(&destination).expect("installed server is readable"),
        b"server-v1"
    );
    fs::remove_dir_all(directory).expect("fixture is removed");
}

#[test]
fn installation_rejects_a_missing_source_without_creating_the_destination() {
    let directory = temporary_directory();
    let destination = directory.join("installed/worklogger-mcp");
    let installation = McpServerInstallation::at(destination.clone());

    assert!(installation.install(&directory.join("missing")).is_err());
    assert!(!destination.exists());
    fs::remove_dir_all(directory).expect("fixture is removed");
}

#[cfg(unix)]
#[test]
fn identical_installation_repairs_executable_permissions() {
    const EXECUTABLE_MODE: u32 = 0o700;
    const READ_ONLY_MODE: u32 = 0o600;
    let directory = temporary_directory();
    let source = directory.join("source/worklogger-mcp");
    let destination = directory.join("installed/worklogger-mcp");
    write_with_mode(&source, EXECUTABLE_MODE);
    write_with_mode(&destination, READ_ONLY_MODE);

    McpServerInstallation::at(destination.clone())
        .install(&source)
        .expect("reinstallation succeeds");

    assert_eq!(file_mode(&destination), EXECUTABLE_MODE);
    fs::remove_dir_all(directory).expect("fixture is removed");
}

#[cfg(unix)]
fn write_with_mode(path: &std::path::Path, mode: u32) {
    fs::create_dir_all(path.parent().expect("fixture has a parent")).unwrap();
    fs::write(path, b"same-server").unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

#[cfg(unix)]
fn file_mode(path: &std::path::Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn temporary_directory() -> PathBuf {
    let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "worklogger-runtime-installation-{}-{sequence}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).expect("temporary directory is created");
    directory
}
