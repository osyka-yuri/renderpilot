//! The query object: normalizes the request once, then matches and orders cards.

use serde::Serialize;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

use super::QueryGameCardsRequest;
use super::normalize::{
    normalize_addon_names, normalize_search_query, normalize_selected_launchers,
    normalize_selected_libraries,
};
use super::sort::{QueryGameCardsPage, QueryGameCardsSort, QuerySortField};
use renderpilot_orchestration::catalog::GameCardData;
use renderpilot_orchestration::domain::AddonKind;

use super::output::GameCardOutput;

pub(super) trait GameCardQueryView {
    fn game_id(&self) -> &str;
    fn title(&self) -> &str;
    fn title_search_key(&self) -> &str;
    fn launcher(&self) -> &str;
    fn library_tags(&self) -> &[String];
    fn addon_capabilities(&self) -> &[AddonKind];
    fn update_count(&self) -> usize;
    fn risk_level(&self) -> renderpilot_orchestration::catalog::CatalogCardRiskLevel;
    fn is_favorite(&self) -> bool;
    fn is_hidden(&self) -> bool;
}

impl GameCardQueryView for GameCardData {
    fn game_id(&self) -> &str {
        self.game.id().as_str()
    }
    fn title(&self) -> &str {
        self.game.identity().title()
    }
    fn title_search_key(&self) -> &str {
        &self.title_search_key
    }
    fn launcher(&self) -> &str {
        self.game.identity().launcher().as_str()
    }
    fn library_tags(&self) -> &[String] {
        &self.library_tags
    }
    fn addon_capabilities(&self) -> &[AddonKind] {
        &self.addon_capabilities
    }
    fn update_count(&self) -> usize {
        self.update_count
    }
    fn risk_level(&self) -> renderpilot_orchestration::catalog::CatalogCardRiskLevel {
        self.risk_level
    }
    fn is_favorite(&self) -> bool {
        self.is_favorite
    }
    fn is_hidden(&self) -> bool {
        self.is_hidden
    }
}

impl GameCardQueryView for GameCardOutput {
    fn game_id(&self) -> &str {
        &self.game_id
    }
    fn title(&self) -> &str {
        &self.title
    }
    fn title_search_key(&self) -> &str {
        &self.title_search_key
    }
    fn launcher(&self) -> &str {
        &self.launcher
    }
    fn library_tags(&self) -> &[String] {
        &self.library_tags
    }
    fn addon_capabilities(&self) -> &[AddonKind] {
        &self.addon_capabilities
    }
    fn update_count(&self) -> usize {
        self.update_count
    }
    fn risk_level(&self) -> renderpilot_orchestration::catalog::CatalogCardRiskLevel {
        self.risk_order
    }
    fn is_favorite(&self) -> bool {
        self.is_favorite
    }
    fn is_hidden(&self) -> bool {
        self.is_hidden
    }
}

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
    selected_addons: Vec<String>,
    selected_launchers: Vec<String>,
    launcher_order: Vec<String>,
    #[serde(flatten)]
    ui_filters: QueryGameCardsUiFilters,
    sort: QueryGameCardsSort,
    pub(super) page: QueryGameCardsPage,

    #[serde(skip_serializing)]
    selected_library_set: BTreeSet<String>,

    #[serde(skip_serializing)]
    selected_addon_set: BTreeSet<String>,

    #[serde(skip_serializing)]
    selected_launcher_set: BTreeSet<String>,

    #[serde(skip_serializing)]
    has_library_filter: bool,

    #[serde(skip_serializing)]
    has_addon_filter: bool,

    #[serde(skip_serializing)]
    has_launcher_filter: bool,

    #[serde(skip_serializing)]
    launcher_rank: HashMap<String, usize>,
}

