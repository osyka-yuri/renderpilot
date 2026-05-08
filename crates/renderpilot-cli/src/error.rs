use std::{borrow::Cow, error::Error, fmt};

use renderpilot_application::{invalid_operation_state_display_message, AppError, AppErrorKind};
use renderpilot_detection::LibraryPatternError;

use crate::output::HELP_HINT;

pub const GENERAL_FAILURE_EXIT_CODE: u8 = 1;
pub const USAGE_FAILURE_EXIT_CODE: u8 = 2;

/// Error returned when CLI arguments cannot be parsed or a command fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// The user passed an argument that is not valid Unicode.
    NonUnicodeArgument,
    /// The user passed an argument that RenderPilot does not recognize.
    UnknownArgument(String),
    /// The user passed more arguments than the current command accepts.
    UnexpectedArgument(String),
    /// The user omitted a required argument.
    MissingArgument(&'static str),
    /// The user passed a game identifier that RenderPilot could not parse.
    InvalidGameId(String),
    /// The user passed a component identifier that RenderPilot could not parse.
    InvalidComponentId(String),
    /// The user passed an artifact identifier that RenderPilot could not parse.
    InvalidArtifactId(String),
    /// The user passed an operation identifier that RenderPilot could not parse.
    InvalidOperationId(String),
    /// The user passed a technology filter that RenderPilot does not recognize.
    InvalidTechnology(String),
    /// The requested game was not found in the catalog.
    GameNotFound(String),
    /// The requested operation was not found in the catalog.
    OperationNotFound(String),
    /// The requested artifact was not found in the catalog.
    ArtifactNotFound(String),
    /// The requested component was not found for the given game.
    ComponentNotFound(String),
    /// A one-time confirmation token did not match.
    ConfirmationTokenMismatch,
    /// The operation is in an invalid state for the requested action.
    InvalidOperationState {
        /// The identifier of the operation in the invalid state.
        operation_id: String,
        /// The name of the invalid state, e.g. "Completed".
        state: String,
    },
    /// A command failed while running.
    CommandFailed(String),
    /// CLI output could not be serialized.
    OutputSerializationFailed(String),
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
}

