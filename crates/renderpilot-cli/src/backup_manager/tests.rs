use std::{
    env,
    fs::{self, OpenOptions},
    io::Write as _,
    path::PathBuf,
    sync::MutexGuard,
    time::{SystemTime, UNIX_EPOCH},
};

use renderpilot_application::{
    ArtifactRepository, ComponentRepository, GameRepository, OperationItemRecord, OperationKind,
    OperationRecord, OperationRepository, OperationStatus, UnixTimestampMillis,
};
use renderpilot_domain::{
    ArtifactId, ComponentFile, ComponentId, ComponentKind, GameId, GameIdentity, GameInstallation,
    GameRuntime, GraphicsComponent, GraphicsTechnology, Launcher, LibraryArtifact, OperationId,
    PathRef, Platform, Swappability, Version,
};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::test_env::lock_process_env;

use super::{
    apply_operation, create_backup, create_backup_with_post_copy,
    filesystem::{backup_operation_root, path_ref_from_path, sanitize_path_segment, sha256_file},
    planned_operation_item_metadata_json, BACKUP_ROOT_DIR_ENV,
};

#[test]
fn backup_marks_operation_failed_when_sha256_verification_mismatches() {
    let _guard = BackupRootGuard::new(temp_backup_root("catalog-backup-mismatch"));
    let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
    let game_dir = temp_backup_root("catalog-backup-game-dir");
    let source_path = game_dir.join("nvngx_dlss.dll");
    let install_path = path_ref_from_path(&game_dir).expect("install path should normalize");
    let game_id =
        GameId::new(format!("manual:{}", install_path.as_str())).expect("game id should be valid");
    let game = sample_game(game_id.clone(), install_path.clone());
    let component = sample_component(
        "component:game-a:dlss",
        game.id().clone(),
        path_ref_from_path(&source_path).expect("source path should normalize"),
    );
    let artifact = sample_artifact("artifact:dlss-3.7", "D:/Library/nvngx_dlss.dll");
    let operation = OperationRecord::new(
        OperationId::new("operation:replace_component:1:component:game-a:dlss:artifact:dlss-3.7")
            .expect("operation id should be valid"),
        game.id().clone(),
        OperationKind::ReplaceComponent,
        OperationStatus::Planned,
        UnixTimestampMillis::new(1).expect("timestamp should be valid"),
    );
    let item = OperationItemRecord::new(
        operation.id.clone(),
        component.id().clone(),
        path_ref_from_path(&source_path).expect("source path should normalize"),
        OperationStatus::Planned,
    )
    .with_artifact_id(artifact.id().clone())
    .with_target_path(PathRef::new("D:/Library/nvngx_dlss.dll").expect("path should be valid"));

    fs::create_dir_all(&game_dir).expect("game dir should be created");
    fs::write(&source_path, b"backup-source-bytes").expect("source file should be written");

    storage.upsert_game(&game).expect("game should be stored");
    storage
        .replace_components_for_game(game.id(), &[component])
        .expect("component should be stored");
    storage
        .upsert_artifact(&artifact)
        .expect("artifact should be stored");
    storage
        .upsert_operation(&operation)
        .expect("operation should be stored");
    storage
        .replace_operation_items(&operation.id, &[item])
        .expect("operation item should be stored");

    let error = create_backup_with_post_copy(&storage, &operation.id, "0.1.0", |path| {
        let mut file = OpenOptions::new().append(true).open(path)?;
        file.write_all(b"corruption")
    })
    .expect_err("backup should fail when copied bytes are mutated");
    let operation_after = storage
        .find_operation(&operation.id)
        .expect("operation lookup should succeed")
        .expect("operation should remain stored");
    let items_after = storage
        .list_operation_items(&operation.id)
        .expect("operation items should load");

    assert!(error.to_string().contains("backup sha256 mismatch"));
    assert_eq!(operation_after.status, OperationStatus::Failed);
    assert_eq!(items_after.len(), 1);
    assert_eq!(items_after[0].status, OperationStatus::Failed);
    assert_directory_missing_or_empty(&backup_operation_root(&operation));
}

#[test]
fn sanitize_path_segment_replaces_windows_reserved_characters() {
    assert_eq!(
        sanitize_path_segment("manual:C:/Games/GameA"),
        "manual_C__Games_GameA"
    );
}

