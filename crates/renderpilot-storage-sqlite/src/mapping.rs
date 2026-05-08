use std::any::type_name;

use renderpilot_application::{AppResult, MetadataJson};
use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId, GameRuntime,
    GraphicsTechnology, Launcher, PathRef, Platform, Sha256Hash, Swappability, Version,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use crate::error::{invalid_row, storage_context};

pub(crate) fn serialize_json<T>(value: &T) -> AppResult<String>
where
    T: Serialize + ?Sized,
{
    serde_json::to_string(value).map_err(|error| storage_context("could not serialize json", error))
}

pub(crate) fn deserialize_json<T>(value: &str) -> AppResult<T>
where
    T: DeserializeOwned,
{
    serde_json::from_str(value)
        .map_err(|error| storage_context("could not deserialize json", error))
}

pub(crate) fn enum_to_text<T>(value: &T) -> AppResult<String>
where
    T: Serialize + ?Sized,
{
    let json = serde_json::to_value(value)
        .map_err(|error| storage_context("could not serialize enum", error))?;

    json.as_str().map(str::to_owned).ok_or_else(|| {
        crate::error::storage_error(format!(
            "enum {} did not serialize to a string: {json}",
            type_name::<T>(),
        ))
    })
}

pub(crate) fn enum_from_text<T>(value: &str) -> AppResult<T>
where
    T: DeserializeOwned,
{
    serde_json::from_value(Value::String(value.to_owned()))
        .map_err(|error| storage_context("could not deserialize enum", error))
}

macro_rules! validated_parser {
    ($fn_name:ident, $ty:ty, $constructor:path) => {
        pub(crate) fn $fn_name(value: String) -> AppResult<$ty> {
            $constructor(value).map_err(invalid_row)
        }
    };
}

macro_rules! enum_parser {
    ($fn_name:ident, $ty:ty) => {
        pub(crate) fn $fn_name(value: String) -> AppResult<$ty> {
            enum_from_text(&value)
        }
    };
}

validated_parser!(game_id, GameId, GameId::new);
validated_parser!(component_id, ComponentId, ComponentId::new);
validated_parser!(artifact_id, ArtifactId, ArtifactId::new);
validated_parser!(
    operation_id,
    renderpilot_domain::OperationId,
    renderpilot_domain::OperationId::new
);
validated_parser!(path_ref, PathRef, PathRef::new);
validated_parser!(sha256_hash, Sha256Hash, Sha256Hash::new);
validated_parser!(metadata_json, MetadataJson, MetadataJson::new);

enum_parser!(launcher, Launcher);
enum_parser!(platform, Platform);
enum_parser!(runtime, GameRuntime);
enum_parser!(component_kind, ComponentKind);
enum_parser!(graphics_technology, GraphicsTechnology);
enum_parser!(artifact_trust_level, ArtifactTrustLevel);
enum_parser!(swappability, Swappability);

pub(crate) fn version(value: String) -> AppResult<Version> {
    Version::parse(value).map_err(invalid_row)
}

pub(crate) fn component_files(value: String) -> AppResult<Vec<ComponentFile>> {
    deserialize_json(&value)
}
