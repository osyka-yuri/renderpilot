//! Catalog orchestration: scan, query, and library management.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use renderpilot_application::{
    find_replacement_candidates, AppError, ArtifactRepository, ComponentReplacementCandidates,
    ComponentRepository, GameRepository, OperationPlan, OperationRecord,
};
use renderpilot_detection::DetectedLibraryFile;
use renderpilot_domain::{
    GameId, GameInstallation, GraphicsComponent, GraphicsTechnology, LibraryArtifact,
};

use crate::ServiceError;

use self::scan::scan_folder_impl;

#[cfg(windows)]
pub mod auto_scan;
mod cards;
pub mod execute;
mod operations;
pub mod output;
/// Auto-discovery and scanning.
pub mod scan;
mod swap;

#[cfg(windows)]
pub use scan::prune_auto_scan_orphans;

/// The game installation and detected library files produced by a folder scan.
pub struct ScanFolderCatalogResult {
    /// The game installation discovered at the scanned path.
    pub game: GameInstallation,
    /// Library files detected within the game installation.
    pub libraries: Vec<DetectedLibraryFile>,
}

/// Game id and replacement candidate groups for a component swap UI.
pub struct CandidateCatalogResult {
    /// The game id the candidates belong to.
    pub game_id: GameId,
    /// Grouped replacement candidates, one group per component.
    pub groups: Vec<ComponentReplacementCandidates>,
}

/// Full game details for the main detail view.
pub struct GameDetailsCatalogResult {
    /// The game installation.
    pub game: GameInstallation,
    /// All graphics components for this game.
    pub components: Vec<GraphicsComponent>,
    /// Replacement candidate groups across all components.
    pub candidate_groups: Vec<ComponentReplacementCandidates>,
    /// Operation history for this game.
    pub operations: OperationListCatalogResult,
}

/// Resolved swap operation plan ready for execution.
pub struct SwapPlanCatalogResult {
    /// The resolved operation plan.
    pub plan: OperationPlan,
}

/// Operation history for a game.
pub struct OperationListCatalogResult {
    /// The game id the operations belong to.
    pub game_id: GameId,
    /// Ordered list of operation entries.
    pub operations: Vec<OperationListCatalogEntry>,
}

/// A single entry in the operation history list.
pub struct OperationListCatalogEntry {
    /// The operation record.
    pub operation: OperationRecord,
    /// Number of items (files) affected by the operation.
    pub item_count: usize,
    /// String ids of the components affected.
    pub component_ids: Vec<String>,
}

/// Scans a folder path for game installations and persists or updates catalog rows.
pub fn scan_folder(
    context: &crate::Context,
    path: PathBuf,
) -> Result<Vec<ScanFolderCatalogResult>, ServiceError> {
    let storage = context.storage();
    scan_folder_impl(storage, path)
}

/// Returns all game installations stored in the catalog.
pub fn list_games(context: &crate::Context) -> Result<Vec<GameInstallation>, ServiceError> {
    context.storage().list_games().map_err(Into::into)
}

/// The per-query-constant inputs for replacement-candidate matching: the local +
/// manifest artifact universe and the candidate context.
///
/// Loading these is independent of the game, so a multi-game caller (the
/// dashboard's [`game_cards`]) builds this **once** and reuses it for every game
/// instead of re-reading the artifacts table and re-parsing the libraries
/// manifest per game.
pub(crate) struct ReplacementUniverse {
    artifacts: Vec<LibraryArtifact>,
    candidate_context: renderpilot_application::CandidateContext,
}

/// Loads the artifact universe (local artifacts + manifest entries) and the
/// candidate context once. A missing/unparseable manifest degrades to
/// local-only artifacts — identical to the previous per-game behavior.
pub(crate) fn load_replacement_universe(
    context: &crate::Context,
) -> Result<ReplacementUniverse, ServiceError> {
    let local_artifacts = context.storage().list_artifacts()?;

    let downloaded_ids: HashSet<_> = local_artifacts.iter().map(|a| a.id().clone()).collect();
    let mut artifacts = local_artifacts;
    let (manifest_entry_ids, debug_entry_ids) =
        match crate::libraries::manifest_entries_as_artifacts() {
            Ok((manifest_artifacts, entry_ids, debug_ids)) => {
                artifacts.extend(
                    manifest_artifacts
                        .into_iter()
                        .filter(|a| !downloaded_ids.contains(a.id())),
                );
                (entry_ids, debug_ids)
            }
            Err(_) => (HashMap::new(), HashSet::new()),
        };

    let candidate_context = renderpilot_application::CandidateContext::new(
        downloaded_ids,
        manifest_entry_ids,
        debug_entry_ids,
    );

    Ok(ReplacementUniverse {
        artifacts,
        candidate_context,
    })
}

