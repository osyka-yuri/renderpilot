use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process,
};

use renderpilot_application::{
    AppError, AppResult, ArtifactRepository, BackupId, BackupRecord, BackupRepository,
    ComponentRepository, OperationItemRecord, OperationRecord,
};
use renderpilot_domain::{
    ComponentFile, ComponentId, GameId, LibraryArtifact, OperationId, PathRef, Sha256Hash,
};
use renderpilot_storage_sqlite::SqliteStorage;

use super::{
    filesystem::{
        copy_file_with_verification, current_timestamp_millis, ensure_file_exists,
        ensure_file_matches_sha256, sha256_file,
    },
    journal::{require_replacement_plan_item, ReplacementPlanItem, RetryableReplaceOperation},
    plan_metadata::planned_item_metadata,
    shared::{backup_lookup_map, rebuild_component_catalog, BackupLookupKey, CatalogUpdateItem},
};

#[derive(Debug)]
pub(crate) struct ApplyOperationCatalogResult {
    pub(crate) operation: OperationRecord,
    pub(crate) items: Vec<ApplyOperationCatalogItemResult>,
}

#[derive(Debug, Clone)]
pub(crate) struct ApplyOperationCatalogItemResult {
    pub(crate) backup_id: BackupId,
    pub(crate) component_id: ComponentId,
    pub(crate) applied_path: PathRef,
    pub(crate) replacement_path: PathRef,
    pub(crate) backup_path: PathRef,
}

pub(crate) fn apply_operation(
    storage: &SqliteStorage,
    operation_id: OperationId,
) -> AppResult<ApplyOperationCatalogResult> {
    let mut operation = RetryableReplaceOperation::load(storage, &operation_id, "apply", "apply")?;
    let prepared_items = match prepare_apply_items(
        storage,
        operation.journal.operation(),
        operation.journal.items(),
    ) {
        Ok(prepared_items) => prepared_items,
        Err(error) => return Err(operation.capture_blocked(storage, error)),
    };

    match apply_operation_records(storage, &mut operation, &prepared_items) {
        Ok(result) => Ok(result),
        Err(error) => Err(operation.capture_failure(storage, error)),
    }
}

fn prepare_apply_items(
    storage: &SqliteStorage,
    operation: &OperationRecord,
    items: &[OperationItemRecord],
) -> AppResult<Vec<PreparedApplyItem>> {
    let backups = storage.list_backups_for_game(&operation.game_id)?;
    let artifacts = storage.list_artifacts()?;
    let preparation = ApplyPreparation::new(operation, &backups, &artifacts);

    items
        .iter()
        .map(|item| preparation.prepare_item(item))
        .collect()
}

fn apply_operation_records(
    storage: &SqliteStorage,
    operation: &mut RetryableReplaceOperation,
    prepared_items: &[PreparedApplyItem],
) -> AppResult<ApplyOperationCatalogResult> {
    operation.mark_running(storage)?;

    let mut applied_items: Vec<&PreparedApplyItem> = Vec::with_capacity(prepared_items.len());

    for prepared_item in prepared_items {
        if let Err(error) = apply_prepared_item(prepared_item) {
            return Err(rollback_after_apply_failure(error, &applied_items));
        }

        applied_items.push(prepared_item);
    }

    if let Err(error) = update_components_after_apply(
        storage,
        &operation.journal.operation().game_id,
        prepared_items,
    ) {
        return Err(rollback_after_apply_failure(error, &applied_items));
    }

    operation.mark_completed(storage)?;

    Ok(ApplyOperationCatalogResult {
        operation: operation.journal.operation().clone(),
        items: prepared_items
            .iter()
            .map(|item| item.result.clone())
            .collect(),
    })
}

fn apply_prepared_item(prepared_item: &PreparedApplyItem) -> AppResult<()> {
    prepared_item.apply()
}

fn rollback_after_apply_failure(error: AppError, applied_items: &[&PreparedApplyItem]) -> AppError {
    match rollback_applied_items(applied_items) {
        Ok(()) => error,
        Err(rollback_error) => {
            AppError::provider_failed(format!("{error}; rollback failed: {rollback_error}"))
        }
    }
}

fn rollback_applied_items(applied_items: &[&PreparedApplyItem]) -> AppResult<()> {
    let mut first_error = None;

    for prepared_item in applied_items.iter().rev() {
        if let Err(error) = prepared_item.restore() {
            if first_error.is_none() {
                first_error = Some(error);
            }
        }
    }

    if let Some(error) = first_error {
        return Err(error);
    }

    Ok(())
}
fn update_components_after_apply(
    storage: &SqliteStorage,
    game_id: &GameId,
    prepared_items: &[PreparedApplyItem],
) -> AppResult<()> {
    let components = storage.list_components_for_game(game_id)?;
    let updated_components = rebuild_component_catalog(components, prepared_items)?;

    storage.replace_components_for_game(game_id, &updated_components)
}

fn noop_post_copy(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

struct ApplyPreparation<'a> {
    operation: &'a OperationRecord,
    backups_by_key: HashMap<BackupLookupKey, BackupRecord>,
    artifacts_by_id: HashMap<String, LibraryArtifact>,
}

