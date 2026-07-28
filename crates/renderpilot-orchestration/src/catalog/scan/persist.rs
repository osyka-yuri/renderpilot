//! Persist scan results into the catalog and clean stale parent rows.

use std::path::PathBuf;

use renderpilot_detection::DetectedLibraryFile;
use renderpilot_domain::GameInstallation;
use renderpilot_storage_sqlite::{ScanWriteUnit, SqliteStorage};

use crate::ServiceError;
use crate::catalog::{CatalogScanChange, ScanFolderCatalogResult};

use super::reconcile::{
    CatalogInstallIndex, build_graphics_components, build_library_artifacts,
    reconcile_game_with_catalog,
};
use super::recovery;
use super::roots::{bucket_libraries_by_longest_install_prefix, discover_sub_installations};

pub(super) fn persist_scan_results(
    storage: &SqliteStorage,
    selected_game: GameInstallation,
    libraries: Vec<DetectedLibraryFile>,
    install_roots: Vec<PathBuf>,
    prefetched_catalog_index: Option<&CatalogInstallIndex>,
) -> Result<Vec<ScanFolderCatalogResult>, ServiceError> {
    let owned_catalog_index;
    let catalog_index = match prefetched_catalog_index {
        Some(index) => index,
        None => {
            owned_catalog_index = CatalogInstallIndex::load(storage)?;
            &owned_catalog_index
        }
    };

    if install_roots.len() <= 1 {
        return Ok(vec![persist_scan_result(
            storage,
            catalog_index,
            selected_game,
            libraries,
        )?]);
    }

    persist_split_scan_results(
        storage,
        catalog_index,
        &selected_game,
        libraries,
        install_roots,
    )
}

fn persist_split_scan_results(
    storage: &SqliteStorage,
    catalog_index: &CatalogInstallIndex,
    selected_game: &GameInstallation,
    libraries: Vec<DetectedLibraryFile>,
    install_roots: Vec<PathBuf>,
) -> Result<Vec<ScanFolderCatalogResult>, ServiceError> {
    let installs = discover_sub_installations(install_roots)?;
    let buckets = bucket_libraries_by_longest_install_prefix(libraries, &installs)?;

    let mut results = Vec::with_capacity(installs.len());

    for (install, libraries) in installs.into_iter().zip(buckets) {
        results.push(persist_scan_result(
            storage,
            catalog_index,
            install.game,
            libraries,
        )?);
    }

    delete_stale_parent_game_if_needed(storage, selected_game, &results)?;

    Ok(results)
}

/// Deletes an old parent scan row only when the selected game was not also detected
/// as one of the current scan results.
///
/// Without this guard, a split scan can accidentally delete a freshly saved result
/// if one of the derived install roots is equal to the originally selected root.
fn delete_stale_parent_game_if_needed(
    storage: &SqliteStorage,
    selected_game: &GameInstallation,
    results: &[ScanFolderCatalogResult],
) -> Result<(), ServiceError> {
    let selected_game_is_current = results
        .iter()
        .any(|result| result.game.id() == selected_game.id());

    if !selected_game_is_current {
        let catalog_path = crate::storage::catalog_database_path()?;
        let deleted = storage.delete_game(selected_game.id())?;
        crate::covers::unlink_cover_file_best_effort(
            &catalog_path,
            deleted.old_cover_file_name.as_deref(),
        );
    }

    Ok(())
}

fn persist_scan_result(
    storage: &SqliteStorage,
    catalog_index: &CatalogInstallIndex,
    game: GameInstallation,
    libraries: Vec<DetectedLibraryFile>,
) -> Result<ScanFolderCatalogResult, ServiceError> {
    let existed = catalog_index.contains_install_path_str(game.install_path().as_str());
    let game = reconcile_game_with_catalog(catalog_index, game);
    let components = build_graphics_components(&game, &libraries)?;
    let artifacts = build_library_artifacts(game.id(), &libraries)?;
    let mut changed = catalog_index.card_facts_changed(&game, &components, &artifacts);

    if changed {
        storage.save_scan_write_unit(ScanWriteUnit {
            game: &game,
            components: &components,
            artifacts: &artifacts,
            prune_empty_operations: false,
        })?;
    }

    let generation_before_recovery = storage.catalog_generation();
    recovery::recover_orphaned_backups(storage, game.id(), &components)?;
    changed |= storage.catalog_generation() != generation_before_recovery;

    Ok(ScanFolderCatalogResult {
        game,
        libraries,
        change: if !changed {
            CatalogScanChange::Unchanged
        } else if existed {
            CatalogScanChange::Updated
        } else {
            CatalogScanChange::Added
        },
    })
}
