use std::collections::HashSet;
use std::fs;
use std::path::Path;

use renderpilot_domain::GameId;
use renderpilot_storage_sqlite::{BeginFileMutationPreparation, PreparedMutationResolutionFence};

use super::super::scope::{MutationScope, require_path_in_scope};
use super::io;
use super::model::{
    FORMAT_VERSION, ManifestOperationV2, ManifestV2, RetryableFileOperation, RetryableFilePlan,
    SnapshotV2, V2DiskObservation, digest_bytes, matches_regular_digest, serialize,
};
use super::observe;
use crate::game_mutation_lock::GameMutationGuard;
use crate::{Context, ServiceError};

/// Live transaction state. `applied` records only operations whose postimage
/// was observed successfully; rollback never guesses about a foreign object.
pub(crate) struct RetryableFileMutationV2 {
    id: String,
    game_id: GameId,
    manifest: ManifestV2,
    applied: Vec<usize>,
}

impl RetryableFileMutationV2 {
    #[cfg(test)]
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    /// Recovers old rows, validates every exact target, snapshots every current
    /// regular file, then publishes `Prepared` before the first live mutation.
    pub(crate) fn prepare(
        context: &Context,
        guard: &GameMutationGuard,
        scope: &MutationScope,
        feature: &str,
        subject_id: Option<&str>,
        plan: &RetryableFilePlan,
    ) -> Result<Self, ServiceError> {
        super::super::recover_pending(context, guard)?;
        if plan.operations.is_empty() {
            return Err(crate::failed("v2 file mutation requires an operation"));
        }
        if feature.trim().is_empty() {
            return Err(crate::failed("feature must not be empty"));
        }

        let id = ulid::Ulid::generate().to_string();
        let transaction_dir = context.file_mutation_root().join(&id);
        let roots: Vec<String> = scope
            .roots
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect();
        let initial = ManifestV2 {
            format_version: FORMAT_VERSION,
            roots: roots.clone(),
            transaction_dir: transaction_dir.to_string_lossy().into_owned(),
            operations: Vec::new(),
            snapshots: Vec::new(),
        };
        context
            .storage()
            .begin_file_mutation_preparation(&BeginFileMutationPreparation {
                id: id.clone(),
                game_id: guard.game_id().clone(),
                feature: feature.to_owned(),
                subject_id: subject_id.map(str::to_owned),
                initial_manifest_json: serialize(&initial)?,
            })?;

        let prepared = (|| {
            fs::create_dir_all(&transaction_dir).map_err(|error| {
                crate::failed(format!(
                    "failed to create v2 file transaction directory {}: {error}",
                    transaction_dir.display()
                ))
            })?;
            let manifest = build_manifest(scope, &transaction_dir, roots, &plan.operations)?;
            context
                .storage()
                .finish_preparing_file_mutation(&id, &serialize(&manifest)?)?;
            Ok(manifest)
        })();

        let manifest = match prepared {
            Ok(manifest) => manifest,
            Err(error) => {
                if let Err(cleanup_error) = cleanup_preparing(context, &id, &transaction_dir) {
                    return Err(crate::failed(format!(
                        "{error}; abandoning the preparing v2 file transaction also failed: {cleanup_error}"
                    )));
                }
                return Err(error);
            }
        };
        Ok(Self {
            id,
            game_id: guard.game_id().clone(),
            manifest,
            applied: Vec::new(),
        })
    }

