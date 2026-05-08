use std::{
    env,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Write as _},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        MutexGuard,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use renderpilot_application::{
    ArtifactRepository, ComponentRepository, GameRepository, OperationItemRecord,
    OperationJournalEntry, OperationKind, OperationRecord, OperationRepository, OperationStatus,
    UnixTimestampMillis,
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

const APP_VERSION: &str = "0.1.0";
const DLL_NAME: &str = "nvngx_dlss.dll";
const SOURCE_BYTES: &[u8] = b"backup-source-bytes";
const REPLACEMENT_BYTES: &[u8] = b"replacement-bytes";

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[test]
fn backup_marks_operation_failed_when_sha256_verification_mismatches() {
    let _backup_root = BackupRootGuard::new("catalog-backup-mismatch");
    let storage = in_memory_storage();
    let game_dir = TempDir::new("catalog-backup-game-dir");

    let source_path = game_dir.join(DLL_NAME);
    write_file(&source_path, SOURCE_BYTES);

    let game = sample_game_from_install_dir(game_dir.path());
    let component = sample_component(
        "component:game-a:dlss",
        game.id().clone(),
        path_ref(&source_path),
    );
    let artifact = sample_artifact("artifact:dlss-3.7", "D:/Library/nvngx_dlss.dll");

    let operation = planned_replace_operation(
        "operation:replace_component:1:component:game-a:dlss:artifact:dlss-3.7",
        &game,
    );

    let item = planned_replace_item(
        &operation,
        &component,
        path_ref(&source_path),
        &artifact,
        path_ref_literal("D:/Library/nvngx_dlss.dll"),
    );

    persist_planned_replace_operation(
        &storage,
        &game,
        &[component],
        &artifact,
        &operation,
        &[item],
    );

    let error = create_backup_with_post_copy(&storage, &operation.id, APP_VERSION, |path| {
        append_file(path, b"corruption")
    })
    .expect_err("backup should fail when copied bytes are mutated");

    assert!(error.to_string().contains("backup sha256 mismatch"));

    assert_operation_and_single_item_failed(&storage, &operation.id);
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
    let _backup_root = BackupRootGuard::new("apply-rollback");
    let storage = in_memory_storage();

    let game_dir = TempDir::new("apply-rollback-game-dir");
    let artifact_dir = TempDir::new("apply-rollback-artifact-dir");

    let source_path = game_dir.join(DLL_NAME);
    let artifact_path = artifact_dir.join(DLL_NAME);

    write_file(&source_path, SOURCE_BYTES);
    write_file(&artifact_path, REPLACEMENT_BYTES);

    let game = sample_game_from_install_dir(game_dir.path());

    // Intentionally stale catalog path: the operation item still points to the
    // real source file, but catalog refresh should fail after apply.
    let component = sample_component(
        "component:game-a:dlss",
        game.id().clone(),
        path_ref_literal("C:/broken/nvngx_dlss.dll"),
    );

    let artifact_sha256 = sha256_file(&artifact_path).expect("artifact sha256 should compute");
    let artifact = sample_artifact_from_file(
        "artifact:dlss-3.7",
        path_ref(&artifact_path),
        artifact_sha256,
    );

    let operation = planned_replace_operation(
        "operation:replace_component:rollback:component:game-a:dlss:artifact:dlss-3.7",
        &game,
    );

    let source_sha256 = sha256_file(&source_path).expect("source sha256 should compute");

    let item = planned_replace_item(
        &operation,
        &component,
        path_ref(&source_path),
        &artifact,
        path_ref(&artifact_path),
    )
    .with_metadata_json(
        planned_operation_item_metadata_json(Some(&source_sha256), Some(artifact.sha256()))
            .expect("planned item metadata should build"),
    );

    persist_planned_replace_operation(
        &storage,
        &game,
        &[component],
        &artifact,
        &operation,
        &[item],
    );

    let _backup =
        create_backup(&storage, operation.id.clone(), APP_VERSION).expect("backup should succeed");

    let error = apply_operation(&storage, operation.id.clone())
        .expect_err("apply should fail and roll back");

    assert!(
        error.message().contains("missing file"),
        "unexpected apply error ({:?}): {}",
        error.kind(),
        error.message()
    );

    assert_file_bytes(&source_path, SOURCE_BYTES);

    let operation_after = assert_operation_and_single_item_failed(&storage, &operation.id);
    assert!(operation_after.completed_at.is_some());

    assert!(
        renderpilot_temp_files(game_dir.path()).is_empty(),
        "staged apply temp files should be cleaned up"
    );
}

fn in_memory_storage() -> SqliteStorage {
    SqliteStorage::in_memory().expect("in-memory sqlite should open")
}

fn persist_planned_replace_operation(
    storage: &SqliteStorage,
    game: &GameInstallation,
    components: &[GraphicsComponent],
    artifact: &LibraryArtifact,
    operation: &OperationRecord,
    items: &[OperationItemRecord],
) {
    storage.upsert_game(game).expect("game should be stored");

    storage
        .replace_components_for_game(game.id(), components)
        .expect("components should be stored");

    storage
        .upsert_artifact(artifact)
        .expect("artifact should be stored");

    let entry = OperationJournalEntry::try_new(operation.clone(), items.to_vec())
        .expect("operation journal entry should be valid");
    storage
        .save_operation_entry(&entry)
        .expect("operation journal entry should be stored");
}

fn planned_replace_operation(id: &str, game: &GameInstallation) -> OperationRecord {
    OperationRecord::new(
        OperationId::new(id).expect("operation id should be valid"),
        game.id().clone(),
        OperationKind::ReplaceComponent,
        OperationStatus::Planned,
        UnixTimestampMillis::new(1).expect("timestamp should be valid"),
    )
}

fn planned_replace_item(
    operation: &OperationRecord,
    component: &GraphicsComponent,
    source_path: PathRef,
    artifact: &LibraryArtifact,
    target_path: PathRef,
) -> OperationItemRecord {
    OperationItemRecord::new(
        operation.id.clone(),
        component.id().clone(),
        source_path,
        OperationStatus::Planned,
    )
    .with_artifact_id(artifact.id().clone())
    .with_target_path(target_path)
}

fn assert_operation_and_single_item_failed(
    storage: &SqliteStorage,
    operation_id: &OperationId,
) -> OperationRecord {
    let entry = storage
        .find_operation_entry(operation_id)
        .expect("operation lookup should succeed")
        .expect("operation should remain stored");

    assert_eq!(entry.operation().status, OperationStatus::Failed);
    assert_eq!(entry.len(), 1);
    assert_eq!(entry.items()[0].status, OperationStatus::Failed);

    entry.operation().clone()
}

fn assert_directory_missing_or_empty(path: &Path) {
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

fn assert_file_bytes(path: &Path, expected: &[u8]) {
    assert_eq!(
        fs::read(path).expect("file should be readable"),
        expected,
        "unexpected file contents for {}",
        path.display()
    );
}

fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent directory should be created");
    }

    fs::write(path, bytes).expect("file should be written");
}

