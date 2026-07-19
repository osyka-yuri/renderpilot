use std::fs;

use renderpilot_orchestration::application::ComponentRepository;
use renderpilot_orchestration::domain::{GraphicsTechnology, Swappability};

use crate::hash::sha256_hex;

use super::super::{
    CatalogFixture, TempGameFolder, args, path_string, sample_artifact, sample_component,
    sample_game,
};
use super::helpers::REPLACEMENT_SHA256;

#[test]
fn apply_rejects_blocked_technology_mismatch_before_mutating_files() {
    let fixture = CatalogFixture::new("apply-mismatch");
    let game_folder = TempGameFolder::new("apply-mismatch-game");
    let artifact_folder = TempGameFolder::new("apply-mismatch-artifact");

    fs::create_dir_all(game_folder.path()).expect("game folder should be created");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder should be created");

    let source_path = game_folder.path().join("nvngx_dlss.dll");
    let artifact_path = artifact_folder.path().join("nvngx_dlssg.dll");
    fs::write(&source_path, b"original-bytes").expect("source file should be written");
    fs::write(&artifact_path, b"mismatched-artifact").expect("artifact file should be written");

    let install_path = path_string(game_folder.path());
    let game_id = format!("manual:{install_path}");
    let game = sample_game(&game_id, "Game A", &install_path);

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[sample_component(
            "component:game-a:dlss",
            game.id().as_str(),
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            &path_string(&source_path),
            Some("3.5.0"),
            &sha256_hex(b"original-bytes"),
        )],
    );
    fixture.store_artifact(&sample_artifact(
        "artifact:fg-3.7",
        GraphicsTechnology::DlssFrameGeneration,
        &path_string(&artifact_path),
        Some("3.7.0"),
        &sha256_hex(b"mismatched-artifact"),
        None,
    ));

    let error = fixture
        .run(args(&[
            "apply",
            "--game",
            game.id().as_str(),
            "--component",
            "component:game-a:dlss",
            "--artifact",
            "artifact:fg-3.7",
        ]))
        .expect_err("apply should reject blocked mismatch");

    assert!(error.to_string().contains("technology_mismatch"));
    assert_eq!(
        fs::read(&source_path).expect("source bytes should remain unchanged"),
        b"original-bytes"
    );
    assert!(
        !source_path.with_extension("dll.bak").exists(),
        "blocked apply must not create a backup sidecar"
    );
}

#[test]
fn apply_rejects_artifact_that_already_matches_current_component() {
    let fixture = CatalogFixture::new("apply-noop");
    let game_folder = TempGameFolder::new("apply-noop-game");
    let artifact_folder = TempGameFolder::new("apply-noop-artifact");

    fs::create_dir_all(game_folder.path()).expect("game folder should be created");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder should be created");

    let source_path = game_folder.path().join("nvngx_dlss.dll");
    let artifact_path = artifact_folder.path().join("nvngx_dlss.dll");
    fs::write(&source_path, b"same-bytes").expect("source file should be written");
    fs::write(&artifact_path, b"same-bytes").expect("artifact file should be written");

    let install_path = path_string(game_folder.path());
    let source_sha256 = sha256_hex(b"same-bytes");
    let game_id = format!("manual:{install_path}");
    let game = sample_game(&game_id, "Game A", &install_path);

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[sample_component(
            "component:game-a:dlss",
            game.id().as_str(),
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            &path_string(&source_path),
            Some("3.7.0"),
            &source_sha256,
        )],
    );
    fixture.store_artifact(&sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        &path_string(&artifact_path),
        Some("3.7.0"),
        &source_sha256,
        None,
    ));

    let error = fixture
        .run(args(&[
            "apply",
            "--game",
            game.id().as_str(),
            "--component",
            "component:game-a:dlss",
            "--artifact",
            "artifact:dlss-3.7",
        ]))
        .expect_err("apply should reject a no-op artifact");

    assert!(error.to_string().contains("artifact_matches_current_file"));
    assert_eq!(
        fs::read(&source_path).expect("source bytes should remain unchanged"),
        b"same-bytes"
    );
    assert!(
        !source_path.with_extension("dll.bak").exists(),
        "no-op apply must not create a backup sidecar"
    );
}

