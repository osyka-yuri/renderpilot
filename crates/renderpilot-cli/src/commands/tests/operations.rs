use std::{fs, path::Path};

#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt as _;

use renderpilot_application::{
    BackupRepository, ComponentRepository, OperationRepository, OperationStatus,
};
use renderpilot_domain::{GraphicsTechnology, Swappability};
use sha2::{Digest, Sha256};

use crate::run;

use super::{
    CatalogFixture, TempGameFolder, args, path_string, sample_artifact, sample_component,
    sample_game,
};

const REPLACEMENT_SHA256: &str =
    "70bf69c13743b7193ffd7a3718caab18522b61d4643fe13ac80caa5301e2345a";

#[test]
fn list_operations_reports_backup_readiness() {
    let fixture = CatalogFixture::new("list-operations");
    let game_folder = TempGameFolder::new("list-operations-game");
    let artifact_folder = TempGameFolder::new("list-operations-artifact");

    fs::create_dir_all(game_folder.path()).expect("game folder should be created");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder should be created");

    let source_path = game_folder.path().join("nvngx_dlss.dll");
    let artifact_path = artifact_folder.path().join("nvngx_dlss.dll");
    fs::write(&source_path, b"backup-source-bytes").expect("source file should be written");
    fs::write(&artifact_path, b"replacement-bytes").expect("artifact file should be written");
    let source_sha256 = sha256_hex(b"backup-source-bytes");

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

    let plan_output = run(args(&[
        "plan-swap",
        "--game",
        game.id().as_str(),
        "--component",
        "component:game-a:dlss",
        "--artifact",
        "artifact:dlss-3.7",
    ]))
    .expect("plan swap should render");
    let plan_json: serde_json::Value = serde_json::from_str(&plan_output).expect("valid json");
    let operation_id = plan_json["operation_id"]
        .as_str()
        .expect("operation id string")
        .to_owned();

    run(args(&["backup", "--operation", &operation_id])).expect("backup should succeed");

    let list_output = run(args(&["list-operations", "--game", game.id().as_str()]))
        .expect("operation list should render");
    let list_json: serde_json::Value = serde_json::from_str(&list_output).expect("valid json");

    assert_eq!(list_json["game_id"], game.id().as_str());
    assert_eq!(list_json["operations"].as_array().expect("operations array").len(), 1);
    assert_eq!(list_json["operations"][0]["operation_id"], operation_id);
    assert_eq!(list_json["operations"][0]["status"], "planned");
    assert_eq!(list_json["operations"][0]["backup_status"], "ready");
    assert_eq!(list_json["operations"][0]["backup_count"], 1);
    assert_eq!(list_json["operations"][0]["item_count"], 1);
}

