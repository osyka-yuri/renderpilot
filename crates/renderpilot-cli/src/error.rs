use std::{error::Error, fmt};

use renderpilot_application::AppError;
use renderpilot_detection::LibraryPatternError;

use crate::output::HELP_HINT;

const GENERAL_FAILURE_EXIT_CODE: u8 = 1;
const USAGE_FAILURE_EXIT_CODE: u8 = 2;

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
    /// A command failed while running.
    CommandFailed(String),
    /// CLI output could not be serialized.
    OutputSerializationFailed(String),
}

impl CliError {
    /// Returns the process exit code appropriate for this CLI error.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        if self.is_usage_error() {
            return USAGE_FAILURE_EXIT_CODE;
        }

        GENERAL_FAILURE_EXIT_CODE
    }

    const fn is_usage_error(&self) -> bool {
        matches!(
            self,
            Self::NonUnicodeArgument
                | Self::UnknownArgument(_)
                | Self::UnexpectedArgument(_)
                | Self::MissingArgument(_)
                | Self::InvalidGameId(_)
                | Self::InvalidComponentId(_)
                | Self::InvalidArtifactId(_)
                | Self::InvalidOperationId(_)
                | Self::InvalidTechnology(_)
        )
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonUnicodeArgument => {
                write_usage_error(formatter, "arguments must be valid Unicode")
            }
            Self::UnknownArgument(argument) => {
                write_usage_error(formatter, format_args!("unknown argument: {argument}"))
            }
            Self::UnexpectedArgument(argument) => {
                write_usage_error(formatter, format_args!("unexpected argument: {argument}"))
            }
            Self::MissingArgument(argument) => write_usage_error(
                formatter,
                format_args!("missing required argument: {argument}"),
            ),
            Self::InvalidGameId(game_id) => {
                write_usage_error(formatter, format_args!("invalid game id: {game_id}"))
            }
            Self::InvalidComponentId(component_id) => write_usage_error(
                formatter,
                format_args!("invalid component id: {component_id}"),
            ),
            Self::InvalidArtifactId(artifact_id) => write_usage_error(
                formatter,
                format_args!("invalid artifact id: {artifact_id}"),
            ),
            Self::InvalidOperationId(operation_id) => write_usage_error(
                formatter,
                format_args!("invalid operation id: {operation_id}"),
            ),
            Self::InvalidTechnology(technology) => {
                write_usage_error(formatter, format_args!("unknown technology: {technology}"))
            }
            Self::CommandFailed(message) => formatter.write_str(message),
            Self::OutputSerializationFailed(message) => {
                write!(formatter, "could not serialize CLI output: {message}")
            }
        }
    }
}

impl From<AppError> for CliError {
    fn from(error: AppError) -> Self {
        Self::CommandFailed(error.to_string())
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
    use super::CliError;

    #[test]
    fn argument_errors_include_help_hint() {
        let error = CliError::UnknownArgument("--bad".to_owned());

        assert_eq!(
            error.to_string(),
            "unknown argument: --bad\nRun `renderpilot --help` for usage."
        );
    }

    #[test]
    fn usage_errors_use_usage_exit_code() {
        assert_eq!(CliError::NonUnicodeArgument.exit_code(), 2);
        assert_eq!(CliError::UnknownArgument("--bad".to_owned()).exit_code(), 2);
        assert_eq!(
            CliError::UnexpectedArgument("--bad".to_owned()).exit_code(),
            2
        );
        assert_eq!(CliError::MissingArgument("<path>").exit_code(), 2);
        assert_eq!(CliError::InvalidGameId("bad".to_owned()).exit_code(), 2);
        assert_eq!(
            CliError::InvalidComponentId("bad".to_owned()).exit_code(),
            2
        );
        assert_eq!(CliError::InvalidArtifactId("bad".to_owned()).exit_code(), 2);
        assert_eq!(
            CliError::InvalidOperationId("bad".to_owned()).exit_code(),
            2
        );
        assert_eq!(CliError::InvalidTechnology("bad".to_owned()).exit_code(), 2);
    }

    #[test]
    fn runtime_errors_use_general_failure_exit_code() {
        assert_eq!(
            CliError::CommandFailed("scan failed".to_owned()).exit_code(),
            1
        );
        assert_eq!(
            CliError::OutputSerializationFailed("json failed".to_owned()).exit_code(),
            1
        );
    }
}
