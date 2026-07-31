//! Single-game details: detected components, replacement candidates, and
//! operation history, filtered to the technologies the GUI surfaces.

use renderpilot_orchestration::application::ComponentReplacementCandidates;
use renderpilot_orchestration::catalog as orch_catalog;
use renderpilot_orchestration::catalog::output as catalog_output;
use renderpilot_orchestration::domain::{
    AddonKind, GameId, GameInstallation, LibraryComponent, RootAuthority,
};
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
    component: LibraryComponent,
    rollback_available: bool,
    d3d12_executable_status: Option<catalog_output::D3d12ExecutableStatusOutput>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GameDetailsOutput {
    game: GameDetailsGameOutput,
    components: Vec<GameComponentOutput>,
    candidate_groups: Vec<catalog_output::ComponentCandidateOutput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    streamline_candidate_options: Vec<catalog_output::CoordinatedCandidateOptionOutput>,
    operations: Vec<catalog_output::OperationSummaryOutput>,
    addon_capabilities: Vec<AddonKind>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GameDetailsGameOutput {
    identity: GameDetailsIdentityOutput,
    platform: String,
    runtime: String,
    install_path: String,
    can_remove_from_catalog: bool,
}

#[derive(Debug, Serialize)]
struct GameDetailsIdentityOutput {
    id: String,
    title: String,
    launcher: String,
    external_id: Option<String>,
}

impl From<GameInstallation> for GameDetailsGameOutput {
    fn from(game: GameInstallation) -> Self {
        Self {
            identity: GameDetailsIdentityOutput {
                id: game.id().as_str().to_owned(),
                title: game.identity().title().to_owned(),
                launcher: game.identity().launcher().as_str().to_owned(),
                external_id: game.identity().external_id().map(str::to_owned),
            },
            platform: game.platform().as_str().to_owned(),
            runtime: game.runtime().as_str().to_owned(),
            install_path: game.install_path().as_str().to_owned(),
            can_remove_from_catalog: game.root_authority() != RootAuthority::LauncherManifest,
        }
    }
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
            streamline_candidate_options,
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
        let streamline_candidate_options =
            catalog_output::coordinated_candidate_option_outputs(streamline_candidate_options);
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
            game: game.into(),
            components,
            candidate_groups,
            streamline_candidate_options,
            operations,
            addon_capabilities,
        })
    }
}

fn filter_visible_components(components: Vec<LibraryComponent>) -> Vec<LibraryComponent> {
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

#[cfg(test)]
mod tests {
    use renderpilot_orchestration::domain::{
        GameIdentity, GameRuntime, Launcher, PathRef, Platform,
    };
    use serde_json::json;

    use super::*;

    #[test]
    fn game_details_uses_an_explicit_minimal_wire_contract() {
        let identity = GameIdentity::new(
            GameId::new("game:details-contract").expect("game id"),
            "Details Contract",
            Launcher::Manual,
        )
        .expect("identity")
        .with_external_id("external-42")
        .expect("external id");
        let game = GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new("D:/Games/Details Contract").expect("install path"),
        )
        .with_root_authority(RootAuthority::UserConfirmed);

        let encoded = serde_json::to_value(GameDetailsGameOutput::from(game))
            .expect("serialize details game");

        assert_eq!(
            encoded,
            json!({
                "identity": {
                    "id": "game:details-contract",
                    "title": "Details Contract",
                    "launcher": "Manual",
                    "external_id": "external-42"
                },
                "platform": "Windows",
                "runtime": "NativeWindows",
                "install_path": "D:/Games/Details Contract",
                "can_remove_from_catalog": true
            })
        );
        assert!(encoded.get("root_authority").is_none());
        assert!(encoded.get("executable_candidates").is_none());
        assert!(encoded.get("confirmed_executable").is_none());
    }
}
