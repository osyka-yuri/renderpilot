//! Rollback / uninstall logic for the install engine.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::canonicalize_best_effort;
use super::errors;
use super::helpers;
use crate::ServiceError;

/// Reverses an install described by its recorded file lists, returning the folder to
/// its prior state. Idempotent and safe to re-run: missing files are ignored.
///
/// Deletes files the install created that did not shadow a pre-existing file, then
/// restores every backed-up original from its `.bak` (which also overwrites a file
/// written on top of it, such as a merged foreign config).
pub fn uninstall(
    created_files: &[PathBuf],
    backed_up_files: &[PathBuf],
) -> Result<(), ServiceError> {
    let backed_up: HashSet<&Path> = backed_up_files.iter().map(PathBuf::as_path).collect();

    for path in created_files {
        if !backed_up.contains(path.as_path()) {
            helpers::remove_file_if_exists(path)?;
        }
    }

    let mut touched_dirs: HashSet<PathBuf> = HashSet::new();
    for path in backed_up_files {
        let bak = helpers::bak_path(path);
        if !bak.exists() {
            log::warn!(
                "addon uninstall: backup `{}` is missing; cannot restore the original file",
                bak.display()
            );
            continue;
        }
        helpers::remove_file_if_exists(path)?;
        fs::rename(&bak, path).map_err(|error| errors::io("restore backup", path, &error))?;
        helpers::insert_parent(&mut touched_dirs, path);
    }

    for path in created_files {
        helpers::insert_parent(&mut touched_dirs, path);
    }
    for dir in touched_dirs {
        crate::fs::sync_directory_best_effort(&dir);
    }
    Ok(())
}

/// Like [`uninstall`], but for a tree-shaped install (one that used
/// [`super::FileOp::CreateNested`]).
pub fn uninstall_tree(
    created_files: &[PathBuf],
    backed_up_files: &[PathBuf],
    boundary: &Path,
) -> Result<(), ServiceError> {
    uninstall(created_files, backed_up_files)?;
    cleanup_empty_dirs_best_effort(created_files, boundary);
    Ok(())
}

/// Best-effort removes now-empty directories derived from the parent chains of
/// `created_files`, deepest first, strictly below (excluding) `boundary`.
pub fn cleanup_empty_dirs_best_effort(created_files: &[PathBuf], boundary: &Path) {
    let boundary = canonicalize_best_effort(boundary);
    let mut candidates: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for file in created_files {
        let mut current = file.parent();
        while let Some(dir) = current {
            let canonical = canonicalize_best_effort(dir);
            if canonical == boundary || !canonical.starts_with(&boundary) {
                break;
            }
            if seen.insert(canonical) {
                candidates.push(dir.to_path_buf());
            }
            current = dir.parent();
        }
    }

    // deepest first
    candidates.sort_by_key(|p| p.components().count());
    candidates.reverse();

    for dir in candidates {
        let _ = helpers::remove_dir_if_empty(&dir);
    }
}