#[test]
fn apply_swap_creates_sidecar_bak_and_updates_catalog() {
    let fixture = CatalogFixture::new("apply-swap");
    let game_folder = TempGameFolder::new("apply-swap-game");
    let artifact_folder = TempGameFolder::new("apply-swap-artifact");

    fs::create_dir_all(game_folder.path()).expect("game folder should be created");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder should be created");

    let source_path = game_folder.path().join("nvngx_dlss.dll");
    let artifact_path = artifact_folder.path().join("nvngx_dlss.dll");
    fs::write(&source_path, b"original-bytes").expect("source file should be written");
    fs::write(&artifact_path, b"replacement-bytes").expect("artifact file should be written");
    let source_sha256 = sha256_hex(b"original-bytes");
    let artifact_sha256 = sha256_hex(b"replacement-bytes");

    let install_path = path_string(game_folder.path());
    let artifact_path_string = path_string(&artifact_path);
    let source_path_string = path_string(&source_path);
    let game_id = format!("manual:{install_path}");
    let game = sample_game(&game_id, "Game A", &install_path);

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[sample_component(
            "component:game-a:dlss",
            game.id().as_str(),
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            &source_path_string,
            Some("3.5.0"),
            &source_sha256,
        )],
    );
    fixture.store_artifact(&sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        &artifact_path_string,
        Some("3.7.0"),
        &artifact_sha256,
        None,
    ));

    let apply_output = fixture
        .run(args(&[
            "apply",
            "--game",
            game.id().as_str(),
            "--component",
            "component:game-a:dlss",
            "--artifact",
            "artifact:dlss-3.7",
        ]))
        .expect("apply should succeed");
    let apply_json: serde_json::Value =
        serde_json::from_str(&apply_output).expect("valid apply json");
    let components = fixture
        .storage()
        .list_components_for_game(game.id())
        .expect("components should load");

    assert_eq!(apply_json["game_id"], game.id().as_str());
    assert_eq!(apply_json["component_id"], "component:game-a:dlss");
    assert_eq!(
        fs::read(&source_path).expect("applied bytes should be readable"),
        fs::read(&artifact_path).expect("artifact bytes should be readable")
    );
    let sidecar_path = source_path.with_extension("dll.bak");
    assert!(
        sidecar_path.exists(),
        ".bak sidecar should exist next to target after apply"
    );
    assert_eq!(components.len(), 1);
    assert_eq!(components[0].files().len(), 1);
    assert_eq!(
        components[0].files()[0]
            .version()
            .map(|version| version.as_str()),
        None,
        "a non-PE replacement must remain version-unknown rather than inheriting manifest metadata"
    );
    assert_eq!(
        components[0].files()[0]
            .sha256()
            .map(|sha256| sha256.as_str()),
        Some(artifact_sha256.as_str())
    );
}