fn append_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().append(true).open(path)?;
    file.write_all(bytes)
}

fn renderpilot_temp_files(path: &Path) -> Vec<PathBuf> {
    fs::read_dir(path)
        .expect("directory should be readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|entry_path| {
            entry_path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(".renderpilot-") && name.ends_with(".tmp"))
        })
        .collect()
}

struct BackupRootGuard {
    previous: Option<OsString>,
    _root: TempDir,
    _lock: MutexGuard<'static, ()>,
}

impl BackupRootGuard {
    fn new(name: &str) -> Self {
        let lock = lock_process_env();
        let previous = env::var_os(BACKUP_ROOT_DIR_ENV);
        let root = TempDir::new(name);

        env::set_var(BACKUP_ROOT_DIR_ENV, root.path());

        Self {
            previous,
            _root: root,
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
    }
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Self {
        let path = unique_temp_path(name);
        fs::create_dir(&path).expect("temp dir should be created");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.path.join(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn unique_temp_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();

    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);

    env::temp_dir().join(format!("renderpilot-{name}-{nanos}-{seq}"))
}

fn path_ref(path: &Path) -> PathRef {
    path_ref_from_path(path).expect("path should normalize")
}

fn path_ref_literal(path: &str) -> PathRef {
    PathRef::new(path).expect("path should be valid")
}

fn sample_game_from_install_dir(install_dir: &Path) -> GameInstallation {
    let install_path = path_ref(install_dir);
    let game_id =
        GameId::new(format!("manual:{}", install_path.as_str())).expect("game id should be valid");

    sample_game(game_id, install_path)
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
        path_ref_literal(path),
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
        DLL_NAME,
        ComponentFile::new(path)
            .with_version(Version::parse("3.7.0").expect("version should parse"))
            .with_sha256(sha256),
        renderpilot_domain::ArtifactTrustLevel::LocalObserved,
    )
    .expect("artifact should be valid")
}
