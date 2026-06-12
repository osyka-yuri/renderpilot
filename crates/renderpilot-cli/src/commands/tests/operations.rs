use std::fs;
use std::path::Path;

#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt as _;

use renderpilot_orchestration::application::{
    ComponentRepository, OperationItemRecord, OperationJournalEntry, OperationKind,
    OperationRecord, OperationRepository, OperationStatus, UnixTimestampMillis,
};
use renderpilot_orchestration::domain::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, GameInstallation,
    GraphicsTechnology, LibraryArtifact, OperationId, PathRef, Sha256Hash, Swappability, Version,
};

use crate::hash::sha256_hex;
use crate::run;

use super::{
    args, path_string, sample_artifact, sample_bundle_component, sample_component, sample_game,
    CatalogFixture, TempGameFolder,
};

const REPLACEMENT_SHA256: &str = "70bf69c13743b7193ffd7a3718caab18522b61d4643fe13ac80caa5301e2345a";
const FSR_COMPONENT_ID: &str = "component:fsr";
const FSR_ENTRY_POINT_FILE: &str = "amd_fidelityfx_dx12.dll";

#[test]
fn list_operations_renders_item_counts_from_aggregate_entries() {
    let fixture = CatalogFixture::new("list-operations");
    let game = sample_game("manual:C:/Games/GameA", "Game A", "C:/Games/GameA");

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[
            sample_component(
                "component:game-a:dlss",
                game.id().as_str(),
                GraphicsTechnology::DlssSuperResolution,
                Swappability::Swappable,
                "C:/Games/GameA/nvngx_dlss.dll",
                Some("3.5.0"),
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
            sample_component(
                "component:game-a:fg",
                game.id().as_str(),
                GraphicsTechnology::DlssFrameGeneration,
                Swappability::Swappable,
                "C:/Games/GameA/nvngx_dlssg.dll",
                Some("3.5.0"),
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            ),
        ],
    );

    let operation_id =
        OperationId::new("operation:replace_component:list").expect("operation id should be valid");
    let entry = OperationJournalEntry::try_new(
        OperationRecord::new(
            operation_id.clone(),
            game.id().clone(),
            OperationKind::ReplaceComponent,
            OperationStatus::Completed,
            UnixTimestampMillis::new(10).expect("timestamp should be valid"),
        ),
        vec![
            OperationItemRecord::new(
                operation_id.clone(),
                ComponentId::new("component:game-a:dlss").expect("component id should be valid"),
                PathRef::new("C:/Games/GameA/nvngx_dlss.dll").expect("path should be valid"),
                OperationStatus::Completed,
            ),
            OperationItemRecord::new(
                operation_id,
                ComponentId::new("component:game-a:fg").expect("component id should be valid"),
                PathRef::new("C:/Games/GameA/nvngx_dlssg.dll").expect("path should be valid"),
                OperationStatus::Completed,
            ),
        ],
    )
    .expect("journal entry should be valid");
    fixture
        .storage()
        .save_operation_entry(&entry)
        .expect("journal entry should be stored");

    let output = run(args(&["list-operations", "--game", game.id().as_str()]))
        .expect("list operations should succeed");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");
    let operations = json["operations"]
        .as_array()
        .expect("operations array should be present");

    assert_eq!(json["game_id"], game.id().as_str());
    assert_eq!(operations.len(), 1);
    assert_eq!(
        operations[0]["operation_id"],
        "operation:replace_component:list"
    );
    assert_eq!(operations[0]["item_count"], 2);
    assert_eq!(operations[0]["component_id"], "component:game-a:dlss");
}

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

    let error = run(args(&[
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

    let error = run(args(&[
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
        REPLACEMENT_SHA256,
        None,
    ));

    let apply_output = run(args(&[
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
        Some("3.7.0")
    );
    assert_eq!(
        components[0].files()[0]
            .sha256()
            .map(|sha256| sha256.as_str()),
        Some(REPLACEMENT_SHA256)
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
        REPLACEMENT_SHA256,
        None,
    ));

    run(args(&[
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
        Some("3.7.0"),
        "the DLSS component should reflect the new version"
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

    let output = run(args(&[
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
fn apply_replaces_target_even_when_changed_after_plan_swap() {
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

    run(args(&[
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

    run(args(&[
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
        fs::read(&source_path).expect("mutated target should be replaced"),
        b"replacement-bytes"
    );
    assert_eq!(components.len(), 1);
    assert_eq!(
        components[0].files()[0]
            .sha256()
            .map(|sha256| sha256.as_str()),
        Some(REPLACEMENT_SHA256)
    );
}

#[test]
fn rollback_restores_original_file_and_updates_catalog() {
    let scenario = setup_applied_scenario("rollback-success");

    let rollback_output = run(args(&[
        "rollback",
        "--game",
        scenario.game_id.as_str(),
        "--component",
        "component:game-a:dlss",
    ]))
    .expect("rollback should succeed");
    let rollback_json: serde_json::Value =
        serde_json::from_str(&rollback_output).expect("valid rollback json");
    let components = scenario
        .fixture
        .storage()
        .list_components_for_game(&scenario.game_id)
        .expect("components should load");

    assert_eq!(rollback_json["game_id"], scenario.game_id.as_str());
    assert_eq!(rollback_json["component_id"], "component:game-a:dlss");
    assert_eq!(
        fs::read(&scenario.source_path).expect("restored bytes should be readable"),
        ORIGINAL_BYTES
    );
    assert_eq!(sha256_hex(ORIGINAL_BYTES), scenario.original_sha256);
    assert_eq!(components.len(), 1);
    assert_eq!(components[0].files().len(), 1);
    assert_eq!(
        components[0].files()[0]
            .sha256()
            .map(|sha256| sha256.as_str()),
        Some(scenario.original_sha256.as_str())
    );
}

#[test]
fn rollback_consumes_bak_on_first_restore_and_second_fails() {
    let scenario = setup_applied_scenario("rollback-consumed");

    run(args(&[
        "rollback",
        "--game",
        scenario.game_id.as_str(),
        "--component",
        "component:game-a:dlss",
    ]))
    .expect("first rollback should succeed");

    let second_error = run(args(&[
        "rollback",
        "--game",
        scenario.game_id.as_str(),
        "--component",
        "component:game-a:dlss",
    ]))
    .expect_err("second rollback should fail because the baseline is cleared");

    assert!(
        second_error.to_string().contains("no swap to roll back"),
        "expected no-baseline error, got: {}",
        second_error
    );
}

#[cfg(windows)]
#[test]
fn rollback_fails_when_target_file_is_locked() {
    let scenario = setup_applied_scenario("rollback-locked");
    let lock = open_exclusive_file_lock(&scenario.source_path);

    let error = run(args(&[
        "rollback",
        "--game",
        scenario.game_id.as_str(),
        "--component",
        "component:game-a:dlss",
    ]))
    .expect_err("rollback should fail while target is locked");
    drop(lock);

    assert!(
        error.to_string().contains("before restore"),
        "expected restore failure error, got: {}",
        error
    );
    assert_eq!(
        fs::read(&scenario.source_path).expect("applied bytes should remain in place"),
        REPLACEMENT_BYTES
    );
}

const ORIGINAL_BYTES: &[u8] = b"original-bytes";
const REPLACEMENT_BYTES: &[u8] = b"replacement-bytes";

/// Writes a multi-file artifact bundle to disk and builds a `LibraryArtifact`
/// whose id follows the production bundle-id scheme. Returns the artifact and its
/// id string for use as the `--artifact` argument.
fn write_bundle_artifact(
    folder: &Path,
    technology: GraphicsTechnology,
    files: &[(&str, &[u8], Option<&str>)],
) -> (LibraryArtifact, String) {
    let component_files: Vec<ComponentFile> = files
        .iter()
        .map(|(name, bytes, install_as)| {
            let path = folder.join(name);
            fs::write(&path, bytes).expect("artifact file should be written");
            let mut file =
                ComponentFile::new(PathRef::new(path_string(&path)).expect("artifact path valid"))
                    .with_sha256(Sha256Hash::new(sha256_hex(bytes)).expect("sha256 valid"))
                    .with_version(Version::parse("4.0.0").expect("version valid"));
            if let Some(install_as) = install_as {
                file = file.with_install_as(*install_as);
            }
            file
        })
        .collect();

    let id = ArtifactId::for_bundle(component_files.iter().filter_map(ComponentFile::sha256));
    let id_string = id.as_str().to_owned();
    let artifact = LibraryArtifact::new(
        id,
        technology,
        files[0].0,
        component_files,
        ArtifactTrustLevel::LocalObserved,
    )
    .expect("bundle artifact should be valid");

    (artifact, id_string)
}

fn write_fsr_bundle_artifact(
    folder: &Path,
    files: &[(&str, &[u8], Option<&str>)],
) -> (LibraryArtifact, String) {
    write_bundle_artifact(folder, GraphicsTechnology::AmdFsr, files)
}

fn store_manual_game(
    fixture: &CatalogFixture,
    game_folder: &TempGameFolder,
    name: &str,
) -> GameInstallation {
    let install_path = path_string(game_folder.path());
    let game_id = format!("manual:{install_path}");
    let game = sample_game(&game_id, name, &install_path);
    fixture.store_game(&game);
    game
}

fn store_single_file_fsr_component(
    fixture: &CatalogFixture,
    game: &GameInstallation,
    path: &Path,
    version: &str,
    bytes: &[u8],
) {
    fixture.store_components(
        game.id(),
        &[sample_component(
            FSR_COMPONENT_ID,
            game.id().as_str(),
            GraphicsTechnology::AmdFsr,
            Swappability::BundleOnly,
            &path_string(path),
            Some(version),
            &sha256_hex(bytes),
        )],
    );
}

fn write_versioned_component_members<'a>(
    folder: &Path,
    members: &[(&'a str, &[u8], &'a str)],
) -> Vec<(String, Option<&'a str>, String)> {
    let mut written = Vec::with_capacity(members.len());
    for (name, bytes, version) in members {
        let path = folder.join(name);
        fs::write(&path, bytes).expect("member written");
        written.push((path_string(&path), Some(*version), sha256_hex(bytes)));
    }
    written
}

fn store_written_fsr_bundle_component<'a>(
    fixture: &CatalogFixture,
    game: &GameInstallation,
    written: &'a [(String, Option<&'a str>, String)],
) {
    let component_files: Vec<(&str, Option<&str>, &str)> = written
        .iter()
        .map(|(path, version, sha)| (path.as_str(), *version, sha.as_str()))
        .collect();

    fixture.store_components(
        game.id(),
        &[sample_bundle_component(
            FSR_COMPONENT_ID,
            game.id().as_str(),
            GraphicsTechnology::AmdFsr,
            Swappability::BundleOnly,
            &component_files,
        )],
    );
}

fn dir_file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("directory should be readable")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Headline FSR 3.1 → FSR 4 scenario: the loader installs *as* the game's
/// `amd_fidelityfx_dx12.dll` entry point (replacing it; the original is backed up
/// once), while the upscaler and frame-generation members are added alongside under
/// their own names. Rollback restores the original entry point and removes the two
/// added members, leaving the directory clean. Exercises first-swap backup of an
/// overwritten file, additive copies (no `.bak` for adds), and a deterministic N→1
/// rollback.
#[test]
fn apply_then_rollback_fsr_upgrade_replaces_entrypoint_and_adds_members() {
    let fixture = CatalogFixture::new("fsr-upgrade");
    let game_folder = TempGameFolder::new("fsr-upgrade-game");
    let artifact_folder = TempGameFolder::new("fsr-upgrade-artifact");
    fs::create_dir_all(game_folder.path()).expect("game folder");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder");

    // The FSR 3.1 game loads a single `amd_fidelityfx_dx12.dll` entry point.
    let original_name = FSR_ENTRY_POINT_FILE;
    let original_path = game_folder.path().join(original_name);
    fs::write(&original_path, b"fsr3-original").expect("original written");

    // FSR 4 package: the loader takes over `amd_fidelityfx_dx12.dll` via `install_as`;
    // the upscaler (the representative member) and frame generation are added under
    // their own names.
    let bundle: [(&str, &[u8], Option<&str>); 3] = [
        ("amd_fidelityfx_upscaler_dx12.dll", b"fsr4-upscaler", None),
        (
            "amd_fidelityfx_loader_dx12.dll",
            b"fsr4-loader",
            Some(original_name),
        ),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"fsr4-framegen",
            None,
        ),
    ];
    let (artifact, artifact_id) = write_fsr_bundle_artifact(artifact_folder.path(), &bundle);

    let game = store_manual_game(&fixture, &game_folder, "FSR Game");
    store_single_file_fsr_component(&fixture, &game, &original_path, "3.1.0", b"fsr3-original");
    fixture.store_artifact(&artifact);

    // --- apply (1 -> 3) ---
    run(args(&[
        "apply",
        "--game",
        game.id().as_str(),
        "--component",
        "component:fsr",
        "--artifact",
        &artifact_id,
    ]))
    .map(|output| serde_json::from_str::<serde_json::Value>(&output).expect("valid apply json"))
    .map(|json| {
        assert_eq!(json["component_id"], "component:fsr");
        assert_eq!(json["applied_path"], path_string(&original_path));
    })
    .expect("apply should succeed");

    // The loader took over the entry-point name; the original is backed up once.
    let original_bak = game_folder.path().join(format!("{original_name}.bak"));
    assert_eq!(
        fs::read(&original_path).expect("entry point present"),
        b"fsr4-loader",
        "the loader is installed as the entry point"
    );
    assert_eq!(
        fs::read(&original_bak).expect("entry point backed up"),
        b"fsr3-original",
        "the original FSR 3.1 entry point is preserved as .bak"
    );

    // The other members are added under their own names, with no `.bak`.
    let added: [(&str, &[u8]); 2] = [
        ("amd_fidelityfx_upscaler_dx12.dll", b"fsr4-upscaler"),
        ("amd_fidelityfx_framegeneration_dx12.dll", b"fsr4-framegen"),
    ];
    for (name, bytes) in added {
        let placed = game_folder.path().join(name);
        assert_eq!(fs::read(&placed).expect("member copied"), bytes);
        assert!(
            !game_folder.path().join(format!("{name}.bak")).exists(),
            "no .bak should be created for added member {name}"
        );
    }
    let components = fixture
        .storage()
        .list_components_for_game(game.id())
        .expect("components load");
    assert_eq!(components.len(), 1);
    assert_eq!(
        components[0].files().len(),
        3,
        "active set becomes the three-file FSR 4 package"
    );

    // --- rollback (3 -> 1) ---
    run(args(&[
        "rollback",
        "--game",
        game.id().as_str(),
        "--component",
        "component:fsr",
    ]))
    .expect("rollback should succeed");

    assert_eq!(
        fs::read(&original_path).expect("original restored"),
        b"fsr3-original",
        "rollback restores the original FSR 3.1 entry point"
    );
    assert!(!original_bak.exists(), ".bak consumed on restore");
    assert_eq!(
        dir_file_names(game_folder.path()),
        vec![original_name.to_string()],
        "directory is clean: only the original remains, no FSR 4 orphans"
    );
    let components = fixture
        .storage()
        .list_components_for_game(game.id())
        .expect("components load");
    assert_eq!(
        components[0].files().len(),
        1,
        "catalog rolled back to the single original file"
    );

    let second = run(args(&[
        "rollback",
        "--game",
        game.id().as_str(),
        "--component",
        "component:fsr",
    ]))
    .expect_err("second rollback should fail because the baseline is cleared");
    assert!(second.to_string().contains("no swap to roll back"));
}

/// Native FSR 4 components are single-file overlays: swapping the upscaler must
/// leave the loader and frame-generation siblings untouched, and rollback must
/// restore only that one DLL.
#[test]
fn apply_then_rollback_native_fsr_upscaler_only_touches_that_dll() {
    let fixture = CatalogFixture::new("native-fsr-upscaler");
    let game_folder = TempGameFolder::new("native-fsr-upscaler-game");
    let artifact_folder = TempGameFolder::new("native-fsr-upscaler-artifact");
    fs::create_dir_all(game_folder.path()).expect("game folder");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder");

    let loader_path = game_folder.path().join("amd_fidelityfx_loader_dx12.dll");
    let upscaler_path = game_folder.path().join("amd_fidelityfx_upscaler_dx12.dll");
    let framegen_path = game_folder
        .path()
        .join("amd_fidelityfx_framegeneration_dx12.dll");
    fs::write(&loader_path, b"native-loader").expect("loader written");
    fs::write(&upscaler_path, b"native-upscaler-a").expect("upscaler written");
    fs::write(&framegen_path, b"native-framegen").expect("framegen written");

    let replacement_path = artifact_folder
        .path()
        .join("amd_fidelityfx_upscaler_dx12.dll");
    fs::write(&replacement_path, b"native-upscaler-b").expect("replacement written");

    let install_path = path_string(game_folder.path());
    let game_id = format!("manual:{install_path}");
    let game = sample_game(&game_id, "Native FSR Game", &install_path);
    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[
            sample_component(
                "component:fsr-loader",
                game.id().as_str(),
                GraphicsTechnology::AmdFsrLoader,
                Swappability::Swappable,
                &path_string(&loader_path),
                Some("2.1.0"),
                &sha256_hex(b"native-loader"),
            ),
            sample_component(
                "component:fsr-upscaler",
                game.id().as_str(),
                GraphicsTechnology::AmdFsrUpscaler,
                Swappability::Swappable,
                &path_string(&upscaler_path),
                Some("4.0.3"),
                &sha256_hex(b"native-upscaler-a"),
            ),
            sample_component(
                "component:fsr-framegen",
                game.id().as_str(),
                GraphicsTechnology::AmdFsrFrameGeneration,
                Swappability::Swappable,
                &path_string(&framegen_path),
                Some("4.0.0"),
                &sha256_hex(b"native-framegen"),
            ),
        ],
    );
    fixture.store_artifact(&sample_artifact(
        "artifact:fsr-upscaler-4.1",
        GraphicsTechnology::AmdFsrUpscaler,
        &path_string(&replacement_path),
        Some("4.1.0"),
        &sha256_hex(b"native-upscaler-b"),
        None,
    ));

    run(args(&[
        "apply",
        "--game",
        game.id().as_str(),
        "--component",
        "component:fsr-upscaler",
        "--artifact",
        "artifact:fsr-upscaler-4.1",
    ]))
    .expect("apply should succeed");

    assert_eq!(
        fs::read(&upscaler_path).expect("upscaler present"),
        b"native-upscaler-b",
        "the upscaler should be replaced"
    );
    assert_eq!(
        fs::read(&loader_path).expect("loader present"),
        b"native-loader",
        "the loader must remain untouched"
    );
    assert_eq!(
        fs::read(&framegen_path).expect("framegen present"),
        b"native-framegen",
        "frame generation must remain untouched"
    );

    let upscaler_bak = game_folder
        .path()
        .join("amd_fidelityfx_upscaler_dx12.dll.bak");
    assert_eq!(
        fs::read(&upscaler_bak).expect("upscaler backup present"),
        b"native-upscaler-a",
        "the original upscaler should be backed up for rollback"
    );
    assert!(
        !game_folder
            .path()
            .join("amd_fidelityfx_loader_dx12.dll.bak")
            .exists(),
        "untouched siblings must not receive backup sidecars"
    );

    let components = fixture
        .storage()
        .list_components_for_game(game.id())
        .expect("components load");
    assert_eq!(components.len(), 3);
    let upscaler_component = components
        .iter()
        .find(|component| component.id().as_str() == "component:fsr-upscaler")
        .expect("upscaler component present");
    assert_eq!(
        upscaler_component.files()[0]
            .sha256()
            .map(|sha| sha.as_str()),
        Some(sha256_hex(b"native-upscaler-b").as_str()),
        "the catalog should track the replaced upscaler only"
    );

    run(args(&[
        "rollback",
        "--game",
        game.id().as_str(),
        "--component",
        "component:fsr-upscaler",
    ]))
    .expect("rollback should succeed");

    assert_eq!(
        fs::read(&upscaler_path).expect("upscaler restored"),
        b"native-upscaler-a",
        "rollback restores the original upscaler"
    );
    assert!(
        !upscaler_bak.exists(),
        "the upscaler backup is consumed on restore"
    );
    assert_eq!(
        fs::read(&loader_path).expect("loader present after rollback"),
        b"native-loader",
        "rollback still leaves the loader untouched"
    );
    assert_eq!(
        fs::read(&framegen_path).expect("framegen present after rollback"),
        b"native-framegen",
        "rollback still leaves frame generation untouched"
    );
}

/// Re-swapping a component (A → B → C) must keep the *original* A baseline so a
/// later rollback restores A, not the intermediate release B. Both FSR 4 releases
/// install their loader as the same `amd_fidelityfx_dx12.dll` entry point, so the
/// re-swap reverts to A before overlaying C — the backup always holds A, and B's
/// dropped member leaves no orphan.
#[test]
fn reswap_preserves_original_baseline_then_rollback_restores_it() {
    let fixture = CatalogFixture::new("bundle-reswap");
    let game_folder = TempGameFolder::new("bundle-reswap-game");
    let lib_b = TempGameFolder::new("bundle-reswap-b");
    let lib_c = TempGameFolder::new("bundle-reswap-c");
    fs::create_dir_all(game_folder.path()).expect("game folder");
    fs::create_dir_all(lib_b.path()).expect("lib b");
    fs::create_dir_all(lib_c.path()).expect("lib c");

    // The FSR 3.1 game loads a single `amd_fidelityfx_dx12.dll` entry point = A.
    let original_name = FSR_ENTRY_POINT_FILE;
    let original_path = game_folder.path().join(original_name);
    fs::write(&original_path, b"original-A").expect("original written");

    // Release B = loader(as dx12) + upscaler; release C = loader(as dx12) + framegen.
    // Each loader takes over the entry point; C drops B's upscaler member.
    let bundle_b: [(&str, &[u8], Option<&str>); 2] = [
        (
            "amd_fidelityfx_loader_dx12.dll",
            b"B-loader",
            Some(original_name),
        ),
        ("amd_fidelityfx_upscaler_dx12.dll", b"B-upscaler", None),
    ];
    let bundle_c: [(&str, &[u8], Option<&str>); 2] = [
        (
            "amd_fidelityfx_loader_dx12.dll",
            b"C-loader",
            Some(original_name),
        ),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"C-framegen",
            None,
        ),
    ];
    let (artifact_b, id_b) = write_fsr_bundle_artifact(lib_b.path(), &bundle_b);
    let (artifact_c, id_c) = write_fsr_bundle_artifact(lib_c.path(), &bundle_c);

    let game = store_manual_game(&fixture, &game_folder, "Reswap Game");
    store_single_file_fsr_component(&fixture, &game, &original_path, "3.1.0", b"original-A");
    fixture.store_artifact(&artifact_b);
    fixture.store_artifact(&artifact_c);

    let apply = |artifact_id: &str| {
        run(args(&[
            "apply",
            "--game",
            game.id().as_str(),
            "--component",
            "component:fsr",
            "--artifact",
            artifact_id,
        ]))
        .expect("apply should succeed");
    };

    apply(&id_b);
    apply(&id_c);

    // After A → B → C the directory holds release C plus the original A backup.
    let original_bak = game_folder.path().join(format!("{original_name}.bak"));
    assert_eq!(
        dir_file_names(game_folder.path()),
        vec![
            "amd_fidelityfx_dx12.dll".to_string(),
            "amd_fidelityfx_dx12.dll.bak".to_string(),
            "amd_fidelityfx_framegeneration_dx12.dll".to_string(),
        ],
        "B's upscaler member is gone; the entry point is backed up once"
    );
    assert_eq!(
        fs::read(&original_path).expect("entry point present"),
        b"C-loader",
        "the current entry point is release C's loader"
    );
    assert_eq!(
        fs::read(&original_bak).expect("backup present"),
        b"original-A",
        "the backup still holds the original A, not the intermediate release B"
    );

    run(args(&[
        "rollback",
        "--game",
        game.id().as_str(),
        "--component",
        "component:fsr",
    ]))
    .expect("rollback should succeed");

    assert_eq!(
        fs::read(&original_path).expect("original A restored"),
        b"original-A",
        "rollback restores the original A, not intermediate release B"
    );
    assert_eq!(
        dir_file_names(game_folder.path()),
        vec![original_name.to_string()],
        "directory clean after rollback across re-swaps"
    );
}

/// A game **already on FSR 4** (the three split DLLs, with the loader installed as
/// `amd_fidelityfx_dx12.dll`) upgraded to a newer FSR 4 release: every member is a
/// Replace (each backed up once), and rollback restores the *previous FSR 4
/// release* — never a synthetic FSR 3. There is no FSR 3 to fall back to here, so
/// the baseline is the FSR 4 set that was present when RenderPilot first swapped.
#[test]
fn already_fsr4_upgrade_replaces_all_members_then_rollback_restores_prior_release() {
    let fixture = CatalogFixture::new("fsr4-to-fsr4");
    let game_folder = TempGameFolder::new("fsr4-to-fsr4-game");
    let artifact_folder = TempGameFolder::new("fsr4-to-fsr4-artifact");
    fs::create_dir_all(game_folder.path()).expect("game folder");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder");

    // The game is already on FSR 4 release X: the loader sits under the entry-point
    // name `amd_fidelityfx_dx12.dll`, alongside the upscaler and frame generation.
    let members: [(&str, &[u8], &str); 3] = [
        ("amd_fidelityfx_dx12.dll", b"X-loader", "2.0.0"),
        ("amd_fidelityfx_upscaler_dx12.dll", b"X-upscaler", "4.0.2"),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"X-framegen",
            "3.1.5",
        ),
    ];
    let written = write_versioned_component_members(game_folder.path(), &members);

    let game = store_manual_game(&fixture, &game_folder, "FSR4 Game");
    store_written_fsr_bundle_component(&fixture, &game, &written);

    // FSR 4 release Y package: loader (as the dx12 entry point) + upscaler + framegen.
    let bundle: [(&str, &[u8], Option<&str>); 3] = [
        ("amd_fidelityfx_upscaler_dx12.dll", b"Y-upscaler", None),
        (
            "amd_fidelityfx_loader_dx12.dll",
            b"Y-loader",
            Some("amd_fidelityfx_dx12.dll"),
        ),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"Y-framegen",
            None,
        ),
    ];
    let (artifact, artifact_id) = write_fsr_bundle_artifact(artifact_folder.path(), &bundle);
    fixture.store_artifact(&artifact);

    // --- apply X -> Y: every member is replaced, each backed up once ---
    run(args(&[
        "apply",
        "--game",
        game.id().as_str(),
        "--component",
        "component:fsr",
        "--artifact",
        &artifact_id,
    ]))
    .expect("apply should succeed");

    let expectations: [(&str, &[u8], &[u8]); 3] = [
        ("amd_fidelityfx_dx12.dll", b"Y-loader", b"X-loader"),
        (
            "amd_fidelityfx_upscaler_dx12.dll",
            b"Y-upscaler",
            b"X-upscaler",
        ),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"Y-framegen",
            b"X-framegen",
        ),
    ];
    for (name, current, backup) in expectations {
        assert_eq!(
            fs::read(game_folder.path().join(name)).expect("member present"),
            current
        );
        assert_eq!(
            fs::read(game_folder.path().join(format!("{name}.bak"))).expect("member backup"),
            backup,
            "each replaced FSR 4 member is backed up once"
        );
    }
    let components = fixture
        .storage()
        .list_components_for_game(game.id())
        .expect("components load");
    assert_eq!(
        components[0].files().len(),
        3,
        "the active set is still the three-file FSR 4 release"
    );

    // --- rollback Y -> X: restores the prior FSR 4 release, not FSR 3 ---
    run(args(&[
        "rollback",
        "--game",
        game.id().as_str(),
        "--component",
        "component:fsr",
    ]))
    .expect("rollback should succeed");

    let originals: [(&str, &[u8]); 3] = [
        ("amd_fidelityfx_dx12.dll", b"X-loader"),
        ("amd_fidelityfx_upscaler_dx12.dll", b"X-upscaler"),
        ("amd_fidelityfx_framegeneration_dx12.dll", b"X-framegen"),
    ];
    for (name, original) in originals {
        assert_eq!(
            fs::read(game_folder.path().join(name)).expect("restored member"),
            original,
            "rollback restores the prior FSR 4 release, not FSR 3"
        );
    }
    assert_eq!(
        dir_file_names(game_folder.path()),
        vec![
            "amd_fidelityfx_dx12.dll".to_string(),
            "amd_fidelityfx_framegeneration_dx12.dll".to_string(),
            "amd_fidelityfx_upscaler_dx12.dll".to_string(),
        ],
        "exactly the prior FSR 4 release remains, with no .bak leftovers"
    );
}

/// A game **natively on FSR 4** loads the loader under its own name
/// `amd_fidelityfx_loader_dx12.dll` (it was never an FSR 3.1 game, so there is no
/// `amd_fidelityfx_dx12.dll`). An update must overwrite the loader *in place* — not
/// strand it behind a fresh `amd_fidelityfx_dx12.dll`. Every member is replaced, no
/// orphan entry point appears, and rollback restores the prior release.
#[test]
fn native_split_fsr4_update_targets_the_loader_in_place_without_orphan_entrypoint() {
    let fixture = CatalogFixture::new("native-fsr4");
    let game_folder = TempGameFolder::new("native-fsr4-game");
    let artifact_folder = TempGameFolder::new("native-fsr4-artifact");
    fs::create_dir_all(game_folder.path()).expect("game folder");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder");

    // Native FSR 4 release X: the loader is under its OWN name; no `amd_fidelityfx_dx12.dll`.
    let members: [(&str, &[u8], &str); 3] = [
        ("amd_fidelityfx_loader_dx12.dll", b"X-loader", "2.0.0"),
        ("amd_fidelityfx_upscaler_dx12.dll", b"X-upscaler", "4.0.2"),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"X-framegen",
            "3.1.5",
        ),
    ];
    let written = write_versioned_component_members(game_folder.path(), &members);

    let game = store_manual_game(&fixture, &game_folder, "Native FSR4 Game");
    store_written_fsr_bundle_component(&fixture, &game, &written);

    // FSR 4 release Y package: the loader's `install_as` default is `amd_fidelityfx_dx12.dll`,
    // but it must adapt to the game's real entry point (`amd_fidelityfx_loader_dx12.dll`).
    let bundle: [(&str, &[u8], Option<&str>); 3] = [
        ("amd_fidelityfx_upscaler_dx12.dll", b"Y-upscaler", None),
        (
            "amd_fidelityfx_loader_dx12.dll",
            b"Y-loader",
            Some("amd_fidelityfx_dx12.dll"),
        ),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"Y-framegen",
            None,
        ),
    ];
    let (artifact, artifact_id) = write_fsr_bundle_artifact(artifact_folder.path(), &bundle);
    fixture.store_artifact(&artifact);

    run(args(&[
        "apply",
        "--game",
        game.id().as_str(),
        "--component",
        "component:fsr",
        "--artifact",
        &artifact_id,
    ]))
    .expect("apply should succeed");

    // The loader was updated IN PLACE under its own name; no orphan dx12 entry point.
    assert!(
        !game_folder.path().join("amd_fidelityfx_dx12.dll").exists(),
        "no stray amd_fidelityfx_dx12.dll is created for a natively split game"
    );
    let expectations: [(&str, &[u8], &[u8]); 3] = [
        ("amd_fidelityfx_loader_dx12.dll", b"Y-loader", b"X-loader"),
        (
            "amd_fidelityfx_upscaler_dx12.dll",
            b"Y-upscaler",
            b"X-upscaler",
        ),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"Y-framegen",
            b"X-framegen",
        ),
    ];
    for (name, current, backup) in expectations {
        assert_eq!(
            fs::read(game_folder.path().join(name)).expect("member present"),
            current,
            "the loader and members are updated in place"
        );
        assert_eq!(
            fs::read(game_folder.path().join(format!("{name}.bak"))).expect("member backup"),
            backup,
        );
    }

    // Rollback restores release X exactly, still no orphan dx12.
    run(args(&[
        "rollback",
        "--game",
        game.id().as_str(),
        "--component",
        "component:fsr",
    ]))
    .expect("rollback should succeed");

    assert!(!game_folder.path().join("amd_fidelityfx_dx12.dll").exists());
    assert_eq!(
        dir_file_names(game_folder.path()),
        vec![
            "amd_fidelityfx_framegeneration_dx12.dll".to_string(),
            "amd_fidelityfx_loader_dx12.dll".to_string(),
            "amd_fidelityfx_upscaler_dx12.dll".to_string(),
        ],
        "rollback restores the prior native FSR 4 release in place"
    );
}

/// The FSR 3↔4 toggle: a dx12-lineage game (loads `amd_fidelityfx_dx12.dll`) we
/// upgraded to FSR 4 can pick a **unified FSR 3.1** version again from the selector.
/// The re-swap reverts to the FSR 3.1 baseline first — deleting the upscaler and
/// frame-gen — then installs the chosen 3.1.x, so the folder lands on a clean FSR 3.1
/// (no FSR 4 leftovers) while the original baseline is preserved for rollback.
#[test]
fn dx12_lineage_downgrade_to_unified_fsr3_cleans_up_split_members() {
    let fixture = CatalogFixture::new("fsr-downgrade");
    let game_folder = TempGameFolder::new("fsr-downgrade-game");
    let fsr4_folder = TempGameFolder::new("fsr-downgrade-fsr4");
    let fsr3_folder = TempGameFolder::new("fsr-downgrade-fsr3");
    for folder in [&game_folder, &fsr4_folder, &fsr3_folder] {
        fs::create_dir_all(folder.path()).expect("folder");
    }

    let original_name = FSR_ENTRY_POINT_FILE;
    let original_path = game_folder.path().join(original_name);
    fs::write(&original_path, b"fsr3-original").expect("original written");

    // FSR 4 package: loader (as the dx12 entry point) + upscaler + frame generation.
    let bundle: [(&str, &[u8], Option<&str>); 3] = [
        ("amd_fidelityfx_upscaler_dx12.dll", b"fsr4-upscaler", None),
        (
            "amd_fidelityfx_loader_dx12.dll",
            b"fsr4-loader",
            Some(original_name),
        ),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"fsr4-framegen",
            None,
        ),
    ];
    let (fsr4_artifact, fsr4_id) = write_fsr_bundle_artifact(fsr4_folder.path(), &bundle);

    // A newer unified FSR 3.1.4 backend (single `amd_fidelityfx_dx12.dll`).
    let fsr314_source = fsr3_folder.path().join(original_name);
    fs::write(&fsr314_source, b"fsr3.1.4").expect("fsr3.1.4 written");
    let fsr314 = sample_artifact(
        "artifact:fsr-3.1.4",
        GraphicsTechnology::AmdFsr,
        &path_string(&fsr314_source),
        Some("3.1.4"),
        &sha256_hex(b"fsr3.1.4"),
        None,
    );
    let fsr314_id = fsr314.id().as_str().to_owned();

    let game = store_manual_game(&fixture, &game_folder, "FSR Game");
    store_single_file_fsr_component(&fixture, &game, &original_path, "3.1.0", b"fsr3-original");
    fixture.store_artifact(&fsr4_artifact);
    fixture.store_artifact(&fsr314);

    let apply = |artifact_id: &str| {
        run(args(&[
            "apply",
            "--game",
            game.id().as_str(),
            "--component",
            "component:fsr",
            "--artifact",
            artifact_id,
        ]))
        .expect("apply should succeed");
    };

    // 3.1 -> 4, then 4 -> unified 3.1.4 (the downgrade via the selector).
    apply(&fsr4_id);
    apply(&fsr314_id);

    assert_eq!(
        dir_file_names(game_folder.path()),
        vec![
            "amd_fidelityfx_dx12.dll".to_string(),
            "amd_fidelityfx_dx12.dll.bak".to_string(),
        ],
        "downgrade leaves a clean FSR 3.1: the upscaler and frame-gen are gone"
    );
    assert_eq!(
        fs::read(&original_path).expect("entry point"),
        b"fsr3.1.4",
        "the entry point is now the chosen FSR 3.1.4 backend"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll.bak")).expect("backup"),
        b"fsr3-original",
        "the backup still holds the original FSR 3.1, so rollback returns there"
    );
    let components = fixture
        .storage()
        .list_components_for_game(game.id())
        .expect("components load");
    assert_eq!(
        components[0].files().len(),
        1,
        "the active set is a single unified FSR 3.1 file again"
    );

    run(args(&[
        "rollback",
        "--game",
        game.id().as_str(),
        "--component",
        "component:fsr",
    ]))
    .expect("rollback should succeed");
    assert_eq!(
        fs::read(&original_path).expect("restored"),
        b"fsr3-original",
        "rollback restores the original FSR 3.1"
    );
    assert_eq!(
        dir_file_names(game_folder.path()),
        vec![original_name.to_string()],
        "directory is clean after rollback"
    );
}

/// A folder upgraded to FSR 4 **outside** RenderPilot (loader installed as the dx12
/// entry point, no FSR 3.1 baseline) must still downgrade cleanly: the first swap
/// backs up and removes the split members, so a later rollback restores that external
/// FSR 4 state exactly.
#[test]
fn externally_upgraded_fsr4_downgrade_removes_split_members_on_first_swap() {
    let fixture = CatalogFixture::new("fsr-external");
    let game_folder = TempGameFolder::new("fsr-external-game");
    let fsr3_folder = TempGameFolder::new("fsr-external-fsr3");
    for folder in [&game_folder, &fsr3_folder] {
        fs::create_dir_all(folder.path()).expect("folder");
    }

    // Externally upgraded FSR 4: the loader sits under the dx12 entry-point name.
    let members: [(&str, &[u8], &str); 3] = [
        ("amd_fidelityfx_dx12.dll", b"X-loader", "2.0.0"),
        ("amd_fidelityfx_upscaler_dx12.dll", b"X-upscaler", "4.0.2"),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"X-framegen",
            "3.1.5",
        ),
    ];
    let written = write_versioned_component_members(game_folder.path(), &members);

    let fsr314_source = fsr3_folder.path().join(FSR_ENTRY_POINT_FILE);
    fs::write(&fsr314_source, b"fsr3.1.4").expect("fsr3.1.4 written");
    let fsr314 = sample_artifact(
        "artifact:fsr-3.1.4",
        GraphicsTechnology::AmdFsr,
        &path_string(&fsr314_source),
        Some("3.1.4"),
        &sha256_hex(b"fsr3.1.4"),
        None,
    );
    let fsr314_id = fsr314.id().as_str().to_owned();

    let game = store_manual_game(&fixture, &game_folder, "External FSR4 Game");
    store_written_fsr_bundle_component(&fixture, &game, &written);
    fixture.store_artifact(&fsr314);

    run(args(&[
        "apply",
        "--game",
        game.id().as_str(),
        "--component",
        "component:fsr",
        "--artifact",
        &fsr314_id,
    ]))
    .expect("apply should succeed");

    // The split members are removed (backed up), and the entry point is FSR 3.1.4.
    assert!(
        !game_folder
            .path()
            .join("amd_fidelityfx_upscaler_dx12.dll")
            .exists(),
        "the upscaler is removed on the downgrade"
    );
    assert!(
        !game_folder
            .path()
            .join("amd_fidelityfx_framegeneration_dx12.dll")
            .exists(),
        "the frame-gen is removed on the downgrade"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll")).expect("entry point"),
        b"fsr3.1.4",
    );
    let backups: [(&str, &[u8]); 3] = [
        ("amd_fidelityfx_dx12.dll.bak", b"X-loader"),
        ("amd_fidelityfx_upscaler_dx12.dll.bak", b"X-upscaler"),
        ("amd_fidelityfx_framegeneration_dx12.dll.bak", b"X-framegen"),
    ];
    for (name, bytes) in backups {
        assert_eq!(
            fs::read(game_folder.path().join(name)).expect("backup present"),
            bytes,
            "the external FSR 4 member is backed up so rollback can restore it"
        );
    }
    let components = fixture
        .storage()
        .list_components_for_game(game.id())
        .expect("components load");
    assert_eq!(
        components[0].files().len(),
        1,
        "active set is the single FSR 3.1"
    );

    // Rollback restores the external FSR 4 state exactly.
    run(args(&[
        "rollback",
        "--game",
        game.id().as_str(),
        "--component",
        "component:fsr",
    ]))
    .expect("rollback should succeed");
    assert_eq!(
        dir_file_names(game_folder.path()),
        vec![
            "amd_fidelityfx_dx12.dll".to_string(),
            "amd_fidelityfx_framegeneration_dx12.dll".to_string(),
            "amd_fidelityfx_upscaler_dx12.dll".to_string(),
        ],
        "rollback restores the external FSR 4 set, no orphans"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll")).expect("loader"),
        b"X-loader",
    );
}

/// Mixed lineage: a real unified FSR 3.1 entry point next to a loader+denoiser
/// Ray Regeneration stack the developers ship themselves. A unified FSR 3.1.x
/// update must replace ONLY the entry point — the RR stack is an independent,
/// working feature, not an upscaling leftover. A re-swap and a rollback must
/// leave it untouched too.
#[test]
fn mixed_lineage_unified_update_replaces_only_the_entry_point() {
    let fixture = CatalogFixture::new("fsr-mixed-unified");
    let game_folder = TempGameFolder::new("fsr-mixed-unified-game");
    let lib_a = TempGameFolder::new("fsr-mixed-unified-a");
    let lib_b = TempGameFolder::new("fsr-mixed-unified-b");
    for folder in [&game_folder, &lib_a, &lib_b] {
        fs::create_dir_all(folder.path()).expect("folder");
    }

    // Entry point = real unified FSR 3.1; loader+denoiser = the RR stack.
    let members: [(&str, &[u8], &str); 3] = [
        ("amd_fidelityfx_dx12.dll", b"fsr3-original", "1.0.1.41314"),
        ("amd_fidelityfx_loader_dx12.dll", b"rr-loader", "2.1.0.604"),
        (
            "amd_fidelityfx_denoiser_dx12.dll",
            b"rr-denoiser",
            "1.0.0.604",
        ),
    ];
    let written = write_versioned_component_members(game_folder.path(), &members);

    let unified_artifact = |folder: &TempGameFolder, bytes: &[u8], version: &str, id: &str| {
        let source = folder.path().join(FSR_ENTRY_POINT_FILE);
        fs::write(&source, bytes).expect("unified artifact written");
        sample_artifact(
            id,
            GraphicsTechnology::AmdFsr,
            &path_string(&source),
            Some(version),
            &sha256_hex(bytes),
            None,
        )
    };
    let fsr313 = unified_artifact(&lib_a, b"fsr3.1.3", "3.1.3", "artifact:fsr-3.1.3");
    let fsr314 = unified_artifact(&lib_b, b"fsr3.1.4", "3.1.4", "artifact:fsr-3.1.4");
    let fsr313_id = fsr313.id().as_str().to_owned();
    let fsr314_id = fsr314.id().as_str().to_owned();

    let game = store_manual_game(&fixture, &game_folder, "Mixed Lineage Game");
    store_written_fsr_bundle_component(&fixture, &game, &written);
    fixture.store_artifact(&fsr313);
    fixture.store_artifact(&fsr314);

    let apply = |artifact_id: &str| {
        run(args(&[
            "apply",
            "--game",
            game.id().as_str(),
            "--component",
            "component:fsr",
            "--artifact",
            artifact_id,
        ]))
        .expect("apply should succeed");
    };

    // --- unified update: only the entry point changes ---
    apply(&fsr314_id);

    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll")).expect("entry point"),
        b"fsr3.1.4",
        "the unified update replaces the FSR 3.1 the game loads"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll.bak")).expect("backup"),
        b"fsr3-original",
    );
    assert_eq!(
        dir_file_names(game_folder.path()),
        vec![
            "amd_fidelityfx_denoiser_dx12.dll".to_string(),
            "amd_fidelityfx_dx12.dll".to_string(),
            "amd_fidelityfx_dx12.dll.bak".to_string(),
            "amd_fidelityfx_loader_dx12.dll".to_string(),
        ],
        "the RR stack is untouched: no removals, no extra backups"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_loader_dx12.dll")).expect("rr loader"),
        b"rr-loader",
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_denoiser_dx12.dll")).expect("rr denoiser"),
        b"rr-denoiser",
    );

    // --- re-swap to another unified version: the RR stack still survives ---
    apply(&fsr313_id);

    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll")).expect("entry point"),
        b"fsr3.1.3",
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll.bak")).expect("backup"),
        b"fsr3-original",
        "the baseline backup still holds the original, not the intermediate 3.1.4"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_loader_dx12.dll")).expect("rr loader"),
        b"rr-loader",
        "a re-swap must not resurrect, remove, or overwrite the RR loader"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_denoiser_dx12.dll")).expect("rr denoiser"),
        b"rr-denoiser",
    );

    // --- rollback: the original entry point returns, the RR stack persists ---
    run(args(&[
        "rollback",
        "--game",
        game.id().as_str(),
        "--component",
        "component:fsr",
    ]))
    .expect("rollback should succeed");

    assert_eq!(
        dir_file_names(game_folder.path()),
        vec![
            "amd_fidelityfx_denoiser_dx12.dll".to_string(),
            "amd_fidelityfx_dx12.dll".to_string(),
            "amd_fidelityfx_loader_dx12.dll".to_string(),
        ],
        "rollback restores the entry point and leaves the RR stack in place"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll")).expect("restored"),
        b"fsr3-original",
    );
}

