//! Persisted catalog settings and per-game UI flags (favorite / hidden).

use renderpilot_orchestration::catalog as orch_catalog;

use crate::utils::{parse_game_id, to_json, JsonResult};

/// Reads one persisted catalog settings value (typically used for integration keys).
pub fn get_catalog_setting(
    context: &renderpilot_orchestration::Context,
    key: impl Into<String>,
) -> JsonResult {
    let key = key.into();
    let value = orch_catalog::get_catalog_setting(context, &key)?;
    to_json(serde_json::json!({ "value": value }))
}

/// Upserts a persisted catalog settings value, or deletes the row when `value` is blank after trim.
pub fn set_catalog_setting(
    context: &renderpilot_orchestration::Context,
    key: impl Into<String>,
    value: impl Into<String>,
) -> JsonResult {
    let key = key.into();
    let value = value.into();
    orch_catalog::set_catalog_setting(context, &key, &value)?;
    to_json(serde_json::json!({ "saved": true }))
}

/// Sets the favorite status of a game.
pub fn set_game_favorite(
    context: &renderpilot_orchestration::Context,
    game_id: impl Into<String>,
    is_favorite: bool,
) -> JsonResult {
    let game_id = parse_game_id(game_id.into())?;
    orch_catalog::set_game_favorite(context, &game_id, is_favorite)?;
    to_json(serde_json::json!({ "saved": true }))
}

/// Sets the hidden status of a game.
pub fn set_game_hidden(
    context: &renderpilot_orchestration::Context,
    game_id: impl Into<String>,
    is_hidden: bool,
) -> JsonResult {
    let game_id = parse_game_id(game_id.into())?;
    orch_catalog::set_game_hidden(context, &game_id, is_hidden)?;
    to_json(serde_json::json!({ "saved": true }))
}
