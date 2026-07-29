//! Manual-folder and auto-scan orchestration for the game catalog.
//!
//! ## Modules
//!
//! - `detect` -- library-file detection modes + hash-cache I/O glue
//! - `reconcile` -- catalog identity merge for stable game ids
//! - `persist` -- write one explicit installation scan unit
//! - `hash_cache` -- populate/load/save `file_hash_cache` (crate-visible for
//!   auto_scan batch prefetch)
//! - existing: `discovery`, `paths`, `prune`, `recovery`, `scan_plan`, `auto`
//!
//! ## Dependency rules
//!
//! ```text
//! mod (scan_impl) -> detect, persist
//! detect          -> hash_cache, scan_plan
//! persist         -> reconcile, recovery
//! ```

mod detect;
// crate-visible for auto_scan batch prefetch (hard path).
pub(crate) mod hash_cache;
mod persist;
mod prune;
mod reconcile;
mod recovery;
mod scan_plan;

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
use renderpilot_detection::{FileHashCache, LibraryPatternComponentDetector};
use renderpilot_domain::{GameId, GraphicsComponent, RootAuthority};
use renderpilot_platform_windows::ManualFolderGameSource;
use scan_plan::DetectionMode;

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
        DetectionMode::FullCached,
        None,
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
    detection_mode: DetectionMode,
    prefetched_cache: Option<&FileHashCache>,
    catalog_index: Option<&reconcile::CatalogInstallIndex>,
    root_change: ExplicitRootChange,
    consolidation_candidates: &[GameId],
) -> Result<ScanFolderCatalogResult, ServiceError> {
    let storage = inputs.context.storage();
    let detector = inputs.detector;

    let selected_game = source.discover_game()?;
    let mut affected_ids = consolidation_candidates.to_vec();
    affected_ids.push(selected_game.id().clone());
    let _guards =
        crate::game_mutation_lock::enter_game_mutation_boundaries(inputs.context, affected_ids)?;
    if root_change != ExplicitRootChange::Unchanged {
        ensure_root_change_not_blocked_before_scan(inputs.context, &selected_game)?;
    }
    let libraries = detect_libraries(
        storage,
        detector,
        &selected_game,
        detection_mode,
        prefetched_cache,
    )?;
    let components = reconcile::build_graphics_components(&selected_game, &libraries)?;
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
            prune_empty_operations: root_change == ExplicitRootChange::Narrowed,
            root_correction_recovery_bundle_path,
            prefetched_catalog_index: catalog_index,
            consolidation_candidates,
        },
    )
}

fn archive_pruned_operation_history(
    context: &crate::Context,
    game: &renderpilot_domain::GameInstallation,
    prospective_components: &[GraphicsComponent],
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
    prospective_components: &[GraphicsComponent],
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
    prospective_components: Option<&[GraphicsComponent]>,
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
