use std::any::type_name;

use renderpilot_application::{AppResult, MetadataJson};
use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId, GameRuntime,
    Launcher, LibraryTechnology, PathRef, Platform, RootAuthority, Swappability,
};
use serde::{Serialize, de::DeserializeOwned};
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
validated_parser!(metadata_json, MetadataJson, MetadataJson::new);

enum_parser!(launcher, Launcher);
enum_parser!(platform, Platform);
enum_parser!(runtime, GameRuntime);
enum_parser!(root_authority, RootAuthority);
enum_parser!(component_kind, ComponentKind);
enum_parser!(library_technology, LibraryTechnology);
enum_parser!(artifact_trust_level, ArtifactTrustLevel);
enum_parser!(swappability, Swappability);

pub(crate) fn component_files(value: &str) -> AppResult<Vec<ComponentFile>> {
    deserialize_json(value)
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{ArtifactTrustLevel, LibraryTechnology};

    use super::{enum_from_text, enum_to_text, library_technology};

    #[test]
    fn library_technology_codec_roundtrips_direct_storage_slug() {
        let serialized =
            enum_to_text(&LibraryTechnology::DirectStorage).expect("enum should serialize");

        assert_eq!(serialized, "direct_storage");

        let parsed: LibraryTechnology =
            enum_from_text(&serialized).expect("enum should deserialize");
        assert_eq!(parsed, LibraryTechnology::DirectStorage);
    }

    #[test]
    fn library_technology_storage_parser_accepts_slug_values() {
        let parsed = library_technology("direct_storage".to_owned())
            .expect("slug value should deserialize through storage mapping");
        assert_eq!(parsed, LibraryTechnology::DirectStorage);
    }

    #[test]
    fn library_technology_codec_roundtrips_new_graphics_slugs() {
        for technology in [
            LibraryTechnology::IntelXeLl,
            LibraryTechnology::AmdFsrRayRegeneration,
        ] {
            let serialized = enum_to_text(&technology).expect("enum should serialize");
            assert_eq!(serialized, technology.as_slug());

            let parsed: LibraryTechnology =
                enum_from_text(&serialized).expect("enum should deserialize");
            assert_eq!(parsed, technology);
        }
    }

    #[test]
    fn library_technology_storage_parser_rejects_legacy_pascal_case_values() {
        let intel = library_technology("IntelXeLl".to_owned());
        let amd = library_technology("AmdFsrRayRegeneration".to_owned());

        assert!(intel.is_err());
        assert!(amd.is_err());
    }

    #[test]
    fn enum_from_text_rejects_legacy_pascal_case_graphics_values() {
        let parsed = enum_from_text::<LibraryTechnology>("IntelXeLl");

        assert!(parsed.is_err());
    }

    #[test]
    fn artifact_trust_level_writes_snake_case_and_reads_legacy_values() {
        let serialized = enum_to_text(&ArtifactTrustLevel::CatalogDownloaded)
            .expect("trust level should serialize");
        assert_eq!(serialized, "catalog_downloaded");

        for (legacy, expected) in [
            ("LocalObserved", ArtifactTrustLevel::LocalObserved),
            ("UserImported", ArtifactTrustLevel::UserImported),
            ("ManifestDownloaded", ArtifactTrustLevel::CatalogDownloaded),
            ("CatalogDownloaded", ArtifactTrustLevel::CatalogDownloaded),
            ("Unknown", ArtifactTrustLevel::Unknown),
        ] {
            let parsed: ArtifactTrustLevel =
                enum_from_text(legacy).expect("legacy trust level should remain readable");
            assert_eq!(parsed, expected);
        }
    }
}
