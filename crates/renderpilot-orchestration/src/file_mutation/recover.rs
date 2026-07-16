//! Crash recovery for pending durable file transactions.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::PathBuf;

use renderpilot_storage_sqlite::PendingFileMutationState;

use super::manifest::{
    FileMutationManifest, MANIFEST_FORMAT_VERSION, cleanup_manifest, deserialize_manifest,
    restore_manifest,
};
use super::scope::{MutationScope, require_path_in_scope};
use crate::game_mutation_lock::GameMutationGuard;
use crate::{Context, ServiceError};

/// Recovers every row for a game and removes snapshot directories no DB row
/// owns. Must run under the corresponding mutation guard.
pub(crate) fn recover_pending(
    context: &Context,
    guard: &GameMutationGuard,
) -> Result<(), ServiceError> {
    let rows = context
        .storage()
        .pending_file_mutations_for_game(guard.game_id())?;
    for row in rows {
        let manifest = deserialize_manifest(&row)?;
        validate_manifest_scope(&manifest, context.file_mutation_root())?;
        match row.state {
            PendingFileMutationState::Preparing => {
                cleanup_manifest(&manifest)?;
                context.storage().delete_pending_file_mutation(&row.id)?;
            }
            PendingFileMutationState::Prepared => {
                restore_manifest(&manifest)?;
                context.storage().delete_pending_file_mutation(&row.id)?;
                if let Err(error) = cleanup_manifest(&manifest) {
                    log::warn!(
                        "recovered file transaction {} left orphan cleanup: {error}",
                        row.id
                    );
                }
            }
            PendingFileMutationState::Committed => {
                cleanup_manifest(&manifest)?;
                context.storage().delete_pending_file_mutation(&row.id)?;
            }
        }
    }
    sweep_orphan_transaction_dirs(context)
}

fn validate_manifest_scope(
    manifest: &FileMutationManifest,
    transaction_root: &std::path::Path,
) -> Result<(), ServiceError> {
    if manifest.format_version != MANIFEST_FORMAT_VERSION {
        return Err(crate::failed(format!(
            "unsupported file transaction manifest version {}",
            manifest.format_version
        )));
    }
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
