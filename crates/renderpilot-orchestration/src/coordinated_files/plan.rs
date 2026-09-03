//! Pure coordinated-file plans and their path-level executor.

use std::fs;
use std::path::{Path, PathBuf};

use renderpilot_domain::{ManagedFileBaseline, Sha256Hash};

/// Expected live state captured by a pure coordinated-file planner.
#[derive(Debug, Clone)]
pub(crate) enum ExpectedLive {
    Absent,
    Hashes(Vec<Sha256Hash>),
}

/// On-disk source path for a binding overlay write (catalog installs, staged
/// Luma DLSS). Catalog batch plans use [`CoordinatedFilePlan::OverlayFromPath`]
/// instead; both shapes stay path-only (no in-memory payload).
pub(crate) type OverlaySource = PathBuf;

/// A validated, add-on-neutral transition for one live/sidecar pair.
///
/// Variants group into two audiences:
/// - **Binding** (managed-file ownership): `Keep` … `RemoveAndRelease`
/// - **Catalog batch**: `EnsureBaselineSidecar` … `RemoveLive`
///
/// Catalog restore batches must run every `RestorePreservingSidecar` before any
/// `ReleaseSidecar` — use [`execute_restore_batch`] to enforce that order.
#[derive(Debug, Clone)]
pub(crate) enum CoordinatedFilePlan {
    // --- binding / managed-file ownership ---
    Keep,
    Reuse {
        path: PathBuf,
        sha256: Sha256Hash,
    },
    CreateBaselineAndOverlay {
        path: PathBuf,
        expected_live: Sha256Hash,
        source: OverlaySource,
    },
    OverlayPreservingBaseline {
        path: PathBuf,
        baseline: ManagedFileBaseline,
        expected_live: ExpectedLive,
        source: OverlaySource,
    },
    RestoreAndRelease {
        path: PathBuf,
        baseline_sha256: Sha256Hash,
        expected_live: Vec<Sha256Hash>,
    },
    RemoveAndRelease {
        path: PathBuf,
        expected_live: Vec<Sha256Hash>,
    },
    // --- catalog batch ---
    /// Ensure a classic baseline sidecar exists (create from live if absent).
    EnsureBaselineSidecar {
        path: PathBuf,
        expected_live: Sha256Hash,
    },
    /// Crash-atomic copy from an on-disk source onto `path` (no in-memory bytes).
    OverlayFromPath {
        path: PathBuf,
        source: PathBuf,
    },
    /// Create/verify sidecar for live, then delete the live file.
    #[allow(dead_code)]
    ArchiveLiveToSidecarAndRemove {
        path: PathBuf,
        expected_live: Sha256Hash,
    },
    /// Catalog transition archive: the immutable original must still be live
    /// and its sidecar must be absent.  Unlike the retry-oriented legacy
    /// variant, this never adopts an already-existing `.bak` as ownership.
    CreateVerifiedArchiveAndRemove {
        path: PathBuf,
        expected_live: Sha256Hash,
    },
    /// Catalog transition proof that a previous committed mutation owns an
    /// immutable sidecar.  It mutates nothing, but rejects a recreated live
    /// file or an arbitrary/mismatched sidecar before a later overlay.
    RequireOwnedArchive {
        path: PathBuf,
        expected_baseline: Sha256Hash,
    },
    /// Restore live from sidecar without removing the sidecar. When the sidecar
    /// is absent, live must already match `baseline_sha256` (retry-safe no-op).
    RestorePreservingSidecar {
        path: PathBuf,
        baseline_sha256: Sha256Hash,
    },
    /// Remove a verified baseline sidecar after a successful restore batch.
    ReleaseSidecar {
        path: PathBuf,
    },
    /// Delete a live path if present (overlay-added files during catalog revert).
    RemoveLive {
        path: PathBuf,
    },
}

