use std::{env, fs, path::PathBuf};

const DEFAULT_PROFILE: &str = "resources/defaults.json";
const GENERATED_PROFILE: &str = "worklogger-distribution-profile.json";
const PROFILE_ENVIRONMENT_VARIABLE: &str = "WORKLOGGER_DISTRIBUTION_PROFILE";
const MANAGED_FEATURE_ENVIRONMENT_VARIABLE: &str = "CARGO_FEATURE_MANAGED_DISTRIBUTION";
const CONFIGURABLE_FEATURE_ENVIRONMENT_VARIABLE: &str = "CARGO_FEATURE_CONFIGURABLE_ORGANIZATION";
const MUTABLE_CONFIGURATION_CFG: &str = "organization_configuration_mutable";
const MAXIMUM_PROFILE_BYTES: usize = 1_048_576;

fn main() {
    announce_inputs();
    let managed = feature_enabled(MANAGED_FEATURE_ENVIRONMENT_VARIABLE);
    announce_configuration_mode(managed);
    let source = selected_profile(managed);
    let profile = read_valid_json(&source);
    write_generated_profile(&profile);
    println!("cargo:rerun-if-changed={}", source.display());
}

fn announce_inputs() {
    println!("cargo:rerun-if-env-changed={PROFILE_ENVIRONMENT_VARIABLE}");
    println!("cargo:rerun-if-env-changed={MANAGED_FEATURE_ENVIRONMENT_VARIABLE}");
    println!("cargo:rerun-if-env-changed={CONFIGURABLE_FEATURE_ENVIRONMENT_VARIABLE}");
    println!("cargo:rustc-check-cfg=cfg({MUTABLE_CONFIGURATION_CFG})");
}

fn announce_configuration_mode(managed: bool) {
    if feature_enabled(CONFIGURABLE_FEATURE_ENVIRONMENT_VARIABLE) && !managed {
        println!("cargo:rustc-cfg={MUTABLE_CONFIGURATION_CFG}");
    }
}

fn feature_enabled(name: &str) -> bool {
    env::var_os(name).is_some()
}

fn selected_profile(managed: bool) -> PathBuf {
    let configured = env::var_os(PROFILE_ENVIRONMENT_VARIABLE).filter(|value| !value.is_empty());
    match (managed, configured) {
        (true, Some(path)) => resolve_path(PathBuf::from(path)),
        (false, Some(_)) => panic!("an embedded profile requires managed-distribution"),
        (_, None) => manifest_directory().join(DEFAULT_PROFILE),
    }
}

fn resolve_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        return path;
    }
    manifest_directory().join(path)
}

fn manifest_directory() -> PathBuf {
    PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo defines CARGO_MANIFEST_DIR"))
}

fn read_valid_json(path: &PathBuf) -> Vec<u8> {
    let bytes =
        fs::read(path).unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
    assert!(
        bytes.len() <= MAXIMUM_PROFILE_BYTES,
        "the distribution profile exceeds {MAXIMUM_PROFILE_BYTES} bytes"
    );
    serde_json::from_slice::<serde_json::Value>(&bytes)
        .unwrap_or_else(|error| panic!("{} does not contain valid JSON: {error}", path.display()));
    bytes
}

fn write_generated_profile(profile: &[u8]) {
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo defines OUT_DIR"));
    fs::write(output.join(GENERATED_PROFILE), profile)
        .expect("Cargo must allow writing the generated profile");
}
