//! Prepare / commit / rollback for a durable game-file transaction.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use renderpilot_domain::GameId;
use renderpilot_storage_sqlite::{BeginFileMutationPreparation, SqliteStorage};

use super::manifest::{
    FileBeforeSnapshot, FileMutationManifest, MANIFEST_FORMAT_VERSION, cleanup_manifest,
    restore_manifest, serialize_manifest,
};
use super::scope::{MutationScope, require_path_in_scope};
use crate::game_mutation_lock::GameMutationGuard;
use crate::{Context, ServiceError};

/// Durable before-state for one feature transaction over authorized file roots
/// (game directory and optional external add-on roots).
pub(crate) struct DurableFileTransaction {
    id: String,
    game_id: GameId,
    transaction_root: PathBuf,
    manifest: FileMutationManifest,
}

impl DurableFileTransaction {
    /// Reserves a row, snapshots every target, then publishes `Prepared` before
    /// the caller may perform its first game-file mutation.
    pub(crate) fn prepare(
        context: &Context,
        guard: &GameMutationGuard,
        scope: &MutationScope,
        feature: &str,
        subject_id: Option<&str>,
        paths: impl IntoIterator<Item = PathBuf>,
    ) -> Result<Self, ServiceError> {
        super::recover_pending(context, guard)?;
        validate_required_text("feature", feature)?;

        let id = ulid::Ulid::generate().to_string();
        let transaction_dir = context.file_mutation_root().join(&id);
        let initial_manifest = FileMutationManifest {
            format_version: MANIFEST_FORMAT_VERSION,
            roots: scope
                .roots
                .iter()
                .map(|root| root.to_string_lossy().into_owned())
                .collect(),
            transaction_dir: transaction_dir.to_string_lossy().into_owned(),
            snapshots: Vec::new(),
        };
        let initial_json = serialize_manifest(&initial_manifest)?;
        context
            .storage()
            .begin_file_mutation_preparation(&BeginFileMutationPreparation {
                id: id.clone(),
                game_id: guard.game_id().clone(),
                feature: feature.to_owned(),
                subject_id: subject_id.map(str::to_owned),
                initial_manifest_json: initial_json,
            })?;

        // Scoped so preparation errors run cleanup before returning.
        let prepared = (|| {
            fs::create_dir_all(&transaction_dir).map_err(|error| {
                crate::failed(format!(
                    "failed to create file transaction directory {}: {error}",
                    transaction_dir.display()
                ))
            })?;
            let manifest = build_manifest(scope, &transaction_dir, paths)?;
            let manifest_json = serialize_manifest(&manifest)?;
            context
                .storage()
                .finish_preparing_file_mutation(&id, &manifest_json)?;
            Ok(Self {
                id: id.clone(),
                game_id: guard.game_id().clone(),
                transaction_root: context.file_mutation_root().to_path_buf(),
                manifest,
            })
        })();

        if let Err(error) = prepared {
            if let Err(cleanup_error) = cleanup_preparing(context.storage(), &id, &transaction_dir)
            {
                return Err(crate::failed(format!(
                    "{error}; abandoning the preparing file transaction also failed: {cleanup_error}"
                )));
            }
            return Err(error);
        }
        prepared
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    /// Restores the exact before-state after durably fencing catalog authority.
    /// A restore error deliberately retains both the Prepared row and matching
    /// invalidation token for retry/recovery.
    pub(crate) fn rollback(self, storage: &SqliteStorage) -> Result<(), ServiceError> {
        super::recover::validate_manifest_scope(&self.manifest, &self.transaction_root)?;
        let fence = storage.fence_prepared_file_mutation_restore(&self.game_id, &self.id)?;
        restore_manifest(&self.manifest, &fence)?;
        storage.complete_prepared_file_mutation_restore(&fence)?;
        if let Err(error) = cleanup_manifest(&self.manifest) {
            log::warn!("rolled-back file transaction left cleanup for the orphan sweep: {error}");
        }
        Ok(())
    }

    /// Cleans snapshots after the feature commit atomically marked this row
    /// `Committed`.
    pub(crate) fn cleanup_committed(self, storage: &SqliteStorage) -> Result<(), ServiceError> {
        super::recover::validate_manifest_scope(&self.manifest, &self.transaction_root)?;
        cleanup_manifest(&self.manifest)?;
        storage.cleanup_committed_file_mutation(&self.id)?;
        Ok(())
    }

    /// Runs `work` under the durable transaction. On success: calls
    /// `on_committed`, then [`cleanup_committed`]. On failure: calls
    /// [`rollback`], then `on_rolled_back` (if rollback succeeded) or
    /// [`combine_rollback_error`] (if rollback also failed).
    ///
    /// Every site that previously hand-rolled the Ok→cleanup / Err→rollback
    /// match should route through this so the cleanup/rollback contract cannot
    /// be partially skipped.
    pub(crate) fn commit_or_rollback<T, E: Into<crate::ServiceError>>(
        self,
        storage: &SqliteStorage,
        work: impl FnOnce() -> Result<T, E>,
        on_committed: impl FnOnce(&T),
        on_rolled_back: impl FnOnce(),
    ) -> Result<T, crate::ServiceError> {
        match work() {
            Ok(value) => {
                on_committed(&value);
                if let Err(error) = self.cleanup_committed(storage) {
                    log::warn!("transaction committed but cleanup is pending: {error}");
                }
                Ok(value)
            }
            Err(error) => {
                let error = error.into();
                match self.rollback(storage) {
                    Ok(()) => {
                        on_rolled_back();
                        Err(error)
                    }
                    Err(rollback_error) => Err(combine_rollback_error(&error, &rollback_error)),
                }
            }
        }
    }
}

/// Combines a primary error with a rollback failure into a single error so the
/// caller does not silently lose either side. Used by every durable-transaction
/// site after [`DurableFileTransaction::rollback`] fails.
pub(super) fn combine_rollback_error(
    primary: &ServiceError,
    rollback_error: &ServiceError,
) -> ServiceError {
    ServiceError::rollback_also_failed(primary.to_string(), rollback_error.to_string())
}

/// Inputs for one crash-recoverable game-file mutation.
pub(crate) struct DurableMutation<'a> {
    pub(crate) context: &'a Context,
    pub(crate) guard: &'a GameMutationGuard,
    pub(crate) scope: &'a MutationScope,
    pub(crate) feature: &'a str,
    pub(crate) subject_id: Option<&'a str>,
    pub(crate) paths: Vec<PathBuf>,
}