    /// Applies each operation with a final no-follow compare-and-swap check.
    /// Identical writes are intentionally omitted and never become rollback work.
    pub(crate) fn apply(&mut self) -> Result<(), ServiceError> {
        for index in 0..self.manifest.operations.len() {
            let operation = self.manifest.operations.get(index).ok_or_else(|| {
                crate::failed(format!(
                    "v2 manifest operation {index} disappeared during apply"
                ))
            })?;
            let path = Path::new(operation.path());
            let expected = operation.expected();
            let live = observe(path);
            if &live != expected {
                return Err(token_drift(path, expected, &live));
            }
            match operation {
                ManifestOperationV2::Write {
                    post_digest,
                    expected,
                    ..
                } => {
                    if matches_regular_digest(expected, post_digest) {
                        continue;
                    }
                    let bytes = match self.write_bytes(index, path, expected, post_digest) {
                        Ok(bytes) => bytes,
                        Err(error) => {
                            // A successful atomic replace/create can be reported
                            // after a later durability error. If the full exact
                            // postimage landed, record it so synchronous rollback
                            // can restore the immutable preimage.
                            if matches_regular_digest(&observe(path), post_digest) {
                                self.applied.push(index);
                            }
                            return Err(error);
                        }
                    };
                    // Recheck by content after the write so synchronous rollback has
                    // an exact postimage even if an OS call reported success oddly.
                    if !matches_regular_digest(&observe(path), post_digest) {
                        return Err(crate::failed(format!(
                            "v2 write postimage drift at {} after {} bytes",
                            path.display(),
                            bytes.len()
                        )));
                    }
                    self.applied.push(index);
                }
                ManifestOperationV2::Delete { .. } => {
                    fs::remove_file(path).map_err(|error| {
                        crate::failed(format!("failed to delete {}: {error}", path.display()))
                    })?;
                    crate::fs::sync_parent_directory_best_effort(path);
                    if observe(path) != V2DiskObservation::Absent {
                        return Err(crate::failed(format!(
                            "v2 delete did not leave {} absent",
                            path.display()
                        )));
                    }
                    self.applied.push(index);
                }
            }
        }
        Ok(())
    }

    /// Runs the feature persistence step. On any synchronous failure only
    /// applied postimages are reversed; a foreign replacement is preserved and
    /// reported as a combined rollback conflict.
    pub(crate) fn commit_or_rollback<T>(
        mut self,
        context: &Context,
        work: impl FnOnce(&str) -> Result<T, ServiceError>,
    ) -> Result<T, ServiceError> {
        match self.apply().and_then(|()| work(&self.id)) {
            Ok(value) => {
                if let Err(error) = self.cleanup_committed(context) {
                    log::warn!("v2 transaction committed but cleanup is pending: {error}");
                }
                Ok(value)
            }
            Err(primary) => match self.rollback_prepared(context) {
                Ok(()) => Err(primary),
                Err(rollback) => Err(ServiceError::rollback_also_failed(
                    primary.to_string(),
                    rollback.to_string(),
                )),
            },
        }
    }

    fn cleanup_committed(&self, context: &Context) -> Result<(), ServiceError> {
        super::super::remove_dir_if_exists(Path::new(&self.manifest.transaction_dir))?;
        context
            .storage()
            .cleanup_committed_file_mutation(&self.id)?;
        Ok(())
    }

    fn rollback_prepared(&mut self, context: &Context) -> Result<(), ServiceError> {
        let fence = context
            .storage()
            .fence_prepared_file_mutation_resolution(&self.game_id, &self.id)?;
        self.rollback_sync(&fence)?;
        context
            .storage()
            .complete_prepared_file_mutation_restored(fence)?;
        if let Err(error) =
            super::super::remove_dir_if_exists(Path::new(&self.manifest.transaction_dir))
        {
            log::warn!("rolled-back v2 transaction left orphan cleanup: {error}");
        }
        Ok(())
    }

    fn write_bytes(
        &self,
        index: usize,
        path: &Path,
        expected: &V2DiskObservation,
        post_digest: &str,
    ) -> Result<Vec<u8>, ServiceError> {
        // The immutable artifact prevents a closure from changing the forward
        // payload after the Prepared manifest was published. Its digest must
        // match the manifest before the live write begins.
        let bytes = io::read_verified_payload(
            Path::new(&self.manifest.transaction_dir),
            index,
            post_digest,
        )?;
        io::write_forward(path, expected, &bytes)?;
        Ok(bytes)
    }

