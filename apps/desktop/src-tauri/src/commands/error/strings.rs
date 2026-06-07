//! Defines the definitive registry of stable localization keys and fallback English copy for the frontend UI.
//!
//! The defined localization keys constitute a strictly stable API contract between the Rust backend and the
//! TypeScript frontend. Conversely, the default English fallback strings are treated as implementation details
//! and may be refined without requiring corresponding frontend modifications.

use std::fmt;

use serde::{Serialize, Serializer};

pub(crate) type SuggestedActions = &'static [SuggestedAction];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LocalizedText {
    key: &'static str,
    default_text: &'static str,
}

impl LocalizedText {
    pub(crate) const fn new(key: &'static str, default_text: &'static str) -> Self {
        Self { key, default_text }
    }

    #[must_use]
    pub(crate) const fn key(self) -> &'static str {
        self.key
    }

    #[must_use]
    pub(crate) const fn default_text(self) -> &'static str {
        self.default_text
    }
}

impl fmt::Display for LocalizedText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.default_text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum UserMessage {
    InvalidArgument,

    InvalidGameReference,
    InvalidComponentReference,
    InvalidArtifactReference,
    InvalidOperationReference,

    ResponseSerializationFailed,
    PlanChangedRebuild,

    GameNotInCatalog,
    OperationNotFound,
    ArtifactNotFound,
    ComponentNotFound,

    InvalidOperationState,

    OperationCouldNotComplete,
    CommandTaskFailed,

    SteamGridDbApiKeyMissing,
    UnsupportedCoverImageType,
    CoverDownloadFailed,
    CoverArtworkNotFound,
    CoverFileSystemError,

    NvapiRequiresAdministrator,
}

impl UserMessage {
    #[cfg(test)]
    pub(crate) const ALL: &'static [Self] = &[
        Self::InvalidArgument,
        Self::InvalidGameReference,
        Self::InvalidComponentReference,
        Self::InvalidArtifactReference,
        Self::InvalidOperationReference,
        Self::ResponseSerializationFailed,
        Self::PlanChangedRebuild,
        Self::GameNotInCatalog,
        Self::OperationNotFound,
        Self::ArtifactNotFound,
        Self::ComponentNotFound,
        Self::InvalidOperationState,
        Self::OperationCouldNotComplete,
        Self::CommandTaskFailed,
        Self::SteamGridDbApiKeyMissing,
        Self::UnsupportedCoverImageType,
        Self::CoverDownloadFailed,
        Self::CoverArtworkNotFound,
        Self::CoverFileSystemError,
        Self::NvapiRequiresAdministrator,
    ];

    #[must_use]
    pub(crate) const fn localized_text(self) -> LocalizedText {
        match self {
            Self::InvalidArgument => LocalizedText::new(
                "user_message.invalid_argument",
                "The request contains an invalid value.",
            ),

            Self::InvalidGameReference => LocalizedText::new(
                "user_message.invalid_game_reference",
                "That game reference is not valid.",
            ),
            Self::InvalidComponentReference => LocalizedText::new(
                "user_message.invalid_component_reference",
                "That component reference is not valid.",
            ),
            Self::InvalidArtifactReference => LocalizedText::new(
                "user_message.invalid_artifact_reference",
                "That artifact reference is not valid.",
            ),
            Self::InvalidOperationReference => LocalizedText::new(
                "user_message.invalid_operation_reference",
                "That operation reference is not valid.",
            ),

            Self::ResponseSerializationFailed => LocalizedText::new(
                "user_message.response_serialization_failed",
                "The app could not prepare the response.",
            ),
            Self::PlanChangedRebuild => LocalizedText::new(
                "user_message.plan_changed_rebuild",
                "The plan may have changed. Rebuild the plan and try again.",
            ),

            Self::GameNotInCatalog => LocalizedText::new(
                "user_message.game_not_in_catalog",
                "That game is not in the catalog.",
            ),
            Self::OperationNotFound => LocalizedText::new(
                "user_message.operation_not_found",
                "That operation was not found.",
            ),
            Self::ArtifactNotFound => LocalizedText::new(
                "user_message.artifact_not_found",
                "That artifact was not found.",
            ),
            Self::ComponentNotFound => LocalizedText::new(
                "user_message.component_not_found",
                "That component was not found.",
            ),

            Self::InvalidOperationState => LocalizedText::new(
                "user_message.invalid_operation_state",
                "This operation cannot be used in its current state.",
            ),

            Self::OperationCouldNotComplete => LocalizedText::new(
                "user_message.operation_could_not_complete",
                "The operation could not be completed.",
            ),
            Self::CommandTaskFailed => LocalizedText::new(
                "user_message.command_task_failed",
                "The command could not be completed.",
            ),

            Self::SteamGridDbApiKeyMissing => LocalizedText::new(
                "user_message.steamgriddb_api_key_missing",
                "Add a SteamGridDB API key to fetch artwork for this game.",
            ),

            Self::UnsupportedCoverImageType => LocalizedText::new(
                "user_message.unsupported_cover_image_type",
                "That image type cannot be used as a cover.",
            ),

            Self::CoverDownloadFailed => LocalizedText::new(
                "user_message.cover_download_failed",
                "The cover image could not be downloaded.",
            ),

            Self::CoverArtworkNotFound => LocalizedText::new(
                "user_message.cover_artwork_not_found",
                "No cover artwork was found for this game.",
            ),

            Self::CoverFileSystemError => LocalizedText::new(
                "user_message.cover_file_system_error",
                "Cover storage reported a filesystem error.",
            ),

            Self::NvapiRequiresAdministrator => LocalizedText::new(
                "user_message.nvapi_requires_administrator",
                "Administrator privileges are required to change this NVIDIA setting.",
            ),
        }
    }

    #[must_use]
    pub(crate) const fn key(self) -> &'static str {
        self.localized_text().key()
    }

    #[must_use]
    pub(crate) const fn default_text(self) -> &'static str {
        self.localized_text().default_text()
    }
}

