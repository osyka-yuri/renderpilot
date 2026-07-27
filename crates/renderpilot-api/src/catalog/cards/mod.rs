//! Game-card listing: backend-owned filtering, sorting, and paging over the
//! dashboard's game cards, plus the card DTO the GUI renders.

use renderpilot_orchestration::catalog as orch_catalog;

use crate::ApiError;
use crate::utils::{JsonResult, to_json};

mod bootstrap_filters;
mod normalize;
mod output;
mod query;
mod sort;

#[cfg(test)]
mod tests;

use self::normalize::{
    expand_library_filter_aliases, normalize_launcher_names, normalize_library_names,
};
use self::output::{GameCardOutput, QueryGameCardsOutput};
use self::query::QueryGameCards;
use bootstrap_filters::{EffectiveGamesFilters, parse_bootstrap_filters};

const GAMES_FILTERS_SETTING_KEY: &str = "games_filters_v3";

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BootstrapGamesCatalogOutput {
    filters: EffectiveGamesFilters,
    result: QueryGameCardsOutput,
}

/// Reads persisted filters and their matching first page as one atomic UI bootstrap.
pub fn bootstrap_games_catalog(
    context: &renderpilot_orchestration::Context,
    page_limit: i64,
) -> JsonResult {
    let stored_filters = orch_catalog::get_catalog_setting(context, GAMES_FILTERS_SETTING_KEY)?;
    let persisted = parse_bootstrap_filters(stored_filters.as_deref());
    let selected_libraries = expand_library_filter_aliases(persisted.libraries.clone());
    let result = query_game_cards_output(
        context,
        QueryGameCardsRequest {
            search_query: persisted.search_query.clone(),
            selected_libraries,
            selected_addons: persisted.addons.clone(),
            selected_launchers: persisted.launchers.clone(),
            launcher_order: persisted.launcher_order.clone(),
            show_hidden: persisted.show_hidden,
            favorites_only: persisted.favorites_only,
            sort_field: "title".to_owned(),
            sort_direction: "asc".to_owned(),
            page_limit,
            page_offset: 0,
        },
    )?;
    to_json(BootstrapGamesCatalogOutput {
        filters: persisted,
        result,
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
    /// Preferred visual order of launcher groups.
    #[serde(default)]
    pub launcher_order: Vec<String>,
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
    to_json(query_game_cards_output(context, req)?)
}

fn query_game_cards_output(
    context: &renderpilot_orchestration::Context,
    req: QueryGameCardsRequest,
) -> Result<QueryGameCardsOutput, ApiError> {
    let snapshot = orch_catalog::CatalogReadService::new(context).snapshot()?;
    let catalog_size = snapshot.cards().len();
    let available_libraries = normalize_library_names(snapshot.available_libraries().to_vec());
    let available_launchers = normalize_launcher_names(snapshot.available_launchers().to_vec());

    let query = QueryGameCards::new(req, &available_launchers);

    // Count all hidden games in the catalog (before query filters) so the
    // toolbar badge always reflects the total, not just the filtered subset.
    let hidden_count = snapshot.hidden_count();
    let candidate_indices = snapshot.candidate_indices(
        query.selected_libraries(),
        query.selected_addons(),
        query.selected_launchers(),
    );
    let mut filtered = candidate_indices
        .into_iter()
        .filter_map(|index| snapshot.cards().get(index))
        .map(AsRef::as_ref)
        .filter(|card| query.matches(*card))
        .collect::<Vec<_>>();

    filtered.sort_by(|left, right| query.compare(*left, *right));

    let total = filtered.len();
    let items = filtered[query.page.bounds(total)]
        .iter()
        .map(|card| GameCardOutput::from_card(card))
        .collect();
    let next_offset = query.page.next_offset(total);

    Ok(QueryGameCardsOutput {
        items,
        catalog_size,
        total,
        hidden_count,
        available_libraries,
        available_launchers,
        catalog_revision: snapshot.revision(),
        next_offset,
    })
}

/// Rebuilds or waits for the snapshot matching the current storage generation
/// and returns the revision safe to publish in a catalog delta.
pub fn refresh_catalog_snapshot_revision(
    context: &renderpilot_orchestration::Context,
) -> Result<orch_catalog::CatalogRevision, ApiError> {
    Ok(orch_catalog::CatalogReadService::new(context)
        .refresh_snapshot()?
        .revision())
}

/// Typed result of the mandatory background validation of live card facts.
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedCatalogRefreshOutput {
    /// Revision of the atomically retained or replaced snapshot.
    pub catalog_revision: orch_catalog::CatalogRevision,
    /// Cards whose effective projection changed after filesystem validation.
    pub changed_game_ids: Vec<String>,
}

/// Validates filesystem-sensitive card facts without putting filesystem work
/// on the interactive catalog query path.
pub fn refresh_validated_catalog_snapshot(
    context: &renderpilot_orchestration::Context,
) -> Result<ValidatedCatalogRefreshOutput, ApiError> {
    let (snapshot, changed_game_ids) =
        orch_catalog::CatalogReadService::new(context).refresh_validated_snapshot()?;
    Ok(ValidatedCatalogRefreshOutput {
        catalog_revision: snapshot.revision(),
        changed_game_ids: changed_game_ids
            .into_iter()
            .map(|game_id| game_id.as_str().to_owned())
            .collect(),
    })
}