/// Regression test: applying a swap to ONE component must not delete the
/// game's OTHER components from the catalog. Earlier, `apply_swap` called
/// `storage.replace_components_for_game(game_id, &[rebuilt])`, which the
/// SQLite layer interprets as "this is now the full set" and deletes any
/// component not in the slice. Symptom in the UI: after changing any
/// version in the GameDetailsPage selector, all other graphics tabs
/// disappeared until the next full rescan.
#[test]
fn apply_swap_preserves_sibling_components_for_same_game() {
    let fixture = CatalogFixture::new("apply-swap-keeps-siblings");
    let game_folder = TempGameFolder::new("apply-swap-keeps-siblings-game");
    let artifact_folder = TempGameFolder::new("apply-swap-keeps-siblings-artifact");

    fs::create_dir_all(game_folder.path()).expect("game folder should be created");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder should be created");

    // Two real DLLs side-by-side in the same game folder: DLSS (the one we
    // swap) and an FSR sibling we want to make sure survives.
    let dlss_source_path = game_folder.path().join("nvngx_dlss.dll");
    let fsr_sibling_path = game_folder.path().join("amd_fidelityfx_dx12.dll");
    let artifact_path = artifact_folder.path().join("nvngx_dlss.dll");
    fs::write(&dlss_source_path, b"dlss-original").expect("dlss source should be written");
    fs::write(&fsr_sibling_path, b"fsr-sibling-bytes").expect("fsr sibling should be written");
    fs::write(&artifact_path, b"dlss-replacement").expect("artifact should be written");

    let install_path = path_string(game_folder.path());
    let dlss_source_string = path_string(&dlss_source_path);
    let fsr_sibling_string = path_string(&fsr_sibling_path);
    let artifact_path_string = path_string(&artifact_path);
    let dlss_source_sha = sha256_hex(b"dlss-original");
    let fsr_sibling_sha = sha256_hex(b"fsr-sibling-bytes");
    let artifact_sha256 = sha256_hex(b"dlss-replacement");
    let game_id = format!("manual:{install_path}");
    let game = sample_game(&game_id, "Game With Two Components", &install_path);

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[
            sample_component(
                "component:game-a:dlss",
                game.id().as_str(),
                GraphicsTechnology::DlssSuperResolution,
                Swappability::Swappable,
                &dlss_source_string,
                Some("3.5.0"),
                &dlss_source_sha,
            ),
            sample_component(
                "component:game-a:fsr",
                game.id().as_str(),
                GraphicsTechnology::AmdFsr,
                Swappability::Swappable,
                &fsr_sibling_string,
                Some("3.1.0"),
                &fsr_sibling_sha,
            ),
        ],
    );
    fixture.store_artifact(&sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        &artifact_path_string,
        Some("3.7.0"),
        &artifact_sha256,
        None,
    ));

    fixture
        .run(args(&[
            "apply",
            "--game",
            game.id().as_str(),
            "--component",
            "component:game-a:dlss",
            "--artifact",
            "artifact:dlss-3.7",
        ]))
        .expect("apply should succeed");

    let components = fixture
        .storage()
        .list_components_for_game(game.id())
        .expect("components should load");

    assert_eq!(
        components.len(),
        2,
        "both DLSS and the FSR sibling must remain in the catalog after the swap"
    );
    let component_ids: Vec<&str> = components.iter().map(|c| c.id().as_str()).collect();
    assert!(
        component_ids.contains(&"component:game-a:dlss"),
        "the swapped DLSS component must still be present"
    );
    assert!(
        component_ids.contains(&"component:game-a:fsr"),
        "the untouched FSR sibling must still be present"
    );

    let dlss_component = components
        .iter()
        .find(|c| c.id().as_str() == "component:game-a:dlss")
        .expect("DLSS component must be present");
    assert_eq!(
        dlss_component.files()[0].version().map(|v| v.as_str()),
        None,
        "a non-PE replacement must remain version-unknown rather than inheriting manifest metadata"
    );
    let fsr_component = components
        .iter()
        .find(|c| c.id().as_str() == "component:game-a:fsr")
        .expect("FSR component must be present");
    assert_eq!(
        fsr_component.files()[0].sha256().map(|s| s.as_str()),
        Some(fsr_sibling_sha.as_str()),
        "the untouched FSR sibling should keep its original hash"
    );
}

