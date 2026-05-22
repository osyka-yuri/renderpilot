use std::fs;
#[cfg(windows)]
use std::path::Path;

#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt as _;

use renderpilot_application::ComponentRepository;
use renderpilot_domain::{GraphicsTechnology, Swappability};

use crate::hash::sha256_hex;
use crate::run;

use super::{
    args, path_string, sample_artifact, sample_component, sample_game, CatalogFixture,
    TempGameFolder,
};

const REPLACEMENT_SHA256: &str = "70bf69c13743b7193ffd7a3718caab18522b61d4643fe13ac80caa5301e2345a";

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
    fixture.store_artifact(sample_artifact(
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
        .storage
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
    fixture.store_artifact(sample_artifact(
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
    fixture.store_artifact(sample_artifact(
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
        .storage
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
        .storage
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
    .expect_err("second rollback should fail because .bak is consumed");

    assert!(
        second_error
            .to_string()
            .contains("backup file does not exist"),
        "expected missing backup error, got: {}",
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
        error.to_string().contains("failed to restore backup"),
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

struct AppliedScenario {
    fixture: CatalogFixture,
    game_id: renderpilot_domain::GameId,
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
    fixture.store_artifact(sample_artifact(
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
