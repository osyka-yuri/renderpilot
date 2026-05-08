use std::{collections::HashMap, path::Path};

use renderpilot_application::{
    AppError, AppResult, BackupId, BackupRecord, BackupRepository, ComponentRepository,
    OperationItemRecord, OperationRecord,
};
use renderpilot_domain::{
    ComponentFile, ComponentId, GameId, OperationId, PathRef, Sha256Hash, Version,
};
use renderpilot_storage_sqlite::SqliteStorage;

use super::{
    filesystem::{
        copy_file_with_verification, ensure_file_exists, ensure_file_is_writable,
        ensure_file_matches_sha256,
    },
    journal::RetryableReplaceOperation,
    plan_metadata::planned_item_metadata,
    shared::{backup_lookup_map, rebuild_component_catalog, BackupLookupKey, CatalogUpdateItem},
};

#[derive(Debug)]
pub(crate) struct RollbackOperationCatalogResult {
    pub(crate) operation: OperationRecord,
    pub(crate) items: Vec<RollbackOperationCatalogItemResult>,
}

#[derive(Debug, Clone)]
pub(crate) struct RollbackOperationCatalogItemResult {
    pub(crate) backup_id: BackupId,
    pub(crate) component_id: ComponentId,
    pub(crate) restored_path: PathRef,
    pub(crate) backup_path: PathRef,
}

pub(crate) fn rollback_operation(
    storage: &SqliteStorage,
    operation_id: OperationId,
) -> AppResult<RollbackOperationCatalogResult> {
    let mut operation = RetryableReplaceOperation::load_for_rollback(
        storage,
        &operation_id,
        "rollback",
        "roll back",
    )?;
    let prepared_items = match prepare_rollback_items(
        storage,
        operation.journal.operation(),
        operation.journal.items(),
    ) {
        Ok(prepared_items) => prepared_items,
        Err(error) => return Err(operation.capture_blocked(storage, error)),
    };

    match rollback_operation_records(storage, &mut operation, &prepared_items) {
        Ok(result) => Ok(result),
        Err(error) => Err(operation.capture_failure(storage, error)),
    }
}

fn prepare_rollback_items(
    storage: &SqliteStorage,
    operation: &OperationRecord,
    items: &[OperationItemRecord],
) -> AppResult<Vec<PreparedRollbackItem>> {
    let backups = storage.list_backups_for_game(&operation.game_id)?;
    let preparation = RollbackPreparation::new(operation, &backups);

    items
        .iter()
        .map(|item| preparation.prepare_item(item))
        .collect()
}

fn rollback_operation_records(
    storage: &SqliteStorage,
    operation: &mut RetryableReplaceOperation,
    prepared_items: &[PreparedRollbackItem],
) -> AppResult<RollbackOperationCatalogResult> {
    operation.mark_running(storage)?;

    for prepared_item in prepared_items {
        prepared_item.restore()?;
    }

    update_components_after_rollback(
        storage,
        &operation.journal.operation().game_id,
        prepared_items,
    )?;
    operation.mark_rolled_back(storage)?;

    Ok(RollbackOperationCatalogResult {
        operation: operation.journal.operation().clone(),
        items: prepared_items
            .iter()
            .map(|item| item.result.clone())
            .collect(),
    })
}

fn update_components_after_rollback(
    storage: &SqliteStorage,
    game_id: &GameId,
    prepared_items: &[PreparedRollbackItem],
) -> AppResult<()> {
    let components = storage.list_components_for_game(game_id)?;
    let updated_components = rebuild_component_catalog(components, prepared_items)?;

    storage.replace_components_for_game(game_id, &updated_components)
}

fn noop_post_copy(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

struct RollbackPreparation<'a> {
    operation: &'a OperationRecord,
    backups_by_key: HashMap<BackupLookupKey, BackupRecord>,
}

impl<'a> RollbackPreparation<'a> {
    fn new(operation: &'a OperationRecord, backups: &[BackupRecord]) -> Self {
        let backups_by_key = backup_lookup_map(backups);

        Self {
            operation,
            backups_by_key,
        }
    }

    fn prepare_item(&self, item: &OperationItemRecord) -> AppResult<PreparedRollbackItem> {
        let backup = self.resolve_backup(item)?;
        let planned_metadata = planned_item_metadata(item)?;
        let expected_original_sha256 = backup.sha256.clone().ok_or_else(|| {
            blocked_error(format!(
                "backup sha256 is missing for operation {} and component {}",
                self.operation.id.as_str(),
                item.component_id.as_str()
            ))
        })?;
        let prepared_item = PreparedRollbackItem::new(
            item.clone(),
            backup,
            expected_original_sha256,
            planned_metadata.original_version,
        );

        prepared_item.ensure_preflight_integrity()?;

        Ok(prepared_item)
    }

