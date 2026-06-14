//! Filesystem mutations for an overlay apply and its revert, with crash-durable
//! flushing and best-effort rollback of partial changes.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::{ComponentFile, GraphicsComponent};

use crate::fs_sync;

use super::types::{AppliedFsChanges, PlannedFile};

pub(super) fn perform_apply_fs(
    component: &GraphicsComponent,
    baseline: &[ComponentFile],
    planned: &[PlannedFile],
    removed: &[ComponentFile],
    first_swap: bool,
) -> AppResult<AppliedFsChanges> {
    let mut changes = AppliedFsChanges::default();
    let result = apply_fs_steps(
        component,
        baseline,
        planned,
        removed,
        first_swap,
        &mut changes,
    );
    if let Err(error) = result {
        changes.undo();
        return Err(error);
    }
    Ok(changes)
}

fn apply_fs_steps(
    component: &GraphicsComponent,
    baseline: &[ComponentFile],
    planned: &[PlannedFile],
    removed: &[ComponentFile],
    first_swap: bool,
    changes: &mut AppliedFsChanges,
) -> AppResult<()> {
    // On a re-swap, revert the current overlay back to the recorded baseline first
    // so the new package installs cleanly — no mixed versions, no stale leftovers.
    if !first_swap {
        revert_to_baseline_fs(component.files(), baseline)?;
    }

    remove_downgrade_members(removed, changes)?;
    overlay_planned_files(planned, changes)?;

    // Flush the directory entries (renames to `.bak`, new files) so the layout
    // is durable, not just the file contents above. Best-effort: the data is
    // already durable, and a directory-sync failure must not fail the swap.
    sync_touched_directories(changes);

    Ok(())
}

/// Downgrade cleanup: a unified FSR 3.x target drops the upscaling-stack
/// members the baseline holds. Back each up to its `.bak` (so rollback can
/// restore it) and remove it, so the folder lands on a clean FSR 3.1 — never
/// a mix. On a re-swap the revert that precedes this restores baseline-owned
/// members from their `.bak`s first, and this pass removes them again.
fn remove_downgrade_members(
    removed: &[ComponentFile],
    changes: &mut AppliedFsChanges,
) -> AppResult<()> {
    for file in removed {
        let target = real_path(file);
        if !target.exists() {
            continue;
        }
        let bak = bak_path(file);
        if bak.exists() {
            fs::remove_file(&bak).map_err(|error| {
                AppError::provider_failed(format!(
                    "failed to clear stale backup {}: {error}",
                    bak.display()
                ))
            })?;
        }
        fs::rename(&target, &bak).map_err(|error| {
            AppError::provider_failed(format!(
                "failed to back up {} before removing it: {error}",
                target.display()
            ))
        })?;
        changes.renamed_to_bak.push((target, bak));
    }
    Ok(())
}

/// Overlay the package: back up + replace any file already at a target, add the
/// rest. Untouched files stay where they are.
fn overlay_planned_files(planned: &[PlannedFile], changes: &mut AppliedFsChanges) -> AppResult<()> {
    for plan in planned {
        let target = plan.target();
        if target.exists() {
            let bak = plan.target_bak();
            // Drop any stale leftover backup so `.bak` holds the current original.
            if bak.exists() {
                fs::remove_file(&bak).map_err(|error| {
                    AppError::provider_failed(format!(
                        "failed to clear stale backup {}: {error}",
                        bak.display()
                    ))
                })?;
            }
            fs::rename(&target, &bak).map_err(|error| {
                AppError::provider_failed(format!(
                    "failed to back up {} before replacing it: {error}",
                    target.display()
                ))
            })?;
            changes.renamed_to_bak.push((target.clone(), bak));
        }

        fs::copy(&plan.source, &target).map_err(|error| {
            AppError::provider_failed(format!(
                "failed to install file to {}: {error}",
                target.display()
            ))
        })?;
        // Track the copy before flushing so a `sync_file` failure still triggers
        // its removal during `undo()`.
        changes.copied.push(target.clone());
        // Flush the installed bytes to durable storage. A crash/power-loss after
        // a bare `fs::copy` can leave a torn or zero-length DLL on disk, which
        // the game would then load. Treat a flush failure as fatal so the swap
        // rolls back rather than committing an undurable file.
        fs_sync::sync_file(&target).map_err(|error| {
            AppError::provider_failed(format!(
                "failed to flush {} to disk: {error}",
                target.display()
            ))
        })?;
    }
    Ok(())
}