/// The mixed lineage upgraded to an FSR 4 package: the package's loader must
/// install AS the entry point (what the game loads for upscaling) — never onto
/// the RR stack's `amd_fidelityfx_loader_dx12.dll`, which pairs with the game's
/// own denoiser. Rollback removes the added members and leaves the RR stack as
/// it was.
#[test]
fn mixed_lineage_fsr4_package_targets_entry_point_not_the_rr_loader() {
    let fixture = CatalogFixture::new("fsr-mixed-package");
    let game_folder = TempGameFolder::new("fsr-mixed-package-game");
    let artifact_folder = TempGameFolder::new("fsr-mixed-package-artifact");
    for folder in [&game_folder, &artifact_folder] {
        fs::create_dir_all(folder.path()).expect("folder");
    }

    let members: [(&str, &[u8], &str); 3] = [
        ("amd_fidelityfx_dx12.dll", b"fsr3-original", "1.0.1.41314"),
        ("amd_fidelityfx_loader_dx12.dll", b"rr-loader", "2.1.0.604"),
        (
            "amd_fidelityfx_denoiser_dx12.dll",
            b"rr-denoiser",
            "1.0.0.604",
        ),
    ];
    let written = write_versioned_component_members(game_folder.path(), &members);

    // FSR 4 package: loader (as the dx12 entry point) + upscaler + framegen.
    let bundle: [(&str, &[u8], Option<&str>); 3] = [
        ("amd_fidelityfx_upscaler_dx12.dll", b"fsr4-upscaler", None),
        (
            "amd_fidelityfx_loader_dx12.dll",
            b"fsr4-loader",
            Some(FSR_ENTRY_POINT_FILE),
        ),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"fsr4-framegen",
            None,
        ),
    ];
    let (artifact, artifact_id) = write_fsr_bundle_artifact(artifact_folder.path(), &bundle);

    let game = store_manual_game(&fixture, &game_folder, "Mixed Lineage Game");
    store_written_fsr_bundle_component(&fixture, &game, &written);
    fixture.store_artifact(&artifact);

    run(args(&[
        "apply",
        "--game",
        game.id().as_str(),
        "--component",
        "component:fsr",
        "--artifact",
        &artifact_id,
    ]))
    .expect("apply should succeed");

    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll")).expect("entry point"),
        b"fsr4-loader",
        "the package loader takes over the entry point the game loads"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_loader_dx12.dll")).expect("rr loader"),
        b"rr-loader",
        "the RR stack's loader must not be overwritten by the package loader"
    );
    assert!(
        !game_folder
            .path()
            .join("amd_fidelityfx_loader_dx12.dll.bak")
            .exists(),
        "the RR loader is never touched, so it gets no backup"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_denoiser_dx12.dll")).expect("rr denoiser"),
        b"rr-denoiser",
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_upscaler_dx12.dll")).expect("upscaler"),
        b"fsr4-upscaler",
        "the upscaler is added under its own name"
    );
    assert_eq!(
        fs::read(
            game_folder
                .path()
                .join("amd_fidelityfx_framegeneration_dx12.dll")
        )
        .expect("framegen"),
        b"fsr4-framegen",
    );

    run(args(&[
        "rollback",
        "--game",
        game.id().as_str(),
        "--component",
        "component:fsr",
    ]))
    .expect("rollback should succeed");

    assert_eq!(
        dir_file_names(game_folder.path()),
        vec![
            "amd_fidelityfx_denoiser_dx12.dll".to_string(),
            "amd_fidelityfx_dx12.dll".to_string(),
            "amd_fidelityfx_loader_dx12.dll".to_string(),
        ],
        "rollback removes the added members; the RR stack and entry point are back to the original state"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll")).expect("restored"),
        b"fsr3-original",
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_loader_dx12.dll")).expect("rr loader"),
        b"rr-loader",
    );
}

