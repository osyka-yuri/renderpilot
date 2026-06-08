//! GUI catalog facade: game-card listing/query, single-game details, and
//! persisted catalog/UI settings. Split by responsibility:
//!
//! * [`cards`] — backend-owned filtering, sorting, and paging of game cards.
//! * [`details`] — one game's components, candidates, and operation history.
//! * [`settings`] — persisted catalog settings and per-game favorite/hidden flags.

mod cards;
mod details;
mod settings;

use std::collections::BTreeSet;

use renderpilot_orchestration::domain::GraphicsComponent;

use crate::utils::is_visible_graphics_technology;

pub use cards::{list_games, query_game_cards, QueryGameCardsRequest};
pub use details::get_game_details;
pub use settings::{get_catalog_setting, set_catalog_setting, set_game_favorite, set_game_hidden};

// Re-exported for `scan.rs`, which loads game details for freshly scanned games.
pub(crate) use details::GameDetailsOutput;

/// Whether the GUI surfaces this component (single source of the visibility rule,
/// shared by [`cards`] and [`details`] so it can never drift).
fn is_component_visible(component: &GraphicsComponent) -> bool {
    is_visible_graphics_technology(component.technology())
}

/// Stable string ids of the components the GUI surfaces for a game.
///
/// Shared by [`cards`] (update counts per visible component) and [`details`]
/// (candidate-group filtering).
fn visible_component_ids(components: &[GraphicsComponent]) -> BTreeSet<String> {
    components
        .iter()
        .filter(|component| is_component_visible(component))
        .map(|component| component.id().as_str().to_owned())
        .collect()
}
