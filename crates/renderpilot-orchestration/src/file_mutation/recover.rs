//! Crash recovery for pending durable file transactions.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::PathBuf;

use renderpilot_storage_sqlite::{PendingFileMutationRow, PendingFileMutationState};

use super::manifest::{
    FileMutationManifest, MANIFEST_FORMAT_VERSION, deserialize_manifest, restore_manifest,
};
use super::retryable_v2;
use super::scope::{MutationScope, require_path_in_scope};
use crate::game_mutation_lock::GameMutationGuard;
use crate::{Context, ServiceError};

/// Recovers every row for a game and removes snapshot directories no DB row
/// owns. Must run under the corresponding mutation guard.
pub(crate) fn recover_pending(
    context: &Context,
    guard: &GameMutationGuard,
) -> Result<(), ServiceError> {
    recover_pending_matching(context, guard, |_| true)?;
    sweep_orphan_transaction_dirs(context)
}

/// Recovers only rows explicitly authorized by `select`. Unselected rows and
/// their transaction directories remain untouched. This is the boundary for a
/// feature-specific recovery action; ordinary mutation entry uses
/// [`recover_pending`] to recover the complete game workset.
pub(crate) fn recover_pending_matching(
    context: &Context,
    guard: &GameMutationGuard,
    mut select: impl FnMut(&PendingFileMutationRow) -> bool,
) -> Result<usize, ServiceError> {
    let rows: Vec<PendingFileMutationRow> = context
        .storage()
        .pending_file_mutations_for_game(guard.game_id())?
        .into_iter()
        .filter(|row| select(row))
        .collect();
    if rows.is_empty() {
        return Ok(0);
    }
    let recovered = rows.len();
    for row in rows {
        if retryable_v2::is_v2_manifest(&row.manifest_json)? {
            retryable_v2::recover_pending_v2(context, guard, &row)?;
            continue;
        }
        let manifest = deserialize_manifest(&row)?;
        validate_manifest_format(&manifest)?;
        let transaction_dir = super::validate_transaction_directory_owner(
            context.file_mutation_root(),
            &row.id,
            std::path::Path::new(&manifest.transaction_dir),
        )?;
        match row.state {
            PendingFileMutationState::Preparing => {
                super::remove_dir_if_exists(&transaction_dir)?;
                context
                    .storage()
                    .abandon_file_mutation_preparation(&row.id)?;
            }
            PendingFileMutationState::Prepared => {
                if is_legacy_dlss_feature(&row.feature) {
                    let fence = context
                        .storage()
                        .fence_prepared_file_mutation_resolution(guard.game_id(), &row.id)?;
                    context
                        .storage()
                        .complete_prepared_file_mutation_without_restore(fence)?;
                } else {
                    // This is the only V1 branch that mutates a live target.
                    // Cleanup-only recovery must tolerate an unavailable game.
                    validate_manifest_scope(&manifest, context.file_mutation_root())?;
                    let fence = context
                        .storage()
                        .fence_prepared_file_mutation_resolution(guard.game_id(), &row.id)?;
                    restore_manifest(&manifest, &fence)?;
                    context
                        .storage()
                        .complete_prepared_file_mutation_restored(fence)?;
                }
                if let Err(error) = super::remove_dir_if_exists(&transaction_dir) {
                    log::warn!(
                        "recovered file transaction {} left orphan cleanup: {error}",
                        row.id
                    );
                }
            }
            PendingFileMutationState::Committed => {
                super::remove_dir_if_exists(&transaction_dir)?;
                context.storage().cleanup_committed_file_mutation(&row.id)?;
            }
        }
    }
    Ok(recovered)
}

fn is_legacy_dlss_feature(feature: &str) -> bool {
    matches!(
        feature,
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_INSTALL
            | renderpilot_domain::mutation_features::RENODX_DLSS_FIX_UPDATE
            | renderpilot_domain::mutation_features::RENODX_DLSS_FIX_UNINSTALL
            | renderpilot_domain::mutation_features::RENODX_UPDATE
    )
}

pub(super) fn validate_manifest_scope(
    manifest: &FileMutationManifest,
    transaction_root: &std::path::Path,
) -> Result<(), ServiceError> {
    validate_manifest_format(manifest)?;
    let scope = MutationScope::new(manifest.roots.iter().map(PathBuf::from))?;
    let transaction_root = super::canonical_candidate(transaction_root)?;
    let transaction_dir =
        super::canonical_candidate(std::path::Path::new(&manifest.transaction_dir))?;
    if !crate::paths::is_within(&transaction_dir, &transaction_root) {
        return Err(crate::failed(
            "pending transaction directory is outside app data",
        ));
    }
    for before in &manifest.snapshots {
        require_path_in_scope(std::path::Path::new(&before.path), &scope)?;
        if let Some(snapshot) = &before.snapshot {
            let snapshot = super::canonical_candidate(std::path::Path::new(snapshot))?;
            if !crate::paths::is_within(&snapshot, &transaction_dir) {
                return Err(crate::failed(
                    "pending before-snapshot is outside its transaction",
                ));
            }
        }
    }
    Ok(())
}

fn validate_manifest_format(manifest: &FileMutationManifest) -> Result<(), ServiceError> {
    if manifest.format_version != MANIFEST_FORMAT_VERSION {
        return Err(crate::failed(format!(
            "unsupported file transaction manifest version {}",
            manifest.format_version
        )));
    }
    Ok(())
}

fn sweep_orphan_transaction_dirs(context: &Context) -> Result<(), ServiceError> {
    let root = context.file_mutation_root();
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(crate::failed(format!(
                "failed to inspect file transaction root {}: {error}",
                root.display()
            )));
        }
    };
    let active: HashSet<String> = context
        .storage()
        .all_pending_file_mutation_ids()?
        .into_iter()
        .collect();
    for entry in entries {
        let entry = entry
            .map_err(|error| crate::failed(format!("failed to inspect transaction: {error}")))?;
        if !entry
            .file_type()
            .map_err(|error| crate::failed(format!("failed to inspect transaction type: {error}")))?
            .is_dir()
        {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if !active.contains(&name) {
            super::remove_dir_if_exists(&entry.path())?;
        }
    }
    Ok(())
}
