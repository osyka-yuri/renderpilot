use renderpilot_domain::GameId;
use serde::Serialize;
use serde_json::Value;

use super::utils::{
    available_update_count, dashboard_risk_level, technology_tags, to_json, JsonResult,
};
use crate::{catalog, output, CliError};

/// Lists all games currently stored in the local catalog.
pub fn list_games() -> JsonResult {
    to_json(GameListOutput {
        games: catalog::list_games()?,
    })
}

/// Lists persisted games as lightweight cards for the desktop Games feature.
pub fn get_game_cards() -> JsonResult {
    let storage = catalog::open_catalog_storage()?;
    let games = storage.list_games()?;
    let covers_by_game = storage.list_all_game_covers().map_err(CliError::from)?;
    let mut cards = Vec::with_capacity(games.len());

    for game in &games {
        let details = catalog::get_game_details_with_storage(&storage, game.id().clone())?;
        let cover_updated_at_ms = covers_by_game
            .get(game.id())
            .map(|record| record.updated_at_ms);
        cards.push(GameCardOutput::from_details(
            game,
            &details,
            cover_updated_at_ms,
        ));
    }

    to_json(cards)
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

#[derive(Debug, Serialize)]
struct GameCardOutput {
    game_id: String,
    title: String,
    launcher: String,
    platform: String,
    runtime: String,
    install_path: String,
    external_id: Option<String>,
    technology_tags: Vec<String>,
    component_count: usize,
    updates_available: bool,
    update_count: usize,
    risk_level: String,
    backup_available: bool,
    operation_count: usize,
    last_operation_status: Option<String>,
    cover_updated_at_ms: Option<i64>,
}

impl GameCardOutput {
    fn from_details(
        game: &renderpilot_domain::GameInstallation,
        details: &catalog::GameDetailsCatalogResult,
        cover_updated_at_ms: Option<i64>,
    ) -> Self {
        let identity = game.identity();
        let metrics = GameCardMetrics::from_details(details);

        Self {
            game_id: game.id().as_str().to_owned(),
            title: identity.title().to_owned(),
            launcher: identity.launcher().as_str().to_owned(),
            platform: game.platform().as_str().to_owned(),
            runtime: game.runtime().as_str().to_owned(),
            install_path: game.install_path().as_str().to_owned(),
            external_id: identity.external_id().map(str::to_owned),
            technology_tags: metrics.technology_tags,
            component_count: metrics.component_count,
            updates_available: metrics.available_update_count > 0,
            update_count: metrics.available_update_count,
            risk_level: metrics.risk_level.as_str().to_owned(),
            backup_available: metrics.backup_available,
            operation_count: metrics.operation_count,
            last_operation_status: metrics.last_operation_status,
            cover_updated_at_ms,
        }
    }
}

struct GameCardMetrics {
    technology_tags: Vec<String>,
    component_count: usize,
    available_update_count: usize,
    risk_level: super::utils::DashboardRiskLevel,
    backup_available: bool,
    operation_count: usize,
    last_operation_status: Option<String>,
}

impl GameCardMetrics {
    fn from_details(details: &catalog::GameDetailsCatalogResult) -> Self {
        let operation_entries = &details.operations.operations;

        Self {
            technology_tags: technology_tags(&details.components),
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