impl QueryGameCards {
    pub(super) fn new(req: QueryGameCardsRequest, available_launchers: &[String]) -> Self {
        let search_query = normalize_search_query(&req.search_query);
        let selected_libraries = normalize_selected_libraries(req.selected_libraries);
        let has_library_filter = !selected_libraries.is_empty();
        let selected_library_set = selected_libraries.iter().cloned().collect();
        let selected_addons = normalize_addon_names(req.selected_addons);
        let has_addon_filter = !selected_addons.is_empty();
        let selected_addon_set = selected_addons.iter().cloned().collect();
        let selected_launchers = normalize_selected_launchers(req.selected_launchers);
        let has_launcher_filter = !selected_launchers.is_empty();
        let selected_launcher_set = selected_launchers.iter().cloned().collect();
        let mut launcher_order = Vec::with_capacity(available_launchers.len());
        for launcher in req.launcher_order {
            let launcher = launcher.trim();
            if available_launchers
                .iter()
                .any(|available| available == launcher)
                && !launcher_order.iter().any(|current| current == launcher)
            {
                launcher_order.push(launcher.to_owned());
            }
        }
        for launcher in available_launchers {
            if !launcher_order.contains(launcher) {
                launcher_order.push(launcher.clone());
            }
        }
        let launcher_rank = launcher_order
            .iter()
            .enumerate()
            .map(|(rank, launcher)| (launcher.clone(), rank))
            .collect();

        Self {
            search_query,
            selected_libraries,
            selected_addons,
            selected_launchers,
            launcher_order,
            ui_filters: QueryGameCardsUiFilters {
                show_hidden: req.show_hidden,
                favorites_only: req.favorites_only,
            },
            sort: QueryGameCardsSort::new(&req.sort_field, &req.sort_direction),
            page: QueryGameCardsPage::new(req.page_limit, req.page_offset),
            selected_library_set,
            selected_addon_set,
            selected_launcher_set,
            has_library_filter,
            has_addon_filter,
            has_launcher_filter,
            launcher_rank,
        }
    }

    pub(super) fn matches(&self, card: &impl GameCardQueryView) -> bool {
        if card.is_hidden() && !self.ui_filters.show_hidden {
            return false;
        }

        if self.ui_filters.favorites_only && !card.is_favorite() {
            return false;
        }

        let matches_library_or_addon = if self.has_library_filter && self.has_addon_filter {
            self.matches_selected_libraries(card) || self.matches_selected_addons(card)
        } else if self.has_library_filter {
            self.matches_selected_libraries(card)
        } else if self.has_addon_filter {
            self.matches_selected_addons(card)
        } else {
            true
        };

        self.matches_search_query(card)
            && self.matches_selected_launchers(card)
            && matches_library_or_addon
    }

    fn matches_search_query(&self, card: &impl GameCardQueryView) -> bool {
        self.search_query.is_empty() || card.title_search_key().contains(&self.search_query)
    }

    fn matches_selected_libraries(&self, card: &impl GameCardQueryView) -> bool {
        !self.has_library_filter
            || card
                .library_tags()
                .iter()
                .any(|tag| self.selected_library_set.contains(tag))
    }

    fn matches_selected_addons(&self, card: &impl GameCardQueryView) -> bool {
        !self.has_addon_filter
            || card
                .addon_capabilities()
                .iter()
                .any(|kind| self.selected_addon_set.contains(kind.as_str()))
    }

    fn matches_selected_launchers(&self, card: &impl GameCardQueryView) -> bool {
        !self.has_launcher_filter || self.selected_launcher_set.contains(card.launcher())
    }

    pub(super) fn compare(
        &self,
        left: &impl GameCardQueryView,
        right: &impl GameCardQueryView,
    ) -> Ordering {
        self.launcher_rank
            .get(left.launcher())
            .unwrap_or(&usize::MAX)
            .cmp(
                self.launcher_rank
                    .get(right.launcher())
                    .unwrap_or(&usize::MAX),
            )
            // Favorites float to the top inside each launcher group.
            .then_with(|| left.is_favorite()
            .cmp(&right.is_favorite())
            .reverse())
            .then_with(|| {
                let ordering = match self.sort.field {
                    QuerySortField::Title => left.title().cmp(right.title()),
                    QuerySortField::Updates => left
                        .update_count()
                        .cmp(&right.update_count())
                        .then_with(|| left.title().cmp(right.title())),
                    QuerySortField::Risk => left
                        .risk_level()
                        .cmp(&right.risk_level())
                        .then_with(|| left.title().cmp(right.title())),
                };
                self.sort.direction.apply(ordering)
            })
            .then_with(|| left.game_id().cmp(right.game_id()))
    }

    pub(super) fn selected_libraries(&self) -> &[String] {
        &self.selected_libraries
    }

    pub(super) fn selected_addons(&self) -> &[String] {
        &self.selected_addons
    }

    pub(super) fn selected_launchers(&self) -> &[String] {
        &self.selected_launchers
    }
}
