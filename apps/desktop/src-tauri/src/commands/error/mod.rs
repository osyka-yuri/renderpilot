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

use strings::{SuggestedActions, UserMessage, user_message as user_messages};

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

    /// Stable validation reason for typed errors such as `invalid_install_root`.
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,

    /// Published recovery artifact that the user may need after a fail-closed
    /// managed cleanup. This is structured separately from debug diagnostics
    /// and remains available in release builds.
    #[serde(skip_serializing_if = "Option::is_none")]
    recovery_bundle_path: Option<String>,

    suggested_actions: SuggestedActions,
}

impl CommandError {
    pub(crate) fn new(kind: CommandErrorKind, user_message: UserMessage) -> Self {
        let spec = kind.spec();

        Self {
            code: spec.code,
            severity: spec.severity,
            message_key: user_message.key(),
            details: user_message.default_text().to_owned(),
            reason: None,
            recovery_bundle_path: None,
            suggested_actions: spec.suggested_actions,
        }
    }

    pub(crate) fn user_facing(kind: CommandErrorKind, user_message: UserMessage) -> Self {
        Self::new(kind, user_message)
    }

    pub(crate) fn task_failed(error: impl std::fmt::Display) -> Self {
        log::error!("Command task failed: {error}");
        Self::user_facing(
            CommandErrorKind::CommandTaskFailed,
            user_messages::COMMAND_TASK_FAILED,
        )
    }

    pub(crate) fn invalid_argument(name: &'static str, reason: &'static str) -> Self {
        log::warn!("Invalid argument `{name}`: {reason}");
        Self::user_facing(
            CommandErrorKind::InvalidArgument,
            user_messages::INVALID_ARGUMENT,
        )
    }

    pub(crate) fn invalid_id(
        kind: CommandErrorKind,
        user_message: UserMessage,
        debug_label: &'static str,
        raw: impl std::fmt::Display,
    ) -> Self {
        log::warn!("{debug_label}: {raw}");
        Self::user_facing(kind, user_message)
    }

    fn with_reason(mut self, reason: &'static str) -> Self {
        self.reason = Some(reason);
        self
    }

