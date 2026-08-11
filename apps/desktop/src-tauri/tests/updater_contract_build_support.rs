//! Characterization tests for the build-generated updater contract.

#[path = "../build-support/tauri_config.rs"]
mod tauri_config;
#[path = "../build-support/updater_contract.rs"]
mod updater_contract;

#[test]
fn overlay_key_is_the_only_key_rendered_into_the_runtime_contract() {
    let base = r#"{
        "productName": "RenderPilot",
        "plugins": {
            "updater": {
                "pubkey": "base-key",
                "endpoints": ["https://example.test/base/latest.json"]
            }
        }
    }"#;
    let overlay = r#"{
        "version": "1.9.0-test.2",
        "plugins": {
            "updater": {
                "pubkey": "smoke-key",
                "endpoints": ["https://example.test/overlay/latest.json"]
            }
        }
    }"#;

    let config = tauri_config::effective_config(base, Some(overlay)).expect("effective config");
    assert_eq!(config["productName"], "RenderPilot");
    assert_eq!(config["version"], "1.9.0-test.2");
    assert_eq!(
        updater_contract::render(&config).expect("render contract"),
        "pub(crate) const UPDATER_PUBLIC_KEY: &str = \"smoke-key\";\n\
         pub(crate) const UPDATER_ENDPOINTS: &[&str] = &[\"https://example.test/overlay/latest.json\"];\n"
    );
}

#[test]
fn missing_or_empty_public_key_fails_closed() {
    for source in [
        r#"{}"#,
        r#"{
            "plugins": {
                "updater": {
                    "pubkey": "",
                    "endpoints": ["https://example.test/latest.json"]
                }
            }
        }"#,
    ] {
        let config = tauri_config::effective_config(source, None).expect("valid config");
        assert!(updater_contract::render(&config).is_err());
    }
}

#[test]
fn missing_empty_or_non_https_endpoints_fail_closed() {
    for source in [
        r#"{ "plugins": { "updater": { "pubkey": "key" } } }"#,
        r#"{ "plugins": { "updater": { "pubkey": "key", "endpoints": [] } } }"#,
        r#"{ "plugins": { "updater": { "pubkey": "key", "endpoints": ["http://example.test/latest.json"] } } }"#,
    ] {
        let config = tauri_config::effective_config(source, None).expect("valid config");
        assert!(updater_contract::render(&config).is_err());
    }
}
