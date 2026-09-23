//! Crash recovery for pending durable file transactions.

use std::collections::HashMap;
use std::path::PathBuf;

use renderpilot_storage_sqlite::{
    PendingFileMutationRecoveryCandidate, PendingFileMutationRow, PendingFileMutationState,
};

use super::manifest::{
    FileMutationManifest, MANIFEST_FORMAT_VERSION, deserialize_manifest, restore_manifest,
};
use super::optiscaler;
use super::peer_recovery;
use super::retryable_v2;
use super::scope::{MutationScope, require_path_in_scope};
use crate::game_mutation_lock::GameMutationGuard;
use crate::{Context, ServiceError};

/// Recovers every row for a game. Must run under the corresponding mutation
/// guard.
pub(crate) fn recover_pending(
    context: &Context,
    guard: &GameMutationGuard,
) -> Result<(), ServiceError> {
    recover_pending_matching(context, guard, |_| true)?;
    Ok(())
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
    let selected_rows: Vec<PendingFileMutationRow> = context
        .storage()
        .pending_file_mutations_for_game(guard.game_id())?
        .into_iter()
        .filter(|row| select(row))
        .collect();
    if selected_rows.is_empty() {
        return Ok(0);
    }
    let selected_count = selected_rows.len();
    let selected_by_id: HashMap<String, PendingFileMutationRow> = selected_rows
        .into_iter()
        .map(|row| (row.id.clone(), row))
        .collect();
    let candidates = context
        .peer_mutation_executor()
        .recover_pending_file_mutation_candidates_for_game(guard.game_id(), |row| {
            selected_by_id
                .get(&row.id)
                .is_some_and(|selected| selected == row)
        })?;
    if candidates.len() != selected_count {
        return Err(crate::failed(format!(
            "pending recovery selection changed before acquisition: selected {selected_count}, acquired {}",
            candidates.len()
        )));
    }
    for candidate in candidates {
        match candidate {
            PendingFileMutationRecoveryCandidate::OptiScaler(proof) => {
                optiscaler::recover_pending_optiscaler(context, guard, *proof)?;
            }
            PendingFileMutationRecoveryCandidate::NotAggregate(row) => {
                recover_pending_ordinary(context, guard, &row)?;
            }
        }
    }
    Ok(selected_count)
}

fn recover_pending_ordinary(
    context: &Context,
    guard: &GameMutationGuard,
    row: &PendingFileMutationRow,
) -> Result<(), ServiceError> {
    // A Preparing row has not acquired authority to touch live targets. It is
    // safe to abandon only after storage classified it as NotAggregate.
    if matches!(row.state, PendingFileMutationState::Preparing) {
        abandon_preparing_row(context, row)?;
        return Ok(());
    }
    let peer_program =
        renderpilot_storage_sqlite::validated_file_peer_recovery_program_for_feature(
            &row.id,
            &row.feature,
            &row.manifest_json,
        )
        .map_err(|error| {
            crate::failed(format!(
                "peer mutation {} requires repair; recovery retained the {:?} row: {error}",
                row.id, row.state
            ))
        })?;
    if let Some(peer_program) = peer_program {
        peer_recovery::recover_pending_peer(context, guard, row, &peer_program)?;
        return Ok(());
    }
    if retryable_v2::is_v2_manifest(&row.manifest_json)? {
        retryable_v2::recover_pending_v2(context, guard, row)?;
        return Ok(());
    }
    let manifest = deserialize_manifest(row)?;
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
            if is_non_restoring_dlss_feature(&row.feature) {
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
                tracing::warn!(
                    "recovered file transaction {} retained its directory; it is not auto-cleaned: {error}",
                    row.id
                );
            }
        }
        PendingFileMutationState::Committed => {
            super::remove_dir_if_exists(&transaction_dir)?;
            context.storage().cleanup_committed_file_mutation(&row.id)?;
        }
    }
    Ok(())
}

fn abandon_preparing_row(
    context: &Context,
    row: &PendingFileMutationRow,
) -> Result<(), ServiceError> {
    let transaction_dir = super::validate_transaction_directory_owner(
        context.file_mutation_root(),
        &row.id,
        &context.file_mutation_root().join(&row.id),
    )?;
    super::remove_dir_if_exists(&transaction_dir)?;
    context
        .storage()
        .abandon_file_mutation_preparation(&row.id)?;
    Ok(())
}

fn is_non_restoring_dlss_feature(feature: &str) -> bool {
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
    for ancestor in &manifest.peer_ancestors {
        require_path_in_scope(std::path::Path::new(ancestor.path()), &scope)?;
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
