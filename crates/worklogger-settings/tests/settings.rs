use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;
use worklogger_settings::{
    HoursSettings, Language, SettingsDocument, SettingsError, SettingsStore,
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "worklogger-shared-settings-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("test directory created");
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("test directory removed");
    }
}

#[test]
fn stale_frontend_cannot_overwrite_newer_settings() {
    let directory = Directory::new();
    let store = SettingsStore::at(directory.0.join("settings.json"));
    let initial = store.save(&SettingsDocument::default(), 0).unwrap();
    let mut desktop = initial.clone();
    desktop.hours = Some(HoursSettings {
        weekly_target_hours: Some(35),
        ..HoursSettings::default()
    });
    let saved = store.save(&desktop, initial.revision).unwrap();
    assert!(matches!(
        store.save(&initial, initial.revision),
        Err(SettingsError::Conflict {
            expected: 1,
            actual: 2
        })
    ));
    assert_eq!(store.load().unwrap(), Some(saved));
}

#[test]
fn orphaned_lock_file_does_not_block_a_new_writer() {
    let directory = Directory::new();
    let store = SettingsStore::at(directory.0.join("settings.json"));
    fs::write(directory.0.join("settings.json.lock"), []).unwrap();
    let saved = store.save(&SettingsDocument::default(), 0).unwrap();
    assert_eq!(saved.revision, 1);
    assert_eq!(store.load().unwrap(), Some(saved));
}

#[cfg(unix)]
#[test]
fn saved_settings_are_readable_only_by_the_owner() {
    use std::os::unix::fs::PermissionsExt;

    let directory = Directory::new();
    let path = directory.0.join("settings.json");
    let store = SettingsStore::at(path.clone());
    store.save(&SettingsDocument::default(), 0).unwrap();
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn supplied_unknown_secret_fields_are_rejected() {
    let value = json!({"schemaVersion":1,"revision":0,"jira":{"apiToken":"never-persist"}});
    assert!(serde_json::from_value::<SettingsDocument>(value).is_err());
}

#[test]
fn language_is_a_secret_free_shared_preference() {
    let directory = Directory::new();
    let store = SettingsStore::at(directory.0.join("settings.json"));
    let saved = store
        .save(
            &SettingsDocument {
                language: Some(Language::Spanish),
                ..SettingsDocument::default()
            },
            0,
        )
        .unwrap();
    assert_eq!(saved.language, Some(Language::Spanish));
    assert!(!fs::read_to_string(store.path()).unwrap().contains("token"));
}

#[test]
fn incompatible_time_values_fail_without_overflowing() {
    let document = SettingsDocument {
        hours: Some(HoursSettings {
            utc_offset_minutes: Some(i16::MIN),
            ..HoursSettings::default()
        }),
        ..SettingsDocument::default()
    };
    assert!(document.validate().is_err());
}
