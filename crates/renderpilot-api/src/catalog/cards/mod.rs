//! Game-card listing: backend-owned filtering, sorting, and paging over the
//! dashboard's game cards, plus the card DTO the GUI renders.

use renderpilot_orchestration::catalog as orch_catalog;

use crate::ApiError;
use crate::utils::{JsonResult, to_json};

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
    #[serde(default)]
    pub search_query: String,
    /// List of selected library tags to filter by.
    #[serde(default)]
    pub selected_libraries: Vec<String>,
    /// List of selected add-on capability tags to filter by.
    #[serde(default)]
    pub selected_addons: Vec<String>,
    /// List of selected launcher names to filter by.
    #[serde(default)]
    pub selected_launchers: Vec<String>,
    /// Whether to show games marked as hidden.
    #[serde(default)]
    pub show_hidden: bool,
    /// Whether to only show games marked as favorite.
    #[serde(default)]
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
                card.addon_capabilities.clone(),
            )
        })
        .collect())
}

/// Stable cache key for a game-card query.
///
/// Format: `v1:` + canonical JSON of the normalized query. All fields of
/// [`QueryGameCards`] participate; collections keep insertion order as built by
/// the request normalizers. Bump the version prefix when the schema of the
/// serialized query changes so old keys cannot be reused.
///
/// Serialization is treated as infallible: `QueryGameCards` is composed of
/// owned strings, ints, and bools with derive-generated `Serialize`. A failure
/// would indicate a programming bug, not a user-facing error condition.
#[must_use]
fn query_fingerprint(query: &QueryGameCards) -> String {
    let body = serde_json::to_string(query)
        .expect("QueryGameCards Serialize is infallible for owned primitive fields");
    format!("v1:{body}")
}

#[cfg(test)]
mod fingerprint_tests {
    use super::*;

    fn sample_request(
        search: &str,
        libraries: &[&str],
        launchers: &[&str],
    ) -> QueryGameCardsRequest {
        QueryGameCardsRequest {
            search_query: search.to_owned(),
            selected_libraries: libraries.iter().map(|s| (*s).to_owned()).collect(),
            selected_addons: Vec::new(),
            selected_launchers: launchers.iter().map(|s| (*s).to_owned()).collect(),
            show_hidden: false,
            favorites_only: false,
            sort_field: String::from("title"),
            sort_direction: String::from("asc"),
            page_limit: 50,
            page_offset: 0,
        }
    }

    #[test]
    fn fingerprint_is_versioned_and_stable_for_same_query() {
        let available_libs = [String::from("dlss_super_resolution")];
        let available_launchers = [String::from("Steam")];
        let query = QueryGameCards::new(
            sample_request("doom", &["dlss_super_resolution"], &["Steam"]),
            &available_libs,
            &available_launchers,
        );
        let first = query_fingerprint(&query);
        let second = query_fingerprint(&query);
        assert_eq!(first, second);
        assert!(
            first.starts_with("v1:"),
            "fingerprint must be version-prefixed: {first}"
        );
        assert_ne!(first, "v1:{}");
    }

    #[test]
    fn fingerprint_differs_for_semantically_different_queries() {
        let available_libs = [String::from("dlss_super_resolution")];
        let available_launchers = [String::from("Steam"), String::from("Epic")];
        let a = QueryGameCards::new(
            sample_request("doom", &["dlss_super_resolution"], &["Steam"]),
            &available_libs,
            &available_launchers,
        );
        let b = QueryGameCards::new(
            sample_request("doom", &["dlss_super_resolution"], &["Epic"]),
            &available_libs,
            &available_launchers,
        );
        assert_ne!(query_fingerprint(&a), query_fingerprint(&b));
    }

    #[test]
    fn fingerprint_ignores_non_semantic_selection_order() {
        // Normalizers sort/filter every filter collection, so insertion order
        // must not change the key after normalization.
        let available_libs = [
            String::from("dlss_super_resolution"),
            String::from("amd_fsr"),
        ];
        let available_launchers = [String::from("Steam"), String::from("Epic")];
        let mut first = sample_request(
            "",
            &["amd_fsr", "dlss_super_resolution"],
            &["Epic", "Steam"],
        );
        first.selected_addons = vec![String::from("renodx"), String::from("renodx")];
        let mut second = sample_request(
            "",
            &["dlss_super_resolution", "amd_fsr"],
            &["Steam", "Epic"],
        );
        second.selected_addons = vec![String::from("renodx")];

        let a = QueryGameCards::new(first, &available_libs, &available_launchers);
        let b = QueryGameCards::new(second, &available_libs, &available_launchers);
        assert_eq!(query_fingerprint(&a), query_fingerprint(&b));
    }
}