impl fmt::Display for UserMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.localized_text().fmt(f)
    }
}

impl Serialize for UserMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.default_text())
    }
}

pub(crate) mod user_message {
    use super::UserMessage;

    pub(crate) const INVALID_ARGUMENT: UserMessage = UserMessage::InvalidArgument;

    pub(crate) const INVALID_GAME_REFERENCE: UserMessage = UserMessage::InvalidGameReference;
    pub(crate) const INVALID_COMPONENT_REFERENCE: UserMessage =
        UserMessage::InvalidComponentReference;
    pub(crate) const INVALID_ARTIFACT_REFERENCE: UserMessage =
        UserMessage::InvalidArtifactReference;
    pub(crate) const INVALID_OPERATION_REFERENCE: UserMessage =
        UserMessage::InvalidOperationReference;

    pub(crate) const RESPONSE_SERIALIZATION_FAILED: UserMessage =
        UserMessage::ResponseSerializationFailed;
    pub(crate) const PLAN_CHANGED_REBUILD: UserMessage = UserMessage::PlanChangedRebuild;

    pub(crate) const GAME_NOT_IN_CATALOG: UserMessage = UserMessage::GameNotInCatalog;
    pub(crate) const OPERATION_NOT_FOUND: UserMessage = UserMessage::OperationNotFound;
    pub(crate) const ARTIFACT_NOT_FOUND: UserMessage = UserMessage::ArtifactNotFound;
    pub(crate) const COMPONENT_NOT_FOUND: UserMessage = UserMessage::ComponentNotFound;

    pub(crate) const INVALID_OPERATION_STATE: UserMessage = UserMessage::InvalidOperationState;

    pub(crate) const OPERATION_COULD_NOT_COMPLETE: UserMessage =
        UserMessage::OperationCouldNotComplete;
    pub(crate) const COMMAND_TASK_FAILED: UserMessage = UserMessage::CommandTaskFailed;

    pub(crate) const STEAMGRIDDB_API_KEY_MISSING: UserMessage =
        UserMessage::SteamGridDbApiKeyMissing;
    pub(crate) const UNSUPPORTED_COVER_IMAGE_TYPE: UserMessage =
        UserMessage::UnsupportedCoverImageType;
    pub(crate) const COVER_DOWNLOAD_FAILED: UserMessage = UserMessage::CoverDownloadFailed;
    pub(crate) const COVER_ARTWORK_NOT_FOUND: UserMessage = UserMessage::CoverArtworkNotFound;
    pub(crate) const COVER_FILE_SYSTEM_ERROR: UserMessage = UserMessage::CoverFileSystemError;

