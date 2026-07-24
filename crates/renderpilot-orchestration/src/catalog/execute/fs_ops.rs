//! Filesystem mutations for an overlay apply and its revert, with crash-durable
//! flushing and best-effort rollback of partial changes.
//!
//! Catalog apply/revert builds [`CoordinatedFilePlan`] steps and runs them through
//! [`execute_file_plans`](crate::coordinated_files::execute_file_plans):
//!
//! - path-source installs use `OverlayFromPath` (no full-DLL memory load);
//! - baseline archive+remove uses `ArchiveLiveToSidecarAndRemove`;
//! - revert restores with `RestorePreservingSidecar` first, then `ReleaseSidecar`
//!   so a mid-batch failure leaves sidecars for a safe retry;
//! - [`AppliedFsLog`] is filled from the batch touch log for parent-dir fsync.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::{ComponentFile, GraphicsComponent, Sha256Hash};

use crate::coordinated_files::{
    CoordinatedFilePlan, FilePlanBatchLog, execute_file_plans, execute_restore_batch,
};

use super::types::{AppliedFsLog, PlannedFile};

pub(super) fn perform_apply_fs(
    component: &GraphicsComponent,
    baseline: &[ComponentFile],
    planned: &[PlannedFile],
    removed: &[ComponentFile],
) -> AppResult<AppliedFsLog> {
    // Pre-mutation validation and snapshotting are performed once by
    // `DurableFileTransaction::prepare` (→ `build_manifest`) using the path set
    // from `apply_mutation_paths`. `perform_apply_fs` does not recompute or
    // re-validate that set — doing so would be a redundant second pass that
    // could diverge from the snapshotted set.
    let plans = plan_converge_active_set(component, baseline, planned, removed)?;
    let log = execute_file_plans(&plans).map_err(map_service_error)?;
    let changes = AppliedFsLog {
        created_sidecars: log.created_sidecars,
        copied: log.copied,
    };
    sync_touched_directories(&changes);
    Ok(changes)
}

/// Builds the ordered plan list that converges immutable baseline + current
/// active set onto the next desired overlay.
fn plan_converge_active_set(
    component: &GraphicsComponent,
    baseline: &[ComponentFile],
    planned: &[PlannedFile],
    removed: &[ComponentFile],
) -> AppResult<Vec<CoordinatedFilePlan>> {
    let baseline_by_key: HashMap<String, &ComponentFile> = baseline
        .iter()
        .map(|file| (crate::paths::normalized_key(&real_path(file)), file))
        .collect();
    let baseline_paths: HashSet<String> = baseline_by_key.keys().cloned().collect();
    let current_paths: HashSet<String> = component
        .files()
        .iter()
        .map(|file| crate::paths::normalized_key(&real_path(file)))
        .collect();
    let planned_paths: HashSet<String> = planned
        .iter()
        .map(|plan| crate::paths::normalized_key(&plan.target()))
        .collect();
    let removed_paths: HashSet<String> = removed
        .iter()
        .map(|file| crate::paths::normalized_key(&real_path(file)))
        .collect();
    let desired_baseline_paths: HashSet<String> = baseline_paths
        .iter()
        .filter(|path| !planned_paths.contains(*path) && !removed_paths.contains(*path))
        .cloned()
        .collect();

    let mut plans = Vec::new();

    for file in baseline {
        let target = real_path(file);
        if desired_baseline_paths.contains(&crate::paths::normalized_key(&target)) {
            plans.push(CoordinatedFilePlan::RestorePreservingSidecar {
                path: target,
                baseline_sha256: require_baseline_hash(file)?,
            });
        }
    }

    for file in component.files() {
        let target = real_path(file);
        let key = crate::paths::normalized_key(&target);
        if planned_paths.contains(&key) || desired_baseline_paths.contains(&key) {
            continue;
        }
        if let Some(baseline_file) = baseline_by_key.get(&key) {
            plans.push(CoordinatedFilePlan::ArchiveLiveToSidecarAndRemove {
                path: target,
                expected_live: require_baseline_hash(baseline_file)?,
            });
        } else {
            plans.push(CoordinatedFilePlan::RemoveLive { path: target });
        }
    }

    for plan in planned {
        let target = plan.target();
        let key = crate::paths::normalized_key(&target);
        if let Some(baseline_file) = baseline_by_key.get(&key) {
            plans.push(CoordinatedFilePlan::EnsureBaselineSidecar {
                path: target.clone(),
                expected_live: require_baseline_hash(baseline_file)?,
            });
        } else if target.exists() && !current_paths.contains(&key) {
            return Err(AppError::provider_failed(format!(
                "refusing to overwrite untracked file {}",
                target.display()
            )));
        }
        // Install crash-atomically via the shared path-source plan variant.
        plans.push(CoordinatedFilePlan::OverlayFromPath {
            path: target,
            source: plan.source.clone(),
        });
    }

    Ok(plans)
}

