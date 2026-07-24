use std::ffi::OsString;

use renderpilot_orchestration::domain::{ArtifactId, ComponentId, GameId, GraphicsTechnology};

use super::command::Command;
use super::parse_args;
use crate::CliError;
fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

#[test]
fn no_args_parse_as_summary() {
    assert_eq!(
        parse_args(Vec::new()).expect("valid args"),
        Command::Summary
    );
}

#[test]
fn version_flag_parses() {
    assert_eq!(
        parse_args(args(&["--version"])).expect("valid args"),
        Command::Version
    );
}

#[test]
fn scan_folder_requires_path() {
    let error = parse_args(args(&["scan-folder"])).expect_err("path is required");

    assert_eq!(error, CliError::MissingArgument("<path>"));
}

#[test]
fn extra_arg_is_reported() {
    let error = parse_args(args(&["--version", "--bad"])).expect_err("extra arg should fail");

    assert_eq!(error, CliError::UnexpectedArgument("--bad".to_owned()));
}

#[test]
fn scan_folder_rejects_extra_arg() {
    let error =
        parse_args(args(&["scan-folder", "game-dir", "--bad"])).expect_err("extra arg should fail");

    assert_eq!(error, CliError::UnexpectedArgument("--bad".to_owned()));
}

#[test]
fn list_artifacts_parses_without_filter() {
    assert_eq!(
        parse_args(args(&["list-artifacts"])).expect("valid args"),
        Command::ListArtifacts { technology: None }
    );
}

#[test]
fn list_artifacts_parses_technology_filter() {
    assert_eq!(
        parse_args(args(&[
            "list-artifacts",
            "--technology",
            "dlss_super_resolution"
        ]))
        .expect("valid args"),
        Command::ListArtifacts {
            technology: Some(GraphicsTechnology::DlssSuperResolution)
        }
    );
}

#[test]
fn list_artifacts_rejects_unknown_technology() {
    let error = parse_args(args(&["list-artifacts", "--technology", "bad-tech"]))
        .expect_err("unknown technology should fail");

    assert_eq!(error, CliError::InvalidTechnology("bad-tech".to_owned()));
}

#[test]
fn candidates_requires_game_argument() {
    let error = parse_args(args(&["candidates"])).expect_err("game id should be required");

    assert_eq!(error, CliError::MissingArgument("<game_id>"));
}

#[test]
fn candidates_parses_game_argument() {
    assert_eq!(
        parse_args(args(&["candidates", "--game", "manual:C:/Games/GameA"])).expect("valid args"),
        Command::Candidates {
            game_id: GameId::new("manual:C:/Games/GameA").expect("game id should parse")
        }
    );
}

#[test]
fn list_operations_requires_game_argument() {
    let error = parse_args(args(&["list-operations"])).expect_err("game id should be required");

    assert_eq!(error, CliError::MissingArgument("<game_id>"));
}

#[test]
fn list_operations_parses_game_argument() {
    assert_eq!(
        parse_args(args(&[
            "list-operations",
            "--game",
            "manual:C:/Games/GameA"
        ]))
        .expect("valid args"),
        Command::ListOperations {
            game_id: GameId::new("manual:C:/Games/GameA").expect("game id should parse")
        }
    );
}

#[test]
fn plan_swap_requires_all_identifiers() {
    let error = parse_args(args(&["plan-swap", "--game", "manual:C:/Games/GameA"]))
        .expect_err("component id should be required");

    assert_eq!(error, CliError::MissingArgument("<component_id>"));

    let error = parse_args(args(&[
        "plan-swap",
        "--game",
        "manual:C:/Games/GameA",
        "--component",
        "component:game-a:dlss",
    ]))
    .expect_err("artifact id should be required");

    assert_eq!(error, CliError::MissingArgument("<artifact_id>"));
}

#[test]
fn plan_swap_parses_all_identifiers() {
    assert_eq!(
        parse_args(args(&[
            "plan-swap",
            "--game",
            "manual:C:/Games/GameA",
            "--component",
            "component:game-a:dlss",
            "--artifact",
            "artifact:dlss-3.7",
        ]))
        .expect("valid args"),
        Command::PlanSwap {
            game_id: GameId::new("manual:C:/Games/GameA").expect("game id should parse"),
            component_id: ComponentId::new("component:game-a:dlss")
                .expect("component id should parse"),
            artifact_id: ArtifactId::new("artifact:dlss-3.7").expect("artifact id should parse"),
        }
    );
}

