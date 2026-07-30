use renderpilot_orchestration::domain::{LibraryTechnology, Swappability};

use crate::hash::sha256_hex;

use super::{
    CatalogFixture, TempGameFolder, args, path_string, sample_artifact, sample_bundle_artifact,
    sample_bundle_component, sample_component, sample_game,
};

#[test]
fn candidates_show_newer_update_for_same_technology_only() {
    let fixture = CatalogFixture::new("candidates-same-tech");
    let library_folder = TempGameFolder::new("candidates-same-tech-library");
    let dlss_path = write_candidate_file(&library_folder, "nvngx_dlss.dll", b"dlss-3.7");
    let fg_path = write_candidate_file(&library_folder, "nvngx_dlssg.dll", b"fg-3.7");
    let game_a = sample_game("manual:C:/Games/GameA", "Game A", "C:/Games/GameA");
    let game_b = sample_game("manual:C:/Games/GameB", "Game B", "C:/Games/GameB");

    fixture.store_game(&game_a);
    fixture.store_game(&game_b);
    fixture.store_components(
        game_a.id(),
        &[sample_component(
            "component:game-a:dlss",
            game_a.id().as_str(),
            LibraryTechnology::DlssSuperResolution,
            Swappability::Swappable,
            "C:/Games/GameA/nvngx_dlss.dll",
            Some("3.5.0"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )],
    );
    fixture.store_artifact(&sample_artifact(
        "artifact:dlss-3.7",
        LibraryTechnology::DlssSuperResolution,
        &dlss_path,
        Some("3.7.0"),
        &sha256_hex(b"dlss-3.7"),
        Some(game_b.id().as_str()),
    ));
    fixture.store_artifact(&sample_artifact(
        "artifact:fg-3.7",
        LibraryTechnology::DlssFrameGeneration,
        &fg_path,
        Some("3.7.0"),
        &sha256_hex(b"fg-3.7"),
        Some(game_b.id().as_str()),
    ));

    let output = fixture
        .run(args(&["candidates", "--game", game_a.id().as_str()]))
        .expect("candidates should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");
    let groups = json["groups"].as_array().expect("groups array");

    assert_eq!(json["game_id"], game_a.id().as_str());
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["technology"], "dlss_super_resolution");
    assert_eq!(groups[0]["version_report"]["kind"], "known");
    assert_eq!(groups[0]["version_report"]["technical_version"], "3.5.0");
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
    assert_eq!(groups[0]["candidates"][0]["technical_version"], "3.7.0");
}

