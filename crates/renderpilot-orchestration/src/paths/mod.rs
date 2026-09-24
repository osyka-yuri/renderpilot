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
#[cfg(windows)]
mod windows;

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

/// Canonicalizes an existing path and returns the stable native spelling used
/// by filesystem authority records.
///
/// Windows may return an equivalent DOS 8.3 alias from
/// [`std::fs::canonicalize`]. Expand those aliases before the path crosses an
/// orchestration or storage boundary so roots and their descendants cannot be
/// represented by different names for the same directory.
pub(crate) fn canonicalize_existing(path: &Path) -> std::io::Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)?;
    #[cfg(windows)]
    {
        windows::expand_short_names(&canonical)
    }
    #[cfg(not(windows))]
    {
        Ok(canonical)
    }
}

/// Removes a Windows verbatim prefix from a canonical path for APIs that
/// accept ordinary DOS or UNC path syntax only. Drive paths (`\\?\D:\...`)
/// become `D:\...`; verbatim UNC paths (`\\?\UNC\server\share\...`) become
/// `\\server\share\...`. Other device-namespace forms fail closed.
pub(crate) fn strip_windows_verbatim_prefix(path: &Path) -> std::io::Result<PathBuf> {
    let normalized = strip_windows_verbatim_prefix_lexically(path)?;
    if normalized == path {
        return Ok(normalized);
    }

    #[cfg(windows)]
    {
        // Verbatim paths can address names (for example, components ending in
        // dots or spaces) that ordinary DOS/UNC paths normalize differently.
        // Accept the shortened spelling only when Windows resolves both forms
        // to the same canonical native path.
        let original_canonical = std::fs::canonicalize(path)?;
        let normalized_canonical = std::fs::canonicalize(&normalized)?;
        if normalized_key(&original_canonical) != normalized_key(&normalized_canonical) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "ordinary DOS/UNC spelling resolves to a different Windows path",
            ));
        }
    }

    Ok(normalized)
}

fn strip_windows_verbatim_prefix_lexically(path: &Path) -> std::io::Result<PathBuf> {
    let value = path.to_string_lossy();
    let rest = if let Some(rest) = value.strip_prefix(r"\\?\") {
        rest
    } else if let Some(rest) = value.strip_prefix("//?/") {
        rest
    } else {
        return Ok(path.to_path_buf());
    };

    if rest.get(..4).is_some_and(|prefix| {
        prefix.eq_ignore_ascii_case("UNC\\") || prefix.eq_ignore_ascii_case("UNC/")
    }) {
        let unc_tail = &rest[4..];
        let mut parts = unc_tail.split(['\\', '/']).filter(|part| !part.is_empty());
        if parts.next().is_some() && parts.next().is_some() {
            return Ok(PathBuf::from(format!(r"\\{}", unc_tail.replace('/', "\\"))));
        }
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "verbatim UNC path is missing a server or share",
        ));
    }

    let bytes = rest.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
    {
        return Ok(PathBuf::from(rest.replace('/', "\\")));
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "unsupported Windows verbatim device namespace",
    ))
}

/// Best-effort canonicalization: resolves symlinks and `.`/`..` when the path
/// exists on disk, falls back to the input path otherwise. Returns a usable
/// [`PathBuf`], not a comparison key -- for equality use [`same_path`].
#[must_use]
pub(crate) fn canonicalize_best_effort(path: &Path) -> PathBuf {
    canonicalize_existing(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Path equality after best-effort canonicalization, so `.`/relative forms and
/// symlinks compare equal when the targets exist on disk. When either path is
/// missing (or canonicalize fails), falls back to [`normalized_key`] so Windows
/// case variants and slash styles still compare equal.
#[must_use]
pub(crate) fn same_path(left: &Path, right: &Path) -> bool {
    match (canonicalize_existing(left), canonicalize_existing(right)) {
        (Ok(left), Ok(right)) => normalized_key(&left) == normalized_key(&right),
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
/// that already exists this is equivalent to [`canonicalize_existing`].
pub(crate) fn canonical_candidate(path: &Path) -> std::io::Result<PathBuf> {
    if path.exists() {
        return canonicalize_existing(path);
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
    canonicalize_existing(ancestor).map(|canonical| canonical.join(suffix))
}

#[cfg(test)]
mod tests;