    fn with_recovery_bundle_path(mut self, path: String) -> Self {
        self.recovery_bundle_path = Some(path);
        self
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
    fn only_terminal_validation_errors_intentionally_omit_suggested_actions() {
        for &kind in CommandErrorKind::ALL {
            let spec = kind.spec();

            if matches!(
                kind,
                CommandErrorKind::InvalidInstallRoot
                    | CommandErrorKind::MultipleInstallsDetected
                    | CommandErrorKind::StaleInstallInspection
                    | CommandErrorKind::RootCorrectionCleanupRequired
            ) {
                assert!(spec.suggested_actions.is_empty());
            } else {
                assert!(
                    !spec.suggested_actions.is_empty(),
                    "missing suggested action for {}",
                    spec.code
                );
            }
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
        let err = CommandError::user_facing(
            CommandErrorKind::InvalidGameId,
            strings::user_message::INVALID_GAME_REFERENCE,
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
    fn command_failed_maps_technical_message_safely() {
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
    }

    #[test]
    fn service_error_categories_map_to_distinct_codes() {
        // The whole point of carrying the category through ServiceError is that
        // the frontend sees a specific code, not a generic `command_failed`.
        let cases = [
            (
                ServiceError::StorageFailed("db locked".into()),
                "storage_failed",
            ),
            (
                ServiceError::ProviderFailed("install failed".into()),
                "provider_failed",
            ),
            (
                ServiceError::DetectionFailed("pe read failed".into()),
                "detection_failed",
            ),
            (
                ServiceError::InvalidInput("bad id".into()),
                "invalid_argument",
            ),
            (
                ServiceError::StaleReplacementSource,
                "stale_replacement_source",
            ),
            (
                ServiceError::GameRemovalCleanupFailed {
                    game_id: "private-game".into(),
                    action: "private-component rollback".into(),
                    reason: "private path D:\\Games\\secret is unavailable".into(),
                },
                "game_removal_cleanup_failed",
            ),
        ];

        for (service_error, expected_code) in cases {
            let err = CommandError::from(ApiError::Service(service_error));
            let value = serde_json::to_value(&err).expect("serialize CommandError");
            assert_eq!(
                value.get("code"),
                Some(&json!(expected_code)),
                "unexpected code for mapped service error"
            );
            assert_ne!(
                value.get("code"),
                Some(&json!("command_failed")),
                "category must not collapse into the generic command_failed code"
            );
        }
    }

    #[test]
    fn stale_replacement_source_maps_to_user_facing_recovery_message() {
        let err = CommandError::from(ApiError::Service(ServiceError::StaleReplacementSource));
        let value = serde_json::to_value(&err).expect("serialize CommandError");

        assert_eq!(
            value.get("messageKey"),
            Some(&json!(
                strings::user_message::STALE_REPLACEMENT_SOURCE.key()
            ))
        );
        assert_eq!(
            value.get("details"),
            Some(&json!(
                strings::user_message::STALE_REPLACEMENT_SOURCE.default_text()
            ))
        );
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
    fn accessors_reflect_internal_state() {
        let err = CommandError::user_facing(
            CommandErrorKind::GameNotFound,
            strings::user_message::GAME_NOT_IN_CATALOG,
        );

        assert_eq!(
            err.user_message(),
            strings::user_message::GAME_NOT_IN_CATALOG.default_text()
        );
        assert_eq!(
            err.message_key(),
            strings::user_message::GAME_NOT_IN_CATALOG.key()
        );
    }

    #[test]
    fn task_failed_uses_safe_details() {
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
    }

    #[test]
    fn invalid_argument_uses_safe_details() {
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
    }

    #[test]
    fn rejected_game_root_uses_specific_safe_user_message() {
        let err = CommandError::from(ApiError::Service(ServiceError::invalid_install_root(
            renderpilot_orchestration::InvalidInstallRootReason::FilesystemRoot,
            "drive root D:/",
        )));

        let value = serde_json::to_value(&err).expect("serialize CommandError");

        assert_eq!(value.get("code"), Some(&json!("invalid_install_root")));
        assert_eq!(
            value.get("details"),
            Some(&json!(
                strings::user_message::INVALID_INSTALL_ROOT.default_text()
            ))
        );
        assert_eq!(
            value.get("messageKey"),
            Some(&json!(strings::user_message::INVALID_INSTALL_ROOT.key()))
        );
        assert_ne!(value.get("details"), Some(&json!("drive root D:/")));
        assert_eq!(value.get("suggestedActions"), Some(&json!([])));
        assert_eq!(value.get("reason"), Some(&json!("filesystem_root")));
    }

    #[test]
    fn rejected_parent_install_scope_uses_invalid_root_reason() {
        let err = CommandError::from(ApiError::Service(ServiceError::invalid_install_root(
            renderpilot_orchestration::InvalidInstallRootReason::ContainsProvenInstall,
            "selected parent contains private game ids",
        )));

        let value = serde_json::to_value(&err).expect("serialize CommandError");

        assert_eq!(value.get("code"), Some(&json!("invalid_install_root")));
        assert_eq!(
            value.get("details"),
            Some(&json!(
                strings::user_message::INVALID_INSTALL_ROOT.default_text()
            ))
        );
        assert_eq!(
            value.get("messageKey"),
            Some(&json!(strings::user_message::INVALID_INSTALL_ROOT.key()))
        );
        assert_ne!(
            value.get("details"),
            Some(&json!("selected parent contains private game ids"))
        );
        assert_eq!(value.get("reason"), Some(&json!("contains_proven_install")));
        assert_eq!(value.get("suggestedActions"), Some(&json!([])));
    }

    #[test]
    fn multiple_install_container_has_a_distinct_safe_error() {
        let err = CommandError::from(ApiError::Service(ServiceError::MultipleInstallsDetected(
            "private installation roots".to_owned(),
        )));

        let value = serde_json::to_value(&err).expect("serialize CommandError");

        assert_eq!(
            value.get("code"),
            Some(&json!("multiple_installs_detected"))
        );
        assert_eq!(
            value.get("details"),
            Some(&json!(
                strings::user_message::MULTIPLE_INSTALLS_DETECTED.default_text()
            ))
        );
        assert_ne!(
            value.get("details"),
            Some(&json!("private installation roots"))
        );
        assert_eq!(value.get("suggestedActions"), Some(&json!([])));
    }

    #[test]
    fn root_correction_state_errors_are_typed_and_do_not_expose_catalog_ids() {
        let rollback = CommandError::from(ApiError::Service(
            ServiceError::RootCorrectionCleanupRequired {
                game_id: "private-game".to_owned(),
                component_ids: vec!["private-component".to_owned()],
            },
        ));
        let blocked = CommandError::from(ApiError::Service(ServiceError::RootCorrectionBlocked {
            game_id: "private-game".to_owned(),
            blockers: vec!["pending_recovery".to_owned()],
        }));

        let rollback = serde_json::to_value(&rollback).expect("rollback error");
        assert_eq!(
            rollback.get("code"),
            Some(&json!("root_correction_cleanup_required"))
        );
        assert_eq!(
            rollback.get("details"),
            Some(&json!(
                strings::user_message::ROOT_CORRECTION_CLEANUP_REQUIRED.default_text()
            ))
        );
        assert_eq!(rollback.get("suggestedActions"), Some(&json!([])));

        let blocked = serde_json::to_value(&blocked).expect("blocked error");
        assert_eq!(blocked.get("code"), Some(&json!("root_correction_blocked")));
        assert_eq!(
            blocked.get("details"),
            Some(&json!(
                strings::user_message::ROOT_CORRECTION_BLOCKED.default_text()
            ))
        );
        assert!(
            !blocked
                .get("details")
                .expect("details")
                .as_str()
                .expect("string")
                .contains("private-game")
        );
    }

    #[test]
    fn game_removal_cleanup_error_is_specific_and_hides_diagnostics() {
        let technical = "private path D:\\Games\\secret is unavailable";
        let err = CommandError::from(ApiError::Service(ServiceError::GameRemovalCleanupFailed {
            game_id: "private-game".to_owned(),
            action: "private-component rollback".to_owned(),
            reason: technical.to_owned(),
        }));

        let value = serde_json::to_value(&err).expect("serialize CommandError");

        assert_eq!(
            value.get("code"),
            Some(&json!("game_removal_cleanup_failed"))
        );
        assert_eq!(
            value.get("messageKey"),
            Some(&json!(
                strings::user_message::GAME_REMOVAL_CLEANUP_FAILED.key()
            ))
        );
        assert_eq!(
            value.get("details"),
            Some(&json!(
                strings::user_message::GAME_REMOVAL_CLEANUP_FAILED.default_text()
            ))
        );
        assert_ne!(value.get("details"), Some(&json!(technical)));
    }

    #[test]
    fn invalid_id_uses_safe_details() {
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
    }

    #[test]
    fn managed_cleanup_error_exposes_the_published_recovery_bundle_structurally() {
        let recovery_bundle = r"C:\Recovery\renderpilot-bundle";
        let err = CommandError::from(ApiError::Service(ServiceError::ManagedCleanupAmbiguous {
            game_id: "private-game".to_owned(),
            targets: vec!["private target".to_owned()],
            recovery_bundle_path: recovery_bundle.to_owned(),
        }));

        let value = serde_json::to_value(&err).expect("serialize CommandError");

        assert_eq!(value.get("code"), Some(&json!("managed_cleanup_ambiguous")));
        assert_eq!(
            value.get("recoveryBundlePath"),
            Some(&json!(recovery_bundle))
        );
        assert!(
            !value
                .get("details")
                .expect("details")
                .as_str()
                .expect("string")
                .contains("private target"),
            "technical conflict details must remain sanitized"
        );
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
