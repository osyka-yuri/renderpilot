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
