use renderpilot_cli::CliError;

use super::{
    kind::CommandErrorKind as Kind,
    strings::{user_message as Msg, UserMessage},
    CommandError,
};

impl CommandError {
    pub(crate) fn from_cli_error(error: CliError) -> Self {
        match error {
            CliError::InvalidGameId(id) => Self::invalid_id(
                Kind::InvalidGameId,
                Msg::INVALID_GAME_REFERENCE,
                "Invalid game id",
                id,
            ),

            CliError::InvalidComponentId(id) => Self::invalid_id(
                Kind::InvalidComponentId,
                Msg::INVALID_COMPONENT_REFERENCE,
                "Invalid component id",
                id,
            ),

            CliError::InvalidArtifactId(id) => Self::invalid_id(
                Kind::InvalidArtifactId,
                Msg::INVALID_ARTIFACT_REFERENCE,
                "Invalid artifact id",
                id,
            ),

            CliError::InvalidOperationId(id) => Self::invalid_id(
                Kind::InvalidOperationId,
                Msg::INVALID_OPERATION_REFERENCE,
                "Invalid operation id",
                id,
            ),

            CliError::MissingArgument(argument) => Self::debug(
                Kind::MissingArgument,
                Msg::MISSING_REQUIRED_INFO,
                format!("Missing required argument: {argument}"),
            ),

            CliError::UnexpectedArgument(argument) => Self::debug(
                Kind::UnexpectedArgument,
                Msg::UNEXPECTED_INPUT,
                format!("Unexpected argument: {argument}"),
            ),

            CliError::UnknownArgument(argument) => Self::debug(
                Kind::UnknownArgument,
                Msg::UNRECOGNIZED_OPTION,
                format!("Unknown argument: {argument}"),
            ),

            CliError::InvalidTechnology(technology) => Self::debug(
                Kind::InvalidTechnology,
                Msg::UNSUPPORTED_TECHNOLOGY_FILTER,
                format!("Invalid technology: {technology}"),
            ),

            CliError::NonUnicodeArgument => {
                Self::user_facing(Kind::NonUnicodeArgument, Msg::NON_UNICODE_INPUT)
            }

            CliError::OutputSerializationFailed(message) => Self::debug(
                Kind::SerializationFailed,
                Msg::RESPONSE_SERIALIZATION_FAILED,
                format!("Could not serialize command output: {message}"),
            ),

            CliError::ConfirmationTokenMismatch => {
                Self::user_facing(Kind::ConfirmationTokenMismatch, Msg::PLAN_CHANGED_REBUILD)
            }

            CliError::GameNotFound(game_id) => Self::debug(
                Kind::GameNotFound,
                Msg::GAME_NOT_IN_CATALOG,
                format!("Game not found: {game_id}"),
            ),

            CliError::OperationNotFound(operation_id) => Self::debug(
                Kind::OperationNotFound,
                Msg::OPERATION_NOT_FOUND,
                format!("Operation not found: {operation_id}"),
            ),

            CliError::ArtifactNotFound(artifact_id) => Self::debug(
                Kind::ArtifactNotFound,
                Msg::ARTIFACT_NOT_FOUND,
                format!("Artifact not found: {artifact_id}"),
            ),

            CliError::ComponentNotFound(component_id) => Self::debug(
                Kind::ComponentNotFound,
                Msg::COMPONENT_NOT_FOUND,
                format!("Component not found: {component_id}"),
            ),

            CliError::InvalidOperationState {
                operation_id,
                state,
            } => Self::debug(
                Kind::InvalidOperationState,
                Msg::INVALID_OPERATION_STATE,
                format!("Operation {operation_id} is in invalid state: {state}"),
            ),

            CliError::CommandFailed(message) => Self::debug(
                Kind::CommandFailed,
                Msg::OPERATION_COULD_NOT_COMPLETE,
                message,
            ),
        }
    }

    fn debug(kind: Kind, user_message: UserMessage, debug_details: impl Into<String>) -> Self {
        Self::with_debug_details(kind, user_message, debug_details.into())
    }
}

impl From<CliError> for CommandError {
    fn from(error: CliError) -> Self {
        Self::from_cli_error(error)
    }
}
