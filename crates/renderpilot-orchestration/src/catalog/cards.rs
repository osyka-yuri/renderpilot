//! Game-card aggregation: joins games with covers, UI state, details, and
//! rollback availability into typed rows the presentation layer can render.

use std::collections::HashMap;

use renderpilot_domain::GameInstallation;

use crate::ServiceError;

use super::{get_game_details_with_universe, load_replacement_universe, GameDetailsCatalogResult};

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
        .map(|row| (row.game_id.clone(), row))
        .collect();

    // Loaded once and reused for every game: the artifacts table and the
    // libraries manifest are identical across games, so re-reading the table
    // and re-parsing the manifest per game (as the old `get_game_details` did)
    // was pure O(N) waste on the dashboard.
    let universe = load_replacement_universe(context)?;

    games
        .into_iter()
        .map(|game| {
            let details = get_game_details_with_universe(context, game.id().clone(), &universe)?;
            let cover_updated_at_ms = covers_by_game
                .get(game.id())
                .map(|record| record.updated_at_ms);
            let rollback_available = !storage.component_backup_ids_for_game(game.id())?.is_empty();

            let ui_state = ui_states.get(game.id().as_str());
            let is_favorite = ui_state.is_some_and(|state| state.is_favorite);
            let is_hidden = ui_state.is_some_and(|state| state.is_hidden);

            Ok(GameCardData {
                game,
                details,
                cover_updated_at_ms,
                rollback_available,
                is_favorite,
                is_hidden,
            })
        })
        .collect()
}
