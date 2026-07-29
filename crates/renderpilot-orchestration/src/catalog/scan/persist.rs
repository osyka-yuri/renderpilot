//! Persist one explicit installation scan into the catalog.

use std::collections::HashSet;

use renderpilot_detection::DetectedLibraryFile;
use renderpilot_domain::{
    GameId, GameInstallation, GraphicsComponent, InstallKey, Launcher, RootAuthority,
    normalized_path_key,
};
use renderpilot_storage_sqlite::{
    ComponentRekey, ConsolidationPlan, ConsolidationSource, ScanWriteUnit, SqliteStorage,
};

use crate::ServiceError;
use crate::catalog::{
    CatalogScanChange, ScanConsolidationOutcome, ScanFolderCatalogResult, install_paths,
};

use super::reconcile::{CatalogInstallIndex, build_library_artifacts, reconcile_game_with_catalog};
use super::recovery;

/// Inputs owned or borrowed by one aggregate catalog write.
pub(super) struct PersistScanRequest<'a> {
    pub game: GameInstallation,
    pub libraries: Vec<DetectedLibraryFile>,
    pub components: &'a [GraphicsComponent],
    pub prune_empty_operations: bool,
    pub root_correction_recovery_bundle_path: Option<String>,
    pub prefetched_catalog_index: Option<&'a CatalogInstallIndex>,
    pub consolidation_candidates: &'a [GameId],
}

pub(super) fn persist_scan_result(
    storage: &SqliteStorage,
    request: PersistScanRequest<'_>,
) -> Result<ScanFolderCatalogResult, ServiceError> {
    let PersistScanRequest {
        game,
        libraries,
        components,
        prune_empty_operations,
        root_correction_recovery_bundle_path,
        prefetched_catalog_index,
        consolidation_candidates,
    } = request;
    let owned_catalog_index;
    let catalog_index = match prefetched_catalog_index {
        Some(index) => index,
        None => {
            owned_catalog_index = CatalogInstallIndex::load(storage)?;
            &owned_catalog_index
        }
    };

    let existed = catalog_index.contains_install_path_str(game.install_path().as_str());
    let game = reconcile_game_with_catalog(catalog_index, game);
    let artifacts = build_library_artifacts(game.id(), &libraries)?;
    let mut changed = catalog_index.card_facts_changed(&game, components, &artifacts);
    let (consolidation_plan, retained_candidate_game_ids) =
        prove_consolidation_plan(catalog_index, &game, components, consolidation_candidates);
    let conflicts = storage.inspect_consolidation_conflicts(&consolidation_plan)?;
    let recovery_bundle = if conflicts.requires_recovery_bundle() {
        Some(
            super::super::recovery_bundle::create_consolidation_recovery_bundle(
                storage,
                &consolidation_plan,
                &conflicts,
            )?,
        )
    } else {
        None
    };
    if conflicts.has_blocking_conflicts() {
        let Some(recovery_bundle_path) = recovery_bundle.as_ref() else {
            return Err(ServiceError::command_failed(
                "blocking consolidation conflicts were detected without a recovery bundle",
            ));
        };
        return Err(ServiceError::CatalogConsolidationBlocked {
            tables: conflicts.blocking_tables,
            recovery_bundle_path: recovery_bundle_path.to_string_lossy().to_string(),
        });
    }
    let mut consolidation = ScanConsolidationOutcome {
        retained_candidate_game_ids,
        recovery_bundle_path: recovery_bundle
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        ..ScanConsolidationOutcome::default()
    };
    if changed || !consolidation_plan.is_empty() {
        let report = storage.save_install_scan_with_consolidation(
            ScanWriteUnit {
                game: &game,
                components,
                artifacts: &artifacts,
                prune_empty_operations,
            },
            &consolidation_plan,
            &conflicts,
        )?;
        changed = true;
        consolidation.removed_game_ids = report.consolidation.removed_game_ids;
        consolidation.destination_wins_conflicts = report.consolidation.destination_wins_conflicts;

        if let Ok(Some(catalog_path)) = storage.catalog_file_path() {
            for file_name in report.consolidation.discarded_cover_file_names {
                crate::covers::unlink_cover_file_best_effort(
                    &catalog_path,
                    Some(file_name.as_str()),
                );
            }
        }
    }

    let generation_before_recovery = storage.catalog_generation();
    recovery::recover_orphaned_backups(storage, game.id(), components)?;
    changed |= storage.catalog_generation() != generation_before_recovery;

    Ok(ScanFolderCatalogResult {
        game,
        libraries,
        consolidation,
        root_correction_recovery_bundle_path,
        change: if !changed {
            CatalogScanChange::Unchanged
        } else if existed {
            CatalogScanChange::Updated
        } else {
            CatalogScanChange::Added
        },
    })
}

