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
pub(super) enum ExplicitRootChange {
    Unchanged,
    Expanded,
    Narrowed,
}

/// Scans one installation whose identity and root authority were resolved by
/// the add-game use case.
pub(super) fn scan_explicit_install(
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

/// Borrowed storage + detector for one [`scan_impl`] invocation.
#[derive(Clone, Copy)]
struct ScanInputs<'a> {
    context: &'a crate::Context,
    detector: &'a LibraryPatternComponentDetector,
}

fn scan_source_impl(
    inputs: ScanInputs<'_>,
    source: &ManualFolderGameSource,
    catalog_index: Option<&reconcile::CatalogInstallIndex>,
    root_change: ExplicitRootChange,
    consolidation_candidates: &[GameId],
) -> Result<ScanFolderCatalogResult, ServiceError> {
    let storage = inputs.context.storage();
    let detector = inputs.detector;

    let owned_catalog_index;
    let catalog_index = match catalog_index {
        Some(index) => index,
        None => {
            owned_catalog_index = reconcile::CatalogInstallIndex::load(storage)?;
            &owned_catalog_index
        }
    };
    let selected_game =
        reconcile::reconcile_game_with_catalog(catalog_index, source.discover_game()?);
    let mut affected_ids = consolidation_candidates.to_vec();
    affected_ids.push(selected_game.id().clone());
    let _guards =
        crate::mutation_boundary::enter_game_mutation_boundaries(inputs.context, affected_ids)?;
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
    if root_change != ExplicitRootChange::Unchanged {
        ensure_root_change_preserves_state(inputs.context, &selected_game, &components)?;
    }
    let root_correction_recovery_bundle_path = if root_change == ExplicitRootChange::Narrowed {
        archive_pruned_operation_history(inputs.context, &selected_game, &components)?
    } else {
        None
    };

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
        },
    )
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
    let executable_basenames = game
        .executable_candidates()
        .iter()
        .filter_map(|candidate| std::path::Path::new(candidate.as_str()).file_name())
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .collect();
    super::root_correction::assess(
        context,
        game.id(),
        game.install_path().as_str(),
        &executable_basenames,
        prospective_components,
    )
}

#[cfg(test)]
mod tests {
    use std::{fs, fs::FileTimes, time::SystemTime};

    use renderpilot_domain::{GameId, RootAuthority};
    use renderpilot_nvapi::{DlssDllKind, DlssVersion};

    use super::{ExplicitRootChange, scan_explicit_install};

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
}
