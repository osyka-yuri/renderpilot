//! Manual-folder and auto-scan orchestration for the game catalog.
//!
//! ## Modules
//!
//! - `detect` -- complete stable-object library detection
//! - `reconcile` -- catalog identity merge for stable game ids
//! - `persist` -- write one explicit installation scan unit
//! - existing: `discovery`, `paths`, `prune`, `recovery`, `auto`
//!
//! ## Dependency rules
//!
//! ```text
//! mod (scan_impl) -> detect, persist
//! detect          -> detection
//! persist         -> reconcile, recovery
//! ```

mod detect;
mod persist;
mod prune;
mod reconcile;
mod recovery;
mod xiph_admission;
mod xiph_lineage;
#[cfg(test)]
mod xiph_test_support;

#[cfg(windows)]
mod auto;
#[cfg(windows)]
/// Auto-discovery logic.
pub mod discovery;

#[cfg(windows)]
pub(crate) use auto::scan_auto_in_shared_batch;
#[cfg(windows)]
pub(crate) use prune::prune_auto_scan_orphans;
#[cfg(windows)]
pub(crate) use reconcile::CatalogInstallIndex;

use std::path::PathBuf;

use renderpilot_application::{AppError, GameRepository, OperationRepository};
use renderpilot_detection::LibraryPatternComponentDetector;
use renderpilot_domain::{GameId, LibraryComponent, RootAuthority};
use renderpilot_platform_windows::ManualFolderGameSource;
use renderpilot_storage_sqlite::{AuthorityCas, CatalogReadiness};

use crate::ServiceError;

use super::ScanFolderCatalogResult;

use self::detect::detect_libraries;
use self::persist::persist_scan_result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExplicitRootChange {
    Unchanged,
    Expanded,
    Narrowed,
}

/// Scans one installation whose identity and root authority were resolved by
/// the add-game use case.
pub(crate) fn scan_explicit_install(
    context: &crate::Context,
    path: PathBuf,
    game_id: GameId,
    root_authority: RootAuthority,
    explicit_executable: Option<PathBuf>,
    root_change: ExplicitRootChange,
    consolidation_candidates: &[GameId],
) -> Result<ScanFolderCatalogResult, ServiceError> {
    let detector = LibraryPatternComponentDetector::windows_default()
        .map_err(|error| AppError::detection_failed(error.to_string()))?;
    let source = ManualFolderGameSource::new(path)
        .with_game_id(game_id)
        .with_root_authority(root_authority);
    let source = match explicit_executable {
        Some(executable) => source.with_explicit_executable(executable),
        None => source,
    };

    scan_source_impl(
        ScanInputs {
            context,
            detector: &detector,
        },
        &source,
        None,
        root_change,
        consolidation_candidates,
    )
}