/// A stale `.bak` left over from a crashed earlier run (no baseline row) must be
/// replaced by the *current* original on the first swap, so rollback restores the
/// real current bytes rather than the stale leftover.
#[test]
fn first_swap_replaces_stale_backup_so_rollback_restores_current_original() {
    let fixture = CatalogFixture::new("stale-bak");
    let game_folder = TempGameFolder::new("stale-bak-game");
    let artifact_folder = TempGameFolder::new("stale-bak-artifact");
    fs::create_dir_all(game_folder.path()).expect("game folder");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder");

    let name = "nvngx_dlss.dll";
    let original_path = game_folder.path().join(name);
    fs::write(&original_path, b"current-original").expect("original written");
    let original_sha = sha256_hex(b"current-original");

    // A stale leftover backup with unrelated bytes and no baseline row in the DB.
    let bak_path = game_folder.path().join(format!("{name}.bak"));
    fs::write(&bak_path, b"STALE-garbage").expect("stale bak written");

    let artifact_path = artifact_folder.path().join(name);
    fs::write(&artifact_path, b"replacement-bytes").expect("artifact written");

    let install_path = path_string(game_folder.path());
    let game_id = format!("manual:{install_path}");
    let game = sample_game(&game_id, "Game", &install_path);
    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[sample_component(
            "component:dlss",
            game.id().as_str(),
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            &path_string(&original_path),
            Some("3.5.0"),
            &original_sha,
        )],
    );
    fixture.store_artifact(&sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        &path_string(&artifact_path),
        Some("3.7.0"),
        REPLACEMENT_SHA256,
        None,
    ));

    run(args(&[
        "apply",
        "--game",
        game.id().as_str(),
        "--component",
        "component:dlss",
        "--artifact",
        "artifact:dlss-3.7",
    ]))
    .expect("apply should succeed");

    assert_eq!(
        fs::read(&bak_path).expect("backup readable"),
        b"current-original",
        "the stale backup must be replaced by the current original"
    );
    assert_eq!(
        fs::read(&original_path).expect("target readable"),
        b"replacement-bytes"
    );

    run(args(&[
        "rollback",
        "--game",
        game.id().as_str(),
        "--component",
        "component:dlss",
    ]))
    .expect("rollback should succeed");

    assert_eq!(
        fs::read(&original_path).expect("restored readable"),
        b"current-original",
        "rollback restores the current original, never the stale leftover"
    );
}