#[test]
fn apply_operation_uses_saved_backup_and_updates_catalog() {
    let fixture = CatalogFixture::new("apply-operation");
    let game_folder = TempGameFolder::new("apply-operation-game");
    let artifact_folder = TempGameFolder::new("apply-operation-artifact");

    fs::create_dir_all(game_folder.path()).expect("game folder should be created");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder should be created");

    let source_path = game_folder.path().join("nvngx_dlss.dll");
    let artifact_path = artifact_folder.path().join("nvngx_dlss.dll");
    fs::write(&source_path, b"backup-source-bytes").expect("source file should be written");
    fs::write(&artifact_path, b"replacement-bytes").expect("artifact file should be written");
    let source_sha256 = sha256_hex(b"backup-source-bytes");

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

    let plan_output = run(args(&[
        "plan-swap",
        "--game",
        game.id().as_str(),
        "--component",
        "component:game-a:dlss",
        "--artifact",
        "artifact:dlss-3.7",
    ]))
    .expect("plan swap should render");
    let plan_json: serde_json::Value = serde_json::from_str(&plan_output).expect("valid json");
    let operation_id = renderpilot_domain::OperationId::new(
        plan_json["operation_id"]
            .as_str()
            .expect("operation id string")
            .to_owned(),
    )
    .expect("operation id should parse");

    let backup_output =
        run(args(&["backup", "--operation", operation_id.as_str()])).expect("backup should succeed");
    let backup_json: serde_json::Value =
        serde_json::from_str(&backup_output).expect("valid backup json");
    let backup_path = backup_json["items"][0]["backup_path"]
        .as_str()
        .expect("backup path string")
        .to_owned();

    let apply_output = run(args(&["apply", "--operation", operation_id.as_str()]))
        .expect("apply should succeed");
    let apply_json: serde_json::Value = serde_json::from_str(&apply_output).expect("valid apply json");
    let operation = fixture
        .storage
        .find_operation(&operation_id)
        .expect("operation lookup should succeed")
        .expect("operation should exist");
    let items = fixture
        .storage
        .list_operation_items(&operation_id)
        .expect("operation items should load");
    let components = fixture
        .storage
        .list_components_for_game(game.id())
        .expect("components should load");

    assert_eq!(apply_json["operation_id"], operation_id.as_str());
    assert_eq!(apply_json["status"], "completed");
    assert_eq!(apply_json["items"][0]["backup_path"], backup_path);
    assert_eq!(
        fs::read(&source_path).expect("applied bytes should be readable"),
        fs::read(&artifact_path).expect("artifact bytes should be readable")
    );
    assert_eq!(operation.status, OperationStatus::Completed);
    assert!(operation.completed_at.is_some());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, OperationStatus::Completed);
    assert_eq!(components.len(), 1);
    assert_eq!(components[0].files().len(), 1);
    assert_eq!(
        components[0].files()[0].version().map(|version| version.as_str()),
        Some("3.7.0")
    );
    assert_eq!(
        components[0].files()[0].sha256().map(|sha256| sha256.as_str()),
        Some(REPLACEMENT_SHA256)
    );
}

#[test]
fn apply_is_blocked_when_backup_is_missing() {
    let fixture = CatalogFixture::new("apply-blocked-no-backup");
    let game_folder = TempGameFolder::new("apply-blocked-no-backup-game");
    let artifact_folder = TempGameFolder::new("apply-blocked-no-backup-artifact");

    fs::create_dir_all(game_folder.path()).expect("game folder should be created");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder should be created");

    let source_path = game_folder.path().join("nvngx_dlss.dll");
    let artifact_path = artifact_folder.path().join("nvngx_dlss.dll");
    fs::write(&source_path, b"backup-source-bytes").expect("source file should be written");
    fs::write(&artifact_path, b"replacement-bytes").expect("artifact file should be written");

    let install_path = path_string(game_folder.path());
    let artifact_path_string = path_string(&artifact_path);
    let source_path_string = path_string(&source_path);
    let source_sha256 = sha256_hex(b"backup-source-bytes");
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

    let plan_output = run(args(&[
        "plan-swap",
        "--game",
        game.id().as_str(),
        "--component",
        "component:game-a:dlss",
        "--artifact",
        "artifact:dlss-3.7",
    ]))
    .expect("plan swap should render");
    let plan_json: serde_json::Value = serde_json::from_str(&plan_output).expect("valid json");
    let operation_id = renderpilot_domain::OperationId::new(
        plan_json["operation_id"]
            .as_str()
            .expect("operation id string")
            .to_owned(),
    )
    .expect("operation id should parse");

    let error = run(args(&["apply", "--operation", operation_id.as_str()]))
        .expect_err("apply should be blocked without backup");
    let operation = fixture
        .storage
        .find_operation(&operation_id)
        .expect("operation lookup should succeed")
        .expect("operation should exist");
    let items = fixture
        .storage
        .list_operation_items(&operation_id)
        .expect("operation items should load");

    assert!(error.to_string().contains("apply is blocked"));
    assert!(error.to_string().contains("backup is missing"));
    assert_eq!(operation.status, OperationStatus::Blocked);
    assert!(operation.completed_at.is_some());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, OperationStatus::Blocked);
    assert_eq!(
        fs::read(&source_path).expect("source bytes should remain unchanged"),
        b"backup-source-bytes"
    );
}

