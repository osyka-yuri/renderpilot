use std::collections::HashSet;
use std::path::Path;

use renderpilot_storage_sqlite::{PendingFileMutationRow, PendingFileMutationState};

use super::model::{
    FORMAT_VERSION, ManifestOperationV2, ManifestV2, V2DiskObservation, is_sha256_digest,
};
use crate::game_mutation_lock::GameMutationGuard;
use crate::{Context, ServiceError};

pub(in crate::file_mutation) fn is_v2_manifest(json: &str) -> Result<bool, ServiceError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|error| {
        crate::failed(format!(
            "pending file mutation has invalid manifest JSON: {error}"
        ))
    })?;
    let version = value
        .get("format_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| crate::failed("pending file mutation has no valid format version"))?;
    match version {
        1 => Ok(false),
        version if version == u64::from(FORMAT_VERSION) => Ok(true),
        version => Err(crate::failed(format!(
            "unsupported pending file mutation manifest version {version}"
        ))),
    }
}

/// V2 crash recovery is intentionally cleanup-only. A `Prepared` row never
/// proves which target operations reached disk, so touching live targets would
/// risk replacing or deleting a user file created after a crash.
pub(in crate::file_mutation) fn recover_pending_v2(
    context: &Context,
    guard: &GameMutationGuard,
    row: &PendingFileMutationRow,
) -> Result<(), ServiceError> {
    if &row.game_id != guard.game_id() {
        return Err(crate::failed(format!(
            "pending v2 mutation {} belongs to a different game",
            row.id
        )));
    }
    let manifest: ManifestV2 = serde_json::from_str(&row.manifest_json).map_err(|error| {
        crate::failed(format!(
            "pending v2 mutation {} has invalid manifest: {error}",
            row.id
        ))
    })?;
    // Cleanup touches only this app-private directory. Deliberately do not
    // canonicalize manifest game roots or targets: after a crash they may be
    // unreachable, and V2 has no authority to touch them anyway.
    let transaction_dir = super::super::validate_transaction_directory_owner(
        context.file_mutation_root(),
        &row.id,
        Path::new(&manifest.transaction_dir),
    )?;
    validate_manifest_shape(&manifest, &transaction_dir)?;
    match row.state {
        PendingFileMutationState::Preparing => {
            super::super::remove_dir_if_exists(&transaction_dir)?;
            context
                .storage()
                .abandon_file_mutation_preparation(&row.id)?;
        }
        PendingFileMutationState::Prepared => {
            let fence = context
                .storage()
                .fence_prepared_file_mutation_resolution(guard.game_id(), &row.id)?;
            context
                .storage()
                .complete_prepared_file_mutation_without_restore(fence)?;
            if let Err(error) = super::super::remove_dir_if_exists(&transaction_dir) {
                log::warn!(
                    "v2 recovered transaction {} left orphan cleanup: {error}",
                    row.id
                );
            }
        }
        PendingFileMutationState::Committed => {
            super::super::remove_dir_if_exists(&transaction_dir)?;
            context.storage().cleanup_committed_file_mutation(&row.id)?;
        }
    }
    Ok(())
}

fn validate_manifest_shape(
    manifest: &ManifestV2,
    transaction_dir: &Path,
) -> Result<(), ServiceError> {
    if manifest.format_version != FORMAT_VERSION {
        return Err(crate::failed("unsupported v2 file mutation manifest"));
    }
    if manifest.roots.is_empty() || manifest.roots.iter().any(|root| root.trim().is_empty()) {
        return Err(crate::failed("v2 file mutation manifest has invalid roots"));
    }
    if manifest.operations.len() != manifest.snapshots.len() {
        return Err(crate::failed(
            "v2 file mutation manifest has mismatched targets",
        ));
    }

    let mut targets = HashSet::with_capacity(manifest.operations.len());
    for (operation, snapshot) in manifest.operations.iter().zip(&manifest.snapshots) {
        if operation.path().is_empty()
            || snapshot.path.is_empty()
            || operation.path() != snapshot.path
            || !targets.insert(crate::paths::normalized_key(Path::new(operation.path())))
        {
            return Err(crate::failed(
                "v2 file mutation manifest has invalid target shape",
            ));
        }
        if !operation.expected().can_mutate() || snapshot.before != *operation.expected() {
            return Err(crate::failed(
                "v2 file mutation manifest has invalid target observation",
            ));
        }
        match operation {
            ManifestOperationV2::Write { post_digest, .. } if !is_sha256_digest(post_digest) => {
                return Err(crate::failed(
                    "v2 file mutation manifest has invalid postimage digest",
                ));
            }
            ManifestOperationV2::Write { .. } | ManifestOperationV2::Delete { .. } => {}
        }
        match (&snapshot.before, &snapshot.snapshot) {
            (V2DiskObservation::Regular { .. }, Some(path)) if !path.is_empty() => {
                let snapshot = super::super::canonical_candidate(Path::new(path))?;
                if !crate::paths::is_within(&snapshot, transaction_dir) {
                    return Err(crate::failed(
                        "v2 file mutation manifest has a preimage outside its transaction",
                    ));
                }
            }
            (V2DiskObservation::Absent, None) => {}
            _ => {
                return Err(crate::failed(
                    "v2 file mutation manifest has invalid preimage",
                ));
            }
        }
    }
    Ok(())
}