    fn rollback_sync(
        &mut self,
        _fence: &PreparedMutationResolutionFence,
    ) -> Result<(), ServiceError> {
        let mut failures = Vec::new();
        for index in self.applied.iter().rev().copied() {
            let Some(operation) = self.manifest.operations.get(index) else {
                failures.push(format!("v2 operation {index}: missing manifest entry"));
                continue;
            };
            let Some(snapshot) = self
                .manifest
                .snapshots
                .iter()
                .find(|snapshot| snapshot.path == operation.path())
            else {
                failures.push(format!("{}: missing v2 preimage", operation.path()));
                continue;
            };
            let path = Path::new(operation.path());
            let allowed = match operation {
                ManifestOperationV2::Write { post_digest, .. } => {
                    matches_regular_digest(&observe(path), post_digest)
                }
                ManifestOperationV2::Delete { .. } => observe(path) == V2DiskObservation::Absent,
            };
            if !allowed {
                failures.push(format!(
                    "{}: live target no longer matches the v2 rollback token",
                    path.display()
                ));
                continue;
            }
            if let Err(error) = io::restore_snapshot(snapshot) {
                failures.push(format!("{}: {error}", path.display()));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(crate::failed(format!(
                "v2 rollback encountered conflicts: {}",
                failures.join("; ")
            )))
        }
    }
}

fn build_manifest(
    scope: &MutationScope,
    transaction_dir: &Path,
    roots: Vec<String>,
    operations: &[RetryableFileOperation],
) -> Result<ManifestV2, ServiceError> {
    let mut seen = HashSet::new();
    let mut manifest_ops = Vec::with_capacity(operations.len());
    let mut snapshots = Vec::with_capacity(operations.len());
    for (index, operation) in operations.iter().enumerate() {
        let path = operation.path();
        require_path_in_scope(path, scope)?;
        if !seen.insert(crate::paths::normalized_key(path)) {
            return Err(crate::failed(format!(
                "v2 file mutation has duplicate target {}",
                path.display()
            )));
        }
        let current = observe(path);
        if !current.can_mutate() {
            return Err(crate::failed(format!(
                "v2 target is unsafe to mutate {}: {current:?}",
                path.display()
            )));
        }
        if &current != operation.expected() {
            return Err(token_drift(path, operation.expected(), &current));
        }
        let snapshot = io::snapshot_preimage(transaction_dir, index, path, &current)?;
        snapshots.push(SnapshotV2 {
            path: path.to_string_lossy().into_owned(),
            before: current,
            snapshot,
        });
        match operation {
            RetryableFileOperation::Write {
                path,
                bytes,
                expected,
            } => {
                let payload = transaction_dir.join(format!("{index}.payload"));
                io::write_new_no_clobber(&payload, bytes)?;
                manifest_ops.push(ManifestOperationV2::Write {
                    path: path.to_string_lossy().into_owned(),
                    expected: expected.clone(),
                    post_digest: digest_bytes(bytes),
                });
            }
            RetryableFileOperation::Delete { path, expected } => {
                manifest_ops.push(ManifestOperationV2::Delete {
                    path: path.to_string_lossy().into_owned(),
                    expected: expected.clone(),
                });
            }
        }
    }
    Ok(ManifestV2 {
        format_version: FORMAT_VERSION,
        roots,
        transaction_dir: transaction_dir.to_string_lossy().into_owned(),
        operations: manifest_ops,
        snapshots,
    })
}

fn cleanup_preparing(context: &Context, id: &str, directory: &Path) -> Result<(), ServiceError> {
    super::super::remove_dir_if_exists(directory)?;
    context.storage().abandon_file_mutation_preparation(id)?;
    Ok(())
}

fn token_drift(
    path: &Path,
    expected: &V2DiskObservation,
    actual: &V2DiskObservation,
) -> ServiceError {
    crate::failed(format!(
        "v2 file mutation target changed before apply {}: expected {expected:?}, found {actual:?}",
        path.display()
    ))
}