#[test]
fn apply_is_blocked_when_target_changed_after_plan_swap() {
    let fixture = CatalogFixture::new("apply-blocked-target-change");
    let game_folder = TempGameFolder::new("apply-blocked-target-change-game");
    let artifact_folder = TempGameFolder::new("apply-blocked-target-change-artifact");

    fs::create_dir_all(game_folder.path()).expect("game folder should be created");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder should be created");

    let source_path = game_folder.path().join("nvngx_dlss.dll");
    let artifact_path = artifact_folder.path().join("nvngx_dlss.dll");
    fs::write(&source_path, b"backup-source-bytes").expect("source file should be written");
    fs::write(&artifact_path, b"replacement-bytes").expect("artifact file should be written");

    let install_path = path_string(game_folder.path());
    let artifact_path_string = path_string(&artifact_path);
    let source_path_string = path_string(&source_path);
    let source_sha256 = sha256_hex(b"backup-source-bytes");
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

    let plan_output = run(args(&[
        "plan-swap",
        "--game",
        game.id().as_str(),
        "--component",
        "component:game-a:dlss",
        "--artifact",
        "artifact:dlss-3.7",
    ]))
    .expect("plan swap should render");
    let plan_json: serde_json::Value = serde_json::from_str(&plan_output).expect("valid json");
    let operation_id = renderpilot_domain::OperationId::new(
        plan_json["operation_id"]
            .as_str()
            .expect("operation id string")
            .to_owned(),
    )
    .expect("operation id should parse");

    run(args(&["backup", "--operation", operation_id.as_str()]))
        .expect("backup should succeed");
    fs::write(&source_path, b"mutated-target-bytes").expect("source file should be mutated");

    let error = run(args(&["apply", "--operation", operation_id.as_str()]))
        .expect_err("apply should be blocked after target mutation");
    let operation = fixture
        .storage
        .find_operation(&operation_id)
        .expect("operation lookup should succeed")
        .expect("operation should exist");
    let items = fixture
        .storage
        .list_operation_items(&operation_id)
        .expect("operation items should load");

    assert!(error.to_string().contains("apply is blocked"));
    assert!(error.to_string().contains("target changed since plan-swap"));
    assert_eq!(operation.status, OperationStatus::Blocked);
    assert!(operation.completed_at.is_some());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, OperationStatus::Blocked);
    assert_eq!(
        fs::read(&source_path).expect("mutated source bytes should remain in place"),
        b"mutated-target-bytes"
    );
}

#[test]
fn rollback_restores_original_file_and_updates_catalog() {
    let scenario = setup_applied_operation_scenario("rollback-success");

    let rollback_output = run(args(&["rollback", "--operation", scenario.operation_id.as_str()]))
        .expect("rollback should succeed");
    let rollback_json: serde_json::Value =
        serde_json::from_str(&rollback_output).expect("valid rollback json");
    let operation = scenario
        .fixture
        .storage
        .find_operation(&scenario.operation_id)
        .expect("operation lookup should succeed")
        .expect("operation should exist");
    let items = scenario
        .fixture
        .storage
        .list_operation_items(&scenario.operation_id)
        .expect("operation items should load");
    let components = scenario
        .fixture
        .storage
        .list_components_for_game(&scenario.game_id)
        .expect("components should load");

    assert_eq!(rollback_json["operation_id"], scenario.operation_id.as_str());
    assert_eq!(rollback_json["status"], "rolled_back");
    assert_eq!(rollback_json["items"][0]["restored_path"], path_string(&scenario.source_path));
    assert_eq!(
        fs::read(&scenario.source_path).expect("restored bytes should be readable"),
        ORIGINAL_BYTES
    );
    assert_eq!(sha256_hex(ORIGINAL_BYTES), scenario.original_sha256);
    assert_eq!(operation.status, OperationStatus::RolledBack);
    assert!(operation.completed_at.is_some());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, OperationStatus::RolledBack);
    assert_eq!(components.len(), 1);
    assert_eq!(components[0].files().len(), 1);
    assert_eq!(
        components[0].files()[0].version().map(|version| version.as_str()),
        Some("3.5.0")
    );
    assert_eq!(
        components[0].files()[0].sha256().map(|sha256| sha256.as_str()),
        Some(scenario.original_sha256.as_str())
    );
}