/// Scans an installation under the game lock without running file/shared
/// recovery or legacy managed-file reconciliation.
#[cfg(windows)]
pub(crate) fn scan_explicit_install_observational(
    context: &crate::Context,
    game: &renderpilot_domain::GameInstallation,
) -> Result<Option<ScanFolderCatalogResult>, ServiceError> {
    let guard = crate::mutation_boundary::enter_game_observation_boundary(game.id());
    let current_game = context
        .storage()
        .find_game(game.id())?
        .ok_or_else(|| AppError::game_not_found(game.id().as_str()))?;
    if current_game.install_key() != game.install_key() {
        return Err(ServiceError::command_failed(
            "installation root changed before registered-root refresh",
        ));
    }
    let reactivating = match super::installation_lifecycle::coordinator::
        observe_registered_root_under_lock(context, &guard, &current_game)?
    {
        super::installation_lifecycle::coordinator::RegisteredRootObservation::Present {
            was_absent,
        } => was_absent,
        super::installation_lifecycle::coordinator::RegisteredRootObservation::ConfirmedAbsent {
            collection_error,
            ..
        } => {
            if let Some(error) = collection_error {
                return Err(ServiceError::command_failed(format!(
                    "installation root is absent; local metadata collection failed: {error}"
                )));
            }
            return Ok(None);
        }
        super::installation_lifecycle::coordinator::RegisteredRootObservation::Indeterminate {
            reason,
        } => {
            return Err(ServiceError::command_failed(format!(
                "installation root presence is indeterminate: {reason}"
            )));
        }
    };

    let detector = LibraryPatternComponentDetector::windows_default()
        .map_err(|error| AppError::detection_failed(error.to_string()))?;
    let explicit_executable = if reactivating {
        None
    } else {
        current_game.confirmed_executable().map(|relative| {
            PathBuf::from(current_game.install_path().as_str()).join(relative.as_str())
        })
    };
    let source = ManualFolderGameSource::new(current_game.install_path().as_str())
        .with_game_id(current_game.id().clone())
        .with_root_authority(current_game.root_authority());
    let source = match explicit_executable {
        Some(executable) => source.with_explicit_executable(executable),
        None => source,
    };
    let storage = context.storage();
    let catalog_index = reconcile::CatalogInstallIndex::load(storage)?;
    let discovered = source.discover_game()?;
    let selected_game = if reactivating {
        reconcile::reconcile_reappeared_game_with_catalog(&catalog_index, discovered)
    } else {
        reconcile::reconcile_game_with_catalog(&catalog_index, discovered)
    };
    let result = scan_source_impl_locked(
        ScanInputs {
            context,
            detector: &detector,
        },
        selected_game,
        &catalog_index,
        ExplicitRootChange::Unchanged,
        &[],
        Some(&guard),
        reactivating,
    )?;
    Ok(Some(result))
}

/// Borrowed storage + detector for one [`scan_impl`] invocation.
#[derive(Clone, Copy)]
struct ScanInputs<'a> {
    context: &'a crate::Context,
    detector: &'a LibraryPatternComponentDetector,
}

/// Scans one installation whose mutation boundary is already exclusively owned
/// by the caller.
pub(super) fn scan_explicit_install_locked(
    context: &crate::Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    path: PathBuf,
    game_id: GameId,
    root_authority: RootAuthority,
    explicit_executable: Option<PathBuf>,
) -> Result<ScanFolderCatalogResult, ServiceError> {
    if guard.game_id() != &game_id {
        return Err(ServiceError::invalid_input(
            "observation guard does not match the requested installation scan",
        ));
    }
    let registered_game = context.storage().find_game(&game_id)?;
    let reactivating_from_absence = if let Some(registered_game) = &registered_game {
        let requested_key = super::install_paths::install_path_match_key(
            &path.to_string_lossy().replace('\\', "/"),
        );
        if requested_key.as_ref() != Some(registered_game.install_key()) {
            return Err(ServiceError::command_failed(
                "installation root changed before registered-root refresh",
            ));
        }
        require_present_observation(
            super::installation_lifecycle::coordinator::observe_registered_root_under_lock(
                context,
                guard,
                registered_game,
            )?,
            registered_game.install_path().as_str(),
        )?
    } else {
        false
    };

    let detector = LibraryPatternComponentDetector::windows_default()
        .map_err(|error| AppError::detection_failed(error.to_string()))?;
    let source = ManualFolderGameSource::new(path)
        .with_game_id(game_id)
        .with_root_authority(root_authority);
    let source = match if reactivating_from_absence {
        None
    } else {
        explicit_executable
    } {
        Some(executable) => source.with_explicit_executable(executable),
        None => source,
    };

    let storage = context.storage();
    let catalog_index = reconcile::CatalogInstallIndex::load(storage)?;
    let selected_game =
        reconcile::reconcile_game_with_catalog(&catalog_index, source.discover_game()?);

    scan_source_impl_locked(
        ScanInputs {
            context,
            detector: &detector,
        },
        selected_game,
        &catalog_index,
        ExplicitRootChange::Unchanged,
        &[],
        Some(guard),
        reactivating_from_absence,
    )
}

