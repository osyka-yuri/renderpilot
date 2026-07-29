//! Transport contract for removing a manually managed game card.

use renderpilot_orchestration::catalog;

use crate::utils::{JsonResult, parse_game_id, to_json};

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