    pub(crate) const NVAPI_REQUIRES_ADMINISTRATOR: UserMessage =
        UserMessage::NvapiRequiresAdministrator;

    #[cfg(test)]
    pub(crate) const ALL: &[UserMessage] = UserMessage::ALL;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SuggestedAction {
    RefreshGames,
    ReloadGameDetails,
    RefreshCandidates,
    RebuildPlanOrReloadOperations,
    RetryAfterRequiredData,

    InspectLogs,
    RetryOrRestart,
    RebuildOperationPlan,
    RefreshOrScanGameFolder,

    RelaunchAsAdministrator,
}

impl SuggestedAction {
    #[cfg(test)]
    pub(crate) const ALL: &'static [Self] = &[
        Self::RefreshGames,
        Self::ReloadGameDetails,
        Self::RefreshCandidates,
        Self::RebuildPlanOrReloadOperations,
        Self::RetryAfterRequiredData,
        Self::InspectLogs,
        Self::RetryOrRestart,
        Self::RebuildOperationPlan,
        Self::RefreshOrScanGameFolder,
        Self::RelaunchAsAdministrator,
    ];

    #[must_use]
    pub(crate) const fn localized_text(self) -> LocalizedText {
        match self {
            Self::RefreshGames => LocalizedText::new(
                "suggested_action.refresh_games",
                "Refresh the games list and open the game again.",
            ),
            Self::ReloadGameDetails => LocalizedText::new(
                "suggested_action.reload_game_details",
                "Reload the game details before building a new plan.",
            ),
            Self::RefreshCandidates => LocalizedText::new(
                "suggested_action.refresh_candidates",
                "Refresh replacement candidates and try again.",
            ),
            Self::RebuildPlanOrReloadOperations => LocalizedText::new(
                "suggested_action.rebuild_plan_or_reload_operations",
                "Rebuild the plan or reload the operations list before retrying.",
            ),
            Self::RetryAfterRequiredData => LocalizedText::new(
                "suggested_action.retry_after_required_data",
                "Retry the action after the required data is available.",
            ),

            Self::InspectLogs => LocalizedText::new(
                "suggested_action.inspect_logs",
                "Retry the action. If the problem persists, inspect the desktop logs.",
            ),
            Self::RetryOrRestart => LocalizedText::new(
                "suggested_action.retry_or_restart",
                "Retry the action. If the problem persists, restart the desktop app.",
            ),
            Self::RebuildOperationPlan => LocalizedText::new(
                "suggested_action.rebuild_operation_plan",
                "Rebuild the operation plan before applying it again.",
            ),
            Self::RefreshOrScanGameFolder => LocalizedText::new(
                "suggested_action.refresh_or_scan_game_folder",
                "Refresh the catalog or scan the game folder again.",
            ),

            Self::RelaunchAsAdministrator => LocalizedText::new(
                "suggested_action.relaunch_as_administrator",
                "Relaunch RenderPilot as administrator to apply NVIDIA settings.",
            ),
        }
    }

    #[must_use]
    pub(crate) const fn key(self) -> &'static str {
        self.localized_text().key()
    }

    #[must_use]
    pub(crate) const fn default_text(self) -> &'static str {
        self.localized_text().default_text()
    }
}

impl fmt::Display for SuggestedAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.localized_text().fmt(f)
    }
}

impl Serialize for SuggestedAction {
    /// Serializes as `{ key, text }`: the stable localization key drives the
    /// frontend translation, with `text` as the English fallback if the key is
    /// not present in the UI catalog. Symmetric with the (messageKey, details)
    /// pair carried for the primary user message.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("SuggestedAction", 2)?;
        state.serialize_field("key", self.key())?;
        state.serialize_field("text", self.default_text())?;
        state.end()
    }
}

pub(crate) mod suggested_action {
    use super::{SuggestedAction, SuggestedActions};

