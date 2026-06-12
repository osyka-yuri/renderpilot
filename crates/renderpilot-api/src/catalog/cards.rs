//! Game-card listing: backend-owned filtering, sorting, and paging over the
//! dashboard's game cards, plus the card DTO the GUI renders.

use renderpilot_orchestration::catalog as orch_catalog;
use renderpilot_orchestration::domain::{GraphicsComponent, GraphicsTechnology};
use serde::Serialize;
use std::{cmp::Ordering, collections::BTreeSet};

use super::{is_component_visible, visible_component_ids};
use crate::utils::{
    available_update_count, dashboard_risk_level, library_tags, to_json, DashboardRiskLevel,
    JsonResult,
};
use crate::ApiError;

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

#[derive(Debug, Serialize)]
struct GameListOutput {
    games: Vec<renderpilot_orchestration::domain::GameInstallation>,
}

#[derive(Debug, Clone, Serialize)]
struct GameCardOutput {
    game_id: String,
    title: String,

    #[serde(skip_serializing)]
    title_search_key: String,

    launcher: String,
    platform: String,
    runtime: String,
    install_path: String,
    external_id: Option<String>,
    library_tags: Vec<String>,
    component_count: usize,
    updates_available: bool,
    update_count: usize,
    risk_level: String,

    #[serde(skip_serializing)]
    risk_order: DashboardRiskLevel,