/// Prepares a durable transaction, runs `work` with the reserved `mutation_id`,
/// then cleans up or rolls back. Feature commits (`commit_game_mutation`) stay
/// inside `work` so each command can assemble its own DB half.
pub(crate) fn run_durable_mutation<T, E: Into<ServiceError>>(
    mutation: DurableMutation<'_>,
    work: impl FnOnce(&str) -> Result<T, E>,
    on_committed: impl FnOnce(&T),
    on_rolled_back: impl FnOnce(),
) -> Result<T, ServiceError> {
    let prepared = DurableFileTransaction::prepare(
        mutation.context,
        mutation.guard,
        mutation.scope,
        mutation.feature,
        mutation.subject_id,
        mutation.paths,
    )?;
    let mutation_id = prepared.id().to_owned();
    prepared.commit_or_rollback(
        mutation.context.storage(),
        || work(&mutation_id),
        on_committed,
        on_rolled_back,
    )
}

fn build_manifest(
    scope: &MutationScope,
    transaction_dir: &Path,
    paths: impl IntoIterator<Item = PathBuf>,
) -> Result<FileMutationManifest, ServiceError> {
    let mut seen = HashSet::new();
    let mut snapshots = Vec::new();
    for path in paths {
        require_path_in_scope(&path, scope)?;
        if !seen.insert(crate::paths::normalized_key(&path)) {
            continue;
        }
        let snapshot = match fs::metadata(&path) {
            Ok(metadata) if !metadata.is_file() => {
                return Err(crate::failed(format!(
                    "cannot mutate non-file path {}",
                    path.display()
                )));
            }
            Ok(_) => {
                let snapshot = transaction_dir.join(format!("{}.before", snapshots.len()));
                crate::fs::copy_file_atomically(&path, &snapshot)?;
                Some(snapshot.to_string_lossy().into_owned())
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(crate::failed(format!(
                    "failed to inspect {} before mutation: {error}",
                    path.display()
                )));
            }
        };
        snapshots.push(FileBeforeSnapshot {
            path: path.to_string_lossy().into_owned(),
            snapshot,
        });
    }

    Ok(FileMutationManifest {
        format_version: MANIFEST_FORMAT_VERSION,
        roots: scope
            .roots
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect(),
        transaction_dir: transaction_dir.to_string_lossy().into_owned(),
        snapshots,
    })
}

fn cleanup_preparing(
    storage: &SqliteStorage,
    id: &str,
    transaction_dir: &Path,
) -> Result<(), ServiceError> {
    super::remove_dir_if_exists(transaction_dir)?;
    storage.abandon_file_mutation_preparation(id)?;
    Ok(())
}

fn validate_required_text(field: &str, value: &str) -> Result<(), ServiceError> {
    if value.trim().is_empty() {
        Err(crate::failed(format!("{field} must not be empty")))
    } else {
        Ok(())
    }
}
