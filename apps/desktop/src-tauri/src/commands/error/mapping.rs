use renderpilot_api::ApiError;
use renderpilot_orchestration::ServiceError;

use super::{
    kind::CommandErrorKind as Kind,
    strings::{user_message as Msg, UserMessage},
    CommandError,
};

// `from_api_error` must handle every `ApiError` variant; add a match arm when the enum grows.
impl CommandError {
    pub(crate) fn from_api_error(error: ApiError) -> Self {
        match error {
            ApiError::InvalidGameId(id) => Self::invalid_id(
                Kind::InvalidGameId,
                Msg::INVALID_GAME_REFERENCE,
                "Invalid game id",
                id,
            ),

            ApiError::InvalidComponentId(id) => Self::invalid_id(
                Kind::InvalidComponentId,
                Msg::INVALID_COMPONENT_REFERENCE,
                "Invalid component id",
                id,
            ),

            ApiError::InvalidArtifactId(id) => Self::invalid_id(
                Kind::InvalidArtifactId,
                Msg::INVALID_ARTIFACT_REFERENCE,
                "Invalid artifact id",
                id,
            ),

            ApiError::InvalidOperationId(id) => Self::invalid_id(
                Kind::InvalidOperationId,
                Msg::INVALID_OPERATION_REFERENCE,
                "Invalid operation id",
                id,
            ),

            ApiError::OutputSerializationFailed(message) => Self::debug(
                Kind::SerializationFailed,
                Msg::RESPONSE_SERIALIZATION_FAILED,
                format!("Could not serialize command output: {message}"),
            ),

            ApiError::Service(e) => Self::from_service_error(e),
        }
    }

    pub(crate) fn from_service_error(error: ServiceError) -> Self {
        match error {
            ServiceError::ConfirmationTokenMismatch => {
                Self::user_facing(Kind::ConfirmationTokenMismatch, Msg::PLAN_CHANGED_REBUILD)
            }

            ServiceError::GameNotFound(game_id) => Self::debug(
                Kind::GameNotFound,
                Msg::GAME_NOT_IN_CATALOG,
                format!("Game not found: {game_id}"),
            ),

            ServiceError::OperationNotFound(operation_id) => Self::debug(
                Kind::OperationNotFound,
                Msg::OPERATION_NOT_FOUND,
                format!("Operation not found: {operation_id}"),
            ),

            ServiceError::ArtifactNotFound(artifact_id) => Self::debug(
                Kind::ArtifactNotFound,
                Msg::ARTIFACT_NOT_FOUND,
                format!("Artifact not found: {artifact_id}"),
            ),

            ServiceError::ComponentNotFound(component_id) => Self::debug(
                Kind::ComponentNotFound,
                Msg::COMPONENT_NOT_FOUND,
                format!("Component not found: {component_id}"),
            ),

            ServiceError::InvalidOperationState {
                operation_id,
                state,
            } => Self::debug(
                Kind::InvalidOperationState,
                Msg::INVALID_OPERATION_STATE,
                format!("Operation {operation_id} is in invalid state: {state}"),
            ),

            ServiceError::CommandFailed(message) => Self::debug(
                Kind::CommandFailed,
                Msg::OPERATION_COULD_NOT_COMPLETE,
                message,
            ),

            ServiceError::InvalidInput(message) => {
                Self::debug(Kind::InvalidArgument, Msg::INVALID_ARGUMENT, message)
            }

            ServiceError::StorageFailed(message) => {
                Self::debug(Kind::StorageFailed, Msg::STORAGE_FAILED, message)
            }

            ServiceError::ProviderFailed(message) => {
                Self::debug(Kind::ProviderFailed, Msg::PROVIDER_FAILED, message)
            }

            ServiceError::DetectionFailed(message) => {
                Self::debug(Kind::DetectionFailed, Msg::DETECTION_FAILED, message)
            }

            ServiceError::SteamGridDbApiKeyMissing => Self::user_facing(
                Kind::SteamGridDbApiKeyMissing,
                Msg::STEAMGRIDDB_API_KEY_MISSING,
            ),

            ServiceError::UnsupportedCoverImageType => Self::user_facing(
                Kind::UnsupportedCoverImageType,
                Msg::UNSUPPORTED_COVER_IMAGE_TYPE,
            ),

            ServiceError::CoverDownloadFailed(message) => Self::debug(
                Kind::CoverDownloadFailed,
                Msg::COVER_DOWNLOAD_FAILED,
                message,
            ),

            ServiceError::CoverNotFound => {
                Self::user_facing(Kind::CoverNotFound, Msg::COVER_ARTWORK_NOT_FOUND)
            }

            ServiceError::CoverIo(message) => {
                Self::debug(Kind::CoverIoError, Msg::COVER_FILE_SYSTEM_ERROR, message)
            }

            ServiceError::NvapiRequiresElevation => Self::user_facing(
                Kind::NvapiRequiresElevation,
                Msg::NVAPI_REQUIRES_ADMINISTRATOR,
            ),
        }
    }

    fn debug(kind: Kind, user_message: UserMessage, debug_details: impl Into<String>) -> Self {
        let spec = kind.spec();
        let message = format!(
            "CommandError [{}] ({}): {}",
            spec.code,
            user_message.key(),
            debug_details.into()
        );

        match spec.severity {
            super::CommandErrorSeverity::Warning => log::warn!("{message}"),
            super::CommandErrorSeverity::Error => log::error!("{message}"),
        }

        Self::user_facing(kind, user_message)
    }
}

impl From<ApiError> for CommandError {
    fn from(error: ApiError) -> Self {
        Self::from_api_error(error)
    }
}