    rollback_available: bool,
    operation_count: usize,
    last_operation_status: Option<String>,
    cover_updated_at_ms: Option<i64>,
    is_favorite: bool,
    is_hidden: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryGameCardsOutput {
    items: Vec<GameCardOutput>,
    total: usize,
    hidden_count: usize,
    available_libraries: Vec<String>,
    available_launchers: Vec<String>,
    query_fingerprint: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryGameCardsUiFilters {
    show_hidden: bool,
    favorites_only: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryGameCards {
    search_query: String,
    selected_libraries: Vec<String>,
    selected_launchers: Vec<String>,
    #[serde(flatten)]
    ui_filters: QueryGameCardsUiFilters,
    sort: QueryGameCardsSort,
    page: QueryGameCardsPage,

    #[serde(skip_serializing)]
    selected_library_set: BTreeSet<String>,

    #[serde(skip_serializing)]
    selected_launcher_set: BTreeSet<String>,

    #[serde(skip_serializing)]
    has_library_filter: bool,

    #[serde(skip_serializing)]
    has_launcher_filter: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryGameCardsSort {
    field: QuerySortField,
    direction: QuerySortDirection,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryGameCardsPage {
    limit: usize,
    offset: usize,
}

impl QueryGameCardsSort {
    fn new(field: &str, direction: &str) -> Self {
        Self {
            field: QuerySortField::from_input(field),
            direction: QuerySortDirection::from_input(direction),
        }
    }
}

impl QueryGameCardsPage {
    fn new(limit: i64, offset: i64) -> Self {
        Self {
            limit: normalize_page_limit(limit),
            offset: normalize_page_offset(offset),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum QuerySortField {
    Title,
    Updates,
    Risk,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum QuerySortDirection {
    Asc,
    Desc,
}

impl QueryGameCards {
    fn new(
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

    fn matches(&self, card: &GameCardOutput) -> bool {
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

    fn compare(&self, left: &GameCardOutput, right: &GameCardOutput) -> Ordering {
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

impl QuerySortField {
    fn from_input(value: &str) -> Self {
        match value.trim() {
            value if value.eq_ignore_ascii_case("updates") => Self::Updates,
            value if value.eq_ignore_ascii_case("risk") => Self::Risk,
            _ => Self::Title,
        }
    }
}

impl QuerySortDirection {
    fn from_input(value: &str) -> Self {
        if value.trim().eq_ignore_ascii_case("desc") {
            Self::Desc
        } else {
            Self::Asc
        }
    }

    fn apply(self, ordering: Ordering) -> Ordering {
        match self {
            Self::Asc => ordering,
            Self::Desc => ordering.reverse(),
        }
    }
}

impl GameCardOutput {
    fn from_details(
        game: &renderpilot_orchestration::domain::GameInstallation,
        details: &orch_catalog::GameDetailsCatalogResult,
        cover_updated_at_ms: Option<i64>,
        rollback_available: bool,
        is_favorite: bool,
        is_hidden: bool,
    ) -> Self {
        let identity = game.identity();
        let title = identity.title().to_owned();
        let metrics = GameCardMetrics::from_details(details, rollback_available);

        Self {
            game_id: game.id().as_str().to_owned(),
            title_search_key: title.to_lowercase(),
            title,
            launcher: identity.launcher().as_str().to_owned(),
            platform: game.platform().as_str().to_owned(),
            runtime: game.runtime().as_str().to_owned(),
            install_path: game.install_path().as_str().to_owned(),
            external_id: identity.external_id().map(str::to_owned),
            library_tags: metrics.library_tags,
            component_count: metrics.component_count,
            updates_available: metrics.available_update_count > 0,
            update_count: metrics.available_update_count,
            risk_level: metrics.risk_level.as_str().to_owned(),
            risk_order: metrics.risk_level,
            rollback_available: metrics.rollback_available,
            operation_count: metrics.operation_count,
            last_operation_status: metrics.last_operation_status,
            cover_updated_at_ms,
            is_favorite,
            is_hidden,
        }
    }
}

struct GameCardMetrics {
    library_tags: Vec<String>,
    component_count: usize,
    available_update_count: usize,
    risk_level: DashboardRiskLevel,
    rollback_available: bool,
    operation_count: usize,
    last_operation_status: Option<String>,
}

impl GameCardMetrics {
    fn from_details(
        details: &orch_catalog::GameDetailsCatalogResult,
        rollback_available: bool,
    ) -> Self {
        let operation_entries = &details.operations.operations;
        let visible_component_ids = visible_component_ids(&details.components);

        Self {
            library_tags: library_tags(&details.components),
            component_count: visible_component_count(&details.components),
            available_update_count: available_update_count(
                details
                    .candidate_groups
                    .iter()
                    .filter(|group| visible_component_ids.contains(group.component_id().as_str())),
            ),
            risk_level: dashboard_risk_level(&details.components),
            rollback_available,
            operation_count: operation_entries.len(),
            last_operation_status: operation_entries
                .iter()
                .max_by_key(|entry| entry.operation.created_at.as_i64())
                .map(|entry| entry.operation.status.as_str().to_owned()),
        }
    }
}

fn normalize_search_query(value: &str) -> String {
    value.trim().to_lowercase()
}

fn normalize_page_limit(value: i64) -> usize {
    usize::try_from(value.max(1)).unwrap_or(usize::MAX)
}

fn normalize_page_offset(value: i64) -> usize {
    usize::try_from(value.max(0)).unwrap_or(usize::MAX)
}

fn normalize_library_names(values: Vec<String>) -> Vec<String> {
    let mut normalized = values
        .into_iter()
        .filter_map(|value| normalize_library_name(&value))
        .collect::<Vec<_>>();

    normalized.sort();
    normalized.dedup();
    normalized
}

fn normalize_library_name(value: &str) -> Option<String> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        return None;
    }

    match parse_graphics_technology(trimmed) {
        Some(GraphicsTechnology::Unknown) => None,
        Some(technology) => Some(technology.as_slug().to_owned()),
        None => None,
    }
}

fn normalize_selected_libraries(
    selected_libraries: Vec<String>,
    available_libraries: &[String],
) -> Vec<String> {
    if available_libraries.is_empty() {
        return Vec::new();
    }

    let allowed_libraries = available_libraries
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    let mut selected_libraries = normalize_library_names(selected_libraries);

    selected_libraries.retain(|library| allowed_libraries.contains(library.as_str()));

    selected_libraries
}

fn normalize_launcher_names(values: Vec<String>) -> Vec<String> {
    let mut normalized = values
        .into_iter()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    normalized.sort();
    normalized.dedup();
    normalized
}

fn normalize_selected_launchers(
    selected_launchers: Vec<String>,
    available_launchers: &[String],
) -> Vec<String> {
    if available_launchers.is_empty() {
        return Vec::new();
    }

    let allowed_launchers = available_launchers
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    let mut selected_launchers = normalize_launcher_names(selected_launchers);

    selected_launchers.retain(|launcher| allowed_launchers.contains(launcher.as_str()));

    selected_launchers
}

fn parse_graphics_technology(value: &str) -> Option<GraphicsTechnology> {
    GraphicsTechnology::from_slug(value)
}

fn visible_component_count(components: &[GraphicsComponent]) -> usize {
    components
        .iter()
        .filter(|component| is_component_visible(component))
        .count()
}

fn compare_game_card_identity(left: &GameCardOutput, right: &GameCardOutput) -> Ordering {
    left.title
        .cmp(&right.title)
        .then_with(|| left.game_id.cmp(&right.game_id))
}

fn page_items(items: &[GameCardOutput], page: QueryGameCardsPage) -> Vec<GameCardOutput> {
    let total = items.len();
    let start = page.offset.min(total);
    let end = start.saturating_add(page.limit).min(total);

    items[start..end].to_vec()
}

fn query_fingerprint(query: &QueryGameCards) -> String {
    serde_json::to_string(query).unwrap_or_else(|_| String::from("{}"))
}

#[cfg(test)]
mod tests {
    use super::{normalize_library_name, GameCardOutput, QueryGameCards, QueryGameCardsRequest};
    use crate::utils::DashboardRiskLevel;

    fn stub_card(launcher: &str, library_tags: &[&str]) -> GameCardOutput {
        GameCardOutput {
            game_id: String::from("test-id"),
            title: String::from("Test Game"),
            title_search_key: String::from("test game"),
            launcher: String::from(launcher),
            platform: String::from("windows"),
            runtime: String::from("dx11"),
            install_path: String::from("/games/test"),
            external_id: None,
            library_tags: library_tags.iter().map(|&t| String::from(t)).collect(),
            component_count: 1,
            updates_available: false,
            update_count: 0,
            risk_level: String::from("safe"),
            risk_order: DashboardRiskLevel::Low,
            rollback_available: false,
            operation_count: 0,
            last_operation_status: None,
            cover_updated_at_ms: None,
            is_favorite: false,
            is_hidden: false,
        }
    }

    #[test]
    fn normalize_library_name_keeps_current_slugs_and_drops_unknown() {
        assert_eq!(
            normalize_library_name(" dlss_super_resolution "),
            Some(String::from("dlss_super_resolution")),
        );
        assert_eq!(normalize_library_name("unknown"), None);
        assert_eq!(normalize_library_name("   "), None);
    }

    #[test]
    fn normalize_library_name_rejects_legacy_and_non_slug_values() {
        assert_eq!(normalize_library_name("IntelXeLl"), None);
        assert_eq!(normalize_library_name("steam"), None);
    }

    #[test]
    fn empty_selected_launchers_matches_all_cards() {
        let query = QueryGameCards::new(
            QueryGameCardsRequest {
                search_query: String::new(),
                selected_libraries: Vec::new(),
                selected_launchers: Vec::new(),
                show_hidden: false,
                favorites_only: false,
                sort_field: String::from("title"),
                sort_direction: String::from("asc"),
                page_limit: 100,
                page_offset: 0,
            },
            &[String::from("steam")],
            &[String::from("Steam")],
        );
        let steam_card = stub_card("Steam", &["steam"]);
        let epic_card = stub_card("Epic", &["epic"]);

        assert!(query.matches(&steam_card));
        assert!(query.matches(&epic_card));
    }

    #[test]
    fn selected_launcher_matches_only_same_launcher() {
        let query = QueryGameCards::new(
            QueryGameCardsRequest {
                search_query: String::new(),
                selected_libraries: Vec::new(),
                selected_launchers: vec![String::from("Steam")],
                show_hidden: false,
                favorites_only: false,
                sort_field: String::from("title"),
                sort_direction: String::from("asc"),
                page_limit: 100,
                page_offset: 0,
            },
            &[String::from("steam")],
            &[String::from("Steam")],
        );
        let steam_card = stub_card("Steam", &["steam"]);
        let epic_card = stub_card("Epic", &["epic"]);

        assert!(query.matches(&steam_card));
        assert!(!query.matches(&epic_card));
    }

    #[test]
    fn selected_launcher_not_in_available_excludes_all() {
        let query = QueryGameCards::new(
            QueryGameCardsRequest {
                search_query: String::new(),
                selected_libraries: Vec::new(),
                selected_launchers: vec![String::from("Epic")],
                show_hidden: false,
                favorites_only: false,
                sort_field: String::from("title"),
                sort_direction: String::from("asc"),
                page_limit: 100,
                page_offset: 0,
            },
            &[String::from("steam")],
            &[String::from("Steam")],
        );
        let steam_card = stub_card("Steam", &["steam"]);

        assert!(!query.matches(&steam_card));
    }

    #[test]
    fn empty_selected_libraries_matches_all_cards() {
        let query = QueryGameCards::new(
            QueryGameCardsRequest {
                search_query: String::new(),
                selected_libraries: Vec::new(),
                selected_launchers: Vec::new(),
                show_hidden: false,
                favorites_only: false,
                sort_field: String::from("title"),
                sort_direction: String::from("asc"),
                page_limit: 100,
                page_offset: 0,
            },
            &[
                String::from("dlss_super_resolution"),
                String::from("intel_xess"),
            ],
            &[String::from("Steam")],
        );
        let steam_card = stub_card("Steam", &["dlss_super_resolution"]);

        assert!(query.matches(&steam_card));
    }

    #[test]
    fn selected_library_matches_only_same_library() {
        let query = QueryGameCards::new(
            QueryGameCardsRequest {
                search_query: String::new(),
                selected_libraries: vec![String::from("dlss_super_resolution")],
                selected_launchers: Vec::new(),
                show_hidden: false,
                favorites_only: false,
                sort_field: String::from("title"),
                sort_direction: String::from("asc"),
                page_limit: 100,
                page_offset: 0,
            },
            &[
                String::from("dlss_super_resolution"),
                String::from("intel_xess"),
            ],
            &[String::from("Steam")],
        );
        let dlss_card = stub_card("Steam", &["dlss_super_resolution"]);
        let xess_card = stub_card("Epic", &["intel_xess"]);

        assert!(query.matches(&dlss_card));
        assert!(!query.matches(&xess_card));
    }

    #[test]
    fn selected_library_not_in_available_excludes_all() {
        let query = QueryGameCards::new(
            QueryGameCardsRequest {
                search_query: String::new(),
                selected_libraries: vec![String::from("intel_xess")],
                selected_launchers: Vec::new(),
                show_hidden: false,
                favorites_only: false,
                sort_field: String::from("title"),
                sort_direction: String::from("asc"),
                page_limit: 100,
                page_offset: 0,
            },
            &[String::from("dlss_super_resolution")],
            &[String::from("Steam")],
        );
        let dlss_card = stub_card("Steam", &["dlss_super_resolution"]);

        assert!(!query.matches(&dlss_card));
    }
}
