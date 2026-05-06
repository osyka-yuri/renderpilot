use std::{fs, path::PathBuf};

use renderpilot_application::BackupRepository;
use renderpilot_domain::{GraphicsTechnology, Swappability};

use crate::run;

use super::{
    args, path_string, sample_artifact, sample_component, sample_game, CatalogFixture,
    TempGameFolder,
};

#[test]
fn backup_creates_manifest_file_and_backup_record() {
    let fixture = CatalogFixture::new("backup-success");
    let game_folder = TempGameFolder::new("backup-game");
    let artifact_folder = TempGameFolder::new("backup-artifact");

    fs::create_dir_all(game_folder.path()).expect("game folder should be created");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder should be created");

    let source_path = game_folder.path().join("nvngx_dlss.dll");
    let artifact_path = artifact_folder.path().join("nvngx_dlss.dll");
    fs::write(&source_path, b"backup-source-bytes").expect("source file should be written");
    fs::write(&artifact_path, b"replacement-bytes").expect("artifact file should be written");

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
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )],
    );
    fixture.store_artifact(sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        &artifact_path_string,
        Some("3.7.0"),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
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

    let backup_output =
        run(args(&["backup", "--operation", &operation_id])).expect("backup should succeed");
    let backup_json: serde_json::Value =
        serde_json::from_str(&backup_output).expect("valid backup json");
    let backup_path = PathBuf::from(
        backup_json["items"][0]["backup_path"]
            .as_str()
            .expect("backup path string"),
    );
    let manifest_path = PathBuf::from(
        backup_json["items"][0]["manifest_path"]
            .as_str()
            .expect("manifest path string"),
    );
    let manifest_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path).expect("manifest should be readable"),
    )
    .expect("manifest should be valid json");
    let backups = fixture
        .storage
        .list_backups_for_game(game.id())
        .expect("backups should load");

    assert_eq!(backup_json["game_id"], game.id().as_str());
    assert_eq!(
        backup_json["items"].as_array().expect("backup items").len(),
        1
    );
    assert!(backup_path.exists());
    assert!(manifest_path.exists());
    assert_eq!(
        fs::read(&source_path).expect("source bytes should be readable"),
        fs::read(&backup_path).expect("backup bytes should be readable")
    );
    assert_eq!(manifest_json["operation_id"], operation_id);
    assert_eq!(manifest_json["game_id"], game.id().as_str());
    assert_eq!(manifest_json["original_path"], source_path_string);
    assert_eq!(manifest_json["technology"], "dlss_super_resolution");
    assert_eq!(manifest_json["version"], "3.5.0");
    assert_eq!(manifest_json["sha256"], backup_json["items"][0]["sha256"]);
    assert_eq!(backups.len(), 1);
    assert_eq!(backups[0].operation_id.as_str(), operation_id);
    assert_eq!(backups[0].original_path.as_str(), source_path_string);
    assert_eq!(
        backups[0].sha256.as_ref().map(|sha256| sha256.as_str()),
        backup_json["items"][0]["sha256"].as_str()
    );
}
