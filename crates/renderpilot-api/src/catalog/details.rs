//! Single-game details: detected components, replacement candidates, and
//! operation history, filtered to the technologies the GUI surfaces.

use renderpilot_orchestration::application::ComponentReplacementCandidates;
use renderpilot_orchestration::catalog as orch_catalog;
use renderpilot_orchestration::catalog::output as catalog_output;
use renderpilot_orchestration::domain::{GameId, GraphicsComponent};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;

use super::{is_component_visible, visible_component_ids};
use crate::utils::{parse_game_id, to_json, JsonResult};
use crate::ApiError;

/// Loads one game with detected components, candidates, and operation history.
pub fn get_game_details(
    context: &renderpilot_orchestration::Context,
    game_id: impl Into<String>,
) -> JsonResult {
    let game_id = parse_game_id(game_id.into())?;
    to_json(GameDetailsOutput::load(context, game_id)?)
}

#[derive(Debug, Serialize)]
pub(crate) struct GameComponentOutput {
    #[serde(flatten)]
    component: GraphicsComponent,
    rollback_available: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct GameDetailsOutput {
    game: renderpilot_orchestration::domain::GameInstallation,
    components: Vec<GameComponentOutput>,
    candidate_groups: Value,
    operations: Value,
}

impl GameDetailsOutput {
    pub(crate) fn load(
        context: &renderpilot_orchestration::Context,
        game_id: GameId,
    ) -> Result<Self, ApiError> {
        let backup_ids = orch_catalog::backup_component_ids(context, &game_id)?;
        let details = orch_catalog::get_game_details(context, game_id)?;
        let visible_components = filter_visible_components(details.components);
        let visible_component_ids = visible_component_ids(&visible_components);
        let visible_candidate_groups =
            filter_visible_candidate_groups(details.candidate_groups, &visible_component_ids);
        let candidate_groups = serde_json::to_value(catalog_output::component_candidate_outputs(
            visible_candidate_groups,
        ))
        .map_err(ApiError::from)?;
        let operations = serde_json::to_value(catalog_output::operation_summary_outputs(
            &details.operations,
        ))
        .map_err(ApiError::from)?;

        let components = visible_components
            .into_iter()
            .map(|component| {
                let rollback_available = backup_ids.contains(component.id().as_str());
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

fn filter_visible_components(components: Vec<GraphicsComponent>) -> Vec<GraphicsComponent> {
    components
        .into_iter()
        .filter(is_component_visible)
        .collect()
}

fn filter_visible_candidate_groups(
    candidate_groups: Vec<ComponentReplacementCandidates>,
    visible_component_ids: &BTreeSet<String>,
) -> Vec<ComponentReplacementCandidates> {
    candidate_groups
        .into_iter()
        .filter(|group| visible_component_ids.contains(group.component_id().as_str()))
        .collect()
}
