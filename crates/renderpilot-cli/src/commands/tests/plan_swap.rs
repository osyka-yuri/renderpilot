use renderpilot_orchestration::domain::GraphicsTechnology;
use renderpilot_orchestration::domain::Swappability;

use std::path::PathBuf;

use crate::hash::sha256_hex;

use super::{
    CatalogFixture, TempGameFolder, args, path_string, sample_artifact, sample_component,
    sample_game,
};

fn game_with_target(
    name: &str,
    file_name: &str,
    bytes: &[u8],
) -> (TempGameFolder, PathBuf, String) {
    let game_dir = TempGameFolder::new(name);
    std::fs::create_dir_all(game_dir.path()).expect("game dir");
    let target = game_dir.path().join(file_name);
    std::fs::write(&target, bytes).expect("installed");
    let install_path = path_string(game_dir.path());
    (game_dir, target, install_path)
}

fn artifact_source(name: &str, file_name: &str, bytes: &[u8]) -> (TempGameFolder, PathBuf) {
    let directory = TempGameFolder::new(name);
    std::fs::create_dir_all(directory.path()).expect("artifact dir");
    let path = directory.path().join(file_name);
    std::fs::write(&path, bytes).expect("artifact source");
    (directory, path)
}

#[test]
fn plan_swap_renders_operation_plan_json() {
    let fixture = CatalogFixture::new("plan-swap-valid");
    let (_game_dir, target, install_path) =
        game_with_target("plan-swap-valid-game", "nvngx_dlss.dll", b"installed-dlss");
    let (_artifact_dir, artifact_path) = artifact_source(
        "plan-swap-valid-artifact",
        "nvngx_dlss.dll",
        b"candidate-dlss",
    );
    let game = sample_game(&format!("manual:{install_path}"), "Game A", &install_path);

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[sample_component(
            "component:game-a:dlss",
            game.id().as_str(),
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            &path_string(&target),
            Some("3.5.0"),
            &sha256_hex(b"installed-dlss"),
        )],
    );
    fixture.store_artifact(&sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        &path_string(&artifact_path),
        Some("3.7.0"),
        &sha256_hex(b"candidate-dlss"),
        None,
    ));

    let output = fixture
        .run(args(&[
            "plan-swap",
            "--game",
            game.id().as_str(),
            "--component",
            "component:game-a:dlss",
            "--artifact",
            "artifact:dlss-3.7",
        ]))
        .expect("plan swap should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");

    assert_eq!(json["game_id"], game.id().as_str());
    assert_eq!(json["operation_type"], "replace_component");
    assert_eq!(json["target_path"], path_string(&target));
    assert_eq!(json["replacement_path"], path_string(&artifact_path));
    assert!(
        json["original_version"].is_null(),
        "metadata follows the verified on-disk baseline, not stale component version text"
    );
    assert_eq!(json["replacement_version"], "3.7.0");
    assert_eq!(json["risk_level"], "medium");
    assert_eq!(json["requires_elevation"], false);
    assert_eq!(json["artifact_id"], "artifact:dlss-3.7");
    assert!(
        json["operation_id"]
            .as_str()
            .expect("operation id string")
            .starts_with("operation:replace_component:")
    );
    assert!(
        json["blockers"]
            .as_array()
            .expect("blockers array")
            .is_empty()
    );
    assert_eq!(
        json["warnings"].as_array().expect("warnings array"),
        &[serde_json::Value::String(
            "manual_version_comparison_required".to_owned()
        )]
    );
}

#[test]
fn plan_swap_blocks_invalid_artifact() {
    let fixture = CatalogFixture::new("plan-swap-invalid-artifact");
    let (_game_dir, target, install_path) = game_with_target(
        "plan-swap-invalid-game",
        "nvngx_dlss.dll",
        b"installed-dlss",
    );
    let (_artifact_dir, artifact_path) = artifact_source(
        "plan-swap-invalid-artifact-source",
        "nvngx_dlssg.dll",
        b"candidate-frame-generation",
    );
    let game = sample_game(&format!("manual:{install_path}"), "Game A", &install_path);

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[sample_component(
            "component:game-a:dlss",
            game.id().as_str(),
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            &path_string(&target),
            Some("3.5.0"),
            &sha256_hex(b"installed-dlss"),
        )],
    );
    fixture.store_artifact(&sample_artifact(
        "artifact:fg-3.7",
        GraphicsTechnology::DlssFrameGeneration,
        &path_string(&artifact_path),
        Some("3.7.0"),
        &sha256_hex(b"candidate-frame-generation"),
        None,
    ));

    let output = fixture
        .run(args(&[
            "plan-swap",
            "--game",
            game.id().as_str(),
            "--component",
            "component:game-a:dlss",
            "--artifact",
            "artifact:fg-3.7",
        ]))
        .expect("plan swap should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");

    assert_eq!(json["risk_level"], "blocked");
    assert_eq!(json["blockers"][0], "technology_mismatch");
}

#[test]
fn plan_swap_surfaces_streamline_confirmation_warning() {
    let fixture = CatalogFixture::new("plan-swap-streamline");
    let (_game_dir, target, install_path) = game_with_target(
        "plan-swap-streamline-game",
        "sl.interposer.dll",
        b"installed-streamline",
    );
    let (_artifact_dir, artifact_path) = artifact_source(
        "plan-swap-streamline-artifact",
        "sl.interposer.dll",
        b"candidate-streamline",
    );
    let game = sample_game(&format!("manual:{install_path}"), "Game A", &install_path);

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[sample_component(
            "component:game-a:streamline",
            game.id().as_str(),
            GraphicsTechnology::NvidiaStreamline,
            Swappability::BundleOnly,
            &path_string(&target),
            Some("2.4.0"),
            &sha256_hex(b"installed-streamline"),
        )],
    );
    fixture.store_artifact(&sample_artifact(
        "artifact:streamline-2.5",
        GraphicsTechnology::NvidiaStreamline,
        &path_string(&artifact_path),
        Some("2.5.0"),
        &sha256_hex(b"candidate-streamline"),
        None,
    ));

    let output = fixture
        .run(args(&[
            "plan-swap",
            "--game",
            game.id().as_str(),
            "--component",
            "component:game-a:streamline",
            "--artifact",
            "artifact:streamline-2.5",
        ]))
        .expect("plan swap should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");
    let warnings = json["warnings"].as_array().expect("warnings array");

    // Streamline stays HIGH risk via the bundle-only confirmation warning now
    // that the dedicated streamline_partial_swap warning is gone.
    assert_eq!(json["risk_level"], "high");
    assert!(
        warnings
            .iter()
            .any(|warning| warning == "confirmation_required_for_swappability")
    );
    assert!(
        !warnings
            .iter()
            .any(|warning| warning == "streamline_partial_swap"),
        "streamline_partial_swap warning should no longer be emitted"
    );
}
