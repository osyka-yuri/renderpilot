use std::{
    fs,
    path::{Path, PathBuf},
};

use renderpilot_application::{
    AppError, AppResult, BackupId, BackupRecord, BackupRepository, ComponentRepository,
    MetadataJson, OperationItemRecord, OperationRecord, UnixTimestampMillis,
};
use renderpilot_domain::{ComponentFile, GraphicsComponent, OperationId, Sha256Hash};
use renderpilot_storage_sqlite::SqliteStorage;

use super::{
    filesystem::{
        backup_operation_root, copy_backup_file_with_verification, current_timestamp_millis,
        file_system_error, path_ref_from_path, sanitize_path_segment,
    },
    journal::{require_replacement_plan_item, RetryableReplaceOperation},
    manifest::{write_backup_manifest, BackupManifest},
    BackupCatalogItemResult, BackupCatalogResult,
};

pub(crate) fn create_backup(
    storage: &SqliteStorage,
    operation_id: OperationId,
    app_version: &str,
) -> AppResult<BackupCatalogResult> {
    create_backup_with_post_copy(storage, &operation_id, app_version, |_path| Ok(()))
}

pub(super) fn create_backup_with_post_copy<F>(
    storage: &SqliteStorage,
    operation_id: &OperationId,
    app_version: &str,
    post_copy: F,
) -> AppResult<BackupCatalogResult>
where
    F: Fn(&Path) -> std::io::Result<()>,
{
    let mut operation =
        RetryableReplaceOperation::load(storage, operation_id, "backup", "back up")?;

    match create_backup_records(storage, &operation, app_version, &post_copy) {
        Ok(result) => Ok(result),
        Err(error) => Err(operation.capture_failure(storage, error)),
    }
}

fn create_backup_records<F>(
    storage: &SqliteStorage,
    operation: &RetryableReplaceOperation,
    app_version: &str,
    post_copy: &F,
) -> AppResult<BackupCatalogResult>
where
    F: Fn(&Path) -> std::io::Result<()>,
{
    let created_at = current_timestamp_millis()?;
    let components = storage.list_components_for_game(&operation.operation.game_id)?;
    let backup_root = backup_operation_root(&operation.operation);

    ensure_directory(&backup_root, "backup directory")?;

    let mut backup_items = Vec::with_capacity(operation.items.len());

    for (index, item) in operation.items.iter().enumerate() {
        let _replacement_plan = require_replacement_plan_item(&operation.operation, item)?;

        let (component, component_file) = resolve_backup_component_file(&components, item)?;
        let backup_item = create_backup_item(
            &operation.operation,
            item,
            component,
            component_file,
            &backup_root,
            index,
            created_at,
            app_version,
            post_copy,
        )?;

        storage.upsert_backup(&backup_item.record)?;
        backup_items.push(backup_item.result);
    }

    Ok(BackupCatalogResult {
        operation_id: operation.operation.id.clone(),
        game_id: operation.operation.game_id.clone(),
        backup_root: path_ref_from_path(&backup_root)?,
        items: backup_items,
    })
}

fn create_backup_item<F>(
    operation: &OperationRecord,
    item: &OperationItemRecord,
    component: &GraphicsComponent,
    component_file: &ComponentFile,
    backup_root: &Path,
    index: usize,
    created_at: UnixTimestampMillis,
    app_version: &str,
    post_copy: &F,
) -> AppResult<BackupCreatedItem>
where
    F: Fn(&Path) -> std::io::Result<()>,
{
    let source_path = Path::new(item.source_path.as_str());
    let workspace = BackupItemWorkspace::create(backup_root, item, index)?;
    let backup_file_path = workspace.backup_file_path(source_file_name(item)?);
    let sha256 = copy_backup_file_with_verification(source_path, &backup_file_path, post_copy)?;
    let manifest_path = workspace.manifest_path();
    let manifest = BackupManifest::new(
        operation.id.clone(),
        operation.game_id.clone(),
        item.source_path.clone(),
        component.technology(),
        component_file.version().cloned(),
        sha256.clone(),
        created_at,
        app_version,
    );

    write_backup_manifest(&manifest_path, &manifest)?;

    let record = build_backup_record(
        operation,
        item,
        &backup_file_path,
        &manifest_path,
        sha256.clone(),
        created_at,
        app_version,
    )?;

    workspace.commit();

    Ok(BackupCreatedItem {
        result: BackupCatalogItemResult {
            backup_id: record.id.clone(),
            component_id: item.component_id.clone(),
            original_path: item.source_path.clone(),
            backup_path: record.backup_path.clone(),
            manifest_path: path_ref_from_path(&manifest_path)?,
            sha256,
        },
        record,
    })
}

