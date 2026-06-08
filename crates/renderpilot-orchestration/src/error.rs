use std::{error::Error, fmt};

use renderpilot_application::{invalid_operation_state_display_message, AppError, AppErrorKind};
use renderpilot_detection::LibraryPatternError;

/// Service-layer errors produced by orchestration feature modules.
///
/// These variants cover domain, infrastructure, and runtime failure modes.
/// Presentation concerns (id parsing, output serialisation) belong in the
/// consuming crates (`renderpilot-api` or `renderpilot-cli`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceError {
    /// The requested game was not found in the catalog.
    GameNotFound(String),
    /// The requested operation was not found in the catalog.
    OperationNotFound(String),
    /// The requested artifact was not found in the catalog.
    ArtifactNotFound(String),
    /// The requested component was not found for the given game.
    ComponentNotFound(String),
    /// Caller supplied malformed, incomplete, or inconsistent input.
    InvalidInput(String),
    /// A storage adapter (the catalog database) failed.
    StorageFailed(String),
    /// A game-source or remote provider failed.
    ProviderFailed(String),
    /// A graphics-component detector failed.
    DetectionFailed(String),
    /// A one-time confirmation token did not match.
    ConfirmationTokenMismatch,
    /// The operation is in an invalid state for the requested action.
    InvalidOperationState {
        /// The identifier of the operation in the invalid state.
        operation_id: String,
        /// The name of the invalid state, e.g. "completed".
        state: String,
    },
    /// A command failed while running.
    CommandFailed(String),
    /// SteamGridDB API key is required for this cover lookup but is not configured.
    SteamGridDbApiKeyMissing,
    /// Cover bytes are not a supported raster image type.
    UnsupportedCoverImageType,
    /// Cover artwork could not be fetched over the network.
    CoverDownloadFailed(String),
    /// No cover artwork was available from providers.
    CoverNotFound,
    /// Local filesystem error while reading or writing cover files.
    CoverIo(String),
    /// An NVAPI write was attempted without administrator privileges.
    NvapiRequiresElevation,
}

impl fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GameNotFound(id) => write!(formatter, "game not found: {id}"),
            Self::OperationNotFound(id) => write!(formatter, "operation not found: {id}"),
            Self::ArtifactNotFound(id) => write!(formatter, "artifact not found: {id}"),
            Self::ComponentNotFound(id) => write!(formatter, "component not found: {id}"),
            Self::InvalidInput(message) => write!(formatter, "invalid input: {message}"),
            Self::StorageFailed(message) => write!(formatter, "storage failed: {message}"),
            Self::ProviderFailed(message) => write!(formatter, "provider failed: {message}"),
            Self::DetectionFailed(message) => write!(formatter, "detection failed: {message}"),
            Self::ConfirmationTokenMismatch => {
                formatter.write_str("confirmation token mismatch for operation")
            }
            Self::InvalidOperationState {
                operation_id,
                state,
            } => formatter.write_str(&invalid_operation_state_display_message(
                operation_id,
                state.as_str(),
            )),
            Self::CommandFailed(message) => formatter.write_str(message),
            Self::SteamGridDbApiKeyMissing => {
                formatter.write_str("steamgriddb api key is not configured")
            }
            Self::UnsupportedCoverImageType => formatter.write_str("unsupported cover image type"),
            Self::CoverDownloadFailed(message) => {
                write!(formatter, "cover download failed: {message}")
            }
            Self::CoverNotFound => formatter.write_str("cover artwork was not found"),
            Self::CoverIo(message) => write!(formatter, "cover file error: {message}"),
            Self::NvapiRequiresElevation => formatter
                .write_str("administrator privileges are required to modify NVAPI settings"),
        }
    }
}

impl Error for ServiceError {}

