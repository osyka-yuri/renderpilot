//! Single-game details: detected components, replacement candidates, and
//! operation history, filtered to the technologies the GUI surfaces.

use renderpilot_orchestration::application::ComponentReplacementCandidates;
use renderpilot_orchestration::catalog as orch_catalog;
use renderpilot_orchestration::catalog::output as catalog_output;
use renderpilot_orchestration::domain::{AddonKind, GameId, GraphicsComponent};
use serde::Serialize;
use std::collections::BTreeSet;

use super::{is_component_visible, visible_component_ids};
use crate::ApiError;
use crate::utils::{JsonResult, parse_game_id, to_json};

/// Loads one game with detected components, candidates, and operation history.
pub fn get_game_details(
    context: &renderpilot_orchestration::Context,
    game_id: impl Into<String>,
) -> JsonResult {
    let game_id = parse_game_id(game_id.into())?;
    to_json(GameDetailsOutput::load(context, &game_id)?)
}

#[derive(Debug, Serialize)]
pub(crate) struct GameComponentOutput {
    #[serde(flatten)]
    component: GraphicsComponent,
    rollback_available: bool,
    d3d12_executable_status: Option<catalog_output::D3d12ExecutableStatusOutput>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GameDetailsOutput {
    game: renderpilot_orchestration::domain::GameInstallation,
    components: Vec<GameComponentOutput>,
    candidate_groups: Vec<catalog_output::ComponentCandidateOutput>,
    operations: Vec<catalog_output::OperationSummaryOutput>,
    addon_capabilities: Vec<AddonKind>,
}

impl GameDetailsOutput {
    pub(crate) fn load(
        context: &renderpilot_orchestration::Context,
        game_id: &GameId,
    ) -> Result<Self, ApiError> {
        let orch_catalog::GameDetailsCatalogResult {
            game,
            components,
            backup_component_ids,
            candidate_groups,
            d3d12_executable_status,
            operations,
            addon_capabilities,
        } = orch_catalog::CatalogReadService::new(context).game_details(game_id)?;
        let mut d3d12_executable_status = d3d12_executable_status.as_ref().map(|status| {
            (
                status.component_id().as_str().to_owned(),
                catalog_output::D3d12ExecutableStatusOutput::from(status),
            )
        });
        let visible_components = filter_visible_components(components);
        let visible_component_ids = visible_component_ids(&visible_components);
        let visible_candidate_groups =
            filter_visible_candidate_groups(candidate_groups, &visible_component_ids);
        let candidate_groups =
            catalog_output::component_candidate_outputs(visible_candidate_groups);
        let operations = catalog_output::operation_summary_outputs(&operations);

        let components = visible_components
            .into_iter()
            .map(|component| {
                let rollback_available = backup_component_ids.contains(component.id().as_str());
                let component_d3d12_status = if d3d12_executable_status
                    .as_ref()
                    .is_some_and(|(component_id, _)| component_id == component.id().as_str())
                {
                    d3d12_executable_status.take().map(|(_, status)| status)
                } else {
                    None
                };
                GameComponentOutput {
                    d3d12_executable_status: component_d3d12_status,
                    component,
                    rollback_available,
                }
            })
            .collect();

        Ok(Self {
            game,
            components,
            candidate_groups,
            operations,
            addon_capabilities,
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
