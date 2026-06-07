//! Game-card aggregation: joins games with covers, UI state, details, and
//! rollback availability into typed rows the presentation layer can render.

use std::collections::HashMap;

use renderpilot_domain::GameInstallation;

use crate::ServiceError;

use super::{get_game_details, GameDetailsCatalogResult};

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

    let mut cards = Vec::with_capacity(games.len());
    for game in games {
        let details = get_game_details(context, game.id().clone())?;
        let cover_updated_at_ms = covers_by_game
            .get(game.id())
            .map(|record| record.updated_at_ms);
        let rollback_available = !storage.component_backup_ids_for_game(game.id())?.is_empty();

        let ui_state = ui_states.get(game.id().as_str());
        let is_favorite = ui_state.map(|state| state.is_favorite).unwrap_or(false);
        let is_hidden = ui_state.map(|state| state.is_hidden).unwrap_or(false);

        cards.push(GameCardData {
            game,
            details,
            cover_updated_at_ms,
            rollback_available,
            is_favorite,
            is_hidden,
        });
    }

    Ok(cards)
}
