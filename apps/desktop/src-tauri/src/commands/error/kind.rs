use super::strings::{suggested_action, SuggestedActions};
use super::CommandErrorSeverity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ErrorSpec {
    pub code: &'static str,
    pub severity: CommandErrorSeverity,
    pub message_key: &'static str,
    pub suggested_actions: SuggestedActions,
}

macro_rules! command_error_kinds {
    (
        $(
            $kind:ident => {
                code: $code:literal,
                severity: $severity:ident,
                actions: $actions:expr $(,)?
            }
        ),+ $(,)?
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub(crate) enum CommandErrorKind {
            $($kind),+
        }

        impl CommandErrorKind {
            #[cfg(test)]
            pub(crate) const ALL: &'static [Self] = &[
                $(Self::$kind),+
            ];

            #[must_use]
            pub(crate) const fn spec(self) -> ErrorSpec {
                match self {
                    $(
                        Self::$kind => ErrorSpec {
                            code: $code,
                            severity: CommandErrorSeverity::$severity,
                            message_key: concat!("errors.", $code),
                            suggested_actions: $actions,
                        },
                    )+
                }
            }
        }
    };
}

command_error_kinds! {
    InvalidArgument => {
        code: "invalid_argument",
        severity: Warning,
        actions: suggested_action::RETRY_AFTER_REQUIRED_DATA,
    },
    InvalidGameId => {
        code: "invalid_game_id",
        severity: Warning,
        actions: suggested_action::REFRESH_GAMES,
    },
    InvalidComponentId => {
        code: "invalid_component_id",
        severity: Warning,
        actions: suggested_action::RELOAD_GAME_DETAILS,
    },
    InvalidArtifactId => {
        code: "invalid_artifact_id",
        severity: Warning,
        actions: suggested_action::REFRESH_CANDIDATES,
    },
    InvalidOperationId => {
        code: "invalid_operation_id",
        severity: Warning,
        actions: suggested_action::REBUILD_PLAN_OR_RELOAD_OPERATIONS,
    },

    SerializationFailed => {
        code: "serialization_failed",
        severity: Error,
        actions: suggested_action::INSPECT_LOGS,
    },
    ConfirmationTokenMismatch => {
        code: "confirmation_token_mismatch",
        severity: Warning,
        actions: suggested_action::REBUILD_OPERATION_PLAN,
    },
    GameNotFound => {
        code: "game_not_found",
        severity: Warning,
        actions: suggested_action::REFRESH_OR_SCAN_GAME_FOLDER,
    },
    OperationNotFound => {
        code: "operation_not_found",
        severity: Warning,
        actions: suggested_action::REBUILD_PLAN_OR_RELOAD_OPERATIONS,
    },
    ArtifactNotFound => {
        code: "artifact_not_found",
        severity: Warning,
        actions: suggested_action::REFRESH_CANDIDATES,
    },
    ComponentNotFound => {
        code: "component_not_found",
        severity: Warning,
        actions: suggested_action::RELOAD_GAME_DETAILS,
    },
    InvalidOperationState => {
        code: "invalid_operation_state",
        severity: Warning,
        actions: suggested_action::REBUILD_PLAN_OR_RELOAD_OPERATIONS,
    },
    CommandFailed => {
        code: "command_failed",
        severity: Error,
        actions: suggested_action::INSPECT_LOGS,
    },
    StorageFailed => {
        code: "storage_failed",
        severity: Error,
        actions: suggested_action::INSPECT_LOGS,
    },
    ProviderFailed => {
        code: "provider_failed",
        severity: Error,
        actions: suggested_action::RETRY_OR_RESTART,
    },
    DetectionFailed => {
        code: "detection_failed",
        severity: Error,
        actions: suggested_action::REFRESH_OR_SCAN_GAME_FOLDER,
    },
    CommandTaskFailed => {
        code: "command_task_failed",
        severity: Error,
        actions: suggested_action::RETRY_OR_RESTART,
    },
    SteamGridDbApiKeyMissing => {
        code: "steamgriddb_api_key_missing",
        severity: Warning,
        actions: suggested_action::INSPECT_LOGS,
    },
    UnsupportedCoverImageType => {
        code: "unsupported_cover_image_type",
        severity: Warning,
        actions: suggested_action::RETRY_OR_RESTART,
    },
    CoverDownloadFailed => {
        code: "cover_download_failed",
        severity: Error,
        actions: suggested_action::INSPECT_LOGS,
    },
    CoverNotFound => {
        code: "cover_not_found",
        severity: Warning,
        actions: suggested_action::REFRESH_OR_SCAN_GAME_FOLDER,
    },
    CoverIoError => {
        code: "cover_io_error",
        severity: Error,
        actions: suggested_action::INSPECT_LOGS,
    },
    NvapiRequiresElevation => {
        code: "nvapi_requires_elevation",
        severity: Warning,
        actions: suggested_action::RELAUNCH_AS_ADMINISTRATOR,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_unique_and_non_empty() {
        let mut seen = std::collections::HashSet::new();

        for kind in CommandErrorKind::ALL {
            let spec = kind.spec();

            assert!(!spec.code.is_empty(), "{kind:?} has an empty error code");

            assert!(
                seen.insert(spec.code),
                "duplicate command error code: {:?}",
                spec.code
            );
        }
    }

    #[test]
    fn message_keys_are_derived_from_error_codes() {
        for kind in CommandErrorKind::ALL {
            let spec = kind.spec();

            assert_eq!(
                spec.message_key,
                format!("errors.{}", spec.code),
                "{kind:?} has an invalid message key"
            );
        }
    }
}
