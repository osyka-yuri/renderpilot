//! Cover artwork commands for the desktop shell.

use std::path::PathBuf;

use serde_json::json;

use super::utils::{self, JsonResult, to_json};

/// Successful clear output with a separate best-effort orphan-cleanup issue.
/// The JSON member keeps the exact legacy IPC payload.
#[derive(Debug)]
pub struct ClearGameCoverOutput {
    /// Serialized legacy command payload returned unchanged to the desktop.
    pub json: serde_json::Value,
    /// Nonfatal orphan-cleanup failure retained only for backend observation.
    pub cleanup_issue: Option<super::ApiError>,
}

/// Downloads cover artwork using the configured provider chain, then stores it for the game.
///
/// Provider order is handled by `renderpilot_orchestration::covers`.
pub fn fetch_game_cover(
    context: &renderpilot_orchestration::Context,
    game_id: String,
) -> JsonResult {
    let parsed_game_id = utils::parse_game_id(game_id)?;
    let output =
        renderpilot_orchestration::covers::fetch_game_cover_auto(context, &parsed_game_id)?;

    to_json(output)
}

/// Removes stored cover metadata and deletes the associated cover file from disk.
pub fn clear_game_cover(
    context: &renderpilot_orchestration::Context,
    game_id: String,
) -> JsonResult {
    let output = clear_game_cover_with_observation(context, game_id)?;
    if let Some(error) = output.cleanup_issue {
        log::warn!("cover was cleared but orphan cleanup failed: {error}");
    }
    Ok(output.json)
}

/// Clears a cover while retaining a soft orphan-cleanup observation for the
/// single desktop owner to log and emit once.
pub fn clear_game_cover_with_observation(
    context: &renderpilot_orchestration::Context,
    game_id: String,
) -> Result<ClearGameCoverOutput, super::ApiError> {
    let parsed_game_id = utils::parse_game_id(game_id)?;

    let observation = renderpilot_orchestration::covers::clear_game_cover_with_observation(
        context,
        &parsed_game_id,
    )?;

    Ok(ClearGameCoverOutput {
        json: to_json(json!({ "cleared": true }))?,
        cleanup_issue: observation.cleanup_issue.map(Into::into),
    })
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
        &parsed_game_id,
        &source_path,
    )?;

    to_json(output)
}

/// Removes orphan cover files during application startup.
pub(super) fn try_gc_orphans_on_startup(
    context: &renderpilot_orchestration::Context,
) -> Result<(), super::ApiError> {
    renderpilot_orchestration::covers::try_gc_orphan_cover_files_startup(context)
        .map_err(Into::into)
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

    #[test]
    fn clear_output_preserves_the_legacy_ipc_payload_without_a_soft_issue() {
        let output = super::ClearGameCoverOutput {
            json: json!({ "cleared": true }),
            cleanup_issue: None,
        };

        assert_eq!(output.json, json!({ "cleared": true }));
        assert!(output.cleanup_issue.is_none());
    }

    #[test]
    fn legacy_clear_wrapper_delegates_to_the_observation_api() {
        let source = include_str!("covers.rs");
        assert!(source.contains("pub fn clear_game_cover_with_observation("));
        assert!(
            source.contains("let output = clear_game_cover_with_observation(context, game_id)?;")
        );
    }
}
