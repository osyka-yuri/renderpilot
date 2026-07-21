//! Cross-feature path comparison, normalization, and bare-name safety.
//!
//! Add-on-neutral: coordinated files, crash-recoverable transactions, catalog
//! swaps, and individual tool modules all build on these. Nothing here knows
//! about a concrete add-on kind, so generic layers (`coordinated_files`,
//! `file_mutation`) never need to import a tool implementation.
//!
//! ## Modules
//!
//! - root items -- path equality / containment / canonical candidates
//! - [`names`] -- pure bare file-name and path-component safety (no I/O)
//!
//! Call sites use the flat `crate::paths::*` surface (including name helpers
//! re-exported from [`names`]).
//!
//! Distinct from [`crate::catalog::scan::paths`]: that module works on already
//! normalized `PathRef`-style `/` strings; this one accepts OS paths.

mod names;

pub(crate) use names::is_safe_file_name;

use std::path::{Path, PathBuf};

/// Lowercased, forward-slash-normalized comparison key for a path.
///
/// Thin wrapper over [`renderpilot_domain::normalized_path_key`] for
/// [`std::path::Path`]. Purely lexical -- no filesystem access. Use
/// [`same_path`] when `.`/`..` or symlinks must compare equal.
#[must_use]
pub(crate) fn normalized_key(path: &Path) -> String {
    renderpilot_domain::normalized_path_key(&path.to_string_lossy())
}

/// Best-effort canonicalization: resolves symlinks and `.`/`..` when the path
/// exists on disk, falls back to the input path otherwise. Returns a usable
/// [`PathBuf`], not a comparison key -- for equality use [`same_path`].
#[must_use]
pub(crate) fn canonicalize_best_effort(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Path equality after best-effort canonicalization, so `.`/relative forms and
/// symlinks compare equal when the targets exist on disk. When either path is
/// missing (or canonicalize fails), falls back to [`normalized_key`] so Windows
/// case variants and slash styles still compare equal.
#[must_use]
pub(crate) fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => normalized_key(left) == normalized_key(right),
    }
}

/// Returns `true` when `path` is `root` itself or a descendant of it, compared
/// case-insensitively after forward-slash normalization. No filesystem access.
/// Handles Windows drive-root scopes (e.g. `D:/`) correctly.
#[must_use]
pub(crate) fn is_within(path: &Path, root: &Path) -> bool {
    let path = normalized_key(path);
    let root = normalized_key(root);
    if path == root {
        return true;
    }
    let root_prefix = if root.ends_with('/') {
        root
    } else {
        format!("{root}/")
    };
    path.starts_with(&root_prefix)
}

/// Resolves a possibly-not-yet-existing path to its canonical form by walking
/// up to the nearest existing ancestor and re-joining the remaining suffix.
///
/// Use when a path must be canonicalized before its target is created (a live
/// file about to be overwritten, a sidecar that does not exist yet). For a path
/// that already exists this is equivalent to [`std::fs::canonicalize`].
pub(crate) fn canonical_candidate(path: &Path) -> std::io::Result<PathBuf> {
    if path.exists() {
        return std::fs::canonicalize(path);
    }
    let mut ancestor = path;
    while !ancestor.exists() {
        ancestor = ancestor.parent().ok_or_else(|| {
            std::io::Error::other(format!("path has no existing ancestor: {}", path.display()))
        })?;
    }
    let suffix = path.strip_prefix(ancestor).map_err(|error| {
        std::io::Error::other(format!(
            "failed to resolve path {} against ancestor {}: {error}",
            path.display(),
            ancestor.display()
        ))
    })?;
    std::fs::canonicalize(ancestor).map(|canonical| canonical.join(suffix))
}

#[cfg(test)]
mod tests;