/// Paths touched while executing one or more [`CoordinatedFilePlan`]s, for
/// best-effort parent-directory fsync (catalog apply).
#[derive(Debug, Default, Clone)]
pub(crate) struct FilePlanBatchLog {
    /// Classic sidecars created during the batch `(live, sidecar)`.
    pub(crate) created_sidecars: Vec<(PathBuf, PathBuf)>,
    /// Live paths written via path-source overlay.
    pub(crate) copied: Vec<PathBuf>,
}

/// Executes a plan after rechecking every path-level premise immediately
/// before its first mutation.
pub(crate) fn execute_file_plan(plan: &CoordinatedFilePlan) -> Result<(), crate::ServiceError> {
    let mut log = FilePlanBatchLog::default();
    execute_file_plan_into(plan, &mut log)
}

/// Executes every plan in order, accumulating sidecar/copy touch log for fsync.
///
/// Catalog restore batches must list all [`CoordinatedFilePlan::RestorePreservingSidecar`]
/// steps before any [`CoordinatedFilePlan::ReleaseSidecar`] so a mid-batch failure
/// leaves sidecars in place for a safe retry. Prefer [`execute_restore_batch`]
/// when assembling those two steps.
pub(crate) fn execute_file_plans(
    plans: &[CoordinatedFilePlan],
) -> Result<FilePlanBatchLog, crate::ServiceError> {
    let mut log = FilePlanBatchLog::default();
    for plan in plans {
        execute_file_plan_into(plan, &mut log)?;
    }
    Ok(log)
}

/// Runs restore-preserving steps, then release steps, enforcing the
/// restore-before-release contract for catalog rollback batches.
pub(crate) fn execute_restore_batch(
    restores: impl IntoIterator<Item = CoordinatedFilePlan>,
    releases: impl IntoIterator<Item = CoordinatedFilePlan>,
) -> Result<FilePlanBatchLog, crate::ServiceError> {
    let mut plans = Vec::new();
    for plan in restores {
        match plan {
            CoordinatedFilePlan::RestorePreservingSidecar { .. }
            | CoordinatedFilePlan::RemoveLive { .. } => plans.push(plan),
            other => {
                return Err(crate::failed(format!(
                    "restore batch received a non-restore plan: {other:?}"
                )));
            }
        }
    }
    for plan in releases {
        match plan {
            CoordinatedFilePlan::ReleaseSidecar { .. } => plans.push(plan),
            other => {
                return Err(crate::failed(format!(
                    "restore batch release phase received a non-release plan: {other:?}"
                )));
            }
        }
    }
    execute_file_plans(&plans)
}

