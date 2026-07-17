//! Strict RenoDX v1 document adapter.

use renderpilot_domain::{Architecture, GraphicsApi};
use serde::Deserialize;

use crate::addons::catalog_message::WireCatalogMessage;
use crate::addons::matching::{Engine, MatchRule, Status};

use super::catalog::{
    RenoDxCategory, RenoDxCompatibility, RenoDxGeneric, RenoDxManifest, RenoDxTitle,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WireManifestV1 {
    schema_version: u32,
    generated_at: String,
    games: Vec<WireGame>,
    engine_profiles: Vec<WireEngineProfile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireGame {
    id: String,
    name: String,
    architecture: Architecture,
    status: Status,
    r#match: Vec<MatchRule>,
    addon: WireAddon,
    #[serde(default)]
    availability: Option<WireAvailability>,
    #[serde(default)]
    constraints: WireCompatibility,
    #[serde(default)]
    proxy_dll: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireAddon {
    slug: String,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireEngineProfile {
    engine: Engine,
    status: Status,
    addon: WireEngineAddon,
    message: WireCatalogMessage,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireEngineAddon {
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    sources: Option<WireArchitectureSources>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireArchitectureSources {
    x64: String,
    x86: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCompatibility {
    #[serde(default)]
    required_api: Vec<GraphicsApi>,
    #[serde(default)]
    conflicts: Vec<String>,
    #[serde(default)]
    source: Option<String>,
}

impl From<WireCompatibility> for RenoDxCompatibility {
    fn from(value: WireCompatibility) -> Self {
        Self {
            required_api: value.required_api,
            conflicts: value.conflicts,
            source: value.source,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
enum WireAvailability {
    External {
        url: String,
        message: WireCatalogMessage,
    },
    NativeHdr,
    Blocked {
        message: WireCatalogMessage,
    },
}

impl RenoDxManifest {
    /// Converts a strict schema-v1 document to the private runtime model.
    pub(crate) fn from_wire_v1(wire: WireManifestV1) -> Self {
        Self {
            schema_version: wire.schema_version,
            generated_at: wire.generated_at,
            generics: wire
                .engine_profiles
                .into_iter()
                .map(|profile| {
                    let WireEngineProfile {
                        engine,
                        status,
                        addon,
                        message,
                    } = profile;
                    let WireEngineAddon { slug, sources } = addon;
                    let (url64, url32) = sources
                        .map(|sources| (Some(sources.x64), Some(sources.x86)))
                        .unwrap_or((None, None));
                    RenoDxGeneric {
                        engine,
                        status,
                        slug,
                        url64,
                        url32,
                        message: message.into(),
                    }
                })
                .collect(),
            titles: wire
                .games
                .into_iter()
                .map(|game| RenoDxTitle {
                    id: game.id,
                    name: game.name,
                    category: game
                        .availability
                        .map_or(RenoDxCategory::Installable, Into::into),
                    slug: game.addon.slug,
                    arch: game.architecture,
                    status: game.status,
                    match_rules: game.r#match,
                    compatibility: game.constraints.into(),
                    proxy_dll_override: game.proxy_dll,
                    download_url: game.addon.source,
                })
                .collect(),
        }
    }
}

impl From<WireAvailability> for RenoDxCategory {
    fn from(value: WireAvailability) -> Self {
        match value {
            WireAvailability::External { url, message } => Self::External {
                url,
                message: message.into(),
            },
            WireAvailability::NativeHdr => Self::NativeHdr,
            WireAvailability::Blocked { message } => Self::Blacklist {
                message: message.into(),
            },
        }
    }
}
