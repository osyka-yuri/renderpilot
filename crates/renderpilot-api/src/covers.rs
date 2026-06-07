//! Cover artwork commands for the desktop shell.

use std::path::PathBuf;

use serde_json::json;

use super::utils::{self, to_json, JsonResult};

/// Downloads cover artwork using the configured provider chain, then stores it for the game.
///
/// Provider order is handled by `renderpilot_orchestration::covers`.
pub fn fetch_game_cover(
    context: &renderpilot_orchestration::Context,
    game_id: String,
) -> JsonResult {
    let parsed_game_id = utils::parse_game_id(game_id)?;
    let output = renderpilot_orchestration::covers::fetch_game_cover_auto(context, parsed_game_id)?;

    to_json(output)
}

/// Removes stored cover metadata and deletes the associated cover file from disk.
pub fn clear_game_cover(
    context: &renderpilot_orchestration::Context,
    game_id: String,
) -> JsonResult {
    let parsed_game_id = utils::parse_game_id(game_id)?;

    renderpilot_orchestration::covers::clear_game_cover(context, parsed_game_id)?;

    to_json(json!({ "cleared": true }))
}

/// Copies a user-selected image into the catalog cover store after validation.
pub fn set_game_cover(
    context: &renderpilot_orchestration::Context,
    game_id: String,
    source_path: String,
) -> JsonResult {
    let parsed_game_id = utils::parse_game_id(game_id)?;
    let source_path = PathBuf::from(source_path);

    let output = renderpilot_orchestration::covers::set_game_cover_from_file(
        context,
        parsed_game_id,
        source_path,
    )?;

    to_json(output)
}

/// Removes orphan cover files during application startup.
pub(super) fn gc_orphans_on_startup(context: &renderpilot_orchestration::Context) {
    renderpilot_orchestration::covers::gc_orphan_cover_files_startup(context);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use renderpilot_orchestration::covers::CoverMutationOutput;

    #[test]
    fn cover_mutation_output_serializes_snapshot_keys() {
        let value = serde_json::to_value(CoverMutationOutput {
            file_name: "cover-test-ulid.webp".into(),
            updated_at_ms: 42,
        })
        .expect("serialize cover mutation output");

        assert_eq!(
            value,
            json!({
                "file_name": "cover-test-ulid.webp",
                "updated_at_ms": 42,
            })
        );
    }
}