impl From<AppError> for ServiceError {
    fn from(error: AppError) -> Self {
        let (kind, message) = error.into_parts();

        // Exhaustive on purpose: every `AppErrorKind` maps to a distinct
        // `ServiceError` so the stable error category survives all the way to the
        // frontend instead of collapsing into a generic `CommandFailed`. Adding a
        // new `AppErrorKind` must force a decision here.
        match kind {
            AppErrorKind::InvalidInput => Self::InvalidInput(message),
            AppErrorKind::StorageFailed => Self::StorageFailed(message),
            AppErrorKind::ProviderFailed => Self::ProviderFailed(message),
            AppErrorKind::DetectionFailed => Self::DetectionFailed(message),
            AppErrorKind::ConfirmationTokenMismatch => Self::ConfirmationTokenMismatch,
            AppErrorKind::GameNotFound => Self::GameNotFound(message),
            AppErrorKind::OperationNotFound => Self::OperationNotFound(message),
            AppErrorKind::ArtifactNotFound => Self::ArtifactNotFound(message),
            AppErrorKind::ComponentNotFound => Self::ComponentNotFound(message),
            AppErrorKind::InvalidOperationState {
                operation_id,
                state,
            } => Self::InvalidOperationState {
                operation_id,
                state: state.as_str().to_owned(),
            },
        }
    }
}

impl From<LibraryPatternError> for ServiceError {
    fn from(error: LibraryPatternError) -> Self {
        Self::CommandFailed(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_application::{AppError, AppErrorKind, OperationStatus};

    use super::ServiceError;

    #[test]
    fn not_found_variants_are_usage_like() {
        let errors = [
            ServiceError::GameNotFound("g1".to_owned()),
            ServiceError::OperationNotFound("op1".to_owned()),
            ServiceError::ArtifactNotFound("a1".to_owned()),
            ServiceError::ComponentNotFound("c1".to_owned()),
            ServiceError::ConfirmationTokenMismatch,
            ServiceError::InvalidOperationState {
                operation_id: "op".to_owned(),
                state: "planned".to_owned(),
            },
        ];

        for err in &errors {
            assert!(!err.to_string().is_empty(), "{err:?} has empty display");
        }
    }

    #[test]
    fn runtime_variants_display_correctly() {
        let errors = [
            ServiceError::CommandFailed("scan failed".to_owned()),
            ServiceError::SteamGridDbApiKeyMissing,
            ServiceError::UnsupportedCoverImageType,
            ServiceError::CoverDownloadFailed("timeout".to_owned()),
            ServiceError::CoverNotFound,
            ServiceError::CoverIo("permission denied".to_owned()),
            ServiceError::NvapiRequiresElevation,
        ];

        for err in &errors {
            assert!(!err.to_string().is_empty(), "{err:?} has empty display");
        }
    }

    #[test]
    fn app_error_invalid_operation_state_maps_to_service_error() {
        let app_error = AppError::invalid_operation_state("op-123", OperationStatus::Completed);
        assert!(matches!(
            app_error.kind(),
            &AppErrorKind::InvalidOperationState { .. }
        ));

        assert_eq!(
            ServiceError::from(app_error),
            ServiceError::InvalidOperationState {
                operation_id: "op-123".to_owned(),
                state: "completed".to_owned(),
            }
        );
    }

    #[test]
    fn app_error_invalid_operation_state_preserves_colon_in_operation_id() {
        let app_error = AppError::invalid_operation_state("op:part", OperationStatus::Running);
        assert_eq!(
            ServiceError::from(app_error),
            ServiceError::InvalidOperationState {
                operation_id: "op:part".to_owned(),
                state: "running".to_owned(),
            }
        );
    }

    #[test]
    fn app_error_categories_preserve_their_kind_through_service_error() {
        // Each stable category must survive the conversion with its own variant,
        // not collapse into a generic CommandFailed.
        assert_eq!(
            ServiceError::from(AppError::storage_failed("database locked")),
            ServiceError::StorageFailed("database locked".to_owned()),
        );
        assert_eq!(
            ServiceError::from(AppError::invalid_input("game id is required")),
            ServiceError::InvalidInput("game id is required".to_owned()),
        );
        assert_eq!(
            ServiceError::from(AppError::provider_failed("failed to install file")),
            ServiceError::ProviderFailed("failed to install file".to_owned()),
        );
        assert_eq!(
            ServiceError::from(AppError::detection_failed("could not read PE header")),
            ServiceError::DetectionFailed("could not read PE header".to_owned()),
        );
    }
}
