use renderpilot_application::BackupId;
use renderpilot_domain::{ComponentId, GameId, OperationId, PathRef, Sha256Hash};

mod apply;
mod create;
mod filesystem;
mod journal;
mod manifest;
mod plan_metadata;
mod rollback;
mod shared;

#[cfg(test)]
mod tests;

// Public crate-facing API.

pub(crate) use apply::{
    apply_operation, ApplyOperationCatalogItemResult, ApplyOperationCatalogResult,
};

pub(crate) use create::create_backup;

pub(crate) use plan_metadata::{
    metadata_json_for_planned_item, metadata_json_for_planned_operation,
    planned_operation_confirmation_token,
};

pub(crate) use rollback::{
    rollback_operation, RollbackOperationCatalogItemResult, RollbackOperationCatalogResult,
};

// Test-only API.

#[cfg(test)]
use create::create_backup_with_post_copy;

#[cfg(test)]
pub(crate) use filesystem::BACKUP_ROOT_DIR_ENV;

#[cfg(test)]
pub(crate) use plan_metadata::planned_operation_item_metadata_json;

/// Result of creating a backup catalog for a single operation.
#[derive(Debug)]
#[must_use]
pub(crate) struct BackupCatalogResult {
    pub(crate) operation_id: OperationId,
    pub(crate) game_id: GameId,
    pub(crate) backup_root: PathRef,
    pub(crate) items: Vec<BackupCatalogItemResult>,
}

/// Result of backing up a single component.
#[derive(Debug)]
#[must_use]
pub(crate) struct BackupCatalogItemResult {
    pub(crate) backup_id: BackupId,
    pub(crate) component_id: ComponentId,
    pub(crate) original_path: PathRef,
    pub(crate) backup_path: PathRef,
    pub(crate) manifest_path: PathRef,
    pub(crate) sha256: Sha256Hash,
}