#[test]
fn apply_requires_all_identifiers() {
    let error = parse_args(args(&["apply"])).expect_err("game id should be required");

    assert_eq!(error, CliError::MissingArgument("<game_id>"));
}

#[test]
fn apply_parses_all_identifiers() {
    assert_eq!(
        parse_args(args(&[
            "apply",
            "--game",
            "manual:C:/Games/GameA",
            "--component",
            "component:game-a:dlss",
            "--artifact",
            "artifact:dlss-3.7",
        ]))
        .expect("valid args"),
        Command::ApplyOperation {
            game_id: GameId::new("manual:C:/Games/GameA").expect("game id should parse"),
            component_id: ComponentId::new("component:game-a:dlss")
                .expect("component id should parse"),
            artifact_id: ArtifactId::new("artifact:dlss-3.7").expect("artifact id should parse"),
            confirmation_token: None,
        }
    );
}

#[test]
fn apply_parses_executable_confirmation_token() {
    assert_eq!(
        parse_args(args(&[
            "apply",
            "--game",
            "manual:C:/Games/GameA",
            "--component",
            "component:game-a:d3d12",
            "--artifact",
            "artifact:d3d12-619",
            "--confirmation-token",
            "fresh-preflight-fingerprint",
        ]))
        .expect("valid args"),
        Command::ApplyOperation {
            game_id: GameId::new("manual:C:/Games/GameA").expect("game id"),
            component_id: ComponentId::new("component:game-a:d3d12").expect("component id"),
            artifact_id: ArtifactId::new("artifact:d3d12-619").expect("artifact id"),
            confirmation_token: Some("fresh-preflight-fingerprint".to_owned()),
        }
    );
}

#[test]
fn apply_operation_alias_parses_all_identifiers() {
    assert_eq!(
        parse_args(args(&[
            "apply-operation",
            "--game",
            "manual:C:/Games/GameA",
            "--component",
            "component:game-a:dlss",
            "--artifact",
            "artifact:dlss-3.7",
        ]))
        .expect("valid args"),
        Command::ApplyOperation {
            game_id: GameId::new("manual:C:/Games/GameA").expect("game id should parse"),
            component_id: ComponentId::new("component:game-a:dlss")
                .expect("component id should parse"),
            confirmation_token: None,
            artifact_id: ArtifactId::new("artifact:dlss-3.7").expect("artifact id should parse"),
        }
    );
}

#[test]
fn rollback_requires_game_and_component() {
    let error = parse_args(args(&["rollback"])).expect_err("game id should be required");

    assert_eq!(error, CliError::MissingArgument("<game_id>"));
}

#[test]
fn rollback_parses_game_and_component() {
    assert_eq!(
        parse_args(args(&[
            "rollback",
            "--game",
            "manual:C:/Games/GameA",
            "--component",
            "component:game-a:dlss",
        ]))
        .expect("valid args"),
        Command::RollbackOperation {
            game_id: GameId::new("manual:C:/Games/GameA").expect("game id should parse"),
            component_id: ComponentId::new("component:game-a:dlss")
                .expect("component id should parse"),
        }
    );
}

#[test]
fn plan_rollback_parses_game_and_component_without_a_confirmation_token() {
    assert_eq!(
        parse_args(args(&[
            "plan-rollback",
            "--game",
            "manual:C:/Games/GameA",
            "--component",
            "component:game-a:d3d12",
        ]))
        .expect("valid args"),
        Command::PlanRollback {
            game_id: GameId::new("manual:C:/Games/GameA").expect("game id"),
            component_id: ComponentId::new("component:game-a:d3d12").expect("component id"),
        }
    );
    assert_eq!(
        parse_args(args(&[
            "plan-rollback",
            "--game",
            "manual:C:/Games/GameA",
            "--component",
            "component:game-a:d3d12",
            "--confirmation-token",
            "not-valid-for-a-plan",
        ]))
        .expect_err("a read-only plan never accepts confirmation"),
        CliError::UnexpectedArgument("--confirmation-token".to_owned())
    );
}

