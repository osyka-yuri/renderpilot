//! Integration tests for the shared desktop error contract build support.

#[path = "../build-support/desktop_error_contract.rs"]
mod desktop_error_contract;

use std::{fs, path::PathBuf};

use desktop_error_contract::{parse_contract, render_command_error_kinds};

fn contract_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../data/contracts/desktop-command-errors.json");
    fs::read_to_string(path).expect("read desktop command error contract")
}

#[test]
fn committed_contract_parses_and_generates_every_command_code() {
    let contract = parse_contract(&contract_source()).expect("valid contract");
    let generated = render_command_error_kinds(&contract);

    for error in &contract.command_errors {
        assert!(generated.contains(&error.rust_variant));
        assert!(generated.contains(&format!("{:?}", error.code)));
        let severity = if error.severity == "warning" {
            "Warning"
        } else {
            "Error"
        };
        assert!(generated.contains(&format!(
            "Self::{} => CommandErrorSeverity::{severity}",
            error.rust_variant
        )));
    }
}

#[test]
fn access_denied_has_the_generic_desktop_contract() {
    let contract = parse_contract(&contract_source()).expect("valid contract");
    let access_denied = contract
        .command_errors
        .iter()
        .find(|error| error.code == "access_denied")
        .expect("access_denied command error");

    assert_eq!(access_denied.rust_variant, "AccessDenied");
    assert_eq!(access_denied.severity, "error");
    assert_eq!(
        access_denied
            .actions
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["inspect_logs"]
    );
    let action_codes = contract
        .suggested_actions
        .iter()
        .map(|action| action.code.as_str())
        .collect::<Vec<_>>();
    assert_eq!(action_codes.len(), 9);
    assert!(action_codes.contains(&"inspect_logs"));
}

#[test]
fn updater_codes_are_manifested_once_and_generate_the_shared_variants() {
    let contract = parse_contract(&contract_source()).expect("valid contract");
    let generated = render_command_error_kinds(&contract);
    let expected = [
        ("app_update_apply_failed", "AppUpdateApplyFailed"),
        ("app_update_check_failed", "AppUpdateCheckFailed"),
        ("app_update_download_failed", "AppUpdateDownloadFailed"),
        ("app_update_install_failed", "AppUpdateInstallFailed"),
        ("app_update_invalid_session", "AppUpdateInvalidSession"),
        ("app_update_invalid_state", "AppUpdateInvalidState"),
        ("app_update_session_active", "AppUpdateSessionActive"),
        ("app_update_state_failed", "AppUpdateStateFailed"),
        ("app_update_supervisor_failed", "AppUpdateSupervisorFailed"),
    ];

    for (code, variant) in expected {
        let matches = contract
            .command_errors
            .iter()
            .filter(|error| error.code == code)
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "manifest must own {code} exactly once");
        let error = matches[0];
        assert_eq!(error.rust_variant, variant);
        assert_eq!(error.severity, "error");
        assert_eq!(
            error.message_key,
            "user_message.operation_could_not_complete"
        );
        assert_eq!(error.actions, ["inspect_logs"]);
        assert_eq!(
            generated
                .matches(&format!("Self::{variant} => \"{code}\""))
                .count(),
            1,
            "generated CommandErrorKind must map {code} exactly once"
        );
    }
}

#[test]
fn duplicate_codes_and_variants_are_rejected() {
    let mut value: serde_json::Value =
        serde_json::from_str(&contract_source()).expect("parse test JSON");
    let errors = value["commandErrors"]
        .as_array_mut()
        .expect("commandErrors array");
    errors.push(errors[0].clone());
    assert!(
        parse_contract(&value.to_string())
            .unwrap_err()
            .contains("duplicate")
    );

    let mut value: serde_json::Value =
        serde_json::from_str(&contract_source()).expect("parse test JSON");
    let errors = value["commandErrors"]
        .as_array_mut()
        .expect("commandErrors array");
    errors[1]["rustVariant"] = errors[0]["rustVariant"].clone();
    assert!(
        parse_contract(&value.to_string())
            .unwrap_err()
            .contains("duplicate")
    );
}

#[test]
fn invalid_references_are_rejected() {
    let mut value: serde_json::Value =
        serde_json::from_str(&contract_source()).expect("parse test JSON");
    value["commandErrors"][0]["actions"] = serde_json::json!(["missing_action"]);
    assert!(
        parse_contract(&value.to_string())
            .unwrap_err()
            .contains("unknown suggested action")
    );
}

#[test]
fn strict_fields_and_recovery_marker_are_rejected() {
    let mut value: serde_json::Value =
        serde_json::from_str(&contract_source()).expect("parse test JSON");
    value["unexpected"] = serde_json::json!(true);
    assert!(
        parse_contract(&value.to_string())
            .unwrap_err()
            .contains("unknown field")
    );

    let mut value: serde_json::Value =
        serde_json::from_str(&contract_source()).expect("parse test JSON");
    value["commandErrors"][0]["recoveryBundlePath"] = serde_json::json!(false);
    assert!(
        parse_contract(&value.to_string())
            .unwrap_err()
            .contains("must be true when present")
    );

    let mut value: serde_json::Value =
        serde_json::from_str(&contract_source()).expect("parse test JSON");
    value["commandErrors"][0]["recoveryBundlePath"] = serde_json::json!(true);
    assert!(
        parse_contract(&value.to_string())
            .unwrap_err()
            .contains("not an error")
    );
}

#[test]
fn schema_and_machine_identifier_constraints_are_rejected() {
    let mut value: serde_json::Value =
        serde_json::from_str(&contract_source()).expect("parse test JSON");
    value["schemaVersion"] = serde_json::json!(2);
    assert!(
        parse_contract(&value.to_string())
            .unwrap_err()
            .contains("unsupported")
    );

    let mut value: serde_json::Value =
        serde_json::from_str(&contract_source()).expect("parse test JSON");
    value["commandErrors"][0]["code"] = serde_json::json!(format!("a{}", "b".repeat(64)));
    assert!(
        parse_contract(&value.to_string())
            .unwrap_err()
            .contains("invalid command error")
    );

    let mut value: serde_json::Value =
        serde_json::from_str(&contract_source()).expect("parse test JSON");
    value["commandErrors"][0]["rustVariant"] = serde_json::json!("invalid_variant");
    assert!(
        parse_contract(&value.to_string())
            .unwrap_err()
            .contains("invalid Rust variant")
    );
}