impl CliError {
    /// Returns the process exit code appropriate for this CLI error.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self.category() {
            ErrorCategory::Usage => USAGE_FAILURE_EXIT_CODE,
            ErrorCategory::Runtime => GENERAL_FAILURE_EXIT_CODE,
        }
    }

    const fn category(&self) -> ErrorCategory {
        match self {
            Self::CommandFailed(_)
            | Self::OutputSerializationFailed(_)
            | Self::SteamGridDbApiKeyMissing
            | Self::UnsupportedCoverImageType
            | Self::CoverDownloadFailed(_)
            | Self::CoverNotFound
            | Self::CoverIo(_) => ErrorCategory::Runtime,
            _ => ErrorCategory::Usage,
        }
    }

    fn usage_message(&self) -> Option<Cow<'_, str>> {
        match self {
            Self::NonUnicodeArgument => Some(Cow::Borrowed("arguments must be valid Unicode")),
            Self::UnknownArgument(arg) => Some(Cow::Owned(format!("unknown argument: {arg}"))),
            Self::UnexpectedArgument(arg) => {
                Some(Cow::Owned(format!("unexpected argument: {arg}")))
            }
            Self::MissingArgument(arg) => {
                Some(Cow::Owned(format!("missing required argument: {arg}")))
            }
            Self::InvalidGameId(id) => Some(Cow::Owned(format!("invalid game id: {id}"))),
            Self::InvalidComponentId(id) => Some(Cow::Owned(format!("invalid component id: {id}"))),
            Self::InvalidArtifactId(id) => Some(Cow::Owned(format!("invalid artifact id: {id}"))),
            Self::InvalidOperationId(id) => Some(Cow::Owned(format!("invalid operation id: {id}"))),
            Self::InvalidTechnology(tech) => {
                Some(Cow::Owned(format!("unknown technology: {tech}")))
            }
            Self::GameNotFound(id) => Some(Cow::Owned(format!("game not found: {id}"))),
            Self::OperationNotFound(id) => Some(Cow::Owned(format!("operation not found: {id}"))),
            Self::ArtifactNotFound(id) => Some(Cow::Owned(format!("artifact not found: {id}"))),
            Self::ComponentNotFound(id) => Some(Cow::Owned(format!("component not found: {id}"))),
            Self::ConfirmationTokenMismatch => {
                Some(Cow::Borrowed("confirmation token mismatch for operation"))
            }
            Self::InvalidOperationState {
                operation_id,
                state,
            } => Some(Cow::Owned(invalid_operation_state_display_message(
                operation_id,
                state.as_str(),
            ))),
            Self::CommandFailed(_)
            | Self::OutputSerializationFailed(_)
            | Self::SteamGridDbApiKeyMissing
            | Self::UnsupportedCoverImageType
            | Self::CoverDownloadFailed(_)
            | Self::CoverNotFound
            | Self::CoverIo(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorCategory {
    Usage,
    Runtime,
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(message) = self.usage_message() {
            return write_usage_error(formatter, message);
        }

        match self {
            Self::CommandFailed(message) => formatter.write_str(message),
            Self::OutputSerializationFailed(message) => {
                write!(formatter, "could not serialize CLI output: {message}")
            }
            Self::SteamGridDbApiKeyMissing => {
                formatter.write_str("steamgriddb api key is not configured")
            }
            Self::UnsupportedCoverImageType => formatter.write_str("unsupported cover image type"),
            Self::CoverDownloadFailed(message) => {
                write!(formatter, "cover download failed: {message}")
            }
            Self::CoverNotFound => formatter.write_str("cover artwork was not found"),
            Self::CoverIo(message) => write!(formatter, "cover file error: {message}"),
            _ => unreachable!("usage errors are handled by usage_message"),
        }
    }
}

impl From<AppError> for CliError {
    fn from(error: AppError) -> Self {
        let (kind, message) = error.into_parts();

        match kind {
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
            kind => Self::CommandFailed(format!("{kind}: {message}")),
        }
    }
}

impl From<LibraryPatternError> for CliError {
    fn from(error: LibraryPatternError) -> Self {
        Self::CommandFailed(error.to_string())
    }
}

impl From<serde_json::Error> for CliError {
    fn from(error: serde_json::Error) -> Self {
        Self::OutputSerializationFailed(error.to_string())
    }
}

impl Error for CliError {}

fn write_usage_error(
    formatter: &mut fmt::Formatter<'_>,
    message: impl fmt::Display,
) -> fmt::Result {
    write!(formatter, "{message}\n{HELP_HINT}")
}

#[cfg(test)]
mod tests {
    use renderpilot_application::{AppError, AppErrorKind, OperationStatus};

    use super::{CliError, GENERAL_FAILURE_EXIT_CODE, USAGE_FAILURE_EXIT_CODE};

    #[test]
    fn argument_errors_include_help_hint() {
        let error = CliError::UnknownArgument("--bad".to_owned());

        assert_eq!(
            error.to_string(),
            "unknown argument: --bad\nRun `renderpilot --help` for usage."
        );
    }

    #[test]
    fn all_usage_errors_include_help_hint() {
        let errors = [
            CliError::NonUnicodeArgument,
            CliError::UnknownArgument("--bad".to_owned()),
            CliError::UnexpectedArgument("--bad".to_owned()),
            CliError::MissingArgument("<path>"),
            CliError::InvalidGameId("bad".to_owned()),
            CliError::InvalidComponentId("bad".to_owned()),
            CliError::InvalidArtifactId("bad".to_owned()),
            CliError::InvalidOperationId("bad".to_owned()),
            CliError::InvalidTechnology("bad".to_owned()),
            CliError::GameNotFound("bad".to_owned()),
            CliError::OperationNotFound("bad".to_owned()),
            CliError::ArtifactNotFound("bad".to_owned()),
            CliError::ComponentNotFound("bad".to_owned()),
            CliError::ConfirmationTokenMismatch,
            CliError::InvalidOperationState {
                operation_id: "op".to_owned(),
                state: "planned".to_owned(),
            },
        ];

        for error in errors {
            assert!(
                error
                    .to_string()
                    .ends_with("Run `renderpilot --help` for usage."),
                "{error:?} did not include help hint"
            );
        }
    }

    #[test]
    fn usage_errors_use_usage_exit_code() {
        let errors = [
            CliError::NonUnicodeArgument,
            CliError::UnknownArgument("--bad".to_owned()),
            CliError::UnexpectedArgument("--bad".to_owned()),
            CliError::MissingArgument("<path>"),
            CliError::InvalidGameId("bad".to_owned()),
            CliError::InvalidComponentId("bad".to_owned()),
            CliError::InvalidArtifactId("bad".to_owned()),
            CliError::InvalidOperationId("bad".to_owned()),
            CliError::InvalidTechnology("bad".to_owned()),
            CliError::GameNotFound("bad".to_owned()),
            CliError::OperationNotFound("bad".to_owned()),
            CliError::ArtifactNotFound("bad".to_owned()),
            CliError::ComponentNotFound("bad".to_owned()),
            CliError::ConfirmationTokenMismatch,
            CliError::InvalidOperationState {
                operation_id: "op".to_owned(),
                state: "planned".to_owned(),
            },
        ];

        for error in errors {
            assert_eq!(error.exit_code(), USAGE_FAILURE_EXIT_CODE, "{error:?}");
        }
    }

    #[test]
    fn runtime_errors_use_general_failure_exit_code() {
        let errors = [
            CliError::CommandFailed("scan failed".into()),
            CliError::OutputSerializationFailed("json failed".into()),
            CliError::SteamGridDbApiKeyMissing,
            CliError::UnsupportedCoverImageType,
            CliError::CoverDownloadFailed("timeout".into()),
            CliError::CoverNotFound,
            CliError::CoverIo("permission denied".into()),
        ];

        for error in errors {
            assert_eq!(error.exit_code(), GENERAL_FAILURE_EXIT_CODE, "{error:?}");
        }
    }

    #[test]
    fn runtime_errors_do_not_include_help_hint() {
        let error = CliError::CommandFailed("scan failed".to_owned());

        assert_eq!(error.to_string(), "scan failed");
    }

    #[test]
    fn app_error_invalid_operation_state_maps_to_cli_error() {
        let app_error = AppError::invalid_operation_state("op-123", OperationStatus::Completed);
        assert!(matches!(
            app_error.kind(),
            &AppErrorKind::InvalidOperationState { .. }
        ));

        assert_eq!(
            CliError::from(app_error),
            CliError::InvalidOperationState {
                operation_id: "op-123".to_owned(),
                state: "completed".to_owned(),
            }
        );
    }

    #[test]
    fn app_error_invalid_operation_state_preserves_colon_in_operation_id() {
        let app_error = AppError::invalid_operation_state("op:part", OperationStatus::Running);
        assert_eq!(
            CliError::from(app_error),
            CliError::InvalidOperationState {
                operation_id: "op:part".to_owned(),
                state: "running".to_owned(),
            }
        );
    }
}