fn prove_consolidation_plan(
    catalog: &CatalogInstallIndex,
    destination_game: &GameInstallation,
    destination_components: &[GraphicsComponent],
    candidates: &[GameId],
) -> (ConsolidationPlan, Vec<GameId>) {
    let mut candidates = candidates.to_vec();
    candidates.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    candidates.dedup();
    if candidates.is_empty() {
        return (
            ConsolidationPlan {
                destination_game_id: destination_game.id().clone(),
                sources: Vec::new(),
            },
            Vec::new(),
        );
    }
    let Some(launcher_install_keys) = launcher_install_keys_for_consolidation() else {
        return (
            ConsolidationPlan {
                destination_game_id: destination_game.id().clone(),
                sources: Vec::new(),
            },
            candidates,
        );
    };
    let mut sources = Vec::new();
    let mut retained = Vec::new();

    for candidate in candidates {
        match prove_source_component_mapping(
            catalog,
            destination_game,
            destination_components,
            &candidate,
            &launcher_install_keys,
        ) {
            Some(component_rekeys) => sources.push(ConsolidationSource {
                source_game_id: candidate,
                component_rekeys,
            }),
            None => retained.push(candidate),
        }
    }

    (
        ConsolidationPlan {
            destination_game_id: destination_game.id().clone(),
            sources,
        },
        retained,
    )
}

#[cfg(windows)]
fn launcher_install_keys_for_consolidation() -> Option<HashSet<InstallKey>> {
    Some(
        renderpilot_platform_windows::game_libraries::discover_game_sources()
            .installs
            .into_iter()
            .filter_map(|install| {
                install_paths::install_path_match_key(&install.install_path.to_string_lossy())
            })
            .collect(),
    )
}

#[cfg(not(windows))]
fn launcher_install_keys_for_consolidation() -> Option<HashSet<InstallKey>> {
    None
}

fn prove_source_component_mapping(
    catalog: &CatalogInstallIndex,
    destination_game: &GameInstallation,
    destination_components: &[GraphicsComponent],
    source_game_id: &GameId,
    launcher_install_keys: &HashSet<InstallKey>,
) -> Option<Vec<ComponentRekey>> {
    let source_game = catalog.game(source_game_id)?;
    let source_install_key = source_game.install_key();
    if source_game.identity().launcher() != Launcher::Manual
        || source_game.root_authority() != RootAuthority::Legacy
        || !source_game.executable_candidates().is_empty()
        || launcher_install_keys.contains(source_install_key)
        || !install_paths::normalized_path_within_scope(
            source_game.install_path().as_str(),
            destination_game.install_path().as_str(),
        )
        || source_install_key == destination_game.install_key()
    {
        return None;
    }

    let source_root = std::path::Path::new(source_game.install_path().as_str());
    if !source_root.is_dir()
        || renderpilot_platform_windows::detect_install_identity(source_root).is_some()
    {
        return None;
    }
    let executable_probe = renderpilot_platform_windows::inspect_executable_candidates(source_root);
    if executable_probe.completeness() != renderpilot_detection::InstallTreeCompleteness::Complete
        || executable_probe.candidates().iter().any(|candidate| {
            renderpilot_platform_windows::is_readable_windows_pe_executable(
                &candidate.absolute_path,
            )
        })
    {
        return None;
    }

    let source_components = catalog.components(source_game_id)?;
    if source_components.is_empty() {
        return None;
    }

    let mut used_destinations = HashSet::new();
    let mut rekeys = Vec::with_capacity(source_components.len());
    let mut ordered_sources: Vec<&GraphicsComponent> = source_components.values().collect();
    ordered_sources.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));

    for source in ordered_sources {
        let matching: Vec<&GraphicsComponent> = destination_components
            .iter()
            .filter(|destination| components_represent_same_files(source, destination))
            .collect();
        if matching.len() != 1 {
            return None;
        }
        let destination = matching[0];
        if !used_destinations.insert(destination.id().as_str()) {
            return None;
        }
        rekeys.push(ComponentRekey {
            source_component_id: source.id().as_str().to_owned(),
            destination_component_id: destination.id().as_str().to_owned(),
        });
    }
    Some(rekeys)
}

fn components_represent_same_files(
    source: &GraphicsComponent,
    destination: &GraphicsComponent,
) -> bool {
    if source.kind() != destination.kind() || source.technology() != destination.technology() {
        return false;
    }
    let mut source_files: Vec<String> = source
        .files()
        .iter()
        .map(|file| normalized_path_key(file.path().as_str()))
        .collect();
    let mut destination_files: Vec<String> = destination
        .files()
        .iter()
        .map(|file| normalized_path_key(file.path().as_str()))
        .collect();
    source_files.sort();
    destination_files.sort();
    source_files == destination_files
}

#[cfg(all(test, not(windows)))]
mod non_windows_tests {
    use super::*;

    #[test]
    fn missing_launcher_evidence_is_not_treated_as_an_empty_proof_set() {
        assert!(
            launcher_install_keys_for_consolidation().is_none(),
            "consolidation must remain fail-closed when Windows launcher evidence is unavailable"
        );
    }
}
