use renderpilot_application::{AppResult, MetadataJson};
use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
    GameRuntime, GraphicsTechnology, Launcher, PathRef, Platform, Sha256Hash, Swappability,
    Version,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use crate::error::{invalid_row, storage_context};

pub(crate) fn serialize_json<T>(value: &T) -> AppResult<String>
where
    T: Serialize + ?Sized,
{
    serde_json::to_string(value)
        .map_err(|error| storage_context("could not serialize json", error))
}

pub(crate) fn deserialize_json<T>(value: &str) -> AppResult<T>
where
    T: DeserializeOwned,
{
    serde_json::from_str(value)
        .map_err(|error| storage_context("could not deserialize json", error))
}

pub(crate) fn enum_to_text<T>(value: T) -> AppResult<String>
where
    T: Serialize,
{
    let value = serde_json::to_value(value)
        .map_err(|error| storage_context("could not serialize enum", error))?;

    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| crate::error::storage_error("enum did not serialize to a string"))
}

pub(crate) fn enum_from_text<T>(value: &str) -> AppResult<T>
where
    T: DeserializeOwned,
{
    serde_json::from_value(Value::String(value.to_owned()))
    .map_err(|error| storage_context("could not deserialize enum", error))
}

pub(crate) fn game_id(value: String) -> AppResult<GameId> {
    GameId::new(value).map_err(invalid_row)
}

pub(crate) fn component_id(value: String) -> AppResult<ComponentId> {
    ComponentId::new(value).map_err(invalid_row)
}

pub(crate) fn artifact_id(value: String) -> AppResult<ArtifactId> {
    ArtifactId::new(value).map_err(invalid_row)
}

pub(crate) fn operation_id(value: String) -> AppResult<renderpilot_domain::OperationId> {
    renderpilot_domain::OperationId::new(value).map_err(invalid_row)
}

pub(crate) fn path_ref(value: String) -> AppResult<PathRef> {
    PathRef::new(value).map_err(invalid_row)
}

pub(crate) fn sha256(value: String) -> AppResult<Sha256Hash> {
    Sha256Hash::new(value).map_err(invalid_row)
}

pub(crate) fn launcher(value: String) -> AppResult<Launcher> {
    enum_from_text(&value)
}

pub(crate) fn platform(value: String) -> AppResult<Platform> {
    enum_from_text(&value)
}

pub(crate) fn runtime(value: String) -> AppResult<GameRuntime> {
    enum_from_text(&value)
}

pub(crate) fn component_kind(value: String) -> AppResult<ComponentKind> {
    enum_from_text(&value)
}

pub(crate) fn graphics_technology(value: String) -> AppResult<GraphicsTechnology> {
    enum_from_text(&value)
}

pub(crate) fn artifact_trust_level(value: String) -> AppResult<ArtifactTrustLevel> {
    enum_from_text(&value)
}

pub(crate) fn swappability(value: String) -> AppResult<Swappability> {
    enum_from_text(&value)
}

pub(crate) fn version(value: String) -> AppResult<Version> {
    Version::parse(value).map_err(invalid_row)
}

pub(crate) fn component_files(value: String) -> AppResult<Vec<ComponentFile>> {
    deserialize_json(&value)
}

pub(crate) fn component_file(value: String) -> AppResult<ComponentFile> {
    deserialize_json(&value)
}

pub(crate) fn metadata_json(value: String) -> AppResult<MetadataJson> {
    MetadataJson::new(value).map_err(invalid_row)
}