#[test]
fn candidates_offer_streamline_bundle_swap() {
    let fixture = CatalogFixture::new("candidates-streamline");
    let library_folder = TempGameFolder::new("candidates-streamline-library");
    let streamline_path =
        write_candidate_file(&library_folder, "sl.interposer.dll", b"streamline-2.5");
    let game_a = sample_game("manual:C:/Games/GameA", "Game A", "C:/Games/GameA");
    let game_b = sample_game("manual:C:/Games/GameB", "Game B", "C:/Games/GameB");

    fixture.store_game(&game_a);
    fixture.store_game(&game_b);
    fixture.store_components(
        game_a.id(),
        &[sample_component(
            "component:game-a:streamline",
            game_a.id().as_str(),
            LibraryTechnology::NvidiaStreamline,
            Swappability::BundleOnly,
            "C:/Games/GameA/sl.interposer.dll",
            Some("2.4.0"),
            "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        )],
    );
    fixture.store_artifact(&sample_artifact(
        "artifact:streamline-2.5",
        LibraryTechnology::NvidiaStreamline,
        &streamline_path,
        Some("2.5.0"),
        &sha256_hex(b"streamline-2.5"),
        Some(game_b.id().as_str()),
    ));

    let output = fixture
        .run(args(&["candidates", "--game", game_a.id().as_str()]))
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

#[test]
fn candidates_serialize_mixed_and_unknown_version_reports() {
    // Groups are only emitted when at least one foreign artifact matches, so the
    // library rows below exist solely to surface the version_report payload.
    let fixture = CatalogFixture::new("candidates-version-report-states");
    let library_folder = TempGameFolder::new("candidates-version-report-library");
    let common_path =
        write_candidate_file(&library_folder, "sl.common.dll", b"streamline-common-2.5");
    let interposer_path = write_candidate_file(
        &library_folder,
        "sl.interposer.dll",
        b"streamline-interposer-2.5",
    );
    let dlss_path = write_candidate_file(&library_folder, "nvngx_dlss.dll", b"dlss-3.7");
    let game = sample_game("manual:C:/Games/GameA", "Game A", "C:/Games/GameA");
    let library = sample_game("manual:C:/Games/GameB", "Game B", "C:/Games/GameB");

    fixture.store_game(&game);
    fixture.store_game(&library);
    fixture.store_components(
        game.id(),
        &[
            sample_bundle_component(
                "component:game-a:streamline-mixed",
                game.id().as_str(),
                LibraryTechnology::NvidiaStreamline,
                Swappability::BundleOnly,
                &[
                    (
                        "C:/Games/GameA/sl.common.dll",
                        Some("2.9.0"),
                        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    ),
                    (
                        "C:/Games/GameA/sl.interposer.dll",
                        Some("2.4.0"),
                        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    ),
                ],
            ),
            sample_component(
                "component:game-a:dlss-unknown",
                game.id().as_str(),
                LibraryTechnology::DlssSuperResolution,
                Swappability::Swappable,
                "C:/Games/GameA/nvngx_dlss.dll",
                None,
                "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            ),
        ],
    );
    fixture.store_artifact(&sample_bundle_artifact(
        "artifact:streamline-2.5",
        LibraryTechnology::NvidiaStreamline,
        &[
            (
                common_path.as_str(),
                Some("2.5.0"),
                sha256_hex(b"streamline-common-2.5").as_str(),
            ),
            (
                interposer_path.as_str(),
                Some("2.5.0"),
                sha256_hex(b"streamline-interposer-2.5").as_str(),
            ),
        ],
        Some(library.id().as_str()),
    ));
    fixture.store_artifact(&sample_artifact(
        "artifact:dlss-3.7",
        LibraryTechnology::DlssSuperResolution,
        &dlss_path,
        Some("3.7.0"),
        &sha256_hex(b"dlss-3.7"),
        Some(library.id().as_str()),
    ));

    let output = fixture
        .run(args(&["candidates", "--game", game.id().as_str()]))
        .expect("candidates should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");
    let groups = json["groups"].as_array().expect("groups array");
    assert_eq!(
        groups.len(),
        2,
        "both components must emit a candidate group"
    );

    let mixed = groups
        .iter()
        .find(|group| group["component_id"] == "component:game-a:streamline-mixed")
        .expect("mixed streamline group");
    assert_eq!(
        mixed["version_report"],
        serde_json::json!({
            "kind": "mixed",
            "min_technical_version": "2.4.0",
            "max_technical_version": "2.9.0",
        })
    );
    assert_eq!(
        mixed["candidates"][0]["comparison"], "unknown_version",
        "mixed components have no known baseline for Newer/Older"
    );

    let unknown = groups
        .iter()
        .find(|group| group["component_id"] == "component:game-a:dlss-unknown")
        .expect("unknown version group");
    assert_eq!(
        unknown["version_report"],
        serde_json::json!({ "kind": "unknown" })
    );
    assert_eq!(unknown["candidates"][0]["comparison"], "unknown_version");
}

fn write_candidate_file(folder: &TempGameFolder, file_name: &str, bytes: &[u8]) -> String {
    std::fs::create_dir_all(folder.path()).expect("candidate folder should be created");
    let path = folder.path().join(file_name);
    std::fs::write(&path, bytes).expect("candidate file should be written");
    path_string(&path)
}
