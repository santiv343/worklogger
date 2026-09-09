use std::{env, fs, path::PathBuf};

use worklogger_profile::OrganizationProfile;

const PROFILE_ENVIRONMENT_VARIABLE: &str = "WORKLOGGER_DISTRIBUTION_PROFILE";
const MANAGED_FEATURE_ENVIRONMENT_VARIABLE: &str = "CARGO_FEATURE_MANAGED_DISTRIBUTION";
const GENERATED_PROFILE: &str = "worklogger-mcp-organization-profile.json";
const DEFAULT_PROFILE: &str = "../../desktop-app/resources/defaults.json";
const MAXIMUM_PROFILE_BYTES: usize = 1_048_576;

fn main() {
    announce_inputs();
    let profile = embedded_profile();
    write_generated_profile(&profile);
}

fn announce_inputs() {
    println!("cargo:rerun-if-env-changed={PROFILE_ENVIRONMENT_VARIABLE}");
    println!("cargo:rerun-if-env-changed={MANAGED_FEATURE_ENVIRONMENT_VARIABLE}");
}

fn feature_enabled(name: &str) -> bool {
    env::var_os(name).is_some()
}

fn embedded_profile() -> String {
    let path = selected_profile_path();
    println!("cargo:rerun-if-changed={}", path.display());
    let contents = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("no se pudo leer {}: {error}", path.display()));
    assert!(
        contents.len() <= MAXIMUM_PROFILE_BYTES,
        "el perfil supera {MAXIMUM_PROFILE_BYTES} bytes"
    );
    OrganizationProfile::from_json(&contents)
        .unwrap_or_else(|error| panic!("{} no es válido: {error}", path.display()))
        .to_pretty_json()
        .expect("el perfil validado debe serializarse")
}

fn selected_profile_path() -> PathBuf {
    managed_profile_path().unwrap_or_else(|| manifest_directory().join(DEFAULT_PROFILE))
}

fn managed_profile_path() -> Option<PathBuf> {
    feature_enabled(MANAGED_FEATURE_ENVIRONMENT_VARIABLE)
        .then(|| env::var_os(PROFILE_ENVIRONMENT_VARIABLE))
        .flatten()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn manifest_directory() -> PathBuf {
    PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo define CARGO_MANIFEST_DIR"))
}

fn write_generated_profile(profile: &str) {
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo define OUT_DIR"));
    fs::write(output.join(GENERATED_PROFILE), profile)
        .expect("Cargo debe permitir escribir el perfil MCP generado");
}
