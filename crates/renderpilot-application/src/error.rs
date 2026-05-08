use std::{error::Error, fmt};

use crate::persistence::OperationStatus;

/// Result type used by application use cases and ports.
pub type AppResult<T> = Result<T, AppError>;

/// Stable human-readable line for an invalid operation state (shared by adapters).
#[must_use]
pub fn invalid_operation_state_display_message(
    operation_id: &str,
    state: impl fmt::Display,
) -> String {
    format!("operation {operation_id} is in invalid state: {state}")
}

/// Stable category for application-layer errors.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppErrorKind {
    /// Caller supplied malformed, incomplete or inconsistent input.
    InvalidInput,

    /// A game source provider failed.
    ProviderFailed,

    /// A graphics component detector failed.
    DetectionFailed,

    /// A storage adapter failed.
    StorageFailed,

    /// A one-time confirmation token did not match the expected value.
    ConfirmationTokenMismatch,

    /// The requested game was not found.
    GameNotFound,

    /// The requested operation was not found.
    OperationNotFound,

    /// The requested artifact was not found.
    ArtifactNotFound,

    /// The requested component was not found.
    ComponentNotFound,

    /// The operation is in an invalid state for the requested transition.
    InvalidOperationState {
        /// Operation identifier (as persisted or referenced by the caller).
        operation_id: String,
        /// Observed operation status that does not allow the requested action.
        state: OperationStatus,
    },
}

impl AppErrorKind {
    /// Returns a stable machine-readable error code.
    ///
    /// Prefer this for metrics, logs, telemetry and programmatic diagnostics.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::ProviderFailed => "provider_failed",
            Self::DetectionFailed => "detection_failed",
            Self::StorageFailed => "storage_failed",
            Self::ConfirmationTokenMismatch => "confirmation_token_mismatch",
            Self::GameNotFound => "game_not_found",
            Self::OperationNotFound => "operation_not_found",
            Self::ArtifactNotFound => "artifact_not_found",
            Self::ComponentNotFound => "component_not_found",
            Self::InvalidOperationState { .. } => "invalid_operation_state",
        }
    }

    /// Returns a stable human-readable error category.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid input",
            Self::ProviderFailed => "provider failed",
            Self::DetectionFailed => "detection failed",
            Self::StorageFailed => "storage failed",
            Self::ConfirmationTokenMismatch => "confirmation token mismatch",
            Self::GameNotFound => "game not found",
            Self::OperationNotFound => "operation not found",
            Self::ArtifactNotFound => "artifact not found",
            Self::ComponentNotFound => "component not found",
            Self::InvalidOperationState { .. } => "invalid operation state",
        }
    }
}

impl fmt::Display for AppErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Application-layer error with a stable category and a human-readable message.
///
/// `AppErrorKind` is intended for branching, metrics, logging and diagnostics.
/// `message` is intended for humans and should not be parsed programmatically.
///
/// For [`AppErrorKind::InvalidOperationState`], [`AppError::new`] always derives
/// `message` from `operation_id` and `state`; any caller-supplied text for that
/// variant is ignored so `kind` and `message` cannot diverge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppError {
    kind: AppErrorKind,
    message: String,
}

impl AppError {
    /// Creates an application error.
    #[must_use]
    #[track_caller]
    pub fn new(kind: AppErrorKind, message: impl Into<String>) -> Self {
        let message = match &kind {
            AppErrorKind::InvalidOperationState {
                operation_id,
                state,
            } => Self::normalize_message(
                &kind,
                invalid_operation_state_display_message(operation_id, state),
            ),
            _ => Self::normalize_message(&kind, message),
        };
        Self { kind, message }
    }

    /// Creates an invalid-input error.
    #[must_use]
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(AppErrorKind::InvalidInput, message)
    }

    /// Creates a provider error.
    #[must_use]
    pub fn provider_failed(message: impl Into<String>) -> Self {
        Self::new(AppErrorKind::ProviderFailed, message)
    }

    /// Creates a detection error.
    #[must_use]
    pub fn detection_failed(message: impl Into<String>) -> Self {
        Self::new(AppErrorKind::DetectionFailed, message)
    }

    /// Creates a storage error.
    #[must_use]
    pub fn storage_failed(message: impl Into<String>) -> Self {
        Self::new(AppErrorKind::StorageFailed, message)
    }

    /// Creates a confirmation token mismatch error.
    #[must_use]
    pub fn confirmation_token_mismatch() -> Self {
        Self::new(
            AppErrorKind::ConfirmationTokenMismatch,
            "confirmation token mismatch for operation",
        )
    }

    /// Creates an error for when a game is not found in the catalog.
    #[must_use]
    pub fn game_not_found(game_id: impl Into<String>) -> Self {
        let game_id = game_id.into();

        Self::new(
            AppErrorKind::GameNotFound,
            format!("game `{game_id}` was not found in the catalog"),
        )
    }

    /// Creates an error for when an operation is not found in the catalog.
    #[must_use]
    pub fn operation_not_found(operation_id: impl Into<String>) -> Self {
        let operation_id = operation_id.into();

        Self::new(
            AppErrorKind::OperationNotFound,
            format!("operation `{operation_id}` was not found"),
        )
    }

    /// Creates an error for when a replacement artifact is not found.
    #[must_use]
    pub fn artifact_not_found(artifact_id: impl Into<String>) -> Self {
        let artifact_id = artifact_id.into();

        Self::new(
            AppErrorKind::ArtifactNotFound,
            format!("artifact `{artifact_id}` was not found"),
        )
    }

    /// Creates an error for when a graphics component is not found for a game.
    #[must_use]
    pub fn component_not_found(component_id: impl Into<String>) -> Self {
        let component_id = component_id.into();

        Self::new(
            AppErrorKind::ComponentNotFound,
            format!("graphics component `{component_id}` was not found"),
        )
    }

    /// Creates an error for when an operation is in an invalid state for a transition.
    #[must_use]
    pub fn invalid_operation_state(
        operation_id: impl Into<String>,
        state: OperationStatus,
    ) -> Self {
        Self::new(
            AppErrorKind::InvalidOperationState {
                operation_id: operation_id.into(),
                state,
            },
            "",
        )
    }

    /// Returns the stable error category.
    #[must_use]
    pub fn kind(&self) -> &AppErrorKind {
        &self.kind
    }

    /// Returns the stable machine-readable error code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }

    /// Returns the human-readable error message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Consumes the error into its category and message (for adapters such as CLI).
    #[must_use]
    pub fn into_parts(self) -> (AppErrorKind, String) {
        let Self { kind, message } = self;
        (kind, message)
    }

    /// Returns `true` when this error has the given category.
    #[must_use]
    pub fn is(&self, kind: &AppErrorKind) -> bool {
        &self.kind == kind
    }

    fn normalize_message(kind: &AppErrorKind, message: impl Into<String>) -> String {
        let message = message.into();

        if message.trim().is_empty() {
            kind.as_str().to_owned()
        } else {
            message
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.kind, self.message)
    }
}