#[test]
fn rollback_rejects_obsolete_executable_confirmation_token() {
    assert_eq!(
        parse_args(args(&[
            "rollback",
            "--game",
            "manual:C:/Games/GameA",
            "--component",
            "component:game-a:d3d12",
            "--confirmation-token",
            "fresh-rollback-fingerprint",
        ]))
        .expect_err("safe rollback never accepts confirmation"),
        CliError::UnexpectedArgument("--confirmation-token".to_owned())
    );
}

#[test]
fn renodx_requires_subcommand() {
    let error = parse_args(args(&["renodx"])).expect_err("subcommand should be required");

    assert_eq!(
        error,
        CliError::MissingArgument("<status|uninstall|check-update|check-updates>")
    );
}

#[test]
fn renodx_rejects_unknown_subcommand() {
    let error =
        parse_args(args(&["renodx", "install"])).expect_err("unknown subcommand should fail");

    assert_eq!(error, CliError::UnknownArgument("install".to_owned()));
}

#[test]
fn renodx_requires_game_argument() {
    let error = parse_args(args(&["renodx", "status"])).expect_err("game id should be required");

    assert_eq!(error, CliError::MissingArgument("<game_id>"));
}

#[test]
fn renodx_status_parses_game_argument() {
    assert_eq!(
        parse_args(args(&[
            "renodx",
            "status",
            "--game",
            "manual:C:/Games/RenoGame"
        ]))
        .expect("valid args"),
        Command::RenodxStatus {
            game_id: GameId::new("manual:C:/Games/RenoGame").expect("game id should parse")
        }
    );
}

#[test]
fn renodx_uninstall_parses_game_argument() {
    assert_eq!(
        parse_args(args(&[
            "renodx",
            "uninstall",
            "--game",
            "manual:C:/Games/RenoGame"
        ]))
        .expect("valid args"),
        Command::RenodxUninstall {
            game_id: GameId::new("manual:C:/Games/RenoGame").expect("game id should parse")
        }
    );
}

#[test]
fn renodx_check_update_parses_game_argument() {
    assert_eq!(
        parse_args(args(&[
            "renodx",
            "check-update",
            "--game",
            "manual:C:/Games/RenoGame"
        ]))
        .expect("valid args"),
        Command::RenodxCheckUpdate {
            game_id: GameId::new("manual:C:/Games/RenoGame").expect("game id should parse")
        }
    );
}

#[test]
fn renodx_check_updates_parses() {
    assert_eq!(
        parse_args(args(&["renodx", "check-updates"])).expect("valid args"),
        Command::RenodxCheckUpdates
    );
}

#[test]
fn renodx_rejects_deep_flag() {
    let error = parse_args(args(&[
        "renodx",
        "check-update",
        "--game",
        "manual:C:/Games/X",
        "--deep",
    ]))
    .expect_err("renodx has no --deep");

    assert_eq!(error, CliError::UnexpectedArgument("--deep".to_owned()));
}

#[test]
fn renodx_rejects_extra_arg() {
    let error = parse_args(args(&[
        "renodx",
        "status",
        "--game",
        "manual:C:/Games/X",
        "--bad",
    ]))
    .expect_err("extra arg should fail");

    assert_eq!(error, CliError::UnexpectedArgument("--bad".to_owned()));
}

#[test]
fn luma_status_parses_game_argument() {
    assert_eq!(
        parse_args(args(&[
            "luma",
            "status",
            "--game",
            "manual:C:/Games/LumaGame"
        ]))
        .expect("valid args"),
        Command::LumaStatus {
            game_id: GameId::new("manual:C:/Games/LumaGame").expect("game id should parse")
        }
    );
}

#[test]
fn luma_check_update_parses_deep() {
    assert_eq!(
        parse_args(args(&[
            "luma",
            "check-update",
            "--game",
            "manual:C:/Games/LumaGame",
            "--deep",
        ]))
        .expect("valid args"),
        Command::LumaCheckUpdate {
            game_id: GameId::new("manual:C:/Games/LumaGame").expect("game id should parse"),
            deep: true,
        }
    );
}

#[test]
fn luma_check_updates_parses() {
    assert_eq!(
        parse_args(args(&["luma", "check-updates"])).expect("valid args"),
        Command::LumaCheckUpdates
    );
}
