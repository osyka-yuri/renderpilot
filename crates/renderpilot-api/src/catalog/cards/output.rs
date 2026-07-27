//! The game-card DTOs the GUI renders and the metrics derived for them.

use renderpilot_orchestration::catalog as orch_catalog;
use renderpilot_orchestration::domain::AddonKind;
use serde::Serialize;

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
    pub(super) addon_capabilities: Vec<AddonKind>,
    pub(super) updates_available: bool,
    pub(super) update_count: usize,
    pub(super) risk_level: String,

    #[serde(skip_serializing)]
    pub(super) risk_order: orch_catalog::CatalogCardRiskLevel,

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
    pub(super) catalog_size: usize,
    pub(super) total: usize,
    pub(super) hidden_count: usize,
    pub(super) available_libraries: Vec<String>,
    pub(super) available_launchers: Vec<String>,
    pub(super) catalog_revision: orch_catalog::CatalogRevision,
    pub(super) next_offset: Option<usize>,
}

impl GameCardOutput {
    pub(super) fn from_card(card: &orch_catalog::GameCardData) -> Self {
        let game = &card.game;
        let identity = game.identity();
        let title = identity.title().to_owned();

        Self {
            game_id: game.id().as_str().to_owned(),
            title_search_key: card.title_search_key.clone(),
            title,
            launcher: identity.launcher().as_str().to_owned(),
            platform: game.platform().as_str().to_owned(),
            runtime: game.runtime().as_str().to_owned(),
            install_path: game.install_path().as_str().to_owned(),
            external_id: identity.external_id().map(str::to_owned),
            library_tags: card.library_tags.clone(),
            component_count: card.component_count,
            addon_capabilities: card.addon_capabilities.clone(),
            updates_available: card.update_count > 0,
            update_count: card.update_count,
            risk_level: card.risk_level.as_str().to_owned(),
            risk_order: card.risk_level,
            rollback_available: card.rollback_available,
            operation_count: card.operation_count,
            last_operation_status: card.last_operation_status.clone(),
            cover_updated_at_ms: card.cover_updated_at_ms,
            is_favorite: card.is_favorite,
            is_hidden: card.is_hidden,
        }
    }
}