/// Fsyncs each distinct directory touched by the overlay so renames and new
/// files survive a crash. Best-effort; see [`fs_sync::sync_directory_best_effort`].
fn sync_touched_directories(changes: &AppliedFsChanges) {
    let mut synced: HashSet<PathBuf> = HashSet::new();
    let touched = changes
        .copied
        .iter()
        .chain(changes.renamed_to_bak.iter().map(|(target, _bak)| target));
    for path in touched {
        if let Some(parent) = path.parent() {
            if synced.insert(parent.to_path_buf()) {
                fs_sync::sync_directory_best_effort(parent);
            }
        }
    }
}

/// Reverts the directory to `baseline`: delete files the overlay added (current
/// files whose path is not a baseline path) and restore each baseline file that
/// has a `.bak`. Used by both rollback and the re-swap path. Retry-safe: it only
/// touches files that still have a `.bak`, so re-running after a partial failure
/// completes cleanly.
pub(super) fn revert_to_baseline_fs(
    current: &[ComponentFile],
    baseline: &[ComponentFile],
) -> AppResult<()> {
    let baseline_paths: HashSet<&str> = baseline.iter().map(|f| f.path().as_str()).collect();

    // 1. Delete files the swap added (not part of the baseline).
    for file in current {
        if !baseline_paths.contains(file.path().as_str()) {
            let path = real_path(file);
            if path.exists() {
                fs::remove_file(&path).map_err(|error| {
                    AppError::provider_failed(format!(
                        "failed to remove added file {}: {error}",
                        path.display()
                    ))
                })?;
            }
        }
    }

    // 2. Restore each baseline file that was replaced (has a `.bak`). Files that
    //    were merely kept have no `.bak` and are left as-is.
    for file in baseline {
        let real = real_path(file);
        let bak = bak_path(file);
        if !bak.exists() {
            continue;
        }
        if real.exists() {
            fs::remove_file(&real).map_err(|error| {
                AppError::provider_failed(format!(
                    "failed to clear {} before restore: {error}",
                    real.display()
                ))
            })?;
        }
        fs::rename(&bak, &real).map_err(|error| {
            AppError::provider_failed(format!(
                "failed to restore backup to {}: {error}",
                real.display()
            ))
        })?;
    }

    // Flush the directory entries so the deletes/renames above survive a crash.
    // Without this, `rollback_component` could commit the rollback to SQLite while
    // the `.bak`→live rename never reaches disk, leaving the catalog claiming a
    // restore the filesystem lost. The restored content is the pre-existing `.bak`
    // (already durable), so a directory fsync is sufficient. Best-effort, mirroring
    // the apply path; on a re-swap the later overlay fsync of the same directory is
    // a cheap, idempotent repeat.
    sync_component_file_dirs(current.iter().chain(baseline));

    Ok(())
}

/// Fsyncs the distinct parent directories of `files` (best-effort), making the
/// deletes/renames performed against them durable.
fn sync_component_file_dirs<'a>(files: impl IntoIterator<Item = &'a ComponentFile>) {
    let mut synced: HashSet<PathBuf> = HashSet::new();
    for file in files {
        let path = real_path(file);
        if let Some(parent) = path.parent() {
            if synced.insert(parent.to_path_buf()) {
                fs_sync::sync_directory_best_effort(parent);
            }
        }
    }
}

fn real_path(file: &ComponentFile) -> PathBuf {
    PathBuf::from(file.path().as_str())
}

fn bak_path(file: &ComponentFile) -> PathBuf {
    PathBuf::from(format!("{}.bak", file.path().as_str()))
}