fn execute_file_plan_into(
    plan: &CoordinatedFilePlan,
    log: &mut FilePlanBatchLog,
) -> Result<(), crate::ServiceError> {
    match plan {
        CoordinatedFilePlan::Keep => Ok(()),
        CoordinatedFilePlan::Reuse { path, sha256 } => {
            require_live_hash(path, std::slice::from_ref(sha256))
        }
        CoordinatedFilePlan::CreateBaselineAndOverlay {
            path,
            expected_live,
            source,
        } => {
            require_live_hash(path, std::slice::from_ref(expected_live))?;
            let sidecar = plan_sidecar(path)?;
            require_absent(&sidecar, "baseline sidecar")?;
            crate::fs::create_sidecar(path, &sidecar)?;
            let actual = renderpilot_detection::sha256_file(&sidecar)?;
            if &actual != expected_live {
                return Err(crate::failed(format!(
                    "new baseline {} does not match the accepted live bytes",
                    sidecar.display()
                )));
            }
            log.created_sidecars.push((path.clone(), sidecar));
            apply_overlay_source(path, source, log)
        }
        CoordinatedFilePlan::OverlayPreservingBaseline {
            path,
            baseline,
            expected_live,
            source,
        } => {
            let sidecar = plan_sidecar(path)?;
            match baseline {
                ManagedFileBaseline::Absent => require_absent(&sidecar, "baseline sidecar")?,
                ManagedFileBaseline::Present { sha256 } => {
                    crate::fs::verify_sidecar(&sidecar, sha256)
                        .map_err(map_sidecar_verify_error)?;
                }
            }
            match expected_live {
                ExpectedLive::Absent => require_absent(path, "live coordinated file")?,
                ExpectedLive::Hashes(hashes) => require_live_hash(path, hashes)?,
            }
            apply_overlay_source(path, source, log)
        }
        CoordinatedFilePlan::RestoreAndRelease {
            path,
            baseline_sha256,
            expected_live,
        } => {
            require_live_hash(path, expected_live)?;
            let sidecar = plan_sidecar(path)?;
            crate::fs::verify_sidecar(&sidecar, baseline_sha256)
                .map_err(map_sidecar_verify_error)?;
            crate::fs::restore_from_sidecar(path, &sidecar)?;
            Ok(())
        }
        CoordinatedFilePlan::RemoveAndRelease {
            path,
            expected_live,
        } => {
            require_live_hash(path, expected_live)?;
            let sidecar = plan_sidecar(path)?;
            require_absent(&sidecar, "baseline sidecar")?;
            crate::fs::remove_file_if_exists(path)
        }
        CoordinatedFilePlan::EnsureBaselineSidecar {
            path,
            expected_live,
        } => ensure_baseline_sidecar(path, expected_live, log),
        CoordinatedFilePlan::OverlayFromPath { path, source } => {
            crate::fs::copy_file_atomically(source, path).map_err(|error| {
                crate::failed(format!(
                    "failed to install file to {}: {error}",
                    path.display()
                ))
            })?;
            log.copied.push(path.clone());
            Ok(())
        }
        CoordinatedFilePlan::ArchiveLiveToSidecarAndRemove {
            path,
            expected_live,
        } => {
            ensure_baseline_sidecar(path, expected_live, log)?;
            crate::fs::remove_file_if_exists(path)
        }
        CoordinatedFilePlan::CreateVerifiedArchiveAndRemove {
            path,
            expected_live,
        } => create_verified_archive_and_remove(path, expected_live, log),
        CoordinatedFilePlan::RequireOwnedArchive {
            path,
            expected_baseline,
        } => require_owned_archive(path, expected_baseline),
        CoordinatedFilePlan::RestorePreservingSidecar {
            path,
            baseline_sha256,
        } => restore_preserving_sidecar(path, baseline_sha256),
        CoordinatedFilePlan::ReleaseSidecar { path } => {
            let sidecar = plan_sidecar(path)?;
            crate::fs::remove_file_if_exists(&sidecar)
        }
        CoordinatedFilePlan::RemoveLive { path } => {
            crate::fs::remove_file_if_exists(path).map_err(|error| {
                crate::failed(format!(
                    "failed to remove added file {}: {error}",
                    path.display()
                ))
            })
        }
    }
}

fn ensure_baseline_sidecar(
    path: &Path,
    expected_live: &Sha256Hash,
    log: &mut FilePlanBatchLog,
) -> Result<(), crate::ServiceError> {
    let sidecar = plan_sidecar(path)?;
    if sidecar.exists() {
        crate::fs::verify_sidecar(&sidecar, expected_live).map_err(map_sidecar_verify_error)?;
        return Ok(());
    }
    if !path.exists() {
        return Err(crate::failed(format!(
            "cannot create baseline sidecar because {} is missing",
            path.display()
        )));
    }
    let actual = renderpilot_detection::sha256_file(path)?;
    if &actual != expected_live {
        return Err(crate::failed(format!(
            "live file no longer matches the baseline for {}: expected {expected_live}, got {actual}",
            path.display()
        )));
    }
    crate::fs::create_sidecar(path, &sidecar)?;
    log.created_sidecars.push((path.to_path_buf(), sidecar));
    Ok(())
}

