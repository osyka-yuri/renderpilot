//! On-disk before-state manifest for a durable file transaction.

use std::path::Path;

use renderpilot_storage_sqlite::{PendingFileMutationRow, PreparedRestoreFence};
use serde::{Deserialize, Serialize};

use crate::ServiceError;

pub(super) const MANIFEST_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct FileMutationManifest {
    pub(super) format_version: u32,
    pub(super) roots: Vec<String>,
    pub(super) transaction_dir: String,
    pub(super) snapshots: Vec<FileBeforeSnapshot>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct FileBeforeSnapshot {
    pub(super) path: String,
    pub(super) snapshot: Option<String>,
}

pub(super) fn serialize_manifest(manifest: &FileMutationManifest) -> Result<String, ServiceError> {
    serde_json::to_string(manifest).map_err(|error| {
        crate::failed(format!(
            "failed to serialize file transaction manifest: {error}"
        ))
    })
}

pub(super) fn deserialize_manifest(
    row: &PendingFileMutationRow,
) -> Result<FileMutationManifest, ServiceError> {
    serde_json::from_str(&row.manifest_json).map_err(|error| {
        crate::failed(format!(
            "pending file mutation {} has an invalid manifest: {error}",
            row.id
        ))
    })
}

/// Restores only after storage minted a fence for this exact Prepared row.
pub(super) fn restore_manifest(
    manifest: &FileMutationManifest,
    _fence: &PreparedRestoreFence,
) -> Result<(), ServiceError> {
    for before in manifest.snapshots.iter().rev() {
        let path = Path::new(&before.path);
        match &before.snapshot {
            Some(snapshot) => crate::fs::copy_file_atomically(Path::new(snapshot), path)?,
            None => crate::fs::remove_file_if_exists(path)?,
        }
    }
    Ok(())
}

pub(super) fn cleanup_manifest(manifest: &FileMutationManifest) -> Result<(), ServiceError> {
    super::remove_dir_if_exists(Path::new(&manifest.transaction_dir))
}
