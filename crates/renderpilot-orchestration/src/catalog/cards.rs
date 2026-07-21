//! Game-card aggregation: joins games with covers, UI state, details, and
//! rollback availability into typed rows the presentation layer can render.

use std::collections::HashMap;

use renderpilot_domain::{AddonKind, GameInstallation};

use crate::ServiceError;

use super::{GameDetailsCatalogResult, get_game_details_with_universe, load_replacement_universe};

/// One game's aggregated dashboard data, assembled by [`game_cards`].
///
/// This is a typed orchestration result: the presentation layer maps it into a
/// JSON DTO. It deliberately exposes no storage adapter types.
pub struct GameCardData {
    /// The game installation.
    pub game: GameInstallation,
    /// Detected components, replacement candidates, and operation history.
    pub details: GameDetailsCatalogResult,
    /// Timestamp (ms) of the most recent cover update, if a cover exists.
    pub cover_updated_at_ms: Option<i64>,
    /// Whether any component for this game has a rollback backup available.
    pub rollback_available: bool,
    /// Whether the user marked this game as a favorite.
    pub is_favorite: bool,
    /// Whether the user marked this game as hidden.
    pub is_hidden: bool,
    /// Add-ons with a usable profile or an existing install for this game.
    pub addon_capabilities: Vec<AddonKind>,
}

/// Loads every game in the catalog as an aggregated [`GameCardData`] row.
///
/// Owns all the multi-repository data access (games, covers, UI state, per-game
/// details, rollback backups) so the presentation layer never touches storage.
pub fn game_cards(context: &crate::Context) -> Result<Vec<GameCardData>, ServiceError> {
    let storage = context.storage();

    let games = storage.list_games()?;
    let covers_by_game = storage.list_all_game_covers()?;
    let ui_states: HashMap<String, _> = storage
        .list_all_game_ui_state()?
        .into_iter()
        .map(|mut row| {
            let game_id = std::mem::take(&mut row.game_id);
            (game_id, row)
        })
        .collect();
    let installed_addons: HashMap<_, _> = crate::addons::records::active_records(context)?
        .map(|addon| (addon.game_id().clone(), addon.kind()))
        .collect();
    let profile_capabilities = context.profile_capability_snapshot();

    // Loaded once and reused for every game: the artifacts table and the
    // Library package records are identical across games, so re-reading the table
    // and re-parsing the manifest per game (as the old `get_game_details` did)
    // was pure O(N) waste on the dashboard.
    let universe = load_replacement_universe(context)?;

    games
        .into_iter()
        .map(|game| {
            let details = get_game_details_with_universe(context, game.id(), &universe)?;
            let cover_updated_at_ms = covers_by_game
                .get(game.id())
                .map(|record| record.updated_at_ms);
            let rollback_available = !crate::coordinated_files::available_component_backup_ids(
                storage,
                game.id(),
                &details.components,
            )?
            .is_empty();

            let ui_state = ui_states.get(game.id().as_str());
            let is_favorite = ui_state.is_some_and(|state| state.is_favorite);
            let is_hidden = ui_state.is_some_and(|state| state.is_hidden);
            let profile = profile_capabilities.capabilities_for(game.id());
            // Shared merge (profile OR installed). We preload the installed map for
            // the whole dashboard instead of calling get_installed_addon per game.
            let addon_capabilities =
                super::merge_addon_capabilities(&profile, installed_addons.get(game.id()).copied());

            Ok(GameCardData {
                game,
                details,
                cover_updated_at_ms,
                rollback_available,
                is_favorite,
                is_hidden,
                addon_capabilities,
            })
        })
        .collect()
}