struct AppliedScenario {
    fixture: CatalogFixture,
    game_id: renderpilot_orchestration::domain::GameId,
    source_path: std::path::PathBuf,
    original_sha256: String,
    _game_folder: TempGameFolder,
    _artifact_folder: TempGameFolder,
}

fn setup_applied_scenario(name: &str) -> AppliedScenario {
    let fixture = CatalogFixture::new(name);
    let game_folder = TempGameFolder::new(&format!("{name}-game"));
    let artifact_folder = TempGameFolder::new(&format!("{name}-artifact"));

    fs::create_dir_all(game_folder.path()).expect("game folder should be created");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder should be created");

    let source_path = game_folder.path().join("nvngx_dlss.dll");
    let artifact_path = artifact_folder.path().join("nvngx_dlss.dll");
    fs::write(&source_path, ORIGINAL_BYTES).expect("source file should be written");
    fs::write(&artifact_path, REPLACEMENT_BYTES).expect("artifact file should be written");

    let original_sha256 = sha256_hex(ORIGINAL_BYTES);
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
            &original_sha256,
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

    run(args(&[
        "apply",
        "--game",
        game.id().as_str(),
        "--component",
        "component:game-a:dlss",
        "--artifact",
        "artifact:dlss-3.7",
    ]))
    .expect("apply should succeed");

    AppliedScenario {
        fixture,
        game_id: game.id().clone(),
        source_path,
        original_sha256,
        _game_folder: game_folder,
        _artifact_folder: artifact_folder,
    }
}

#[cfg(windows)]
fn open_exclusive_file_lock(path: &Path) -> std::fs::File {
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).share_mode(0);

    options.open(path).expect("exclusive file lock should open")
}
