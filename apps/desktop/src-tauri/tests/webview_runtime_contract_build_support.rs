//! Integration tests for the WebView2 runtime contract build support.

#[path = "../build-support/webview_runtime_contract.rs"]
mod webview_runtime_contract;

use std::{fs, path::PathBuf};

use webview_runtime_contract::{parse_contract, render_contract};

fn committed_config_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
    fs::read_to_string(path).expect("read Tauri config")
}

fn config_with_version(version: &serde_json::Value) -> String {
    serde_json::json!({
        "bundle": {
            "windows": {
                "minimumWebview2Version": version,
            },
        },
    })
    .to_string()
}

#[test]
fn committed_config_generates_the_expected_runtime_constant() {
    let contract = parse_contract(&committed_config_source()).expect("valid WebView2 contract");

    assert_eq!(contract.minimum_version, "136.0.3240.44");
    assert_eq!(contract.major, 136);
    assert_eq!(
        render_contract(&contract),
        "const CONFIGURED_MINIMUM_WEBVIEW2_VERSION: &str = \"136.0.3240.44\";\n"
    );
}

#[test]
fn missing_or_non_string_versions_are_rejected() {
    let missing = serde_json::json!({ "bundle": { "windows": {} } }).to_string();
    assert!(parse_contract(&missing).is_err());
    assert!(parse_contract(&config_with_version(&serde_json::json!(136))).is_err());
}

#[test]
fn malformed_zero_and_overflowing_versions_are_rejected() {
    for version in [
        "",
        "136.0.3240",
        "136.0.3240.44.1",
        "136.x.3240.44",
        "0.0.0.1",
        "136.4294967296.0.0",
    ] {
        assert!(
            parse_contract(&config_with_version(&serde_json::json!(version))).is_err(),
            "accepted invalid version {version:?}"
        );
    }
}
