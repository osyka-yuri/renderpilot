//! Authorized mutation roots for a durable file transaction.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::ServiceError;

/// Canonical directories an operation is explicitly allowed to mutate.
#[derive(Debug, Clone)]
pub(crate) struct MutationScope {
    pub(super) roots: Vec<PathBuf>,
}

impl MutationScope {
    /// Builds a scope that requires every root to have an existing filesystem
    /// ancestor. Use for install/update/swap where the game folder is expected
    /// to be reachable.
    pub(crate) fn new(roots: impl IntoIterator<Item = PathBuf>) -> Result<Self, ServiceError> {
        let mut seen = HashSet::new();
        let mut canonical_roots = Vec::new();
        for root in roots {
            let root = crate::paths::canonical_candidate(&root)
                .map_err(|error| crate::failed(error.to_string()))?;
            push_root(&mut seen, &mut canonical_roots, root)?;
        }
        if canonical_roots.is_empty() {
            return Err(crate::failed(
                "file mutation scope must contain at least one root",
            ));
        }
        Ok(Self {
            roots: canonical_roots,
        })
    }

    /// Builds a scope from roots that still have an existing filesystem ancestor.
    /// Completely unreachable roots are dropped. Returns `None` when nothing
    /// remains — callers should take a metadata-only path (no game-file
    /// snapshots) rather than failing uninstall of an orphaned install.
    pub(crate) fn try_from_reachable_roots(
        roots: impl IntoIterator<Item = PathBuf>,
    ) -> Result<Option<Self>, ServiceError> {
        let mut seen = HashSet::new();
        let mut canonical_roots = Vec::new();
        for root in roots {
            let Ok(root) = crate::paths::canonical_candidate(&root) else {
                continue;
            };
            push_root(&mut seen, &mut canonical_roots, root)?;
        }
        if canonical_roots.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            roots: canonical_roots,
        }))
    }

    pub(crate) fn single(root: &Path) -> Result<Self, ServiceError> {
        Self::new([root.to_path_buf()])
    }

    /// Canonical roots carried by this validated authority. Durable
    /// coordinators may persist a capability projection of these roots, but
    /// callers cannot construct or mutate the underlying set directly.
    pub(crate) fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub(super) fn contains(&self, path: &Path) -> Result<bool, ServiceError> {
        let candidate = super::canonical_candidate(path)?;
        Ok(self
            .roots
            .iter()
            .any(|root| crate::paths::is_within(&candidate, root)))
    }

    /// Returns whether a path is under this scope after best-effort
    /// canonicalization. Unreachable paths are treated as outside scope.
    pub(crate) fn contains_reachable(&self, path: &Path) -> bool {
        let Ok(candidate) = crate::paths::canonical_candidate(path) else {
            return false;
        };
        self.roots
            .iter()
            .any(|root| crate::paths::is_within(&candidate, root))
    }
}

fn push_root(
    seen: &mut HashSet<String>,
    canonical_roots: &mut Vec<PathBuf>,
    root: PathBuf,
) -> Result<(), ServiceError> {
    if root.exists() && !root.is_dir() {
        return Err(crate::failed(format!(
            "file mutation root is not a directory: {}",
            root.display()
        )));
    }
    if seen.insert(crate::paths::normalized_key(&root)) {
        canonical_roots.push(root);
    }
    Ok(())
}

pub(super) fn require_path_in_scope(
    path: &Path,
    scope: &MutationScope,
) -> Result<(), ServiceError> {
    if scope.contains(path)? {
        Ok(())
    } else {
        Err(crate::failed(format!(
            "refusing to mutate path outside authorized roots: {}",
            path.display()
        )))
    }
}
