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

pub(crate) use self::apply::{
    apply_operation, ApplyOperationCatalogItemResult, ApplyOperationCatalogResult,
};
pub(crate) use self::create::create_backup;
pub(crate) use self::plan_metadata::metadata_json_for_planned_item;
pub(crate) use self::rollback::{
    rollback_operation, RollbackOperationCatalogItemResult, RollbackOperationCatalogResult,
};

#[cfg(test)]
use self::create::create_backup_with_post_copy;

#[cfg(test)]
pub(crate) use self::plan_metadata::planned_operation_item_metadata_json;

#[cfg(test)]
pub(crate) use self::filesystem::BACKUP_ROOT_DIR_ENV;

#[derive(Debug)]
pub(crate) struct BackupCatalogResult {
    pub(crate) operation_id: OperationId,
    pub(crate) game_id: GameId,
    pub(crate) backup_root: PathRef,
    pub(crate) items: Vec<BackupCatalogItemResult>,
}

#[derive(Debug)]
pub(crate) struct BackupCatalogItemResult {
    pub(crate) backup_id: BackupId,
    pub(crate) component_id: ComponentId,
    pub(crate) original_path: PathRef,
    pub(crate) backup_path: PathRef,
    pub(crate) manifest_path: PathRef,
    pub(crate) sha256: Sha256Hash,
}