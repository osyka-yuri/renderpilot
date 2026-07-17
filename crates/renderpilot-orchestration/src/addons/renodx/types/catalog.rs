//! Runtime RenoDX catalogue model.

use renderpilot_domain::{Architecture, GraphicsApi};
use serde::Serialize;

use crate::addons::CatalogMessage;
use crate::addons::matching::{Engine, MatchRule, Status};
use crate::addons::reshade::types::ReshadeIniTweaks;

/// Top-level normalized RenoDX catalogue.
#[derive(Debug, Clone)]
pub struct RenoDxManifest {
    /// Schema version used to interpret the source document.
    pub schema_version: u32,
    /// RFC 3339 source generation timestamp.
    pub generated_at: String,
    /// Engine fallbacks tried after dedicated titles.
    pub generics: Vec<RenoDxGeneric>,
    /// Curated per-game catalogue.
    pub titles: Vec<RenoDxTitle>,
}

/// Default `ReShade.ini` changes requested by a RenoDX install.
#[must_use]
pub(crate) fn renodx_ini_defaults() -> ReshadeIniTweaks {
    ReshadeIniTweaks {
        disabled_addons: vec!["Generic Depth".to_owned(), "Effect Runtime Sync".to_owned()],
        addon_path: None,
        dlss_fix: None,
    }
}

/// Engine-level fallback used when no dedicated title matches.
#[derive(Debug, Clone)]
pub struct RenoDxGeneric {
    /// Engine this fallback targets.
    pub engine: Engine,
    /// Curated compatibility status.
    pub status: Status,
    /// Canonical local add-on slug.
    pub slug: Option<String>,
    /// Optional explicit 64-bit source URL.
    pub url64: Option<String>,
    /// Optional explicit 32-bit source URL.
    pub url32: Option<String>,
    /// Localizable label published with this generic profile.
    pub message: CatalogMessage,
}

/// User-facing identity of an engine-level generic match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenoDxGenericProfile {
    /// Engine matched by the fallback.
    pub engine: Engine,
    /// Localizable catalogue label.
    pub message: CatalogMessage,
}

/// How a matched title is routed after its match rules win.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum RenoDxCategory {
    /// Standard automatic install.
    #[default]
    Installable,
    /// Distribution is external, optionally with local file installation.
    External {
        /// HTTPS destination presented to the user.
        url: String,
        /// Localizable link label.
        message: CatalogMessage,
    },
    /// The game already provides native HDR.
    NativeHdr,
    /// RenoDX is known not to work for this title.
    Blacklist {
        /// Localizable explanation.
        message: CatalogMessage,
    },
}

/// A normalized, matchable RenoDX title.
#[derive(Debug, Clone)]
pub struct RenoDxTitle {
    /// Stable catalogue id.
    pub id: String,
    /// Human-readable game name.
    pub name: String,
    /// Routing after the title matches.
    pub category: RenoDxCategory,
    /// Canonical upstream add-on slug.
    pub slug: String,
    /// Required add-on architecture.
    pub arch: Architecture,
    /// Curated compatibility status.
    pub status: Status,
    /// Ordered rules used to identify the game.
    pub match_rules: Vec<MatchRule>,
    /// Renderer and conflict constraints.
    pub compatibility: RenoDxCompatibility,
    /// Optional proxy DLL override.
    pub proxy_dll_override: Option<String>,
    /// Optional direct add-on source override.
    pub download_url: Option<String>,
}

/// Constraints that gate whether a title can be installed.
#[derive(Debug, Clone, Default)]
pub struct RenoDxCompatibility {
    /// Allowed renderer APIs; empty means no API restriction.
    pub required_api: Vec<GraphicsApi>,
    /// Known conflicting mod identifiers.
    pub conflicts: Vec<String>,
    /// Provenance for a non-empty conflict list.
    pub source: Option<String>,
}
