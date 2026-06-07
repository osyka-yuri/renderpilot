use renderpilot_orchestration::domain::{GraphicsTechnology, Swappability};

use crate::run;

use super::{args, sample_artifact, sample_component, sample_game, CatalogFixture};

#[test]
fn candidates_show_newer_update_for_same_technology_only() {
    let fixture = CatalogFixture::new("candidates-same-tech");
    let game_a = sample_game("manual:C:/Games/GameA", "Game A", "C:/Games/GameA");
    let game_b = sample_game("manual:C:/Games/GameB", "Game B", "C:/Games/GameB");

    fixture.store_game(&game_a);
    fixture.store_game(&game_b);
    fixture.store_components(
        game_a.id(),
        &[sample_component(
            "component:game-a:dlss",
            game_a.id().as_str(),
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
        "C:/Games/GameB/nvngx_dlss.dll",
        Some("3.7.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        Some(game_b.id().as_str()),
    ));
    fixture.store_artifact(sample_artifact(
        "artifact:fg-3.7",
        GraphicsTechnology::DlssFrameGeneration,
        "C:/Games/GameB/nvngx_dlssg.dll",
        Some("3.7.0"),
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        Some(game_b.id().as_str()),
    ));

    let output = run(args(&["candidates", "--game", game_a.id().as_str()]))
        .expect("candidates should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");
    let groups = json["groups"].as_array().expect("groups array");

    assert_eq!(json["game_id"], game_a.id().as_str());
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["technology"], "dlss_super_resolution");
    assert_eq!(groups[0]["current_version"], "3.5.0");
    assert_eq!(
        groups[0]["candidates"]
            .as_array()
            .expect("candidates array")
            .len(),
        1
    );
    assert_eq!(groups[0]["candidates"][0]["comparison"], "newer_version");
    assert_eq!(
        groups[0]["candidates"][0]["source_game_id"],
        game_b.id().as_str()
    );
    assert_eq!(groups[0]["candidates"][0]["file_name"], "nvngx_dlss.dll");
}

#[test]
fn candidates_offer_streamline_bundle_swap() {
    let fixture = CatalogFixture::new("candidates-streamline");
    let game_a = sample_game("manual:C:/Games/GameA", "Game A", "C:/Games/GameA");
    let game_b = sample_game("manual:C:/Games/GameB", "Game B", "C:/Games/GameB");

    fixture.store_game(&game_a);
    fixture.store_game(&game_b);
    fixture.store_components(
        game_a.id(),
        &[sample_component(
            "component:game-a:streamline",
            game_a.id().as_str(),
            GraphicsTechnology::NvidiaStreamline,
            Swappability::BundleOnly,
            "C:/Games/GameA/sl.interposer.dll",
            Some("2.4.0"),
            "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        )],
    );
    fixture.store_artifact(sample_artifact(
        "artifact:streamline-2.5",
        GraphicsTechnology::NvidiaStreamline,
        "C:/Games/GameB/sl.interposer.dll",
        Some("2.5.0"),
        "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        Some(game_b.id().as_str()),
    ));

    let output = run(args(&["candidates", "--game", game_a.id().as_str()]))
        .expect("candidates should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");

    // Streamline is now a full bundle swap: the candidate is offered, and the
    // dedicated per-candidate streamline warning is gone.
    let candidate = &json["groups"][0]["candidates"][0];
    assert_eq!(candidate["artifact_id"], "artifact:streamline-2.5");
    assert_eq!(candidate["comparison"], "newer_version");
    assert!(
        candidate.get("warning").is_none(),
        "candidate warning field should be removed, got: {candidate}"
    );
}
