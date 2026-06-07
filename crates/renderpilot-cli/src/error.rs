use std::{borrow::Cow, error::Error, fmt};

use renderpilot_orchestration::application::AppError;
use renderpilot_orchestration::detection::LibraryPatternError;
use renderpilot_orchestration::ServiceError;

use crate::output::HELP_HINT;

pub const GENERAL_FAILURE_EXIT_CODE: u8 = 1;
pub const USAGE_FAILURE_EXIT_CODE: u8 = 2;

/// Represents an enumerated collection of failure states encountered during CLI argument
/// parsing, command orchestration, or execution runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// Indicates that the provided command-line argument contains invalid, non-Unicode sequences.
    NonUnicodeArgument,
    /// The user passed an argument that RenderPilot does not recognize.
    UnknownArgument(String),
    /// The user passed more arguments than the current command accepts.
    UnexpectedArgument(String),
    /// The user omitted a required argument.
    MissingArgument(&'static str),
    /// The user passed a technology filter that RenderPilot does not recognize.
    InvalidTechnology(String),
    /// The user passed a game identifier that RenderPilot could not parse.
    InvalidGameId(String),
    /// The user passed a component identifier that RenderPilot could not parse.
    InvalidComponentId(String),
    /// The user passed an artifact identifier that RenderPilot could not parse.
    InvalidArtifactId(String),
    /// The user passed an operation identifier that RenderPilot could not parse.
    InvalidOperationId(String),
    /// CLI output could not be serialized.
    OutputSerializationFailed(String),
    /// A service-layer error from orchestration.
    Service(ServiceError),
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
            Self::OutputSerializationFailed(_) => ErrorCategory::Runtime,
            Self::Service(e) => service_error_category(e),
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
            Self::InvalidTechnology(tech) => {
                Some(Cow::Owned(format!("unknown technology: {tech}")))
            }
            Self::InvalidGameId(id) => Some(Cow::Owned(format!("invalid game id: {id}"))),
            Self::InvalidComponentId(id) => Some(Cow::Owned(format!("invalid component id: {id}"))),
            Self::InvalidArtifactId(id) => Some(Cow::Owned(format!("invalid artifact id: {id}"))),
            Self::InvalidOperationId(id) => Some(Cow::Owned(format!("invalid operation id: {id}"))),
            Self::Service(e) if matches!(service_error_category(e), ErrorCategory::Usage) => {
                Some(Cow::Owned(e.to_string()))
            }
            _ => None,
        }
    }
}

/// Classify a `ServiceError` variant into `Usage` or `Runtime`.
///
/// Usage errors (exit 2, show help hint): not-found, invalid state, token mismatch.
/// Runtime errors (exit 1, no hint): everything else.
const fn service_error_category(error: &ServiceError) -> ErrorCategory {
    match error {
        ServiceError::CommandFailed(_)
        | ServiceError::SteamGridDbApiKeyMissing
        | ServiceError::UnsupportedCoverImageType
        | ServiceError::CoverDownloadFailed(_)
        | ServiceError::CoverNotFound
        | ServiceError::CoverIo(_)
        | ServiceError::NvapiRequiresElevation => ErrorCategory::Runtime,
        _ => ErrorCategory::Usage,
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
            Self::OutputSerializationFailed(message) => {
                write!(formatter, "could not serialize CLI output: {message}")
            }
            Self::Service(e) => fmt::Display::fmt(e, formatter),
            _ => unreachable!("usage errors are handled by usage_message"),
        }
    }
}

impl From<ServiceError> for CliError {
    fn from(error: ServiceError) -> Self {
        Self::Service(error)
    }
}

impl From<AppError> for CliError {
    fn from(error: AppError) -> Self {
        Self::Service(ServiceError::from(error))
    }
}

impl From<LibraryPatternError> for CliError {
    fn from(error: LibraryPatternError) -> Self {
        Self::Service(ServiceError::from(error))
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
    use renderpilot_orchestration::application::{AppError, AppErrorKind, OperationStatus};
    use renderpilot_orchestration::ServiceError;

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
            CliError::Service(ServiceError::GameNotFound("bad".to_owned())),
            CliError::Service(ServiceError::OperationNotFound("bad".to_owned())),
            CliError::Service(ServiceError::ArtifactNotFound("bad".to_owned())),
            CliError::Service(ServiceError::ComponentNotFound("bad".to_owned())),
            CliError::Service(ServiceError::ConfirmationTokenMismatch),
            CliError::Service(ServiceError::InvalidOperationState {
                operation_id: "op".to_owned(),
                state: "planned".to_owned(),
            }),
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
            CliError::Service(ServiceError::GameNotFound("bad".to_owned())),
            CliError::Service(ServiceError::OperationNotFound("bad".to_owned())),
            CliError::Service(ServiceError::ArtifactNotFound("bad".to_owned())),
            CliError::Service(ServiceError::ComponentNotFound("bad".to_owned())),
            CliError::Service(ServiceError::ConfirmationTokenMismatch),
            CliError::Service(ServiceError::InvalidOperationState {
                operation_id: "op".to_owned(),
                state: "planned".to_owned(),
            }),
        ];

        for error in errors {
            assert_eq!(error.exit_code(), USAGE_FAILURE_EXIT_CODE, "{error:?}");
        }
    }

    #[test]
    fn runtime_errors_use_general_failure_exit_code() {
        let errors = [
            CliError::OutputSerializationFailed("json failed".into()),
            CliError::Service(ServiceError::CommandFailed("scan failed".into())),
            CliError::Service(ServiceError::SteamGridDbApiKeyMissing),
            CliError::Service(ServiceError::UnsupportedCoverImageType),
            CliError::Service(ServiceError::CoverDownloadFailed("timeout".into())),
            CliError::Service(ServiceError::CoverNotFound),
            CliError::Service(ServiceError::CoverIo("permission denied".into())),
            CliError::Service(ServiceError::NvapiRequiresElevation),
        ];

        for error in errors {
            assert_eq!(error.exit_code(), GENERAL_FAILURE_EXIT_CODE, "{error:?}");
        }
    }

    #[test]
    fn runtime_errors_do_not_include_help_hint() {
        let errors = [
            CliError::Service(ServiceError::CommandFailed("scan failed".to_owned())),
            CliError::Service(ServiceError::NvapiRequiresElevation),
        ];

        for error in errors {
            assert!(
                !error
                    .to_string()
                    .ends_with("Run `renderpilot --help` for usage."),
                "{error:?} should not include help hint",
            );
        }
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
            CliError::Service(ServiceError::InvalidOperationState {
                operation_id: "op-123".to_owned(),
                state: "completed".to_owned(),
            })
        );
    }

    #[test]
    fn app_error_invalid_operation_state_preserves_colon_in_operation_id() {
        let app_error = AppError::invalid_operation_state("op:part", OperationStatus::Running);
        assert_eq!(
            CliError::from(app_error),
            CliError::Service(ServiceError::InvalidOperationState {
                operation_id: "op:part".to_owned(),
                state: "running".to_owned(),
            })
        );
    }
}
