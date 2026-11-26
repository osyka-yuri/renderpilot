//! Crash-recoverable transactions for game and external add-on file roots.
//!
//! ## Contract
//!
//! 1. Hold [`crate::game_mutation_lock::GameMutationGuard`].
//! 2. [`recover_pending`] runs (also from [`DurableFileTransaction::prepare`]).
//!    Boundary entry already recovers; prepare re-runs it idempotently so
//!    hand-rolled multi-step flows cannot skip recovery.
//! 3. Snapshot every path the feature may touch (over-inclusive is correct).
//! 4. Mutate files, then feature-commit DB with the reserved `mutation_id`.
//! 5. On success clean snapshots; on failure restore exact before-state.
//!
//! Prefer [`run_durable_mutation`] at call sites. Hand-rolled prepare/finish is
//! reserved for multi-step flows that open an engine sentinel first.

pub(crate) mod manifest;
pub(crate) mod optiscaler;
mod peer_recovery;
mod recover;
mod retryable_v2;
mod scope;
mod transaction;

#[cfg(test)]
mod tests;

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use renderpilot_application::ProxyTopologyRepository;
use renderpilot_domain::{AddonKind, mutation_features::MutationFeatureOwner};

pub(crate) use recover::{recover_pending, recover_pending_matching};
pub(crate) use retryable_v2::{
    RetryableFileMutationV2, RetryableFileOperation, RetryableFilePlan, V2DiskObservation, observe,
};
pub(crate) use scope::MutationScope;
pub(crate) use transaction::{DurableFileTransaction, DurableMutation, run_durable_mutation};

/// Fences coordinated peer mutations before a durable row or live write can
/// be created. Recovery runs before this check so an already-prepared inverse
/// can still restore its exact pre-state; the new mutation itself remains
/// forbidden until OptiScaler releases the topology.
pub(crate) fn ensure_feature_allowed_with_proxy_topology(
    context: &crate::Context,
    game_id: &renderpilot_domain::GameId,
    feature: &str,
) -> Result<(), crate::ServiceError> {
    if context.storage().get_proxy_topology(game_id)?.is_none() {
        return Ok(());
    }
    let owner = renderpilot_domain::mutation_features::feature_owner(feature).ok_or_else(|| {
        crate::ServiceError::invalid_input(format!(
            "durable mutation feature cannot be classified under an active proxy topology: {feature}"
        ))
    })?;
    let peer_kind = match owner {
        MutationFeatureOwner::Luma => Some(AddonKind::Luma),
        MutationFeatureOwner::RenoDx => Some(AddonKind::RenoDx),
        MutationFeatureOwner::Catalog
        | MutationFeatureOwner::OptiScaler
        | MutationFeatureOwner::SharedVulkan => None,
    };
    if let Some(peer_kind) = peer_kind {
        return Err(crate::ServiceError::peer_topology_conflict(peer_kind));
    }
    Ok(())
}

/// A planned OptiScaler filesystem target. Write plans use an exact file or
/// absence precondition; observational/removal targets may deliberately accept
/// either state when their aggregate transition supplies the stronger guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MutationTarget {
    pub(crate) path: PathBuf,
    expected_preimage: MutationExpectedPreimage,
    kind: MutationTargetKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MutationExpectedPreimage {
    Any,
    Absent,
    File(renderpilot_domain::Sha256Hash),
}

/// Declared endpoint kind for an absent precondition.  The planner never
/// infers a directory from path spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MutationTargetKind {
    File,
    Directory,
}

impl MutationTarget {
    pub(crate) fn quarantine(
        path: &Path,
        expected_sha256: Option<renderpilot_domain::Sha256Hash>,
    ) -> Self {
        Self {
            path: path.to_path_buf(),
            expected_preimage: expected_sha256.map_or(
                MutationExpectedPreimage::Any,
                MutationExpectedPreimage::File,
            ),
            kind: MutationTargetKind::File,
        }
    }

    pub(crate) fn absent_file(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            expected_preimage: MutationExpectedPreimage::Absent,
            kind: MutationTargetKind::File,
        }
    }

    pub(crate) fn absent_directory(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            expected_preimage: MutationExpectedPreimage::Absent,
            kind: MutationTargetKind::Directory,
        }
    }

    pub(crate) fn copy(path: &Path) -> Self {
        Self::quarantine(path, None)
    }

    pub(crate) fn expected_sha256(&self) -> Option<&renderpilot_domain::Sha256Hash> {
        match &self.expected_preimage {
            MutationExpectedPreimage::File(sha256) => Some(sha256),
            MutationExpectedPreimage::Any | MutationExpectedPreimage::Absent => None,
        }
    }

    pub(crate) fn expects_absence(&self) -> bool {
        self.expected_preimage == MutationExpectedPreimage::Absent
    }

    pub(crate) fn is_absent_directory(&self) -> bool {
        self.expects_absence() && self.kind == MutationTargetKind::Directory
    }
}

