use std::ffi::OsString;

use crate::{run, CliError};

use super::args;

#[test]
fn no_args_prints_summary() {
    let output = run(Vec::<OsString>::new()).expect("summary should render");

    assert!(output.contains("RenderPilot CLI"));
    assert!(output.contains("renderpilot --help"));
}

#[test]
fn version_flag_prints_version_line() {
    let output = run(args(&["--version"])).expect("version should render");

    assert_eq!(output, "RenderPilot 1.0.0\n");
}

#[test]
fn short_version_flag_prints_version_line() {
    let output = run(args(&["-V"])).expect("version should render");

    assert_eq!(output, "RenderPilot 1.0.0\n");
}

#[test]
fn help_flag_prints_usage() {
    let output = run(args(&["--help"])).expect("help should render");

    assert!(output.contains("Usage:"));
    assert!(output.contains("renderpilot scan-folder <path>"));
    assert!(output.contains("renderpilot list-artifacts [--technology <technology>]"));
    assert!(output.contains("renderpilot list-operations --game <game_id>"));
    assert!(output.contains("renderpilot candidates --game <game_id>"));
    assert!(output.contains(
        "renderpilot plan-swap --game <game_id> --component <component_id> --artifact <artifact_id>"
    ));
    assert!(output.contains(
        "renderpilot apply --game <game_id> --component <component_id> --artifact <artifact_id>"
    ));
    assert!(output.contains("renderpilot rollback --game <game_id> --component <component_id>"));
    assert!(output.contains("renderpilot --version"));
}

#[test]
fn unknown_arg_is_reported() {
    let error = run(args(&["--bad"])).expect_err("unknown arg should fail");

    assert_eq!(error, CliError::UnknownArgument("--bad".to_owned()));
}
