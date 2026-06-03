use renderpilot_domain::GraphicsTechnology;
use renderpilot_domain::Swappability;

use crate::run;

use super::{args, sample_artifact, sample_component, sample_game, CatalogFixture};

#[test]
fn plan_swap_renders_operation_plan_json() {
    let fixture = CatalogFixture::new("plan-swap-valid");
    let game = sample_game("manual:C:/Games/GameA", "Game A", "C:/Games/GameA");

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[sample_component(
            "component:game-a:dlss",
            game.id().as_str(),
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            "C:/Games/GameA/nvngx_dlss.dll",
            Some("3.5.0"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )],
    );
    fixture.store_artifact(sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        "D:/Library/nvngx_dlss.dll",
        Some("3.7.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        None,
    ));

    let output = run(args(&[
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
    assert_eq!(json["target_path"], "C:/Games/GameA/nvngx_dlss.dll");
    assert_eq!(json["replacement_path"], "D:/Library/nvngx_dlss.dll");
    assert_eq!(json["original_version"], "3.5.0");
    assert_eq!(json["replacement_version"], "3.7.0");
    assert_eq!(json["risk_level"], "low");
    assert_eq!(json["requires_elevation"], false);
    assert_eq!(json["artifact_id"], "artifact:dlss-3.7");
    assert!(json["operation_id"]
        .as_str()
        .expect("operation id string")
        .starts_with("operation:replace_component:"));
    assert!(json["blockers"]
        .as_array()
        .expect("blockers array")
        .is_empty());
    assert!(json["warnings"]
        .as_array()
        .expect("warnings array")
        .is_empty());
}

#[test]
fn plan_swap_blocks_invalid_artifact() {
    let fixture = CatalogFixture::new("plan-swap-invalid-artifact");
    let game = sample_game("manual:C:/Games/GameA", "Game A", "C:/Games/GameA");

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[sample_component(
            "component:game-a:dlss",
            game.id().as_str(),
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            "C:/Games/GameA/nvngx_dlss.dll",
            Some("3.5.0"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )],
    );
    fixture.store_artifact(sample_artifact(
        "artifact:fg-3.7",
        GraphicsTechnology::DlssFrameGeneration,
        "D:/Library/nvngx_dlssg.dll",
        Some("3.7.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        None,
    ));

    let output = run(args(&[
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
    let game = sample_game("manual:C:/Games/GameA", "Game A", "C:/Games/GameA");

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[sample_component(
            "component:game-a:streamline",
            game.id().as_str(),
            GraphicsTechnology::NvidiaStreamline,
            Swappability::BundleOnly,
            "C:/Games/GameA/sl.interposer.dll",
            Some("2.4.0"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )],
    );
    fixture.store_artifact(sample_artifact(
        "artifact:streamline-2.5",
        GraphicsTechnology::NvidiaStreamline,
        "D:/Library/sl.interposer.dll",
        Some("2.5.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        None,
    ));

    let output = run(args(&[
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
    assert!(warnings
        .iter()
        .any(|warning| warning == "confirmation_required_for_swappability"));
    assert!(
        !warnings
            .iter()
            .any(|warning| warning == "streamline_partial_swap"),
        "streamline_partial_swap warning should no longer be emitted"
    );
}
