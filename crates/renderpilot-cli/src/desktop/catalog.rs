use renderpilot_application::ComponentFileReplacementCandidates;
use renderpilot_domain::{GameId, GraphicsComponent, GraphicsTechnology};
use renderpilot_storage_sqlite::SqliteStorage;
use serde::Serialize;
use serde_json::Value;
use std::{cmp::Ordering, collections::BTreeSet, path::Path};

use super::utils::{
    available_update_count, dashboard_risk_level, is_visible_graphics_technology, library_tags,
    to_json, DashboardRiskLevel, JsonResult,
};
use crate::{catalog, output, CliError};

/// Lists all games currently stored in the local catalog.
pub fn list_games() -> JsonResult {
    to_json(GameListOutput {
        games: catalog::list_games()?,
    })
}

/// Queries game cards with backend-owned filtering, sorting, and paging semantics.
pub fn query_game_cards(
    search_query: String,
    selected_libraries: Vec<String>,
    selected_launchers: Vec<String>,
    sort_field: String,
    sort_direction: String,
    page_limit: i64,
    page_offset: i64,
) -> JsonResult {
    let storage = catalog::open_catalog_storage()?;
    let cards = load_game_cards(&storage)?;

    let available_libraries = normalize_library_names(
        storage
            .list_distinct_game_libraries()
            .map_err(CliError::from)?,
    );
    let available_launchers = normalize_launcher_names(
        storage
            .list_distinct_game_launchers()
            .map_err(CliError::from)?,
    );

    let query = QueryGameCards::new(
        search_query,
        selected_libraries,
        selected_launchers,
        sort_field,
        sort_direction,
        page_limit,
        page_offset,
        &available_libraries,
        &available_launchers,
    );

    let query_fingerprint = query_fingerprint(&query);

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
        available_libraries,
        available_launchers,
        query_fingerprint,
    })
}

fn load_game_cards(storage: &SqliteStorage) -> Result<Vec<GameCardOutput>, CliError> {
    let games = storage.list_games()?;
    let covers_by_game = storage.list_all_game_covers().map_err(CliError::from)?;

    let mut cards = Vec::with_capacity(games.len());

    for game in &games {
        let details = catalog::get_game_details_with_storage(storage, game.id().clone())?;
        let cover_updated_at_ms = covers_by_game
            .get(game.id())
            .map(|record| record.updated_at_ms);

        cards.push(GameCardOutput::from_details(
            game,
            &details,
            cover_updated_at_ms,
        ));
    }

    Ok(cards)
}

/// Loads one game with detected components, candidates, and operation history.
pub fn get_game_details(game_id: impl Into<String>) -> JsonResult {
    let game_id = super::utils::parse_game_id(game_id.into())?;

    to_json(GameDetailsOutput::load(game_id)?)
}

