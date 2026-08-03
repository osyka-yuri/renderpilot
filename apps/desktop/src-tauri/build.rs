//! Build script for the RenderPilot Tauri desktop shell.
//!
//! Tauri 2's `tauri-build` already embeds a Windows manifest with
//! `requestedExecutionLevel = asInvoker` and PerMonitorV2 DPI awareness,
//! which is exactly what we need. The EXE therefore does not auto-prompt
//! UAC at launch — elevation is requested at runtime in `src/elevation/`
//! (ShellExecuteW with verb=runas) when needed; if the user cancels the
//! UAC dialog the app keeps running with NVAPI writes disabled.
//!
//! If we ever need to override the default Tauri manifest, do NOT use
//! `embed-manifest` / `embed-resource` here — those collide with the
//! manifest baked into `resource.lib` by `tauri-build` (CVT1100). Use a
//! custom `app.manifest` referenced via `tauri.conf.json` instead.

#[path = "build-support/desktop_error_contract.rs"]
mod desktop_error_contract;

use std::{env, fs, path::PathBuf};

fn main() {
    generate_desktop_error_contract();
    tauri_build::build();
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
