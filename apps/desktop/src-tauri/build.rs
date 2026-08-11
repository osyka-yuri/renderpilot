//! Build script for the RenderPilot Tauri desktop shell.
//!
//! Windows artifacts select an explicit, audited application manifest.
//! Release Apps require an administrator token at process creation; development
//! and release-tooling artifacts remain `asInvoker`. The selector is fail-closed
//! so that a release build can never silently receive the development manifest.

#[path = "build-support/desktop_error_contract.rs"]
mod desktop_error_contract;
#[path = "build-support/tauri_config.rs"]
mod tauri_config;
#[path = "build-support/updater_contract.rs"]
mod updater_contract;
#[path = "build-support/webview_runtime_contract.rs"]
mod webview_runtime_contract;
#[path = "build-support/windows_manifest.rs"]
mod windows_manifest;

use std::{env, fs, path::PathBuf};

fn main() {
    let tauri_config = effective_tauri_config();
    generate_desktop_error_contract();
    generate_updater_contract(&tauri_config);
    generate_webview_runtime_contract(&tauri_config);
    tauri_build::try_build(tauri_attributes()).expect("Tauri build configuration must be valid");
}

fn tauri_attributes() -> tauri_build::Attributes {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be available"),
    );
    windows_manifest::emit_rerun_directives(&manifest_dir);

    let Some(manifest) = windows_manifest::select_from_environment()
        .expect("Windows manifest selection must be explicit and valid")
    else {
        return tauri_build::Attributes::new();
    };

    tauri_build::Attributes::new()
        .windows_attributes(tauri_build::WindowsAttributes::new().app_manifest(manifest.contents()))
}

fn effective_tauri_config() -> serde_json::Value {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be available"),
    );
    let config_path = manifest_dir.join("tauri.conf.json");
    println!("cargo:rerun-if-changed={}", config_path.display());
    println!("cargo:rerun-if-env-changed=TAURI_CONFIG");

    let source = fs::read_to_string(&config_path).expect("Tauri config must be read");
    let overlay = env::var("TAURI_CONFIG").ok();
    tauri_config::effective_config(&source, overlay.as_deref())
        .expect("effective Tauri config must be valid")
}

fn generate_updater_contract(config: &serde_json::Value) {
    let generated = updater_contract::render(config).expect("updater contract must be valid");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR must be available"))
        .join("updater_contract.rs");
    fs::write(output, generated).expect("updater contract must be generated");
}

fn generate_webview_runtime_contract(config: &serde_json::Value) {
    let source = serde_json::to_string(config).expect("effective Tauri config must serialize");
    let contract = webview_runtime_contract::parse_contract(&source)
        .expect("Tauri runtime contract must be valid");
    let generated = webview_runtime_contract::render_contract(&contract);
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR must be available"))
        .join("webview_runtime_contract.rs");
    fs::write(output, generated).expect("WebView2 runtime contract must be generated");
}

fn generate_desktop_error_contract() {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be available"),
    );
    let contract_path = manifest_dir.join("../../../data/contracts/desktop-command-errors.json");
    let contract_path = contract_path
        .canonicalize()
        .expect("desktop command error contract must exist");
    println!("cargo:rerun-if-changed={}", contract_path.display());

    let source =
        fs::read_to_string(&contract_path).expect("desktop command error contract must be read");
    let contract = desktop_error_contract::parse_contract(&source)
        .expect("desktop command error contract must be valid");
    let generated = desktop_error_contract::render_command_error_kinds(&contract);
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR must be available"))
        .join("desktop_error_kinds.rs");
    fs::write(output, generated).expect("generated desktop error kinds must be written");
}