/// Reverts the directory to `baseline`: delete files the overlay added (current
/// files whose path is not a baseline path) and restore each baseline file that
/// has a `.bak`. Retry-safe: restores keep sidecars until every copy succeeds.
pub(crate) fn revert_to_baseline_fs(
    current: &[ComponentFile],
    baseline: &[ComponentFile],
) -> AppResult<()> {
    restore_baseline_preserving_sidecars(current, baseline)?;
    release_baseline_sidecars(current, baseline)
}

/// Restores and verifies every DLL while retaining all baseline sidecars.
pub(super) fn restore_baseline_preserving_sidecars(
    current: &[ComponentFile],
    baseline: &[ComponentFile],
) -> AppResult<()> {
    let baseline_paths: HashSet<String> = baseline
        .iter()
        .map(|file| crate::paths::normalized_key(&real_path(file)))
        .collect();

    // 1. Delete files the swap added (not part of the baseline).
    let removes: Vec<_> = current
        .iter()
        .filter(|file| !baseline_paths.contains(&crate::paths::normalized_key(&real_path(file))))
        .map(|file| CoordinatedFilePlan::RemoveLive {
            path: real_path(file),
        })
        .collect();
    execute_file_plans(&removes).map_err(map_service_error)?;

    // 2. Restore every baseline while retaining sidecars.
    let mut restores = Vec::with_capacity(baseline.len());
    for file in baseline {
        restores.push(CoordinatedFilePlan::RestorePreservingSidecar {
            path: real_path(file),
            baseline_sha256: require_baseline_hash(file)?,
        });
    }
    let _log: FilePlanBatchLog =
        execute_restore_batch(restores, Vec::new()).map_err(map_service_error)?;

    // Flush restored copies before auxiliary-file verification.
    sync_component_file_dirs(current.iter().chain(baseline));

    Ok(())
}

/// Releases DLL sidecars only after every component/auxiliary restore verified.
pub(super) fn release_baseline_sidecars(
    current: &[ComponentFile],
    baseline: &[ComponentFile],
) -> AppResult<()> {
    let releases: Vec<_> = baseline
        .iter()
        .map(|file| CoordinatedFilePlan::ReleaseSidecar {
            path: real_path(file),
        })
        .collect();
    execute_file_plans(&releases).map_err(map_service_error)?;
    sync_component_file_dirs(current.iter().chain(baseline));
    Ok(())
}

/// Fsyncs each distinct directory touched by the overlay so sidecars and new
/// files survive a crash. Best-effort; see [`crate::fs::sync_directory_best_effort`].
fn sync_touched_directories(changes: &AppliedFsLog) {
    let mut synced: HashSet<PathBuf> = HashSet::new();
    let touched = changes
        .copied
        .iter()
        .chain(changes.created_sidecars.iter().map(|(target, _bak)| target));
    for path in touched {
        if let Some(parent) = path.parent()
            && synced.insert(parent.to_path_buf())
        {
            crate::fs::sync_directory_best_effort(parent);
        }
    }
}

/// Fsyncs the distinct parent directories of `files` (best-effort), making the
/// deletes/copies performed against them durable.
fn sync_component_file_dirs<'a>(files: impl IntoIterator<Item = &'a ComponentFile>) {
    let mut synced: HashSet<PathBuf> = HashSet::new();
    for file in files {
        let path = real_path(file);
        if let Some(parent) = path.parent()
            && synced.insert(parent.to_path_buf())
        {
            crate::fs::sync_directory_best_effort(parent);
        }
    }
}

fn real_path(file: &ComponentFile) -> PathBuf {
    PathBuf::from(file.path().as_str())
}

fn require_baseline_hash(file: &ComponentFile) -> AppResult<Sha256Hash> {
    file.sha256().cloned().ok_or_else(|| {
        AppError::provider_failed(format!(
            "baseline {} has no integrity hash",
            file.path().as_str()
        ))
    })
}

fn map_service_error(error: crate::ServiceError) -> AppError {
    match error {
        crate::ServiceError::InvalidInput(message) => AppError::invalid_input(message),
        crate::ServiceError::ProviderFailed(message)
        | crate::ServiceError::CommandFailed(message) => AppError::provider_failed(message),
        other => AppError::provider_failed(other.to_string()),
    }
}