fn create_verified_archive_and_remove(
    path: &Path,
    expected_live: &Sha256Hash,
    log: &mut FilePlanBatchLog,
) -> Result<(), crate::ServiceError> {
    require_live_hash(path, std::slice::from_ref(expected_live))?;
    let sidecar = plan_sidecar(path)?;
    require_absent(&sidecar, "baseline sidecar")?;
    crate::fs::create_sidecar(path, &sidecar)?;
    crate::fs::verify_sidecar(&sidecar, expected_live).map_err(map_sidecar_verify_error)?;
    log.created_sidecars.push((path.to_path_buf(), sidecar));
    crate::fs::remove_file_if_exists(path)
}

fn require_owned_archive(
    path: &Path,
    expected_baseline: &Sha256Hash,
) -> Result<(), crate::ServiceError> {
    require_absent(path, "reserved live original")?;
    let sidecar = plan_sidecar(path)?;
    crate::fs::verify_sidecar(&sidecar, expected_baseline).map_err(map_sidecar_verify_error)
}

fn restore_preserving_sidecar(
    path: &Path,
    baseline_sha256: &Sha256Hash,
) -> Result<(), crate::ServiceError> {
    let sidecar = plan_sidecar(path)?;
    if sidecar.exists() {
        crate::fs::verify_sidecar(&sidecar, baseline_sha256).map_err(map_sidecar_verify_error)?;
        crate::fs::copy_file_atomically(&sidecar, path)?;
        return Ok(());
    }
    let actual = renderpilot_detection::sha256_file(path)?;
    if &actual == baseline_sha256 {
        Ok(())
    } else {
        Err(crate::failed(format!(
            "cannot restore baseline for {} without its sidecar",
            path.display()
        )))
    }
}

fn apply_overlay_source(
    path: &Path,
    source: &OverlaySource,
    log: &mut FilePlanBatchLog,
) -> Result<(), crate::ServiceError> {
    crate::fs::copy_file_atomically(source, path).map_err(|error| {
        crate::failed(format!(
            "failed to install file to {}: {error}",
            path.display()
        ))
    })?;
    log.copied.push(path.to_path_buf());
    Ok(())
}

fn plan_sidecar(path: &Path) -> Result<PathBuf, crate::ServiceError> {
    crate::fs::backup_path(path).map_err(|error| crate::failed(error.to_string()))
}

fn require_absent(path: &Path, label: &str) -> Result<(), crate::ServiceError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(crate::failed(format!(
            "{label} unexpectedly exists at {}",
            path.display()
        ))),
        Err(error) => Err(crate::failed(format!(
            "cannot inspect {label} {}: {error}",
            path.display()
        ))),
    }
}

fn require_live_hash(path: &Path, expected: &[Sha256Hash]) -> Result<(), crate::ServiceError> {
    if expected.is_empty() {
        return Err(crate::failed(format!(
            "no accepted live hash was provided for {}",
            path.display()
        )));
    }
    let actual = renderpilot_detection::sha256_file(path)?;
    if expected.contains(&actual) {
        Ok(())
    } else {
        Err(crate::failed(format!(
            "coordinated live file changed unexpectedly at {}",
            path.display()
        )))
    }
}

fn map_sidecar_verify_error(error: crate::fs::SidecarVerifyError) -> crate::ServiceError {
    match error {
        crate::fs::SidecarVerifyError::HashMismatch { path, .. } => crate::failed(format!(
            "coordinated baseline hash mismatch at {}",
            path.display()
        )),
        crate::fs::SidecarVerifyError::Unreadable { path, detail } => crate::failed(format!(
            "cannot read coordinated baseline {}: {detail}",
            path.display()
        )),
        crate::fs::SidecarVerifyError::NotAFile(path) => crate::failed(format!(
            "coordinated baseline {} is not a regular file",
            path.display()
        )),
        crate::fs::SidecarVerifyError::Empty(path) => {
            crate::failed(format!("coordinated baseline {} is empty", path.display()))
        }
    }
}