impl Error for AppError {}

#[cfg(test)]
mod tests {
    use super::{invalid_operation_state_display_message, AppError, AppErrorKind};
    use crate::persistence::OperationStatus;

    #[test]
    fn app_error_preserves_kind_code_and_message() {
        let error = AppError::invalid_input("game id is required");

        assert_eq!(error.kind(), &AppErrorKind::InvalidInput);
        assert_eq!(error.code(), "invalid_input");
        assert_eq!(error.message(), "game id is required");
        assert_eq!(error.to_string(), "invalid input: game id is required");
    }

    #[test]
    fn app_error_checks_kind() {
        let error = AppError::storage_failed("catalog database is unavailable");

        assert!(error.is(&AppErrorKind::StorageFailed));
        assert!(!error.is(&AppErrorKind::ProviderFailed));
    }

    #[test]
    fn app_error_kind_has_stable_display_text() {
        assert_eq!(AppErrorKind::InvalidInput.as_str(), "invalid input");
        assert_eq!(AppErrorKind::ProviderFailed.as_str(), "provider failed");
        assert_eq!(AppErrorKind::DetectionFailed.as_str(), "detection failed");
        assert_eq!(AppErrorKind::StorageFailed.as_str(), "storage failed");

        assert_eq!(
            AppErrorKind::DetectionFailed.to_string(),
            "detection failed"
        );
    }

    #[test]
    fn app_error_kind_has_stable_codes() {
        assert_eq!(AppErrorKind::InvalidInput.code(), "invalid_input");
        assert_eq!(AppErrorKind::ProviderFailed.code(), "provider_failed");
        assert_eq!(AppErrorKind::DetectionFailed.code(), "detection_failed");
        assert_eq!(AppErrorKind::StorageFailed.code(), "storage_failed");
        assert_eq!(
            AppErrorKind::ConfirmationTokenMismatch.code(),
            "confirmation_token_mismatch"
        );
        assert_eq!(AppErrorKind::GameNotFound.code(), "game_not_found");
        assert_eq!(
            AppErrorKind::OperationNotFound.code(),
            "operation_not_found"
        );
        assert_eq!(AppErrorKind::ArtifactNotFound.code(), "artifact_not_found");
        assert_eq!(
            AppErrorKind::ComponentNotFound.code(),
            "component_not_found"
        );
        assert_eq!(
            AppErrorKind::InvalidOperationState {
                operation_id: "op".into(),
                state: OperationStatus::Planned,
            }
            .code(),
            "invalid_operation_state"
        );
    }

    #[test]
    fn not_found_errors_use_human_readable_messages() {
        let error = AppError::game_not_found("elden-ring");

        assert_eq!(error.kind(), &AppErrorKind::GameNotFound);
        assert_eq!(
            error.message(),
            "game `elden-ring` was not found in the catalog"
        );
    }

    #[test]
    fn invalid_operation_state_uses_human_readable_message() {
        let error = AppError::invalid_operation_state("op-123", OperationStatus::Completed);

        assert_eq!(
            error.kind(),
            &AppErrorKind::InvalidOperationState {
                operation_id: "op-123".into(),
                state: OperationStatus::Completed,
            }
        );
        assert_eq!(
            error.message(),
            invalid_operation_state_display_message("op-123", &OperationStatus::Completed)
        );
        assert_eq!(
            error.message(),
            "operation op-123 is in invalid state: completed"
        );
    }

    #[test]
    fn new_invalid_operation_state_ignores_custom_message() {
        let error = AppError::new(
            AppErrorKind::InvalidOperationState {
                operation_id: "op-1".into(),
                state: OperationStatus::Running,
            },
            "custom text that must not appear",
        );

        assert_eq!(
            error.message(),
            invalid_operation_state_display_message("op-1", &OperationStatus::Running)
        );
        assert!(!error.message().contains("custom"));
    }

    #[test]
    fn empty_message_falls_back_to_kind_text() {
        let error = AppError::new(AppErrorKind::InvalidInput, "   ");

        assert_eq!(error.kind(), &AppErrorKind::InvalidInput);
        assert_eq!(error.message(), "invalid input");
        assert_eq!(error.to_string(), "invalid input: invalid input");
    }
}