#[test]
fn apply_operation_rolls_back_when_catalog_refresh_fails() {
    let _guard = BackupRootGuard::new(temp_backup_root("apply-rollback"));
    let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
    let game_dir = temp_backup_root("apply-rollback-game-dir");
    let artifact_dir = temp_backup_root("apply-rollback-artifact-dir");
    let source_path = game_dir.join("nvngx_dlss.dll");
    let artifact_path = artifact_dir.join("nvngx_dlss.dll");

    fs::create_dir_all(&game_dir).expect("game dir should be created");
    fs::create_dir_all(&artifact_dir).expect("artifact dir should be created");
    fs::write(&source_path, b"backup-source-bytes").expect("source file should be written");
    fs::write(&artifact_path, b"replacement-bytes").expect("artifact file should be written");

    let install_path = path_ref_from_path(&game_dir).expect("install path should normalize");
    let source_path_ref = path_ref_from_path(&source_path).expect("source path should normalize");
    let artifact_path_ref =
        path_ref_from_path(&artifact_path).expect("artifact path should normalize");
    let artifact_sha256 = sha256_file(&artifact_path).expect("artifact sha256 should compute");
    let game_id =
        GameId::new(format!("manual:{}", install_path.as_str())).expect("game id should be valid");
    let game = sample_game(game_id.clone(), install_path.clone());
    let component = sample_component(
        "component:game-a:dlss",
        game.id().clone(),
        PathRef::new("C:/broken/nvngx_dlss.dll").expect("broken path should be valid"),
    );
    let artifact = sample_artifact_from_file(
        "artifact:dlss-3.7",
        artifact_path_ref.clone(),
        artifact_sha256,
    );
    let operation = OperationRecord::new(
        OperationId::new(
            "operation:replace_component:rollback:component:game-a:dlss:artifact:dlss-3.7",
        )
        .expect("operation id should be valid"),
        game.id().clone(),
        OperationKind::ReplaceComponent,
        OperationStatus::Planned,
        UnixTimestampMillis::new(1).expect("timestamp should be valid"),
    );
    let item = OperationItemRecord::new(
        operation.id.clone(),
        component.id().clone(),
        source_path_ref.clone(),
        OperationStatus::Planned,
    )
    .with_artifact_id(artifact.id().clone())
    .with_target_path(artifact_path_ref.clone())
    .with_metadata_json(
        planned_operation_item_metadata_json(
            Some(&sha256_file(&source_path).expect("source sha256 should compute")),
            Some(artifact.sha256()),
        )
        .expect("planned item metadata should build"),
    );

    storage.upsert_game(&game).expect("game should be stored");
    storage
        .replace_components_for_game(game.id(), &[component.clone()])
        .expect("component should be stored");
    storage
        .upsert_artifact(&artifact)
        .expect("artifact should be stored");
    storage
        .upsert_operation(&operation)
        .expect("operation should be stored");
    storage
        .replace_operation_items(&operation.id, &[item])
        .expect("operation item should be stored");

    create_backup(&storage, operation.id.clone(), "0.1.0").expect("backup should succeed");

    let error = apply_operation(&storage, operation.id.clone())
        .expect_err("apply should fail and roll back");
    let operation_after = storage
        .find_operation(&operation.id)
        .expect("operation lookup should succeed")
        .expect("operation should remain stored");
    let items_after = storage
        .list_operation_items(&operation.id)
        .expect("operation items should load");

    assert!(
        error.message().contains("missing file"),
        "unexpected apply error ({:?}): {}",
        error.kind(),
        error.message()
    );
    assert_eq!(
        fs::read(&source_path).expect("source file should be readable after rollback"),
        b"backup-source-bytes"
    );
    assert_eq!(operation_after.status, OperationStatus::Failed);
    assert!(operation_after.completed_at.is_some());
    assert_eq!(items_after.len(), 1);
    assert_eq!(items_after[0].status, OperationStatus::Failed);
    assert!(
        renderpilot_temp_files(&game_dir).is_empty(),
        "staged apply temp files should be cleaned up"
    );
}

fn assert_directory_missing_or_empty(path: &std::path::Path) {
    if !path.exists() {
        return;
    }

    let has_entries = fs::read_dir(path)
        .expect("directory should be readable")
        .next()
        .transpose()
        .expect("directory entry lookup should succeed")
        .is_some();

    assert!(
        !has_entries,
        "expected directory {} to be missing or empty",
        path.display()
    );
}

fn renderpilot_temp_files(path: &std::path::Path) -> Vec<PathBuf> {
    fs::read_dir(path)
        .expect("directory should be readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|entry_path| {
            entry_path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.contains(".renderpilot-") && name.ends_with(".tmp"))
                .unwrap_or(false)
        })
        .collect()
}

struct BackupRootGuard {
    previous: Option<std::ffi::OsString>,
    root: PathBuf,
    _lock: MutexGuard<'static, ()>,
}

impl BackupRootGuard {
    fn new(root: PathBuf) -> Self {
        let lock = lock_process_env();
        let previous = env::var_os(BACKUP_ROOT_DIR_ENV);

        env::set_var(BACKUP_ROOT_DIR_ENV, &root);

        Self {
            previous,
            root,
            _lock: lock,
        }
    }
}

impl Drop for BackupRootGuard {
    fn drop(&mut self) {
        if let Some(previous) = &self.previous {
            env::set_var(BACKUP_ROOT_DIR_ENV, previous);
        } else {
            env::remove_var(BACKUP_ROOT_DIR_ENV);
        }

        if self.root.exists() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

fn temp_backup_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();

    std::env::temp_dir().join(format!("renderpilot-{name}-{nanos}"))
}

fn sample_game(game_id: GameId, install_path: PathRef) -> GameInstallation {
    let identity = GameIdentity::new(game_id, "Test Game", Launcher::Manual)
        .expect("game identity should be valid");

    GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        install_path,
    )
}

fn sample_component(
    component_id: &str,
    game_id: GameId,
    source_path: PathRef,
) -> GraphicsComponent {
    GraphicsComponent::new(
        ComponentId::new(component_id).expect("component id should be valid"),
        game_id,
        ComponentKind::NativeLibrary,
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(source_path)
            .with_version(Version::parse("3.5.0").expect("version should parse")),
    )
}

fn sample_artifact(artifact_id: &str, path: &str) -> LibraryArtifact {
    sample_artifact_from_file(
        artifact_id,
        PathRef::new(path).expect("artifact path should be valid"),
        renderpilot_domain::Sha256Hash::new(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )
        .expect("sha256 should be valid"),
    )
}

fn sample_artifact_from_file(
    artifact_id: &str,
    path: PathRef,
    sha256: renderpilot_domain::Sha256Hash,
) -> LibraryArtifact {
    LibraryArtifact::new(
        ArtifactId::new(artifact_id).expect("artifact id should be valid"),
        GraphicsTechnology::DlssSuperResolution,
        "nvngx_dlss.dll",
        ComponentFile::new(path)
            .with_version(Version::parse("3.7.0").expect("version should parse"))
            .with_sha256(sha256),
        renderpilot_domain::ArtifactTrustLevel::LocalObserved,
    )
    .expect("artifact should be valid")
}