#[derive(Clone, Copy)]
enum ScanBoundaryMode {
    Mutation,
    Observation,
}

fn scan_source_impl(
    inputs: ScanInputs<'_>,
    source: &ManualFolderGameSource,
    catalog_index: Option<&reconcile::CatalogInstallIndex>,
    root_change: ExplicitRootChange,
    consolidation_candidates: &[GameId],
) -> Result<ScanFolderCatalogResult, ServiceError> {
    let boundary_mode = if root_change == ExplicitRootChange::Unchanged {
        ScanBoundaryMode::Observation
    } else {
        ScanBoundaryMode::Mutation
    };
    scan_source_impl_with_boundary(
        inputs,
        source,
        catalog_index,
        root_change,
        consolidation_candidates,
        boundary_mode,
    )
}

#[cfg(any(windows, test))]
fn scan_source_impl_observational(
    inputs: ScanInputs<'_>,
    source: &ManualFolderGameSource,
    catalog_index: Option<&reconcile::CatalogInstallIndex>,
    root_change: ExplicitRootChange,
    consolidation_candidates: &[GameId],
) -> Result<ScanFolderCatalogResult, ServiceError> {
    scan_source_impl_with_boundary(
        inputs,
        source,
        catalog_index,
        root_change,
        consolidation_candidates,
        ScanBoundaryMode::Observation,
    )
}

fn scan_source_impl_with_boundary(
    inputs: ScanInputs<'_>,
    source: &ManualFolderGameSource,
    catalog_index: Option<&reconcile::CatalogInstallIndex>,
    root_change: ExplicitRootChange,
    consolidation_candidates: &[GameId],
    boundary_mode: ScanBoundaryMode,
) -> Result<ScanFolderCatalogResult, ServiceError> {
    let storage = inputs.context.storage();

    let owned_catalog_index;
    let prefetched_catalog_index = if let Some(index) = catalog_index {
        index
    } else {
        owned_catalog_index = reconcile::CatalogInstallIndex::load(storage)?;
        &owned_catalog_index
    };
    let discovered = source.discover_game()?;
    let prefetched_selected_game =
        reconcile::reconcile_game_with_catalog(prefetched_catalog_index, discovered);

    match boundary_mode {
        ScanBoundaryMode::Mutation => {
            let mut affected_ids = consolidation_candidates.to_vec();
            affected_ids.push(prefetched_selected_game.id().clone());
            let _guards = crate::mutation_boundary::enter_game_mutation_boundaries(
                inputs.context,
                affected_ids,
            )?;
            scan_source_impl_locked(
                inputs,
                prefetched_selected_game,
                prefetched_catalog_index,
                root_change,
                consolidation_candidates,
                None,
                false,
            )
        }
        ScanBoundaryMode::Observation => {
            // Retain every affected game's lock, but keep ordinary scans on
            // the observation-only boundary so pending native/file work is
            // never recovered as a side effect of catalog discovery.
            let mut affected_ids = consolidation_candidates.to_vec();
            affected_ids.push(prefetched_selected_game.id().clone());
            affected_ids.sort();
            affected_ids.dedup();
            let guards = affected_ids
                .iter()
                .map(crate::mutation_boundary::enter_game_observation_boundary)
                .collect::<Vec<_>>();
            let guard = guards
                .iter()
                .find(|guard| guard.game_id() == prefetched_selected_game.id())
                .ok_or_else(|| {
                    ServiceError::invalid_input(
                        "observation lock set does not include the selected installation",
                    )
                })?;

            // Batch discovery can hand this scan a catalog snapshot taken
            // before it waited for the per-game lock. Re-read under the lock
            // so current root authority and executable confirmation win.
            let fresh_catalog_index = reconcile::CatalogInstallIndex::load(storage)?;
            let selected_game = reconcile::reconcile_game_with_catalog(
                &fresh_catalog_index,
                source.discover_game()?,
            );
            if selected_game.id() != prefetched_selected_game.id() {
                return Err(ServiceError::command_failed(
                    "installation identity changed while waiting for its observation lock",
                ));
            }

            let observation =
                super::installation_lifecycle::coordinator::observe_registered_root_under_lock(
                    inputs.context,
                    guard,
                    &selected_game,
                )?;
            let reactivating_from_absence =
                require_present_observation(observation, selected_game.install_path().as_str())?;
            let selected_game = if reactivating_from_absence {
                reconcile::reconcile_reappeared_game_with_catalog(
                    &fresh_catalog_index,
                    source.discover_game()?,
                )
            } else {
                selected_game
            };

            scan_source_impl_locked(
                inputs,
                selected_game,
                &fresh_catalog_index,
                root_change,
                consolidation_candidates,
                Some(guard),
                reactivating_from_absence,
            )
        }
    }
}

