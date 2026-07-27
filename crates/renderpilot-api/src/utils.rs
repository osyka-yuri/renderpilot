use renderpilot_orchestration::domain::{ArtifactId, ComponentId, GameId, GraphicsTechnology};
use serde::Serialize;
use serde_json::Value;

use crate::ApiError;

pub(crate) type JsonResult = Result<Value, ApiError>;

pub(crate) fn to_json<T: Serialize>(value: T) -> JsonResult {
    serde_json::to_value(value).map_err(Into::into)
}

pub(crate) fn is_visible_graphics_technology(technology: GraphicsTechnology) -> bool {
    technology != GraphicsTechnology::Unknown
}

pub(crate) fn parse_game_id(value: impl Into<String>) -> Result<GameId, ApiError> {
    parse_identifier(value, ApiError::InvalidGameId)
}

pub(crate) fn parse_component_id(value: impl Into<String>) -> Result<ComponentId, ApiError> {
    parse_identifier(value, ApiError::InvalidComponentId)
}

pub(crate) fn parse_artifact_id(value: impl Into<String>) -> Result<ArtifactId, ApiError> {
    parse_identifier(value, ApiError::InvalidArtifactId)
}

pub(crate) fn parse_identifier<T>(
    value: impl Into<String>,
    invalid: fn(String) -> ApiError,
) -> Result<T, ApiError>
where
    T: TryFrom<String>,
{
    let value = value.into();

    T::try_from(value.clone()).map_err(|_| invalid(value))
}