    fn resolve_backup(&self, item: &OperationItemRecord) -> AppResult<BackupRecord> {
        self.backups_by_key
            .get(&BackupLookupKey::from_operation_item(self.operation, item))
            .cloned()
            .ok_or_else(|| {
                blocked_error(format!(
                    "backup is missing for operation {} and component {}",
                    self.operation.id.as_str(),
                    item.component_id.as_str()
                ))
            })
    }
}

#[derive(Debug, Clone)]
struct PreparedRollbackItem {
    item: OperationItemRecord,
    backup: BackupRecord,
    expected_original_sha256: Sha256Hash,
    original_version: Option<Version>,
    result: RollbackOperationCatalogItemResult,
}

impl PreparedRollbackItem {
    fn new(
        item: OperationItemRecord,
        backup: BackupRecord,
        expected_original_sha256: Sha256Hash,
        original_version: Option<Version>,
    ) -> Self {
        let result = RollbackOperationCatalogItemResult {
            backup_id: backup.id.clone(),
            component_id: item.component_id.clone(),
            restored_path: item.source_path.clone(),
            backup_path: backup.backup_path.clone(),
        };

        Self {
            item,
            backup,
            expected_original_sha256,
            original_version,
            result,
        }
    }

    fn ensure_preflight_integrity(&self) -> AppResult<()> {
        self.ensure_backup_integrity()?;
        self.ensure_target_is_unlocked()
    }

    fn ensure_backup_integrity(&self) -> AppResult<()> {
        ensure_file_exists(self.backup_path(), "backup file").map_err(|_| {
            blocked_error(format!(
                "backup file is missing for component {}: {}",
                self.component_id().as_str(),
                self.backup.backup_path.as_str()
            ))
        })?;
        ensure_file_matches_sha256(
            self.backup_path(),
            &self.expected_original_sha256,
            "backup sha256 mismatch",
        )
        .map_err(|_| {
            blocked_error(format!(
                "backup integrity check failed for component {}",
                self.component_id().as_str()
            ))
        })
    }

    fn ensure_target_is_unlocked(&self) -> AppResult<()> {
        if !self.target_path().exists() {
            return Ok(());
        }

        ensure_file_is_writable(self.target_path(), "target file").map_err(|_| {
            blocked_error(format!(
                "target file is locked for component {}: {}",
                self.component_id().as_str(),
                self.item.source_path.as_str()
            ))
        })
    }

    fn restore(&self) -> AppResult<()> {
        self.ensure_backup_integrity()?;

        copy_file_with_verification(
            self.backup_path(),
            self.target_path(),
            &noop_post_copy,
            "rollback restore sha256 mismatch",
        )?;
        ensure_file_matches_sha256(
            self.target_path(),
            &self.expected_original_sha256,
            "restored file sha256 mismatch",
        )?;

        Ok(())
    }

    fn component_id(&self) -> &ComponentId {
        &self.result.component_id
    }

    fn restored_path(&self) -> &PathRef {
        &self.result.restored_path
    }

    fn target_path(&self) -> &Path {
        Path::new(self.item.source_path.as_str())
    }

    fn backup_path(&self) -> &Path {
        Path::new(self.backup.backup_path.as_str())
    }

    fn updated_component_file(&self, file: &ComponentFile) -> ComponentFile {
        let mut updated_file = ComponentFile::new(file.path().clone())
            .with_sha256(self.expected_original_sha256.clone());

        if let Some(version) = self.original_version.clone() {
            updated_file = updated_file.with_version(version);
        }

        updated_file
    }
}

impl CatalogUpdateItem for PreparedRollbackItem {
    fn catalog_component_id(&self) -> &ComponentId {
        PreparedRollbackItem::component_id(self)
    }

    fn catalog_file_path(&self) -> &PathRef {
        PreparedRollbackItem::restored_path(self)
    }

    fn updated_component_file(&self, file: &ComponentFile) -> ComponentFile {
        PreparedRollbackItem::updated_component_file(self, file)
    }

    fn missing_catalog_entry_error(&self) -> AppError {
        AppError::provider_failed(format!(
            "component {} is missing file {} in catalog",
            self.component_id().as_str(),
            self.restored_path().as_str()
        ))
    }
}

fn blocked_error(message: impl Into<String>) -> AppError {
    AppError::invalid_input(format!("rollback is blocked: {}", message.into()))
}
