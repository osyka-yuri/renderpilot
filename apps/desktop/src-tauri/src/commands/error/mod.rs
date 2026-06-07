//! Facilitates the transformation of `ApiError` / `ServiceError` values into a stable, deterministic JSON payload for the desktop shell frontend.
//!
//! JSON Contract Specification:
//! - `details`: Contains sanitized, user-facing fallback text, guaranteed to be free of sensitive system paths or internals.
//! - `messageKey`: Provides a stable, unchanging localization key corresponding to the `details` string.
//! - `debugDetails`: Serves exclusively for diagnostic purposes and is strictly stripped from release-mode JSON payloads.

mod kind;
mod mapping;
mod strings;

use serde::Serialize;

pub(crate) use kind::CommandErrorKind;

use strings::{user_message as user_messages, SuggestedActions, UserMessage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandErrorSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    code: &'static str,
    severity: CommandErrorSeverity,

    /// A stable, immutable localization key mapping to the sanitized user-facing fallback text provided in `details`.
    message_key: &'static str,

    /// Sanitized user-facing fallback text, scrubbed of internal technical context. Serialized as the JSON field `details`.
    details: String,

    #[cfg_attr(debug_assertions, serde(skip_serializing_if = "Option::is_none"))]
    #[cfg_attr(not(debug_assertions), serde(skip_serializing))]
    debug_details: Option<String>,

    suggested_actions: SuggestedActions,
}

impl CommandError {
    pub(crate) fn new(
        kind: CommandErrorKind,
        user_message: UserMessage,
        debug_details: Option<String>,
    ) -> Self {
        let spec = kind.spec();

        Self {
            code: spec.code,
            severity: spec.severity,
            message_key: user_message.key(),
            details: user_message.default_text().to_owned(),
            debug_details,
            suggested_actions: spec.suggested_actions,
        }
    }

    pub(crate) fn user_facing(kind: CommandErrorKind, user_message: UserMessage) -> Self {
        Self::new(kind, user_message, None)
    }

    pub(crate) fn with_debug_details(
        kind: CommandErrorKind,
        user_message: UserMessage,
        debug_details: impl Into<String>,
    ) -> Self {
        Self::new(kind, user_message, Some(debug_details.into()))
    }

    pub(crate) fn task_failed(error: impl std::fmt::Display) -> Self {
        Self::with_debug_details(
            CommandErrorKind::CommandTaskFailed,
            user_messages::COMMAND_TASK_FAILED,
            format!("Command task failed: {error}"),
        )
    }

    pub(crate) fn invalid_argument(name: &'static str, reason: &'static str) -> Self {
        Self::with_debug_details(
            CommandErrorKind::InvalidArgument,
            user_messages::INVALID_ARGUMENT,
            format!("Invalid argument `{name}`: {reason}"),
        )
    }

    pub(crate) fn invalid_id(
        kind: CommandErrorKind,
        user_message: UserMessage,
        debug_label: &'static str,
        raw: impl std::fmt::Display,
    ) -> Self {
        Self::with_debug_details(kind, user_message, format!("{debug_label}: {raw}"))
    }

    /// Retrieves the sanitized text explicitly intended for UI consumption, serialized as the JSON field `details`.
    #[must_use]
    #[cfg(test)]
    pub(crate) fn user_message(&self) -> &str {
        self.details.as_str()
    }