#[test]
fn rollback_is_idempotent_after_first_restore() {
    let scenario = setup_applied_operation_scenario("rollback-repeat");

    run(args(&["rollback", "--operation", scenario.operation_id.as_str()]))
        .expect("first rollback should succeed");
    let second_output = run(args(&["rollback", "--operation", scenario.operation_id.as_str()]))
        .expect("second rollback should succeed");
    let second_json: serde_json::Value =
        serde_json::from_str(&second_output).expect("valid rollback json");
    let operation = scenario
        .fixture
        .storage
        .find_operation(&scenario.operation_id)
        .expect("operation lookup should succeed")
        .expect("operation should exist");
    let components = scenario
        .fixture
        .storage
        .list_components_for_game(&scenario.game_id)
        .expect("components should load");

    assert_eq!(second_json["status"], "rolled_back");
    assert_eq!(
        fs::read(&scenario.source_path).expect("restored bytes should remain readable"),
        ORIGINAL_BYTES
    );
    assert_eq!(operation.status, OperationStatus::RolledBack);
    assert_eq!(
        components[0].files()[0].sha256().map(|sha256| sha256.as_str()),
        Some(scenario.original_sha256.as_str())
    );
}

#[cfg(windows)]
#[test]
fn rollback_is_blocked_when_target_file_is_locked() {
    let scenario = setup_applied_operation_scenario("rollback-locked");
    let lock = open_exclusive_file_lock(&scenario.source_path);

    let error = run(args(&["rollback", "--operation", scenario.operation_id.as_str()]))
        .expect_err("rollback should be blocked while target is locked");
    drop(lock);
    let operation = scenario
        .fixture
        .storage
        .find_operation(&scenario.operation_id)
        .expect("operation lookup should succeed")
        .expect("operation should exist");
    let items = scenario
        .fixture
        .storage
        .list_operation_items(&scenario.operation_id)
        .expect("operation items should load");

    assert!(error.to_string().contains("rollback is blocked"));
    assert!(error.to_string().contains("target file is locked"));
    assert_eq!(operation.status, OperationStatus::Blocked);
    assert!(operation.completed_at.is_some());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, OperationStatus::Blocked);
    assert_eq!(
        fs::read(&scenario.source_path).expect("applied bytes should remain in place"),
        REPLACEMENT_BYTES
    );
}

const ORIGINAL_BYTES: &[u8] = b"backup-source-bytes";
const REPLACEMENT_BYTES: &[u8] = b"replacement-bytes";

struct AppliedOperationScenario {
    fixture: CatalogFixture,
    game_id: renderpilot_domain::GameId,
    operation_id: renderpilot_domain::OperationId,
    source_path: std::path::PathBuf,
    original_sha256: String,
    _game_folder: TempGameFolder,
    _artifact_folder: TempGameFolder,
}

fn setup_applied_operation_scenario(name: &str) -> AppliedOperationScenario {
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

    let plan_output = run(args(&[
        "plan-swap",
        "--game",
        game.id().as_str(),
        "--component",
        "component:game-a:dlss",
        "--artifact",
        "artifact:dlss-3.7",
    ]))
    .expect("plan swap should render");
    let plan_json: serde_json::Value = serde_json::from_str(&plan_output).expect("valid json");
    let operation_id = renderpilot_domain::OperationId::new(
        plan_json["operation_id"]
            .as_str()
            .expect("operation id string")
            .to_owned(),
    )
    .expect("operation id should parse");

    run(args(&["backup", "--operation", operation_id.as_str()]))
        .expect("backup should succeed");
    run(args(&["apply", "--operation", operation_id.as_str()]))
        .expect("apply should succeed");
    assert_eq!(
        fixture
            .storage
            .list_backups_for_game(game.id())
            .expect("backups should load")
            .len(),
        1,
        "applied scenario should persist one backup record"
    );
    let backups = fixture
        .storage
        .list_backups_for_game(game.id())
        .expect("backups should load after apply");
    let items = fixture
        .storage
        .list_operation_items(&operation_id)
        .expect("operation items should load after apply");

    assert_eq!(backups[0].operation_id, operation_id);
    assert_eq!(backups[0].original_path, items[0].source_path);

    AppliedOperationScenario {
        fixture,
        game_id: game.id().clone(),
        operation_id,
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

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);

    let mut hex = String::with_capacity(64);

    for byte in hasher.finalize() {
        use std::fmt::Write as _;

        let _ = write!(hex, "{byte:02x}");
    }

    hex
}