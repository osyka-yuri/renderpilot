//! The game-card DTOs the GUI renders and the metrics derived for them.

use renderpilot_orchestration::catalog as orch_catalog;
use renderpilot_orchestration::domain::{GameInstallation, GraphicsComponent};
use serde::Serialize;

use crate::catalog::{is_component_visible, visible_component_ids};
use crate::utils::{
    available_update_count, dashboard_risk_level, library_tags, DashboardRiskLevel,
};

#[derive(Debug, Serialize)]
pub(super) struct GameListOutput {
    pub(super) games: Vec<GameInstallation>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct GameCardOutput {
    pub(super) game_id: String,
    pub(super) title: String,

    #[serde(skip_serializing)]
    pub(super) title_search_key: String,

    pub(super) launcher: String,
    pub(super) platform: String,
    pub(super) runtime: String,
    pub(super) install_path: String,
    pub(super) external_id: Option<String>,
    pub(super) library_tags: Vec<String>,
    pub(super) component_count: usize,
    pub(super) updates_available: bool,
    pub(super) update_count: usize,
    pub(super) risk_level: String,

    #[serde(skip_serializing)]
    pub(super) risk_order: DashboardRiskLevel,

    pub(super) rollback_available: bool,
    pub(super) operation_count: usize,
    pub(super) last_operation_status: Option<String>,
    pub(super) cover_updated_at_ms: Option<i64>,
    pub(super) is_favorite: bool,
    pub(super) is_hidden: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QueryGameCardsOutput {
    pub(super) items: Vec<GameCardOutput>,
    pub(super) total: usize,
    pub(super) hidden_count: usize,
    pub(super) available_libraries: Vec<String>,
    pub(super) available_launchers: Vec<String>,
    pub(super) query_fingerprint: String,
}

impl GameCardOutput {
    pub(super) fn from_details(
        game: &GameInstallation,
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

fn visible_component_count(components: &[GraphicsComponent]) -> usize {
    components
        .iter()
        .filter(|component| is_component_visible(component))
        .count()
}
