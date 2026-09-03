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

use renderpilot_application::{
    AppError, AppResult, ArchiveMode, ResolvedPathDisposition, ResolvedTransition,
};
use renderpilot_domain::{ComponentFile, ComponentRollbackBaseline, LibraryComponent, Sha256Hash};

use crate::coordinated_files::{
    CoordinatedFilePlan, FilePlanBatchLog, execute_file_plans, execute_restore_batch,
};

use super::types::{AppliedFsLog, TransitionWrite};

/// Executes the exhaustive typed transition partition.  No filename, target
/// directory, or removal-set inference is repeated below this boundary.
pub(super) fn perform_transition_apply_fs(
    transition: &ResolvedTransition,
    rollback_baseline: Option<&ComponentRollbackBaseline>,
) -> AppResult<AppliedFsLog> {
    let plans = plan_resolved_transition(transition, rollback_baseline)?;
    let log = execute_file_plans(&plans).map_err(map_service_error)?;
    let changes = AppliedFsLog {
        created_sidecars: log.created_sidecars,
        copied: log.copied,
    };
    sync_touched_directories(&changes);
    validate_transition_reservations(transition, rollback_baseline)?;
    Ok(changes)
}

/// Verifies the state the typed transition just published before callers can
/// persist it as the next active component.  In particular, an archived Xiph
/// original must never be silently recreated between its archive mutation and
/// the database commit.
pub(super) fn validate_transition_reservations(
    transition: &ResolvedTransition,
    rollback_baseline: Option<&ComponentRollbackBaseline>,
) -> AppResult<()> {
    for disposition in transition.paths() {
        match disposition {
            ResolvedPathDisposition::ArchiveAndRemove(archive) => {
                let target = std::path::Path::new(archive.target().as_str());
                require_absent_transition_path(target, "reserved archived original")?;
                let expected = require_baseline_hash(archive.baseline())?;
                let sidecar = crate::fs::backup_path(target)
                    .map_err(|error| AppError::invalid_input(error.to_string()))?;
                crate::fs::verify_sidecar(&sidecar, &expected).map_err(|error| {
                    AppError::invalid_input(format!(
                        "reserved archived original has no verified sidecar at {}: {error}",
                        sidecar.display()
                    ))
                })?;
                if matches!(archive.mode(), ArchiveMode::RequireOwnedArchive) {
                    require_persisted_archive_ownership(rollback_baseline, archive.baseline())?;
                }
            }
            ResolvedPathDisposition::Remove(remove) => {
                require_absent_transition_path(
                    std::path::Path::new(remove.target().as_str()),
                    "removed transition path",
                )?;
            }
            ResolvedPathDisposition::Write(_) | ResolvedPathDisposition::UntouchedBaseline(_) => {}
        }
    }
    Ok(())
}

fn require_absent_transition_path(path: &std::path::Path, label: &str) -> AppResult<()> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(AppError::invalid_input(format!(
            "{label} unexpectedly exists at {}",
            path.display()
        ))),
        Err(error) => Err(AppError::invalid_input(format!(
            "cannot inspect {label} {}: {error}",
            path.display()
        ))),
    }
}

fn plan_resolved_transition(
    transition: &ResolvedTransition,
    rollback_baseline: Option<&ComponentRollbackBaseline>,
) -> AppResult<Vec<CoordinatedFilePlan>> {
    let mut plans = Vec::new();

    // An untouched path is intentionally part of the next active component,
    // but its current bytes may be from a previous overlay. Restore every
    // immutable original before any new overlay copy so the typed transition's
    // `expected_active` projection and the filesystem stay identical.
    for disposition in transition.paths() {
        if let ResolvedPathDisposition::UntouchedBaseline(untouched) = disposition {
            plans.push(CoordinatedFilePlan::RestorePreservingSidecar {
                path: PathBuf::from(untouched.target().as_str()),
                baseline_sha256: require_baseline_hash(untouched.baseline())?,
            });
        }
    }

    for disposition in transition.paths() {
        match disposition {
            ResolvedPathDisposition::Write(write) => {
                let target = PathBuf::from(write.target().as_str());
                if let Some(baseline) = write.baseline() {
                    plans.push(CoordinatedFilePlan::EnsureBaselineSidecar {
                        path: target.clone(),
                        expected_live: require_baseline_hash(baseline)?,
                    });
                } else if write.current().is_none() {
                    // New canonical target: both the live name and classic
                    // sidecar namespace are reserved before any copy starts.
                    require_absent_target_and_sidecar(&target)?;
                }
                plans.push(CoordinatedFilePlan::OverlayFromPath {
                    path: target,
                    source: PathBuf::from(write.source().path().as_str()),
                });
            }
            ResolvedPathDisposition::ArchiveAndRemove(archive) => {
                let target = PathBuf::from(archive.target().as_str());
                let expected = require_baseline_hash(archive.baseline())?;
                match archive.mode() {
                    ArchiveMode::Create => {
                        plans.push(CoordinatedFilePlan::CreateVerifiedArchiveAndRemove {
                            path: target,
                            expected_live: expected,
                        });
                    }
                    ArchiveMode::RequireOwnedArchive => {
                        require_persisted_archive_ownership(rollback_baseline, archive.baseline())?;
                        plans.push(CoordinatedFilePlan::RequireOwnedArchive {
                            path: target,
                            expected_baseline: expected,
                        });
                    }
                }
            }
            ResolvedPathDisposition::Remove(remove) => {
                plans.push(CoordinatedFilePlan::RemoveLive {
                    path: PathBuf::from(remove.target().as_str()),
                });
            }
            // Restores are deliberately emitted in the pre-overlay phase.
            ResolvedPathDisposition::UntouchedBaseline(_) => {}
        }
    }
    Ok(plans)
}