#[test]
fn apply_succeeds_without_prior_sidecar_and_creates_sidecar_bak() {
    let fixture = CatalogFixture::new("apply-no-prior-sidecar");
    let game_folder = TempGameFolder::new("apply-no-prior-sidecar-game");
    let artifact_folder = TempGameFolder::new("apply-no-prior-sidecar-artifact");

    fs::create_dir_all(game_folder.path()).expect("game folder should be created");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder should be created");

    let source_path = game_folder.path().join("nvngx_dlss.dll");
    let artifact_path = artifact_folder.path().join("nvngx_dlss.dll");
    fs::write(&source_path, b"original-bytes").expect("source file should be written");
    fs::write(&artifact_path, b"replacement-bytes").expect("artifact file should be written");

    let install_path = path_string(game_folder.path());
    let artifact_path_string = path_string(&artifact_path);
    let source_path_string = path_string(&source_path);
    let source_sha256 = sha256_hex(b"original-bytes");
    let game_id = format!("manual:{install_path}");
    let game = sample_game(&game_id, "Game A", &install_path);

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[sample_component(
            "component:game-a:dlss",
            game.id().as_str(),
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            &source_path_string,
            Some("3.5.0"),
            &source_sha256,
        )],
    );
    fixture.store_artifact(&sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        &artifact_path_string,
        Some("3.7.0"),
        REPLACEMENT_SHA256,
        None,
    ));

    let output = fixture
        .run(args(&[
            "apply",
            "--game",
            game.id().as_str(),
            "--component",
            "component:game-a:dlss",
            "--artifact",
            "artifact:dlss-3.7",
        ]))
        .expect("apply should succeed without sidecar");
    let output_json: serde_json::Value =
        serde_json::from_str(&output).expect("apply output should be valid json");

    assert_eq!(output_json["game_id"], game.id().as_str());
    assert_eq!(output_json["component_id"], "component:game-a:dlss");
    assert_eq!(
        fs::read(&source_path).expect("source bytes should be replaced"),
        b"replacement-bytes"
    );
    let sidecar_path = source_path.with_extension("dll.bak");
    assert!(
        sidecar_path.exists(),
        ".bak sidecar should be created automatically by apply"
    );
    assert_eq!(
        fs::read(&sidecar_path).expect("sidecar bytes should be readable"),
        b"original-bytes",
        ".bak sidecar should contain original bytes"
    );
}

#[test]
fn apply_rejects_a_target_changed_after_plan_swap_without_mutating_it() {
    let fixture = CatalogFixture::new("apply-target-change");
    let game_folder = TempGameFolder::new("apply-target-change-game");
    let artifact_folder = TempGameFolder::new("apply-target-change-artifact");

    fs::create_dir_all(game_folder.path()).expect("game folder should be created");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder should be created");

    let source_path = game_folder.path().join("nvngx_dlss.dll");
    let artifact_path = artifact_folder.path().join("nvngx_dlss.dll");
    fs::write(&source_path, b"original-bytes").expect("source file should be written");
    fs::write(&artifact_path, b"replacement-bytes").expect("artifact file should be written");

    let install_path = path_string(game_folder.path());
    let artifact_path_string = path_string(&artifact_path);
    let source_path_string = path_string(&source_path);
    let source_sha256 = sha256_hex(b"original-bytes");
    let game_id = format!("manual:{install_path}");
    let game = sample_game(&game_id, "Game A", &install_path);

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[sample_component(
            "component:game-a:dlss",
            game.id().as_str(),
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            &source_path_string,
            Some("3.5.0"),
            &source_sha256,
        )],
    );
    fixture.store_artifact(&sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        &artifact_path_string,
        Some("3.7.0"),
        REPLACEMENT_SHA256,
        None,
    ));

    fixture
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

    fs::write(&source_path, b"mutated-target-bytes").expect("source file should be mutated");

    let error = fixture
        .run(args(&[
            "apply",
            "--game",
            game.id().as_str(),
            "--component",
            "component:game-a:dlss",
            "--artifact",
            "artifact:dlss-3.7",
        ]))
        .expect_err("apply must refuse a target that changed after scan");
    assert!(
        error.to_string().contains("changed on disk")
            || error.to_string().contains("hash mismatch"),
        "expected preflight hash mismatch, got: {error}"
    );
    assert_eq!(
        fs::read(&source_path).expect("mutated target must stay untouched"),
        b"mutated-target-bytes"
    );
}