fn scan_source_impl_locked(
    inputs: ScanInputs<'_>,
    selected_game: renderpilot_domain::GameInstallation,
    catalog_index: &reconcile::CatalogInstallIndex,
    root_change: ExplicitRootChange,
    consolidation_candidates: &[GameId],
    observation_guard: Option<&crate::game_mutation_lock::GameMutationGuard>,
    mut reactivating_from_absence: bool,
) -> Result<ScanFolderCatalogResult, ServiceError> {
    let storage = inputs.context.storage();
    let detector = inputs.detector;

    if root_change != ExplicitRootChange::Unchanged {
        ensure_root_change_not_blocked_before_scan(inputs.context, &selected_game)?;
    }
    let initial_readiness = match storage.find_game(selected_game.id())? {
        Some(_) => storage.catalog_readiness(selected_game.id())?,
        None => CatalogReadiness::NeverCompleted { authority_epoch: 0 },
    };
    let authority = AuthorityCas::new(initial_readiness.authority_epoch());
    let libraries = detect_libraries(storage, detector, &selected_game)?;
    let components = reconcile::build_library_components(&selected_game, &libraries)?;
    let components = if reactivating_from_absence {
        components
    } else {
        xiph_lineage::reconcile_managed_xiph_successor_ids(storage, &selected_game, components)?
    };
    if root_change != ExplicitRootChange::Unchanged {
        ensure_root_change_preserves_state(inputs.context, &selected_game, &components)?;
    }
    let root_correction_recovery_bundle_path = if root_change == ExplicitRootChange::Narrowed {
        archive_pruned_operation_history(inputs.context, &selected_game, &components)?
    } else {
        None
    };

    if let Some(guard) = observation_guard {
        let observation =
            super::installation_lifecycle::coordinator::observe_registered_root_under_lock(
                inputs.context,
                guard,
                &selected_game,
            )?;
        reactivating_from_absence |=
            require_present_observation(observation, selected_game.install_path().as_str())?;
    }

    persist_scan_result(
        storage,
        persist::PersistScanRequest {
            game: selected_game,
            libraries,
            components: &components,
            initial_readiness,
            authority,
            prune_empty_operations: root_change == ExplicitRootChange::Narrowed,
            root_correction_recovery_bundle_path,
            prefetched_catalog_index: Some(catalog_index),
            consolidation_candidates,
            reactivating_from_absence,
        },
    )
}

fn require_present_observation(
    observation: super::installation_lifecycle::coordinator::RegisteredRootObservation,
    root: &str,
) -> Result<bool, ServiceError> {
    match observation {
        super::installation_lifecycle::coordinator::RegisteredRootObservation::Present {
            was_absent,
        } => Ok(was_absent),
        super::installation_lifecycle::coordinator::RegisteredRootObservation::ConfirmedAbsent {
            collection_error,
            ..
        } => {
            if let Some(error) = collection_error {
                return Err(ServiceError::command_failed(format!(
                    "installation root is confirmed absent at {root}; local metadata collection failed: {error}"
                )));
            }
            Err(ServiceError::command_failed(format!(
                "installation root is confirmed absent at {root}"
            )))
        }
        super::installation_lifecycle::coordinator::RegisteredRootObservation::Indeterminate {
            reason,
        } => Err(ServiceError::command_failed(format!(
            "installation root presence is indeterminate at {root}: {reason}"
        ))),
    }
}

