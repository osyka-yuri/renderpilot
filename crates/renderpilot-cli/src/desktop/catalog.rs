use renderpilot_domain::GameId;
use renderpilot_storage_sqlite::SqliteStorage;
use serde::Serialize;
use serde_json::Value;
use std::{cmp::Ordering, collections::BTreeSet};

use super::utils::{
    available_update_count, dashboard_risk_level, library_tags, to_json, DashboardRiskLevel,
    JsonResult,
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

    let query = QueryGameCards::new(
        search_query,
        selected_libraries,
        sort_field,
        sort_direction,
        page_limit,
        page_offset,
        &available_libraries,
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

    backup_available: bool,
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
    query_fingerprint: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryGameCards {
    search_query: String,
    selected_libraries: Vec<String>,
    sort: QueryGameCardsSort,
    page: QueryGameCardsPage,

    #[serde(skip_serializing)]
    selected_library_set: BTreeSet<String>,
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
        sort_field: String,
        sort_direction: String,
        page_limit: i64,
        page_offset: i64,
        available_libraries: &[String],
    ) -> Self {
        let search_query = normalize_search_query(search_query);
        let selected_libraries =
            normalize_selected_libraries(selected_libraries, available_libraries);
        let selected_library_set = selected_libraries.iter().cloned().collect();

        Self {
            search_query,
            selected_libraries,
            selected_library_set,
            sort: QueryGameCardsSort::new(&sort_field, &sort_direction),
            page: QueryGameCardsPage::new(page_limit, page_offset),
        }
    }

    fn matches(&self, card: &GameCardOutput) -> bool {
        self.matches_search_query(card) && self.matches_selected_libraries(card)
    }

    fn matches_search_query(&self, card: &GameCardOutput) -> bool {
        self.search_query.is_empty() || card.title_search_key.contains(&self.search_query)
    }

    fn matches_selected_libraries(&self, card: &GameCardOutput) -> bool {
        self.selected_library_set.is_empty()
            || card
                .library_tags
                .iter()
                .any(|tag| self.selected_library_set.contains(tag))
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
            backup_available: metrics.backup_available,
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
    backup_available: bool,
    operation_count: usize,
    last_operation_status: Option<String>,
}

impl GameCardMetrics {
    fn from_details(details: &catalog::GameDetailsCatalogResult) -> Self {
        let operation_entries = &details.operations.operations;

        Self {
            library_tags: library_tags(&details.components),
            component_count: details.components.len(),
            available_update_count: available_update_count(&details.candidate_groups),
            risk_level: dashboard_risk_level(&details.components),
            backup_available: operation_entries.iter().any(|entry| entry.backup_count > 0),
            operation_count: operation_entries.len(),
            last_operation_status: operation_entries
                .iter()
                .max_by_key(|entry| entry.operation.created_at.as_i64())
                .map(|entry| entry.operation.status.as_str().to_owned()),
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct GameDetailsOutput {
    game: renderpilot_domain::GameInstallation,
    components: Vec<renderpilot_domain::GraphicsComponent>,
    candidate_groups: Value,
    operations: Value,
}

impl GameDetailsOutput {
    pub(crate) fn load(game_id: GameId) -> Result<Self, CliError> {
        let details = catalog::get_game_details(game_id)?;
        let candidate_groups = output::candidate_groups_value(details.candidate_groups)?;
        let operations = output::operation_summaries_value(&details.operations)?;

        Ok(Self {
            game: details.game,
            components: details.components,
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
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    normalized.sort();
    normalized.dedup();
    normalized
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