fn require_absent_target_and_sidecar(path: &std::path::Path) -> AppResult<()> {
    let sidecar =
        crate::fs::backup_path(path).map_err(|error| AppError::invalid_input(error.to_string()))?;
    for (candidate, label) in [
        (path, "new transition target"),
        (&sidecar, "target sidecar"),
    ] {
        match std::fs::symlink_metadata(candidate) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(AppError::provider_failed(format!(
                    "refusing to overwrite existing {label} {}",
                    candidate.display()
                )));
            }
            Err(error) => {
                return Err(AppError::provider_failed(format!(
                    "cannot inspect {label} {}: {error}",
                    candidate.display()
                )));
            }
        }
    }
    Ok(())
}

fn require_persisted_archive_ownership(
    rollback_baseline: Option<&ComponentRollbackBaseline>,
    reserved: &ComponentFile,
) -> AppResult<()> {
    let baseline = rollback_baseline.ok_or_else(|| {
        AppError::invalid_input("reserved transition path has no persisted rollback baseline")
    })?;
    let expected = baseline.expected_active_files();
    if expected.is_empty() {
        return Err(AppError::invalid_input(
            "reserved transition path has no persisted expected active projection",
        ));
    }
    let reserved_path = std::path::Path::new(reserved.path().as_str());
    if expected.iter().any(|file| {
        crate::paths::same_path(std::path::Path::new(file.path().as_str()), reserved_path)
    }) {
        return Err(AppError::invalid_input(format!(
            "reserved transition path is still active in the persisted baseline: {}",
            reserved_path.display()
        )));
    }
    let recorded = baseline.files().iter().find(|file| {
        crate::paths::same_path(std::path::Path::new(file.path().as_str()), reserved_path)
    });
    let Some(recorded) = recorded else {
        return Err(AppError::invalid_input(format!(
            "reserved transition path is absent from the persisted baseline: {}",
            reserved_path.display()
        )));
    };
    if recorded.sha256() != reserved.sha256() {
        return Err(AppError::invalid_input(format!(
            "reserved transition baseline hash changed for {}",
            reserved_path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn perform_apply_fs(
    component: &LibraryComponent,
    baseline: &[ComponentFile],
    planned: &[TransitionWrite],
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
#[allow(dead_code)]
fn plan_converge_active_set(
    component: &LibraryComponent,
    baseline: &[ComponentFile],
    planned: &[TransitionWrite],
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use renderpilot_application::{
        ExternalAliasRequirements, ResolvedPathDisposition, resolve_transition,
    };
    use renderpilot_domain::{
        Architecture, ArtifactId, ArtifactMetadata, ArtifactTrustLevel, ComponentFile, ComponentId,
        ComponentKind, ComponentRollbackBaseline, GameId, LibraryArtifact, LibraryComponent,
        LibraryTechnology, PathRef, PeCompatibilityProfile, PeExportSet, PeImportProfile,
        PeImportSet, RuntimeTarget, Sha256Hash, Swappability, xiph::XiphMember,
    };

    use super::{
        perform_transition_apply_fs, require_persisted_archive_ownership,
        validate_transition_reservations,
    };

    fn file(path: &str, hash: char) -> ComponentFile {
        ComponentFile::new(PathRef::new(path).expect("path"))
            .with_sha256(Sha256Hash::new(hash.to_string().repeat(64)).expect("hash"))
    }

    fn xiph_file(name: &str, imports: &[&str], hash: char, root: &str) -> ComponentFile {
        let member = renderpilot_domain::xiph::parse_runtime_file_name(name)
            .expect("runtime name")
            .expect("Xiph name")
            .member();
        let export = match member {
            XiphMember::VorbisFile => "ov_open",
            XiphMember::VorbisEnc => "vorbis_encode_init",
            XiphMember::Vorbis => "vorbis_info_init",
            XiphMember::Ogg => "ogg_sync_init",
        };
        ComponentFile::new(PathRef::new(format!("{root}/{name}")).expect("path"))
            .with_sha256(Sha256Hash::new(hash.to_string().repeat(64)).expect("hash"))
            .with_pe_compatibility(
                PeCompatibilityProfile::new(
                    Architecture::X64,
                    PeExportSet::from_observed_names(vec![export.to_owned()]).expect("exports"),
                )
                .with_imports(PeImportProfile {
                    regular: PeImportSet::from_observed_names(
                        imports.iter().map(|name| (*name).to_owned()).collect(),
                    )
                    .expect("imports"),
                    delay: PeImportSet::default(),
                }),
            )
    }

    fn observed_xiph_file(path: &Path, imports: &[&str]) -> ComponentFile {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("Xiph file name");
        let member = renderpilot_domain::xiph::parse_runtime_file_name(name)
            .expect("runtime name")
            .expect("Xiph name")
            .member();
        let export = match member {
            XiphMember::VorbisFile => "ov_open",
            XiphMember::VorbisEnc => "vorbis_encode_init",
            XiphMember::Vorbis => "vorbis_info_init",
            XiphMember::Ogg => "ogg_sync_init",
        };
        ComponentFile::new(PathRef::new(path.to_string_lossy().into_owned()).expect("path"))
            .with_sha256(renderpilot_detection::sha256_file(path).expect("hash"))
            .with_pe_compatibility(
                PeCompatibilityProfile::new(
                    Architecture::X64,
                    PeExportSet::from_observed_names(vec![export.to_owned()]).expect("exports"),
                )
                .with_imports(PeImportProfile {
                    regular: PeImportSet::from_observed_names(
                        imports.iter().map(|name| (*name).to_owned()).collect(),
                    )
                    .expect("imports"),
                    delay: PeImportSet::default(),
                }),
            )
    }

    fn observed_file(path: &Path) -> ComponentFile {
        observed_file_with_hash_at(path, path)
    }

    fn observed_file_with_hash_at(path: &Path, hash_source: &Path) -> ComponentFile {
        ComponentFile::new(PathRef::new(path.to_string_lossy().into_owned()).expect("path"))
            .with_sha256(renderpilot_detection::sha256_file(hash_source).expect("hash"))
    }

    #[test]
    fn dide_partition_has_three_writes_and_two_archived_originals() {
        let component = [
            xiph_file(
                "vorbisfile_vs2010_x64_rwdi.dll",
                &["vorbis_vs2010_x64_rwdi.dll", "ogg_vs2010_x64_rwdi.dll"],
                '1',
                "C:/Game",
            ),
            xiph_file(
                "vorbis_vs2010_x64_rwdi.dll",
                &["ogg_vs2010_x64_rwdi.dll"],
                '2',
                "C:/Game",
            ),
            xiph_file("ogg_vs2010_x64_rwdi.dll", &[], '3', "C:/Game"),
        ]
        .into_iter()
        .fold(
            LibraryComponent::new(
                ComponentId::new("component:dide-fs").expect("component"),
                GameId::new("game:dide-fs").expect("game"),
                ComponentKind::NativeLibrary,
                LibraryTechnology::XiphVorbis,
                Swappability::BundleOnly,
            ),
            LibraryComponent::with_file,
        );
        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:dide-fs").expect("artifact"),
            LibraryTechnology::XiphVorbis,
            "vorbisfile.dll",
            vec![
                xiph_file(
                    "vorbisfile.dll",
                    &["vorbis.dll", "ogg.dll"],
                    'a',
                    "C:/Library",
                ),
                xiph_file("vorbis.dll", &["ogg.dll"], 'b', "C:/Library"),
                xiph_file("ogg.dll", &[], 'c', "C:/Library"),
            ],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
        .with_metadata(
            ArtifactMetadata::default().with_runtime_target(RuntimeTarget::new(Architecture::X64)),
        );
        let transition = resolve_transition(
            &component,
            &artifact,
            component.files(),
            &ExternalAliasRequirements::Proven(BTreeSet::from([
                "vorbisfile_vs2010_x64_rwdi.dll".to_owned()
            ])),
        )
        .expect("resolved transition");
        assert_eq!(
            transition
                .paths()
                .iter()
                .filter(|path| matches!(path, ResolvedPathDisposition::Write(_)))
                .count(),
            3
        );
        assert_eq!(
            transition
                .paths()
                .iter()
                .filter(|path| matches!(path, ResolvedPathDisposition::ArchiveAndRemove(_)))
                .count(),
            2
        );
        assert_eq!(
            transition
                .paths()
                .iter()
                .filter(|path| !matches!(path, ResolvedPathDisposition::UntouchedBaseline(_)))
                .count(),
            5
        );
    }

    #[test]
    fn late_reservation_recheck_rejects_recreated_archived_original() {
        let root = tempfile::tempdir().expect("root");
        let game_dir = root.path().join("game");
        let library_dir = root.path().join("library");
        std::fs::create_dir_all(&game_dir).expect("game directory");
        std::fs::create_dir_all(&library_dir).expect("library directory");

        let old_wrapper = game_dir.join("vorbisfile_vs2010_x64_rwdi.dll");
        let old_vorbis = game_dir.join("vorbis_vs2010_x64_rwdi.dll");
        let old_ogg = game_dir.join("ogg_vs2010_x64_rwdi.dll");
        std::fs::write(&old_wrapper, b"old-wrapper").expect("old wrapper");
        std::fs::write(&old_vorbis, b"old-vorbis").expect("old vorbis");
        std::fs::write(&old_ogg, b"old-ogg").expect("old ogg");

        let component = [
            observed_xiph_file(
                &old_wrapper,
                &["vorbis_vs2010_x64_rwdi.dll", "ogg_vs2010_x64_rwdi.dll"],
            ),
            observed_xiph_file(&old_vorbis, &["ogg_vs2010_x64_rwdi.dll"]),
            observed_xiph_file(&old_ogg, &[]),
        ]
        .into_iter()
        .fold(
            LibraryComponent::new(
                ComponentId::new("component:dide-late-reservation").expect("component"),
                GameId::new("game:dide-late-reservation").expect("game"),
                ComponentKind::NativeLibrary,
                LibraryTechnology::XiphVorbis,
                Swappability::BundleOnly,
            ),
            LibraryComponent::with_file,
        );

        let new_wrapper = library_dir.join("vorbisfile.dll");
        let new_vorbis = library_dir.join("vorbis.dll");
        let new_ogg = library_dir.join("ogg.dll");
        std::fs::write(&new_wrapper, b"new-wrapper").expect("new wrapper");
        std::fs::write(&new_vorbis, b"new-vorbis").expect("new vorbis");
        std::fs::write(&new_ogg, b"new-ogg").expect("new ogg");
        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:dide-late-reservation").expect("artifact"),
            LibraryTechnology::XiphVorbis,
            "vorbisfile.dll",
            vec![
                observed_xiph_file(&new_wrapper, &["vorbis.dll", "ogg.dll"]),
                observed_xiph_file(&new_vorbis, &["ogg.dll"]),
                observed_xiph_file(&new_ogg, &[]),
            ],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
        .with_metadata(
            ArtifactMetadata::default().with_runtime_target(RuntimeTarget::new(Architecture::X64)),
        );
        let transition = resolve_transition(
            &component,
            &artifact,
            component.files(),
            &ExternalAliasRequirements::Proven(BTreeSet::from([
                "vorbisfile_vs2010_x64_rwdi.dll".to_owned()
            ])),
        )
        .expect("transition");

        perform_transition_apply_fs(&transition, None).expect("initial transition");
        std::fs::write(&old_vorbis, b"recreated after first validation")
            .expect("recreate reserved original");

        assert!(
            validate_transition_reservations(&transition, None).is_err(),
            "the late commit-boundary check must reject a recreated archived original"
        );
    }

    #[test]
    fn require_owned_archive_needs_nonempty_projection_which_omits_reserved_path() {
        let reserved = file("C:/Game/vorbis_vs2010_x64_rwdi.dll", 'a');
        let empty = ComponentRollbackBaseline::new(vec![reserved.clone()]);
        assert!(require_persisted_archive_ownership(Some(&empty), &reserved).is_err());

        let active_same = ComponentRollbackBaseline::new(vec![reserved.clone()])
            .with_expected_active_files(vec![reserved.clone()]);
        assert!(require_persisted_archive_ownership(Some(&active_same), &reserved).is_err());

        let owned = ComponentRollbackBaseline::new(vec![reserved.clone()])
            .with_expected_active_files(vec![file("C:/Game/vorbisfile.dll", 'b')]);
        assert!(require_persisted_archive_ownership(Some(&owned), &reserved).is_ok());
        assert!(require_persisted_archive_ownership(None, &reserved).is_err());
    }

    #[test]
    fn create_archive_refuses_to_adopt_an_existing_sidecar() {
        let directory = tempfile::tempdir().expect("directory");
        let live = directory.path().join("vorbis.dll");
        std::fs::write(&live, b"immutable original").expect("live");
        let sidecar = crate::fs::backup_path(&live).expect("sidecar");
        std::fs::write(&sidecar, b"immutable original").expect("sidecar");
        let expected = renderpilot_detection::sha256_file(&live).expect("hash");
        let plan = crate::coordinated_files::CoordinatedFilePlan::CreateVerifiedArchiveAndRemove {
            path: live.clone(),
            expected_live: expected,
        };
        assert!(crate::coordinated_files::execute_file_plan(&plan).is_err());
        assert!(
            Path::new(&live).exists(),
            "live original must remain intact"
        );
    }

    #[test]
    fn fsr_reswap_restores_untouched_baseline_before_split_overlay() {
        let root = tempfile::tempdir().expect("root");
        let game_dir = root.path().join("game");
        let library_dir = root.path().join("library");
        std::fs::create_dir_all(&game_dir).expect("game directory");
        std::fs::create_dir_all(&library_dir).expect("library directory");

        let entry = game_dir.join("amd_fidelityfx_dx12.dll");
        let split = game_dir.join("amd_fidelityfx_upscaler_dx12.dll");
        let entry_sidecar = crate::fs::backup_path(&entry).expect("entry sidecar");
        let split_sidecar = crate::fs::backup_path(&split).expect("split sidecar");
        let source = library_dir.join("amd_fidelityfx_upscaler_dx12.dll");
        std::fs::write(&entry, b"previous-unified-overlay").expect("current entry");
        std::fs::write(&entry_sidecar, b"immutable-baseline-entry").expect("entry baseline");
        std::fs::write(&split_sidecar, b"immutable-baseline-split").expect("split baseline");
        std::fs::write(&source, b"next-split-overlay").expect("split artifact");

        let baseline = vec![
            observed_file_with_hash_at(&entry, &entry_sidecar),
            observed_file_with_hash_at(&split, &split_sidecar),
        ];
        let component = LibraryComponent::new(
            ComponentId::new("component:fsr-reswap-fs").expect("component"),
            GameId::new("game:fsr-reswap-fs").expect("game"),
            ComponentKind::NativeLibrary,
            LibraryTechnology::AmdFsr,
            Swappability::BundleOnly,
        )
        .with_file(observed_file(&entry));
        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:fsr-split").expect("artifact"),
            LibraryTechnology::AmdFsr,
            "amd_fidelityfx_upscaler_dx12.dll",
            vec![observed_file(&source)],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact");

        let transition = resolve_transition(
            &component,
            &artifact,
            &baseline,
            &ExternalAliasRequirements::NotRequired,
        )
        .expect("FSR reswap transition");
        let expected_entry = transition
            .expected_active()
            .into_iter()
            .find(|file| file.path().file_name() == Some("amd_fidelityfx_dx12.dll"))
            .expect("restored entry in expected active set");
        assert_eq!(
            expected_entry.sha256(),
            baseline[0].sha256(),
            "the active projection must describe restored immutable bytes, not the old overlay"
        );

        perform_transition_apply_fs(&transition, None).expect("FSR reswap filesystem plan");

        assert_eq!(
            std::fs::read(&entry).expect("restored entry"),
            b"immutable-baseline-entry"
        );
        assert_eq!(
            std::fs::read(&split).expect("split overlay"),
            b"next-split-overlay"
        );
        assert_eq!(
            std::fs::read(&entry_sidecar).expect("preserved entry sidecar"),
            b"immutable-baseline-entry"
        );
        assert_eq!(
            std::fs::read(&split_sidecar).expect("preserved split sidecar"),
            b"immutable-baseline-split"
        );
    }
}
