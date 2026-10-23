//! Transport contract for removing a manually managed game card.

use renderpilot_orchestration::catalog;

pub use renderpilot_orchestration::catalog::{
    GameRemovalAssessment, NvapiClaimAbandonmentAuthorization, NvapiClaimAbandonmentToken,
    NvapiClaimAssessment, NvapiClaimDisposition, NvapiRemovalAssessment, NvapiSettingValueDto,
};

use crate::utils::{JsonResult, parse_game_id, to_json};

/// Assesses game removal feasibility and any NVAPI claims requiring user authorization.
pub fn assess_game_removal(
    context: &renderpilot_orchestration::Context,
    game_id: impl Into<String>,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    to_json(catalog::assess_game_removal(context, &game_id)?)
}

/// Removes one user-managed game card without deleting game files.
pub fn remove_game_from_catalog(
    context: &renderpilot_orchestration::Context,
    game_id: impl Into<String>,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    to_json(RemoveGameFromCatalogOutput::from(
        catalog::remove_game_from_catalog(context, &game_id)?,
    ))
}

/// Removes one user-managed game card with explicit user authorization for divergent NVAPI claims.
pub fn remove_game_from_catalog_authorized(
    context: &renderpilot_orchestration::Context,
    game_id: impl Into<String>,
    authorization: NvapiClaimAbandonmentAuthorization,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let options = catalog::RemoveGameOptions {
        abandonment_authorization: Some(authorization),
    };
    to_json(RemoveGameFromCatalogOutput::from(
        catalog::remove_game_from_catalog_with_options(context, &game_id, &options)?,
    ))
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoveGameFromCatalogOutput {
    game_id: String,
}

impl From<catalog::RemoveGameFromCatalogResult> for RemoveGameFromCatalogOutput {
    fn from(value: catalog::RemoveGameFromCatalogResult) -> Self {
        Self {
            game_id: value.game_id,
        }
    }
}
