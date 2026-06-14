//! Game-card listing: backend-owned filtering, sorting, and paging over the
//! dashboard's game cards, plus the card DTO the GUI renders.

use renderpilot_orchestration::catalog as orch_catalog;

use crate::utils::{to_json, JsonResult};
use crate::ApiError;

mod normalize;
mod output;
mod query;
mod sort;

#[cfg(test)]
mod tests;

use self::normalize::{normalize_launcher_names, normalize_library_names};
use self::output::{GameCardOutput, GameListOutput, QueryGameCardsOutput};
use self::query::QueryGameCards;
use self::sort::page_items;

/// Lists all games currently stored in the local catalog using a caller-provided storage connection.
pub fn list_games(context: &renderpilot_orchestration::Context) -> JsonResult {
    to_json(GameListOutput {
        games: orch_catalog::list_games(context)?,
    })
}

/// Queries game cards with backend-owned filtering, sorting, and paging semantics.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QueryGameCardsRequest {
    /// The search query to filter game titles by.
    pub search_query: String,
    /// List of selected library tags to filter by.
    pub selected_libraries: Vec<String>,
    /// List of selected launcher names to filter by.
    pub selected_launchers: Vec<String>,
    /// Whether to show games marked as hidden.
    pub show_hidden: bool,
    /// Whether to only show games marked as favorite.
    pub favorites_only: bool,
    /// The field to sort the results by (e.g., 'title', 'risk').
    pub sort_field: String,
    /// The direction to sort the results ('asc' or 'desc').
    pub sort_direction: String,
    /// The maximum number of results to return per page.
    pub page_limit: i64,
    /// The offset to start returning results from.
    pub page_offset: i64,
}

/// Queries game cards with backend-owned filtering, sorting, and paging semantics.
pub fn query_game_cards(
    context: &renderpilot_orchestration::Context,
    req: QueryGameCardsRequest,
) -> JsonResult {
    let cards = load_game_cards(context)?;

    let available_libraries =
        normalize_library_names(orch_catalog::distinct_game_libraries(context)?);
    let available_launchers =
        normalize_launcher_names(orch_catalog::distinct_game_launchers(context)?);

    let query = QueryGameCards::new(req, &available_libraries, &available_launchers);

    let query_fingerprint = query_fingerprint(&query);

    // Count all hidden games in the catalog (before query filters) so the
    // toolbar badge always reflects the total, not just the filtered subset.
    let hidden_count = cards.iter().filter(|c| c.is_hidden).count();

    let mut filtered = cards
        .into_iter()
        .filter(|card| query.matches(card))
        .collect::<Vec<_>>();

    filtered.sort_by(|left, right| query.compare(left, right));

    let total = filtered.len();
    let items = page_items(&filtered, query.page);

    to_json(QueryGameCardsOutput {
        items,
        total,
        hidden_count,
        available_libraries,
        available_launchers,
        query_fingerprint,
    })
}

fn load_game_cards(
    context: &renderpilot_orchestration::Context,
) -> Result<Vec<GameCardOutput>, ApiError> {
    let cards = orch_catalog::game_cards(context)?;

    Ok(cards
        .iter()
        .map(|card| {
            GameCardOutput::from_details(
                &card.game,
                &card.details,
                card.cover_updated_at_ms,
                card.rollback_available,
                card.is_favorite,
                card.is_hidden,
            )
        })
        .collect())
}

fn query_fingerprint(query: &QueryGameCards) -> String {
    serde_json::to_string(query).unwrap_or_else(|_| String::from("{}"))
}