#[derive(Debug, Serialize)]
struct GameListOutput {
    games: Vec<renderpilot_domain::GameInstallation>,
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
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryGameCardsOutput {
    items: Vec<GameCardOutput>,
    total: usize,
    available_libraries: Vec<String>,
    available_launchers: Vec<String>,
    query_fingerprint: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryGameCards {
    search_query: String,
    selected_libraries: Vec<String>,
    selected_launchers: Vec<String>,
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
    #[allow(clippy::too_many_arguments)]
    fn new(
        search_query: String,
        selected_libraries: Vec<String>,
        selected_launchers: Vec<String>,
        sort_field: String,
        sort_direction: String,
        page_limit: i64,
        page_offset: i64,
        available_libraries: &[String],
        available_launchers: &[String],
    ) -> Self {
        let search_query = normalize_search_query(search_query);
        let has_library_filter = !selected_libraries.is_empty();
        let has_launcher_filter = !selected_launchers.is_empty();
        let selected_libraries =
            normalize_selected_libraries(selected_libraries, available_libraries);
        let selected_library_set = selected_libraries.iter().cloned().collect();
        let selected_launchers =
            normalize_selected_launchers(selected_launchers, available_launchers);
        let selected_launcher_set = selected_launchers.iter().cloned().collect();

        Self {
            search_query,
            selected_libraries,
            selected_launchers,
            sort: QueryGameCardsSort::new(&sort_field, &sort_direction),
            page: QueryGameCardsPage::new(page_limit, page_offset),
            selected_library_set,
            selected_launcher_set,
            has_library_filter,
            has_launcher_filter,
        }
    }

    fn matches(&self, card: &GameCardOutput) -> bool {
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
        game: &renderpilot_domain::GameInstallation,
        details: &catalog::GameDetailsCatalogResult,
        cover_updated_at_ms: Option<i64>,
    ) -> Self {
        let identity = game.identity();
        let title = identity.title().to_owned();
        let metrics = GameCardMetrics::from_details(details);

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
    fn from_details(details: &catalog::GameDetailsCatalogResult) -> Self {
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
            rollback_available: details.components.iter().any(|component| {
                component.files().iter().any(|file| {
                    let bak_path = format!("{}.bak", file.path().as_str());
                    Path::new(&bak_path).exists()
                })
            }),
            operation_count: operation_entries.len(),
            last_operation_status: operation_entries
                .iter()
                .max_by_key(|entry| entry.operation.created_at.as_i64())
                .map(|entry| entry.operation.status.as_str().to_owned()),
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct GameComponentOutput {
    #[serde(flatten)]
    component: GraphicsComponent,
    rollback_available: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct GameDetailsOutput {
    game: renderpilot_domain::GameInstallation,
    components: Vec<GameComponentOutput>,
    candidate_groups: Value,
    operations: Value,
}

impl GameDetailsOutput {
    pub(crate) fn load(game_id: GameId) -> Result<Self, CliError> {
        let details = catalog::get_game_details(game_id)?;
        let visible_components = filter_visible_components(details.components);
        let visible_component_ids = visible_component_ids(&visible_components);
        let visible_candidate_groups =
            filter_visible_candidate_groups(details.candidate_groups, &visible_component_ids);
        let candidate_groups = output::candidate_groups_value(visible_candidate_groups)?;
        let operations = output::operation_summaries_value(&details.operations)?;

        let components = visible_components
            .into_iter()
            .map(|component| {
                let rollback_available = component.files().iter().any(|file| {
                    let bak_path = format!("{}.bak", file.path().as_str());
                    Path::new(&bak_path).exists()
                });
                GameComponentOutput {
                    component,
                    rollback_available,
                }
            })
            .collect();

        Ok(Self {
            game: details.game,
            components,
            candidate_groups,
            operations,
        })
    }
}

/// Reads one persisted catalog settings value (typically used for integration keys).
pub fn get_catalog_setting(key: impl Into<String>) -> JsonResult {
    let key = key.into();
    let storage = catalog::open_catalog_storage()?;
    let value = storage.get_setting(&key).map_err(CliError::from)?;

    to_json(serde_json::json!({ "value": value }))
}

/// Upserts a persisted catalog settings value, or deletes the row when `value` is blank after trim.
pub fn set_catalog_setting(key: impl Into<String>, value: impl Into<String>) -> JsonResult {
    let key = key.into();
    let value = value.into();

    let storage = catalog::open_catalog_storage()?;

    if value.trim().is_empty() {
        storage.delete_setting(&key).map_err(CliError::from)?;
    } else {
        storage.set_setting(&key, &value).map_err(CliError::from)?;
    }

    to_json(serde_json::json!({ "saved": true }))
}

fn normalize_search_query(value: String) -> String {
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
        .filter_map(normalize_library_name)
        .collect::<Vec<_>>();

    normalized.sort();
    normalized.dedup();
    normalized
}

fn normalize_library_name(value: String) -> Option<String> {
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

fn filter_visible_components(components: Vec<GraphicsComponent>) -> Vec<GraphicsComponent> {
    components
        .into_iter()
        .filter(|component| is_visible_graphics_technology(component.technology()))
        .collect()
}

fn filter_visible_candidate_groups(
    candidate_groups: Vec<ComponentFileReplacementCandidates>,
    visible_component_ids: &BTreeSet<String>,
) -> Vec<ComponentFileReplacementCandidates> {
    candidate_groups
        .into_iter()
        .filter(|group| visible_component_ids.contains(group.component_id().as_str()))
        .collect()
}

fn visible_component_ids(components: &[GraphicsComponent]) -> BTreeSet<String> {
    components
        .iter()
        .filter(|component| is_visible_graphics_technology(component.technology()))
        .map(|component| component.id().as_str().to_owned())
        .collect()
}

fn visible_component_count(components: &[GraphicsComponent]) -> usize {
    components
        .iter()
        .filter(|component| is_visible_graphics_technology(component.technology()))
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
    use super::{normalize_library_name, GameCardOutput, QueryGameCards};
    use crate::desktop::utils::DashboardRiskLevel;

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
        }
    }

    #[test]
    fn normalize_library_name_keeps_current_slugs_and_drops_unknown() {
        assert_eq!(
            normalize_library_name(String::from(" dlss_super_resolution ")),
            Some(String::from("dlss_super_resolution")),
        );
        assert_eq!(normalize_library_name(String::from("unknown")), None);
        assert_eq!(normalize_library_name(String::from("   ")), None);
    }

    #[test]
    fn normalize_library_name_rejects_legacy_and_non_slug_values() {
        assert_eq!(normalize_library_name(String::from("IntelXeLl")), None);
        assert_eq!(normalize_library_name(String::from("steam")), None);
    }

    #[test]
    fn empty_selected_launchers_matches_all_cards() {
        let query = QueryGameCards::new(
            String::new(),
            Vec::new(),
            Vec::new(),
            String::from("title"),
            String::from("asc"),
            100,
            0,
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
            String::new(),
            Vec::new(),
            vec![String::from("Steam")],
            String::from("title"),
            String::from("asc"),
            100,
            0,
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
            String::new(),
            Vec::new(),
            vec![String::from("Epic")],
            String::from("title"),
            String::from("asc"),
            100,
            0,
            &[String::from("steam")],
            &[String::from("Steam")],
        );
        let steam_card = stub_card("Steam", &["steam"]);

        assert!(!query.matches(&steam_card));
    }

    #[test]
    fn empty_selected_libraries_matches_all_cards() {
        let query = QueryGameCards::new(
            String::new(),
            Vec::new(),
            Vec::new(),
            String::from("title"),
            String::from("asc"),
            100,
            0,
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
            String::new(),
            vec![String::from("dlss_super_resolution")],
            Vec::new(),
            String::from("title"),
            String::from("asc"),
            100,
            0,
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
            String::new(),
            vec![String::from("intel_xess")],
            Vec::new(),
            String::from("title"),
            String::from("asc"),
            100,
            0,
            &[String::from("dlss_super_resolution")],
            &[String::from("Steam")],
        );
        let dlss_card = stub_card("Steam", &["dlss_super_resolution"]);

        assert!(!query.matches(&dlss_card));
    }
}