    pub(crate) const REFRESH_GAMES: SuggestedActions = &[SuggestedAction::RefreshGames];
    pub(crate) const RELOAD_GAME_DETAILS: SuggestedActions = &[SuggestedAction::ReloadGameDetails];
    pub(crate) const REFRESH_CANDIDATES: SuggestedActions = &[SuggestedAction::RefreshCandidates];

    pub(crate) const REBUILD_PLAN_OR_RELOAD_OPERATIONS: SuggestedActions =
        &[SuggestedAction::RebuildPlanOrReloadOperations];

    pub(crate) const RETRY_AFTER_REQUIRED_DATA: SuggestedActions =
        &[SuggestedAction::RetryAfterRequiredData];

    pub(crate) const INSPECT_LOGS: SuggestedActions = &[SuggestedAction::InspectLogs];
    pub(crate) const RETRY_OR_RESTART: SuggestedActions = &[SuggestedAction::RetryOrRestart];

    pub(crate) const REBUILD_OPERATION_PLAN: SuggestedActions =
        &[SuggestedAction::RebuildOperationPlan];

    pub(crate) const REFRESH_OR_SCAN_GAME_FOLDER: SuggestedActions =
        &[SuggestedAction::RefreshOrScanGameFolder];

    pub(crate) const RELAUNCH_AS_ADMINISTRATOR: SuggestedActions =
        &[SuggestedAction::RelaunchAsAdministrator];

    #[cfg(test)]
    pub(crate) const ALL: &[SuggestedAction] = SuggestedAction::ALL;
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::json;

    use super::{suggested_action, user_message};

    #[test]
    fn user_message_keys_are_unique() {
        assert_unique_keys(user_message::ALL.iter().map(|message| message.key()));
    }

    #[test]
    fn suggested_action_keys_are_unique() {
        assert_unique_keys(suggested_action::ALL.iter().map(|action| action.key()));
    }

    #[test]
    fn user_messages_have_valid_localized_text() {
        for &message in user_message::ALL {
            assert_valid_localized_text("user_message.", message.key(), message.default_text());
        }
    }

    #[test]
    fn suggested_actions_have_valid_localized_text() {
        for &action in suggested_action::ALL {
            assert_valid_localized_text("suggested_action.", action.key(), action.default_text());
        }
    }

    #[test]
    fn user_message_serializes_as_default_text() {
        assert_eq!(
            serde_json::to_value(user_message::INVALID_GAME_REFERENCE)
                .expect("serialize user message"),
            json!("That game reference is not valid.")
        );
    }

    #[test]
    fn invalid_argument_user_message_serializes_as_default_text() {
        assert_eq!(
            serde_json::to_value(user_message::INVALID_ARGUMENT)
                .expect("serialize invalid argument user message"),
            json!("The request contains an invalid value.")
        );
    }

    #[test]
    fn suggested_action_serializes_with_key_and_text() {
        assert_eq!(
            serde_json::to_value(suggested_action::REFRESH_GAMES)
                .expect("serialize suggested actions"),
            json!([{
                "key": "suggested_action.refresh_games",
                "text": "Refresh the games list and open the game again.",
            }])
        );
    }

    fn assert_unique_keys(keys: impl IntoIterator<Item = &'static str>) {
        let mut seen = HashSet::new();

        for key in keys {
            assert!(seen.insert(key), "duplicate localization key: {key}");
        }
    }

    fn assert_valid_localized_text(expected_prefix: &str, key: &str, default_text: &str) {
        assert!(!key.trim().is_empty(), "localization key must not be empty");
        assert_eq!(
            key,
            key.trim(),
            "localization key must not have surrounding whitespace: {key:?}",
        );
        assert!(
            key.starts_with(expected_prefix),
            "localization key must start with {expected_prefix:?}: {key}",
        );
        assert!(
            key.split('.').all(|segment| !segment.is_empty()),
            "localization key must not contain empty segments: {key}",
        );
        assert!(
            key.chars().all(is_valid_key_char),
            "localization key contains unsupported characters: {key}",
        );

        assert!(
            !default_text.trim().is_empty(),
            "default text must not be empty for key: {key}",
        );
        assert_eq!(
            default_text,
            default_text.trim(),
            "default text must not have surrounding whitespace for key: {key}",
        );
    }

    fn is_valid_key_char(ch: char) -> bool {
        ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '.'
    }
}
