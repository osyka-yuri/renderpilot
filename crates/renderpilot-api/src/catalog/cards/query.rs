//! The query object: normalizes the request once, then matches and orders cards.

use serde::Serialize;
use std::cmp::Ordering;
use std::collections::BTreeSet;

use super::normalize::{
    normalize_search_query, normalize_selected_launchers, normalize_selected_libraries,
};
use super::output::GameCardOutput;
use super::sort::{
    compare_game_card_identity, QueryGameCardsPage, QueryGameCardsSort, QuerySortField,
};
use super::QueryGameCardsRequest;

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryGameCardsUiFilters {
    show_hidden: bool,
    favorites_only: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QueryGameCards {
    search_query: String,
    selected_libraries: Vec<String>,
    selected_launchers: Vec<String>,
    #[serde(flatten)]
    ui_filters: QueryGameCardsUiFilters,
    sort: QueryGameCardsSort,
    pub(super) page: QueryGameCardsPage,

    #[serde(skip_serializing)]
    selected_library_set: BTreeSet<String>,

    #[serde(skip_serializing)]
    selected_launcher_set: BTreeSet<String>,

    #[serde(skip_serializing)]
    has_library_filter: bool,

    #[serde(skip_serializing)]
    has_launcher_filter: bool,
}

impl QueryGameCards {
    pub(super) fn new(
        req: QueryGameCardsRequest,
        available_libraries: &[String],
        available_launchers: &[String],
    ) -> Self {
        let search_query = normalize_search_query(&req.search_query);
        let has_library_filter = !req.selected_libraries.is_empty();
        let has_launcher_filter = !req.selected_launchers.is_empty();
        let selected_libraries =
            normalize_selected_libraries(req.selected_libraries, available_libraries);
        let selected_library_set = selected_libraries.iter().cloned().collect();
        let selected_launchers =
            normalize_selected_launchers(req.selected_launchers, available_launchers);
        let selected_launcher_set = selected_launchers.iter().cloned().collect();

        Self {
            search_query,
            selected_libraries,
            selected_launchers,
            ui_filters: QueryGameCardsUiFilters {
                show_hidden: req.show_hidden,
                favorites_only: req.favorites_only,
            },
            sort: QueryGameCardsSort::new(&req.sort_field, &req.sort_direction),
            page: QueryGameCardsPage::new(req.page_limit, req.page_offset),
            selected_library_set,
            selected_launcher_set,
            has_library_filter,
            has_launcher_filter,
        }
    }

    pub(super) fn matches(&self, card: &GameCardOutput) -> bool {
        if card.is_hidden && !self.ui_filters.show_hidden {
            return false;
        }

        if self.ui_filters.favorites_only && !card.is_favorite {
            return false;
        }

        self.matches_search_query(card)
            && self.matches_selected_libraries(card)
            && self.matches_selected_launchers(card)
    }

    fn matches_search_query(&self, card: &GameCardOutput) -> bool {
        self.search_query.is_empty() || card.title_search_key.contains(&self.search_query)
    }

    fn matches_selected_libraries(&self, card: &GameCardOutput) -> bool {
        !self.has_library_filter
            || card
                .library_tags
                .iter()
                .any(|tag| self.selected_library_set.contains(tag))
    }

    fn matches_selected_launchers(&self, card: &GameCardOutput) -> bool {
        !self.has_launcher_filter || self.selected_launcher_set.contains(&card.launcher)
    }

    pub(super) fn compare(&self, left: &GameCardOutput, right: &GameCardOutput) -> Ordering {
        // Favorites always float to the top, regardless of the selected sort field.
        left.is_favorite
            .cmp(&right.is_favorite)
            .reverse()
            .then_with(|| {
                let ordering = match self.sort.field {
                    QuerySortField::Title => compare_game_card_identity(left, right),
                    QuerySortField::Updates => left
                        .update_count
                        .cmp(&right.update_count)
                        .then_with(|| compare_game_card_identity(left, right)),
                    QuerySortField::Risk => left
                        .risk_order
                        .cmp(&right.risk_order)
                        .then_with(|| compare_game_card_identity(left, right)),
                };
                self.sort.direction.apply(ordering)
            })
    }
}