impl<'a> ApplyPreparation<'a> {
    fn new(
        operation: &'a OperationRecord,
        backups: &[BackupRecord],
        artifacts: &[LibraryArtifact],
    ) -> Self {
        let backups_by_key = backup_lookup_map(backups);
        let artifacts_by_id = artifacts
            .iter()
            .cloned()
            .map(|artifact| (artifact.id().as_str().to_owned(), artifact))
            .collect();

        Self {
            operation,
            backups_by_key,
            artifacts_by_id,
        }
    }

    fn prepare_item(&self, item: &OperationItemRecord) -> AppResult<PreparedApplyItem> {
        let replacement_plan = require_replacement_plan_item(self.operation, item)?;
        let expected_hashes = ExpectedItemHashes::from_operation_item(self.operation, item)?;
        let artifact = self.resolve_artifact(&replacement_plan, &expected_hashes.replacement)?;
        let backup = self.resolve_backup(item)?;
        let prepared_item = PreparedApplyItem::new(item.clone(), backup, artifact, expected_hashes);

        prepared_item.ensure_preflight_integrity()?;

        Ok(prepared_item)
    }

    fn resolve_artifact(
        &self,
        replacement_plan: &ReplacementPlanItem<'_>,
        expected_replacement_sha256: &Sha256Hash,
    ) -> AppResult<LibraryArtifact> {
        let artifact = self
            .artifacts_by_id
            .get(replacement_plan.artifact_id.as_str())
            .cloned()
            .ok_or_else(|| {
                blocked_error(format!(
                    "replacement artifact {} is no longer available",
                    replacement_plan.artifact_id.as_str()
                ))
            })?;

        if artifact.path() != replacement_plan.replacement_path {
            return Err(blocked_error(format!(
                "replacement artifact path no longer matches plan for {}",
                replacement_plan.artifact_id.as_str()
            )));
        }

        if artifact.sha256() != expected_replacement_sha256 {
            return Err(blocked_error(format!(
                "replacement artifact {} changed since plan-swap",
                replacement_plan.artifact_id.as_str()
            )));
        }

        Ok(artifact)
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
struct ExpectedItemHashes {
    original: Sha256Hash,
    replacement: Sha256Hash,
}

impl ExpectedItemHashes {
    fn from_operation_item(
        operation: &OperationRecord,
        item: &OperationItemRecord,
    ) -> AppResult<Self> {
        let planned_metadata = planned_item_metadata(item)?;
        let original = planned_metadata.original_sha256.ok_or_else(|| {
            blocked_error(format!(
                "planned original sha256 is missing for component {} in operation {}",
                item.component_id.as_str(),
                operation.id.as_str()
            ))
        })?;
        let replacement = planned_metadata.replacement_sha256.ok_or_else(|| {
            blocked_error(format!(
                "planned replacement sha256 is missing for component {} in operation {}",
                item.component_id.as_str(),
                operation.id.as_str()
            ))
        })?;

        Ok(Self {
            original,
            replacement,
        })
    }
}

#[derive(Debug, Clone)]
struct PreparedApplyItem {
    item: OperationItemRecord,
    backup: BackupRecord,
    artifact: LibraryArtifact,
    expected_original_sha256: Sha256Hash,
    expected_replacement_sha256: Sha256Hash,
    result: ApplyOperationCatalogItemResult,
}

impl PreparedApplyItem {
    fn new(
        item: OperationItemRecord,
        backup: BackupRecord,
        artifact: LibraryArtifact,
        expected_hashes: ExpectedItemHashes,
    ) -> Self {
        let result = ApplyOperationCatalogItemResult {
            backup_id: backup.id.clone(),
            component_id: item.component_id.clone(),
            applied_path: item.source_path.clone(),
            replacement_path: artifact.path().clone(),
            backup_path: backup.backup_path.clone(),
        };

        Self {
            item,
            backup,
            artifact,
            expected_original_sha256: expected_hashes.original,
            expected_replacement_sha256: expected_hashes.replacement,
            result,
        }
    }

    fn ensure_preflight_integrity(&self) -> AppResult<()> {
        self.ensure_backup_integrity()?;
        self.ensure_target_integrity()?;
        self.ensure_artifact_integrity()
    }

    fn ensure_backup_integrity(&self) -> AppResult<()> {
        ensure_file_exists(self.backup_path(), "backup file").map_err(|_| {
            blocked_error(format!(
                "backup file is missing for component {}: {}",
                self.component_id().as_str(),
                self.backup.backup_path.as_str()
            ))
        })?;

        if let Some(expected_sha256) = &self.backup.sha256 {
            ensure_file_matches_sha256(
                self.backup_path(),
                expected_sha256,
                "backup sha256 mismatch",
            )
            .map_err(|_| {
                blocked_error(format!(
                    "backup integrity check failed for component {}",
                    self.component_id().as_str()
                ))
            })?;
        }

        Ok(())
    }

    fn ensure_target_integrity(&self) -> AppResult<()> {
        ensure_file_exists(self.target_path(), "target file").map_err(|_| {
            blocked_error(format!(
                "target file is missing for component {}: {}",
                self.component_id().as_str(),
                self.item.source_path.as_str()
            ))
        })?;

        let actual_sha256 = sha256_file(self.target_path()).map_err(|error| {
            blocked_error(format!(
                "target file could not be hashed for component {}: {error}",
                self.component_id().as_str()
            ))
        })?;

        if actual_sha256 != self.expected_original_sha256 {
            return Err(blocked_error(format!(
                "target changed since plan-swap for component {}",
                self.result.component_id.as_str()
            )));
        }

        Ok(())
    }

    fn ensure_artifact_integrity(&self) -> AppResult<()> {
        ensure_file_exists(self.artifact_path(), "artifact file").map_err(|_| {
            blocked_error(format!(
                "replacement artifact is missing for component {}: {}",
                self.component_id().as_str(),
                self.artifact.path().as_str()
            ))
        })?;
        ensure_file_matches_sha256(
            self.artifact_path(),
            &self.expected_replacement_sha256,
            "artifact sha256 mismatch",
        )
        .map_err(|_| {
            blocked_error(format!(
                "replacement artifact changed since plan-swap for component {}",
                self.component_id().as_str()
            ))
        })
    }

    fn apply(&self) -> AppResult<()> {
        let staged_file = StagedReplacementFile::create(self)?;

        staged_file.replace_target(self)
    }

    fn restore(&self) -> AppResult<()> {
        if let Some(expected_sha256) = &self.backup.sha256 {
            ensure_file_matches_sha256(
                self.backup_path(),
                expected_sha256,
                "backup sha256 mismatch",
            )?;
        }

        copy_file_with_verification(
            self.backup_path(),
            self.target_path(),
            &noop_post_copy,
            "backup restore sha256 mismatch",
        )?;

        Ok(())
    }

    fn component_id(&self) -> &ComponentId {
        &self.result.component_id
    }

    fn applied_path(&self) -> &PathRef {
        &self.result.applied_path
    }

    fn target_path(&self) -> &Path {
        Path::new(self.item.source_path.as_str())
    }

    fn backup_path(&self) -> &Path {
        Path::new(self.backup.backup_path.as_str())
    }

    fn artifact_path(&self) -> &Path {
        Path::new(self.artifact.path().as_str())
    }

    fn updated_component_file(&self, file: &ComponentFile) -> ComponentFile {
        let mut updated_file =
            ComponentFile::new(file.path().clone()).with_sha256(self.artifact.sha256().clone());

        if let Some(version) = self.artifact.version().cloned() {
            updated_file = updated_file.with_version(version);
        }

        updated_file
    }
}

impl CatalogUpdateItem for PreparedApplyItem {
    fn catalog_component_id(&self) -> &ComponentId {
        PreparedApplyItem::component_id(self)
    }

    fn catalog_file_path(&self) -> &PathRef {
        PreparedApplyItem::applied_path(self)
    }

    fn updated_component_file(&self, file: &ComponentFile) -> ComponentFile {
        PreparedApplyItem::updated_component_file(self, file)
    }

    fn missing_catalog_entry_error(&self) -> AppError {
        AppError::provider_failed(format!(
            "component {} is missing file {} in catalog",
            self.component_id().as_str(),
            self.applied_path().as_str()
        ))
    }
}

struct StagedReplacementFile {
    path: PathBuf,
}

impl StagedReplacementFile {
    fn create(prepared_item: &PreparedApplyItem) -> AppResult<Self> {
        let staged_file = Self {
            path: temporary_target_path(prepared_item.target_path())?,
        };

        copy_file_with_verification(
            prepared_item.artifact_path(),
            &staged_file.path,
            &noop_post_copy,
            "temporary replacement sha256 mismatch",
        )?;
        ensure_file_matches_sha256(
            &staged_file.path,
            &prepared_item.expected_replacement_sha256,
            "temporary replacement sha256 mismatch",
        )?;

        Ok(staged_file)
    }

    fn replace_target(self, prepared_item: &PreparedApplyItem) -> AppResult<()> {
        copy_file_with_verification(
            &self.path,
            prepared_item.target_path(),
            &noop_post_copy,
            "applied file sha256 mismatch",
        )?;
        ensure_file_matches_sha256(
            prepared_item.target_path(),
            &prepared_item.expected_replacement_sha256,
            "applied file sha256 mismatch",
        )?;

        Ok(())
    }
}

impl Drop for StagedReplacementFile {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn temporary_target_path(target_path: &Path) -> AppResult<PathBuf> {
    let file_name = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            AppError::invalid_input(format!(
                "target path has no file name: {}",
                target_path.display()
            ))
        })?;
    let timestamp = current_timestamp_millis()?.as_i64();
    let temp_name = format!(".{file_name}.renderpilot-{}-{timestamp}.tmp", process::id());

    Ok(target_path.with_file_name(temp_name))
}

fn blocked_error(message: impl Into<String>) -> AppError {
    AppError::invalid_input(format!("apply is blocked: {}", message.into()))
}
