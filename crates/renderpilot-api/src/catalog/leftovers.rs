//! Explicit cleanup of confirmed leftovers from retired installations.

use renderpilot_orchestration::{Context, catalog::leftovers};

use crate::utils::{JsonResult, parse_game_id, to_json};

/// Observes remaining changes and issues backend-owned action intents.
pub fn list_retired_game_leftovers(context: &Context) -> JsonResult {
    to_json(leftovers::list_retired_game_leftovers(context)?)
}

/// Removes only supported changes bound to the issued current proposal.
pub fn clean_retired_game_leftovers(
    context: &Context,
    game_id: impl Into<String>,
    intent: impl Into<String>,
) -> JsonResult {
    let game_id = parse_game_id(game_id.into())?;
    let intent = intent.into();
    to_json(leftovers::clean_retired_game_leftovers(
        context, &game_id, &intent,
    )?)
}

/// Retains changes and suppresses an unchanged proposal through backend state.
pub fn leave_retired_game_leftovers(
    context: &Context,
    game_id: impl Into<String>,
    intent: impl Into<String>,
) -> JsonResult {
    let game_id = parse_game_id(game_id.into())?;
    let intent = intent.into();
    to_json(leftovers::leave_retired_game_leftovers(
        context, &game_id, &intent,
    )?)
}

#[cfg(test)]
mod tests {
    use super::{
        clean_retired_game_leftovers, leave_retired_game_leftovers, list_retired_game_leftovers,
    };
    use crate::ApiError;
    use renderpilot_orchestration::Context;

    #[test]
    fn empty_catalog_serializes_the_real_discovery_result() {
        let root = tempfile::tempdir().expect("temporary catalog");
        let context = Context::open_at(root.path().join("catalog.sqlite")).expect("context");

        assert_eq!(
            list_retired_game_leftovers(&context).expect("discovery output"),
            serde_json::json!({ "proposals": [], "issues": [] })
        );
    }

    #[test]
    fn unknown_leave_keeps_nullable_wire_fields_and_does_not_create_a_game() {
        let root = tempfile::tempdir().expect("temporary catalog");
        let context = Context::open_at(root.path().join("catalog.sqlite")).expect("context");

        let output = leave_retired_game_leftovers(&context, "manual:missing", "unissued")
            .expect("stale leave output");

        assert_eq!(output["gameId"], "manual:missing");
        assert_eq!(output["status"], "stale");
        assert_eq!(output.get("leaveRevision"), Some(&serde_json::Value::Null));
        assert_eq!(
            output.get("remainingProposal"),
            Some(&serde_json::Value::Null)
        );
        assert_eq!(output["issue"]["code"], "staleIntent");
        assert_eq!(
            list_retired_game_leftovers(&context).expect("unchanged catalog"),
            serde_json::json!({ "proposals": [], "issues": [] })
        );
    }

    #[test]
    fn invalid_game_identity_is_rejected_at_the_api_boundary() {
        let root = tempfile::tempdir().expect("temporary catalog");
        let context = Context::open_at(root.path().join("catalog.sqlite")).expect("context");

        assert_eq!(
            leave_retired_game_leftovers(&context, "", "unissued"),
            Err(ApiError::InvalidGameId(String::new()))
        );
    }

    #[test]
    fn unknown_clean_serializes_stale_intent_without_creating_catalog_state() {
        let root = tempfile::tempdir().expect("temporary catalog");
        let context = Context::open_at(root.path().join("catalog.sqlite")).expect("context");

        let output = clean_retired_game_leftovers(&context, "manual:missing", "unissued")
            .expect("stale cleanup output");

        assert_eq!(output["gameId"], "manual:missing");
        assert_eq!(output["status"], "stale");
        assert_eq!(output["steps"], serde_json::json!([]));
        assert_eq!(
            output.get("remainingProposal"),
            Some(&serde_json::Value::Null)
        );
        assert_eq!(output["issue"]["code"], "staleIntent");
        assert_eq!(
            list_retired_game_leftovers(&context).expect("unchanged catalog"),
            serde_json::json!({ "proposals": [], "issues": [] })
        );
    }
}
