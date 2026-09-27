//! Build script to mechanically extract the authoritative updater public key from `tauri.conf.json`.

use std::{env, fs, path::PathBuf};

const PLATFORM_CONFIGS: &[&str] = &[
    "tauri.windows.conf.json",
    "tauri.linux.conf.json",
    "tauri.macos.conf.json",
    "tauri.android.conf.json",
    "tauri.ios.conf.json",
];

fn main() {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be available"),
    );
    let tauri_src_dir = manifest_dir
        .join("../../apps/desktop/src-tauri")
        .canonicalize()
        .expect("apps/desktop/src-tauri directory must exist");
    let tauri_conf_path = tauri_src_dir.join("tauri.conf.json");

    println!("cargo:rerun-if-changed={}", tauri_src_dir.display());
    println!("cargo:rerun-if-env-changed=TAURI_CONFIG");

    let source = fs::read_to_string(&tauri_conf_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", tauri_conf_path.display()));
    let json: serde_json::Value = serde_json::from_str(&source)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", tauri_conf_path.display()));

    let pubkey = json
        .pointer("/plugins/updater/pubkey")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .expect("plugins.updater.pubkey must be configured in tauri.conf.json");

    check_platform_configs(&tauri_src_dir, pubkey);
    check_tauri_config_env(pubkey);

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR must be set"));
    let generated_code = format!(
        "/// Authoritative updater public key extracted mechanically from `tauri.conf.json`.\n\
         pub(crate) const UPDATER_PUBLIC_KEY: &str = {pubkey:?};\n"
    );

    fs::write(out_dir.join("updater_public_key.rs"), generated_code)
        .expect("failed to write generated updater_public_key.rs");
}

fn assert_pubkey_invariant(
    json: &serde_json::Value,
    expected: &str,
    source: &dyn std::fmt::Display,
) {
    let Some(value) = json.pointer("/plugins/updater/pubkey") else {
        return;
    };
    if value.as_str() != Some(expected) {
        panic!("{source} overrides plugins.updater.pubkey; it must match tauri.conf.json exactly");
    }
}

fn check_platform_configs(tauri_src_dir: &std::path::Path, expected_pubkey: &str) {
    for config_name in PLATFORM_CONFIGS {
        let path = tauri_src_dir.join(config_name);
        if !path.exists() {
            continue;
        }
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let platform_json: serde_json::Value = serde_json::from_str(&content)
            .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()));
        assert_pubkey_invariant(&platform_json, expected_pubkey, &path.display());
    }
}

fn check_tauri_config_env(expected_pubkey: &str) {
    let Ok(overlay) = env::var("TAURI_CONFIG") else {
        return;
    };
    let overlay_json: serde_json::Value = serde_json::from_str(&overlay)
        .unwrap_or_else(|error| panic!("failed to parse TAURI_CONFIG as JSON: {error}"));
    assert_pubkey_invariant(
        &overlay_json,
        expected_pubkey,
        &"TAURI_CONFIG environment variable",
    );
}