/// Builds full game details using a pre-loaded [`ReplacementUniverse`].
///
/// Only the genuinely per-game work runs here (components, candidate match,
/// operations); the constant artifact/manifest load is the caller's
/// responsibility via [`load_replacement_universe`].
pub(crate) fn get_game_details_with_universe(
    context: &crate::Context,
    game_id: GameId,
    universe: &ReplacementUniverse,
) -> Result<GameDetailsCatalogResult, ServiceError> {
    let storage = context.storage();
    let game = require_game(storage, &game_id)?;
    let components = storage.list_components_for_game(&game_id)?;

    let candidate_groups = find_replacement_candidates(
        &components,
        &universe.artifacts,
        &universe.candidate_context,
    );

    let operations = list_operations(context, &game_id)?;

    Ok(GameDetailsCatalogResult {
        game,
        components,
        candidate_groups,
        operations,
    })
}

/// Returns full game details including components, candidates, and operations.
pub fn get_game_details(
    context: &crate::Context,
    game_id: GameId,
) -> Result<GameDetailsCatalogResult, ServiceError> {
    let universe = load_replacement_universe(context)?;
    get_game_details_with_universe(context, game_id, &universe)
}

/// Returns library artifacts stored in the catalog, optionally filtered by technology.
pub fn list_artifacts(
    context: &crate::Context,
    technology: Option<GraphicsTechnology>,
) -> Result<Vec<LibraryArtifact>, ServiceError> {
    let artifacts = context.storage().list_artifacts()?;
    Ok(filter_artifacts_by_technology(artifacts, technology))
}

// Re-export core operations from sub-modules directly.
pub use cards::{game_cards, GameCardData};
pub use execute::{apply_swap, rollback_component};
pub use operations::list_operations;
pub use swap::{build_swap_plan, find_candidates};

/// Returns the distinct graphics-technology library tags present in the catalog.
pub fn distinct_game_libraries(context: &crate::Context) -> Result<Vec<String>, ServiceError> {
    context
        .storage()
        .list_distinct_game_libraries()
        .map_err(Into::into)
}

/// Returns the distinct launcher names present in the catalog.
pub fn distinct_game_launchers(context: &crate::Context) -> Result<Vec<String>, ServiceError> {
    context
        .storage()
        .list_distinct_game_launchers()
        .map_err(Into::into)
}

/// Returns the set of component ids that have a rollback backup for `game_id`.
pub fn backup_component_ids(
    context: &crate::Context,
    game_id: &GameId,
) -> Result<HashSet<String>, ServiceError> {
    context
        .storage()
        .component_backup_ids_for_game(game_id)
        .map_err(Into::into)
}

/// Reads one persisted catalog setting value.
pub fn get_catalog_setting(
    context: &crate::Context,
    key: &str,
) -> Result<Option<String>, ServiceError> {
    context.storage().get_setting(key).map_err(Into::into)
}

/// Upserts a catalog setting, or deletes the row when `value` is blank after trimming.
pub fn set_catalog_setting(
    context: &crate::Context,
    key: &str,
    value: &str,
) -> Result<(), ServiceError> {
    let storage = context.storage();
    if value.trim().is_empty() {
        storage.delete_setting(key).map_err(Into::into)
    } else {
        storage.set_setting(key, value).map_err(Into::into)
    }
}

/// Sets the favorite flag for `game_id`, preserving its hidden flag.
pub fn set_game_favorite(
    context: &crate::Context,
    game_id: &GameId,
    is_favorite: bool,
) -> Result<(), ServiceError> {
    update_game_ui_state(context, game_id, |_, hidden| (is_favorite, hidden))
}

/// Sets the hidden flag for `game_id`, preserving its favorite flag.
pub fn set_game_hidden(
    context: &crate::Context,
    game_id: &GameId,
    is_hidden: bool,
) -> Result<(), ServiceError> {
    update_game_ui_state(context, game_id, |favorite, _| (favorite, is_hidden))
}

/// Reads the current UI state, applies `f` to produce the new
/// `(is_favorite, is_hidden)` pair, and persists it.
fn update_game_ui_state(
    context: &crate::Context,
    game_id: &GameId,
    f: impl FnOnce(bool, bool) -> (bool, bool),
) -> Result<(), ServiceError> {
    let storage = context.storage();
    let current = storage.get_game_ui_state(game_id.as_str())?;
    let (prev_favorite, prev_hidden) = current
        .map(|state| (state.is_favorite, state.is_hidden))
        .unwrap_or((false, false));
    let (is_favorite, is_hidden) = f(prev_favorite, prev_hidden);
    storage
        .save_game_ui_state(game_id.as_str(), is_favorite, is_hidden)
        .map_err(Into::into)
}

fn filter_artifacts_by_technology(
    artifacts: Vec<LibraryArtifact>,
    technology: Option<GraphicsTechnology>,
) -> Vec<LibraryArtifact> {
    match technology {
        Some(required_technology) => artifacts
            .into_iter()
            .filter(|artifact| artifact.technology() == required_technology)
            .collect(),
        None => artifacts,
    }
}

fn require_game<R>(repository: &R, game_id: &GameId) -> Result<GameInstallation, AppError>
where
    R: GameRepository + ?Sized,
{
    repository
        .find_game(game_id)?
        .ok_or_else(|| AppError::game_not_found(game_id.as_str()))
}