fn resolve_backup_component_file<'a>(
    components: &'a [GraphicsComponent],
    item: &OperationItemRecord,
) -> AppResult<(&'a GraphicsComponent, &'a ComponentFile)> {
    let component = components
        .iter()
        .find(|component| component.id() == &item.component_id)
        .ok_or_else(|| {
            AppError::invalid_input(format!(
                "component not found for backup item: {}",
                item.component_id.as_str()
            ))
        })?;
    let component_file = component
        .files()
        .iter()
        .find(|file| file.path() == &item.source_path)
        .or_else(|| component.files().first())
        .ok_or_else(|| {
            AppError::invalid_input(format!(
                "component {} does not contain a file for {}",
                component.id().as_str(),
                item.source_path.as_str()
            ))
        })?;

    Ok((component, component_file))
}

fn source_file_name<'a>(item: &'a OperationItemRecord) -> AppResult<&'a std::ffi::OsStr> {
    Path::new(item.source_path.as_str())
        .file_name()
        .ok_or_else(|| {
            AppError::invalid_input(format!(
                "source path has no file name: {}",
                item.source_path.as_str()
            ))
        })
}

fn ensure_directory(path: &Path, description: &str) -> AppResult<()> {
    fs::create_dir_all(path).map_err(|error| {
        file_system_error(
            format!("failed to create {description} {}", path.display()),
            error,
        )
    })
}

fn build_backup_record(
    operation: &OperationRecord,
    item: &OperationItemRecord,
    backup_file_path: &Path,
    manifest_path: &Path,
    sha256: Sha256Hash,
    created_at: UnixTimestampMillis,
    app_version: &str,
) -> AppResult<BackupRecord> {
    let file_name = backup_file_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            AppError::invalid_input(format!(
                "backup path has no file name: {}",
                backup_file_path.display()
            ))
        })?;
    let backup_id = BackupId::new(format!(
        "backup:{}:{}:{}",
        operation.id.as_str(),
        item.component_id.as_str(),
        file_name
    ))?;
    let backup_path = path_ref_from_path(backup_file_path)?;
    let manifest_path = path_ref_from_path(manifest_path)?;
    let metadata_json = MetadataJson::new(
        serde_json::json!({
            "manifest_path": manifest_path.as_str(),
            "app_version": app_version,
        })
        .to_string(),
    )?;

    Ok(BackupRecord::new(
        backup_id,
        operation.id.clone(),
        operation.game_id.clone(),
        item.source_path.clone(),
        backup_path,
        created_at,
    )
    .with_component_id(item.component_id.clone())
    .with_sha256(sha256)
    .with_metadata_json(metadata_json))
}

fn backup_item_dir(backup_root: &Path, item: &OperationItemRecord, index: usize) -> PathBuf {
    backup_root.join(format!(
        "{:04}-{}",
        index + 1,
        sanitize_path_segment(item.component_id.as_str())
    ))
}

#[derive(Debug)]
struct BackupItemWorkspace {
    item_dir: PathBuf,
    committed: bool,
}

impl BackupItemWorkspace {
    fn create(backup_root: &Path, item: &OperationItemRecord, index: usize) -> AppResult<Self> {
        let item_dir = backup_item_dir(backup_root, item, index);

        ensure_directory(&item_dir, "backup item directory")?;

        Ok(Self {
            item_dir,
            committed: false,
        })
    }

    fn backup_file_path(&self, file_name: &std::ffi::OsStr) -> PathBuf {
        self.item_dir.join(file_name)
    }

    fn manifest_path(&self) -> PathBuf {
        self.item_dir.join("manifest.json")
    }

    fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for BackupItemWorkspace {
    fn drop(&mut self) {
        if !self.committed && self.item_dir.exists() {
            let _ = fs::remove_dir_all(&self.item_dir);
        }
    }
}

#[derive(Debug)]
struct BackupCreatedItem {
    result: BackupCatalogItemResult,
    record: BackupRecord,
}