fn archive_pruned_operation_history(
    context: &crate::Context,
    game: &renderpilot_domain::GameInstallation,
    prospective_components: &[LibraryComponent],
) -> Result<Option<String>, ServiceError> {
    let prospective_ids = prospective_components
        .iter()
        .map(|component| component.id().as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut archived_component_ids = context
        .storage()
        .list_operation_entries_for_game(game.id())?
        .into_iter()
        .flat_map(|entry| entry.into_parts().1)
        .filter(|item| !prospective_ids.contains(item.component_id.as_str()))
        .map(|item| item.component_id.as_str().to_owned())
        .collect::<Vec<_>>();
    archived_component_ids.sort();
    archived_component_ids.dedup();
    if archived_component_ids.is_empty() {
        return Ok(None);
    }

    let previous = context.storage().require_game(game.id())?;
    let bundle = super::recovery_bundle::create_root_correction_recovery_bundle(
        context.storage(),
        game.id().as_str(),
        previous.install_path().as_str(),
        game.install_path().as_str(),
        &archived_component_ids,
    )?;
    Ok(Some(bundle.to_string_lossy().to_string()))
}

fn ensure_root_change_preserves_state(
    context: &crate::Context,
    game: &renderpilot_domain::GameInstallation,
    prospective_components: &[LibraryComponent],
) -> Result<(), ServiceError> {
    let assessment = assess_root_change(context, game, Some(prospective_components))?;
    match assessment.status {
        super::RootCorrectionStatus::Ready => Ok(()),
        super::RootCorrectionStatus::CleanupRequired => {
            Err(ServiceError::RootCorrectionCleanupRequired {
                game_id: assessment.game_id,
                component_ids: assessment
                    .cleanup_actions
                    .into_iter()
                    .map(|action| match action {
                        super::RootCorrectionCleanupAction::RollbackComponent { component_id } => {
                            component_id
                        }
                    })
                    .collect(),
            })
        }
        super::RootCorrectionStatus::Blocked => Err(ServiceError::RootCorrectionBlocked {
            game_id: assessment.game_id,
            blockers: assessment
                .blockers
                .into_iter()
                .map(|blocker| blocker.as_str().to_owned())
                .collect(),
        }),
    }
}

fn ensure_root_change_not_blocked_before_scan(
    context: &crate::Context,
    game: &renderpilot_domain::GameInstallation,
) -> Result<(), ServiceError> {
    let assessment = assess_root_change(context, game, None)?;
    if assessment.status != super::RootCorrectionStatus::Blocked {
        return Ok(());
    }

    Err(ServiceError::RootCorrectionBlocked {
        game_id: assessment.game_id,
        blockers: assessment
            .blockers
            .into_iter()
            .map(|blocker| blocker.as_str().to_owned())
            .collect(),
    })
}

fn assess_root_change(
    context: &crate::Context,
    game: &renderpilot_domain::GameInstallation,
    prospective_components: Option<&[LibraryComponent]>,
) -> Result<super::RootCorrectionAssessment, ServiceError> {
    super::root_correction::assess(
        context,
        game.id(),
        game.install_path().as_str(),
        prospective_components,
    )
}

#[cfg(test)]
mod tests {
    use std::{fs, fs::FileTimes, time::SystemTime};

    use renderpilot_application::GameRepository;
    use renderpilot_detection::LibraryPatternComponentDetector;
    use renderpilot_domain::{
        GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
        RootAuthority,
    };
    use renderpilot_nvapi::{DlssDllKind, DlssVersion};

    use super::{
        ExplicitRootChange, ScanInputs, scan_explicit_install, scan_source_impl_observational,
    };

    fn assert_catalogued_sr_version(
        context: &crate::Context,
        root: &std::path::Path,
        game_id: &GameId,
        expected: DlssVersion,
    ) {
        let setting_context = crate::nvapi::resolve::build_setting_context_with_context(
            context,
            root,
            game_id.as_str(),
        )
        .expect("catalog projection");
        assert_eq!(
            setting_context.catalog_readiness,
            renderpilot_nvapi::CatalogReadiness::Ready
        );
        assert_eq!(
            setting_context.dlls[&DlssDllKind::Sr].version,
            Some(expected)
        );
    }

    fn scan_fixture(context: &crate::Context, root: &std::path::Path, game_id: &GameId) {
        scan_explicit_install(
            context,
            root.to_path_buf(),
            game_id.clone(),
            RootAuthority::UserConfirmed,
            None,
            ExplicitRootChange::Unchanged,
            &[],
        )
        .expect("complete explicit scan");
    }

    #[test]
    fn complete_scan_persistence_and_nvapi_projection_follow_external_same_size_a_b_a_replacement()
    {
        let root = tempfile::tempdir().expect("game root");
        let dll = root.path().join("nvngx_dlss.dll");
        let a = crate::addons::test_support::build_nvidia_dlss_pe([1, 0, 0, 0]);
        let b = crate::addons::test_support::build_nvidia_dlss_pe([2, 0, 0, 0]);
        assert_eq!(a.len(), b.len(), "fixture replacement must be same size");
        fs::write(&dll, &a).expect("write A");
        let mtime = fs::metadata(&dll)
            .expect("A metadata")
            .modified()
            .unwrap_or(SystemTime::UNIX_EPOCH);

        let context = crate::Context::from_storage(
            renderpilot_storage_sqlite::SqliteStorage::in_memory().expect("storage"),
        );
        let game_id = GameId::new("manual:scan-projection-a-b-a").expect("game id");
        scan_fixture(&context, root.path(), &game_id);
        assert_catalogued_sr_version(
            &context,
            root.path(),
            &game_id,
            DlssVersion::new(1, 0, 0, 0),
        );

        fs::write(&dll, &b).expect("write B");
        fs::OpenOptions::new()
            .write(true)
            .open(&dll)
            .expect("open B for timestamp restoration")
            .set_times(FileTimes::new().set_modified(mtime))
            .expect("restore A timestamp on B");
        scan_fixture(&context, root.path(), &game_id);
        assert_catalogued_sr_version(
            &context,
            root.path(),
            &game_id,
            DlssVersion::new(2, 0, 0, 0),
        );

        fs::write(&dll, &a).expect("restore A bytes");
        fs::OpenOptions::new()
            .write(true)
            .open(&dll)
            .expect("open restored A for timestamp restoration")
            .set_times(FileTimes::new().set_modified(mtime))
            .expect("restore A timestamp");
        scan_fixture(&context, root.path(), &game_id);
        assert_catalogued_sr_version(
            &context,
            root.path(),
            &game_id,
            DlssVersion::new(1, 0, 0, 0),
        );
    }

    #[test]
    fn refresh_game_components_updates_catalog_after_dll_mutation() {
        use renderpilot_application::ComponentRepository;

        let root = tempfile::tempdir().expect("game root");
        let context = crate::Context::from_storage(
            renderpilot_storage_sqlite::SqliteStorage::in_memory().expect("storage"),
        );
        let game_id = GameId::new("manual:refresh-components").expect("game id");

        // Initial scan: folder is empty
        scan_fixture(&context, root.path(), &game_id);
        let initial_components = context
            .storage()
            .list_components_for_game(&game_id)
            .expect("list components");
        assert!(initial_components.is_empty());
        let initial_generation = context.storage().catalog_generation();

        // Simulate add-on writing DLSS DLL
        let dll = root.path().join("nvngx_dlss.dll");
        let dlss_bytes = crate::addons::test_support::build_nvidia_dlss_pe([3, 7, 0, 0]);
        fs::write(&dll, &dlss_bytes).expect("write dlss");

        // Call our new refresh_game_components_sync
        crate::catalog::refresh_game_components_sync(&context, &game_id)
            .expect("refresh game components");

        let updated_components = context
            .storage()
            .list_components_for_game(&game_id)
            .expect("list components");
        assert_eq!(updated_components.len(), 1);
        assert_eq!(
            updated_components[0].technology(),
            renderpilot_domain::LibraryTechnology::DlssSuperResolution
        );
        assert!(context.storage().catalog_generation() > initial_generation);

        // Simulate add-on uninstallation (deleting DLSS DLL)
        let generation_before_removal = context.storage().catalog_generation();
        fs::remove_file(&dll).expect("remove dlss");

        // Refresh catalog components again
        crate::catalog::refresh_game_components_sync(&context, &game_id)
            .expect("refresh game components after removal");

        let cleared_components = context
            .storage()
            .list_components_for_game(&game_id)
            .expect("list components");
        assert!(cleared_components.is_empty());
        assert!(context.storage().catalog_generation() > generation_before_removal);
    }

    #[test]
    fn waiting_observation_scan_uses_confirmation_from_the_locked_catalog_snapshot() {
        let root = tempfile::tempdir().expect("game root");
        let game_id = GameId::new("manual:locked-confirmation-refresh").expect("game id");
        let game = GameInstallation::new(
            GameIdentity::new(game_id.clone(), "Game", Launcher::Manual).expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(root.path().to_string_lossy().replace('\\', "/")).expect("root"),
        )
        .with_root_authority(RootAuthority::UserConfirmed)
        .with_confirmed_executable(PathRef::new("Old.exe").expect("old executable"));
        let context = std::sync::Arc::new(crate::Context::from_storage(
            renderpilot_storage_sqlite::SqliteStorage::in_memory().expect("storage"),
        ));
        context
            .storage()
            .upsert_game(&game)
            .expect("seed old registration");

        // This models a batch snapshot taken before another command's lock
        // was released. The queued scan must reload the row after acquiring
        // that lock rather than publishing the older confirmation.
        let stale_index = super::reconcile::CatalogInstallIndex::load(context.storage())
            .expect("prefetched catalog index");
        let held = crate::mutation_boundary::enter_game_observation_boundary(&game_id);
        fs::write(root.path().join("New.exe"), b"game executable")
            .expect("create current executable before registration");
        let current_game =
            game.with_confirmed_executable(PathRef::new("New.exe").expect("new executable"));
        context
            .storage()
            .upsert_game(&current_game)
            .expect("publish current confirmation");

        let source = renderpilot_platform_windows::ManualFolderGameSource::new(root.path())
            .with_game_id(game_id.clone())
            .with_root_authority(RootAuthority::UserConfirmed);
        let (attempt_tx, attempt_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        crate::game_mutation_lock::set_lock_attempt_hook(&game_id, attempt_tx);
        let worker_context = std::sync::Arc::clone(&context);
        let worker = std::thread::spawn(move || {
            let detector =
                LibraryPatternComponentDetector::windows_default().expect("test detector");
            let result = scan_source_impl_observational(
                ScanInputs {
                    context: &worker_context,
                    detector: &detector,
                },
                &source,
                Some(&stale_index),
                ExplicitRootChange::Unchanged,
                &[],
            )
            .map(|_| ())
            .map_err(|error| error.to_string());
            done_tx.send(result).expect("send scan result");
        });

        attempt_rx.recv().expect("scan waits for game lock");
        drop(held);
        done_rx
            .recv()
            .expect("scan result")
            .expect("scan completes");
        worker.join().expect("worker joins");

        let persisted = context
            .storage()
            .find_game(&game_id)
            .expect("read current row")
            .expect("game remains registered");
        assert_eq!(
            persisted.confirmed_executable().map(PathRef::as_str),
            Some("New.exe")
        );
    }
}