/// Merges ordinary snapshots and explicit removal targets without duplicating
/// paths. This is intentionally local to OptiScaler's planner; other add-on
/// engines continue to use their existing durable snapshot contract.
pub(crate) fn apply_snapshot_overrides(
    paths: impl IntoIterator<Item = PathBuf>,
    targets: impl IntoIterator<Item = MutationTarget>,
    publication_paths: impl IntoIterator<Item = PathBuf>,
    roots: &[PathBuf],
) -> Result<Vec<MutationTarget>, crate::ServiceError> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    // Explicit targets carry the plan-time digest used by destructive CAS.
    // Insert them first so an over-inclusive ordinary snapshot of the same
    // path cannot silently erase that stronger precondition.
    for target in targets
        .into_iter()
        .chain(paths.into_iter().map(|path| MutationTarget::copy(&path)))
    {
        if seen.insert(crate::paths::normalized_key(&target.path)) {
            result.push(target);
        }
    }
    let mut parent_paths = std::collections::BTreeMap::new();
    for publication_path in publication_paths {
        let selected_root = roots
            .iter()
            .filter(|root| {
                matches!(
                    renderpilot_domain::normalized_path_relation(
                        &root.to_string_lossy(),
                        &publication_path.to_string_lossy(),
                    ),
                    renderpilot_domain::NormalizedPathRelation::LeftAncestor
                )
            })
            // Scopes normally contain disjoint roots. If a caller supplied
            // nested roots, select the narrowest declared authority rather
            // than inspecting ancestors under the broader root.
            .max_by_key(|root| root.components().count())
            .ok_or_else(|| {
                crate::failed(format!(
                    "OptiScaler publication target is not a strict descendant of a declared root: {}",
                    publication_path.display()
                ))
            })?;
        match std::fs::symlink_metadata(selected_root) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(crate::failed(format!(
                    "OptiScaler declared publication root is a symbolic link or reparse point: {}",
                    selected_root.display()
                )));
            }
            Ok(_) => {
                return Err(crate::failed(format!(
                    "OptiScaler declared publication root is not a directory: {}",
                    selected_root.display()
                )));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(crate::failed(format!(
                    "OptiScaler declared publication root is missing: {}",
                    selected_root.display()
                )));
            }
            Err(error) => {
                return Err(crate::failed(format!(
                    "failed to inspect OptiScaler declared publication root {}: {error}",
                    selected_root.display()
                )));
            }
        }
        let mut parent = publication_path.parent();
        while let Some(path) = parent {
            if crate::paths::normalized_key(path) == crate::paths::normalized_key(selected_root) {
                break;
            }
            if path.is_empty() {
                return Err(crate::failed(format!(
                    "OptiScaler publication target escaped its declared root: {}",
                    publication_path.display()
                )));
            }
            match std::fs::symlink_metadata(path) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => break,
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(crate::failed(format!(
                        "OptiScaler publication ancestor is a symbolic link or reparse point: {}",
                        path.display()
                    )));
                }
                Ok(_) => {
                    return Err(crate::failed(format!(
                        "OptiScaler publication ancestor is not a directory: {}",
                        path.display()
                    )));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    parent_paths
                        .entry(crate::paths::normalized_key(path))
                        .or_insert_with(|| path.to_path_buf());
                }
                Err(error) => {
                    return Err(crate::failed(format!(
                        "failed to inspect OptiScaler publication ancestor {}: {error}",
                        path.display()
                    )));
                }
            }
            parent = path.parent();
        }
    }
    let mut parent_paths = parent_paths.into_iter().collect::<Vec<_>>();
    parent_paths.sort_by(|(left_key, left), (right_key, right)| {
        left.components()
            .count()
            .cmp(&right.components().count())
            .then_with(|| left_key.cmp(right_key))
    });
    let mut parent_targets = Vec::with_capacity(parent_paths.len());
    for (key, path) in parent_paths {
        if let Some(index) = result
            .iter()
            .position(|target| crate::paths::normalized_key(&target.path) == key)
        {
            let target = result.remove(index);
            if !target.is_absent_directory() {
                return Err(crate::failed(format!(
                    "OptiScaler publication parent must be an absent directory target: {}",
                    path.display()
                )));
            }
            parent_targets.push(target);
        } else {
            parent_targets.push(MutationTarget::absent_directory(&path));
        }
    }
    parent_targets.extend(result);
    Ok(parent_targets)
}

pub(super) fn remove_dir_if_exists(directory: &Path) -> Result<(), crate::ServiceError> {
    match fs::remove_dir_all(directory) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(crate::failed(format!(
            "failed to remove file transaction directory {}: {error}",
            directory.display()
        ))),
    }
}

pub(super) fn canonical_candidate(path: &Path) -> Result<PathBuf, crate::ServiceError> {
    crate::paths::canonical_candidate(path).map_err(|error| crate::failed(error.to_string()))
}

/// Proves that a durable row owns exactly its app-private `root/<id>`
/// directory. Containment alone is insufficient because a corrupt manifest
/// could otherwise target the root or a sibling transaction's artifacts.
pub(super) fn validate_transaction_directory_owner(
    root: &Path,
    id: &str,
    declared: &Path,
) -> Result<PathBuf, crate::ServiceError> {
    let mut components = Path::new(id).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(crate::failed(
            "pending transaction id is not a single path component",
        ));
    }
    let canonical_root = canonical_candidate(root)?;
    let expected = canonical_root.join(id);
    let declared = canonical_candidate(declared)?;
    if !crate::paths::is_within(&expected, &canonical_root)
        || crate::paths::normalized_key(&declared) != crate::paths::normalized_key(&expected)
    {
        return Err(crate::failed(
            "pending transaction directory does not match its durable row id",
        ));
    }
    Ok(declared)
}