    /// Retrieves the robust localization key corresponding to the sanitized UI message.
    #[must_use]
    #[cfg(test)]
    pub(crate) fn message_key(&self) -> &'static str {
        self.message_key
    }

    /// Retrieves granular technical details intended strictly for internal logging or debugging workflows.
    ///
    /// To ensure data privacy and prevent leakage of internals, this value is explicitly stripped during serialization in release builds.
    #[must_use]
    #[cfg(test)]
    pub(crate) fn debug_details(&self) -> Option<&str> {
        self.debug_details.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use renderpilot_api::ApiError;
    use renderpilot_orchestration::ServiceError;
    use serde_json::json;
    use std::collections::BTreeSet;

    #[test]
    fn error_specs_have_valid_codes() {
        for &kind in CommandErrorKind::ALL {
            let spec = kind.spec();

            assert!(!spec.code.is_empty(), "empty command error code");
            assert_eq!(
                spec.code,
                spec.code.trim(),
                "command error code has surrounding whitespace: {:?}",
                spec.code,
            );
            assert!(
                spec.code
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_'),
                "command error code contains unsupported characters: {}",
                spec.code,
            );
        }
    }

    #[test]
    fn error_codes_are_unique() {
        let mut seen = BTreeSet::new();

        for &kind in CommandErrorKind::ALL {
            let spec = kind.spec();

            assert!(
                seen.insert(spec.code),
                "duplicate command error code: {}",
                spec.code
            );
        }
    }

    #[test]
    fn every_error_has_suggested_action() {
        for &kind in CommandErrorKind::ALL {
            let spec = kind.spec();

            assert!(
                !spec.suggested_actions.is_empty(),
                "missing suggested action for {}",
                spec.code
            );
        }
    }

    #[test]
    fn severity_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_value(CommandErrorSeverity::Warning).expect("serialize severity"),
            json!("warning")
        );

        assert_eq!(
            serde_json::to_value(CommandErrorSeverity::Error).expect("serialize severity"),
            json!("error")
        );
    }

    #[test]
    fn command_error_json_includes_safe_details_and_message_key() {
        let err = CommandError::with_debug_details(
            CommandErrorKind::InvalidGameId,
            strings::user_message::INVALID_GAME_REFERENCE,
            "Invalid game id: secret-id-123",
        );

        let value = serde_json::to_value(&err).expect("serialize CommandError");

        assert_eq!(
            value.get("details"),
            Some(&json!(
                strings::user_message::INVALID_GAME_REFERENCE.default_text()
            ))
        );
        assert_eq!(
            value.get("messageKey"),
            Some(&json!(strings::user_message::INVALID_GAME_REFERENCE.key()))
        );
        assert_eq!(value.get("code"), Some(&json!("invalid_game_id")));
    }

    #[test]
    fn debug_details_serialization_matches_build_profile() {
        let technical = "sqlite error at C:\\secret\\path";

        let err = CommandError::with_debug_details(
            CommandErrorKind::CommandFailed,
            strings::user_message::OPERATION_COULD_NOT_COMPLETE,
            technical,
        );

        let value = serde_json::to_value(&err).expect("serialize CommandError");
        let json_str = serde_json::to_string(&err).expect("serialize CommandError");

        if cfg!(debug_assertions) {
            assert_eq!(value.get("debugDetails"), Some(&json!(technical)));
        } else {
            assert!(
                value.get("debugDetails").is_none(),
                "expected debugDetails to be stripped in release JSON: {value}"
            );
            assert!(
                !json_str.contains(technical),
                "expected technical details to be stripped in release JSON: {json_str}"
            );
        }
    }

    #[test]
    fn debug_details_are_omitted_when_absent() {
        let err = CommandError::user_facing(
            CommandErrorKind::SteamGridDbApiKeyMissing,
            strings::user_message::STEAMGRIDDB_API_KEY_MISSING,
        );

        let value = serde_json::to_value(&err).expect("serialize CommandError");

        assert!(
            value.get("debugDetails").is_none(),
            "debugDetails should be absent when no debug details exist"
        );
    }

    #[test]
    fn command_failed_maps_technical_message_to_debug_not_details() {
        let technical = "catalog error: permission denied on D:\\Games\\secret";
        let err = CommandError::from(ApiError::Service(ServiceError::CommandFailed(
            technical.into(),
        )));
        let value = serde_json::to_value(&err).expect("serialize CommandError");

        assert_eq!(
            value.get("details"),
            Some(&json!(
                strings::user_message::OPERATION_COULD_NOT_COMPLETE.default_text()
            ))
        );
        assert_eq!(
            value.get("messageKey"),
            Some(&json!(
                strings::user_message::OPERATION_COULD_NOT_COMPLETE.key()
            ))
        );
        assert_ne!(value.get("details"), Some(&json!(technical)));

        if cfg!(debug_assertions) {
            assert_eq!(value.get("debugDetails"), Some(&json!(technical)));
        } else {
            assert!(value.get("debugDetails").is_none());
        }
    }

    #[test]
    fn serialization_contract_has_stable_keys_for_user_facing_error() {
        let err = CommandError::user_facing(
            CommandErrorKind::SteamGridDbApiKeyMissing,
            strings::user_message::STEAMGRIDDB_API_KEY_MISSING,
        );

        let value = serde_json::to_value(&err).expect("serialize CommandError");

        let object = value
            .as_object()
            .expect("CommandError should serialize as a JSON object");

        let keys = object.keys().map(String::as_str).collect::<BTreeSet<_>>();

        assert_eq!(
            keys,
            BTreeSet::from([
                "code",
                "severity",
                "messageKey",
                "details",
                "suggestedActions",
            ])
        );
    }

    #[test]
    fn serialization_contract_has_stable_keys_for_current_build_profile() {
        let err = CommandError::with_debug_details(
            CommandErrorKind::CommandFailed,
            strings::user_message::OPERATION_COULD_NOT_COMPLETE,
            "debug internals",
        );

        let value = serde_json::to_value(&err).expect("serialize CommandError");

        let object = value
            .as_object()
            .expect("CommandError should serialize as a JSON object");

        let keys = object.keys().map(String::as_str).collect::<BTreeSet<_>>();

        if cfg!(debug_assertions) {
            assert_eq!(
                keys,
                BTreeSet::from([
                    "code",
                    "severity",
                    "messageKey",
                    "details",
                    "debugDetails",
                    "suggestedActions",
                ])
            );
        } else {
            assert_eq!(
                keys,
                BTreeSet::from([
                    "code",
                    "severity",
                    "messageKey",
                    "details",
                    "suggestedActions",
                ])
            );
        }
    }

    #[test]
    fn accessors_reflect_internal_state() {
        let err = CommandError::with_debug_details(
            CommandErrorKind::GameNotFound,
            strings::user_message::GAME_NOT_IN_CATALOG,
            "Game not found: x",
        );

        assert_eq!(
            err.user_message(),
            strings::user_message::GAME_NOT_IN_CATALOG.default_text()
        );
        assert_eq!(
            err.message_key(),
            strings::user_message::GAME_NOT_IN_CATALOG.key()
        );
        assert_eq!(err.debug_details(), Some("Game not found: x"));
    }

    #[test]
    fn task_failed_uses_safe_details_and_moves_error_to_debug_details() {
        let err = CommandError::task_failed("worker crashed with private path");
        let value = serde_json::to_value(&err).expect("serialize CommandError");

        assert_eq!(
            value.get("details"),
            Some(&json!(
                strings::user_message::COMMAND_TASK_FAILED.default_text()
            ))
        );
        assert_eq!(
            value.get("messageKey"),
            Some(&json!(strings::user_message::COMMAND_TASK_FAILED.key()))
        );

        if cfg!(debug_assertions) {
            assert_eq!(
                value.get("debugDetails"),
                Some(&json!(
                    "Command task failed: worker crashed with private path"
                ))
            );
        } else {
            assert!(value.get("debugDetails").is_none());
        }
    }

    #[test]
    fn invalid_argument_uses_safe_details_and_moves_reason_to_debug_details() {
        let err = CommandError::invalid_argument("game_id", "must not be empty");

        let value = serde_json::to_value(&err).expect("serialize CommandError");

        assert_eq!(value.get("code"), Some(&json!("invalid_argument")));
        assert_eq!(
            value.get("details"),
            Some(&json!(
                strings::user_message::INVALID_ARGUMENT.default_text()
            ))
        );
        assert_eq!(
            value.get("messageKey"),
            Some(&json!(strings::user_message::INVALID_ARGUMENT.key()))
        );

        if cfg!(debug_assertions) {
            assert_eq!(
                value.get("debugDetails"),
                Some(&json!("Invalid argument `game_id`: must not be empty"))
            );
        } else {
            assert!(value.get("debugDetails").is_none());
        }
    }

    #[test]
    fn invalid_id_uses_safe_details_and_moves_raw_id_to_debug_details() {
        let err = CommandError::invalid_id(
            CommandErrorKind::InvalidGameId,
            strings::user_message::INVALID_GAME_REFERENCE,
            "Invalid game id",
            "raw-secret-game-id",
        );

        let value = serde_json::to_value(&err).expect("serialize CommandError");

        assert_eq!(
            value.get("details"),
            Some(&json!(
                strings::user_message::INVALID_GAME_REFERENCE.default_text()
            ))
        );
        assert_eq!(
            value.get("messageKey"),
            Some(&json!(strings::user_message::INVALID_GAME_REFERENCE.key()))
        );

        if cfg!(debug_assertions) {
            assert_eq!(
                value.get("debugDetails"),
                Some(&json!("Invalid game id: raw-secret-game-id"))
            );
        } else {
            assert!(value.get("debugDetails").is_none());
        }
    }

    #[test]
    fn suggested_actions_serialize_as_safe_user_facing_text() {
        let err = CommandError::user_facing(
            CommandErrorKind::InvalidGameId,
            strings::user_message::INVALID_GAME_REFERENCE,
        );

        let value = serde_json::to_value(&err).expect("serialize CommandError");

        assert_eq!(
            value.get("suggestedActions"),
            Some(&json!([{
                "key": "suggested_action.refresh_games",
                "text": "Refresh the games list and open the game again.",
            }]))
        );
    }
}
