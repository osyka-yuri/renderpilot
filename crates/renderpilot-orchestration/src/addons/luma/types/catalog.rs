//! Catalogue model: manifest document, titles, profile, features, guidance.

use serde::{Deserialize, Serialize};

use crate::addons::CatalogMessage;
use crate::addons::matching::{MatchRule, Status};

use super::managed::LumaExternalRequirement;

// ---------------------------------------------------------------------------
// Package identity — single source of truth for curated Luma release assets
// ---------------------------------------------------------------------------

/// Shared Generic Unreal Engine release asset on the Luma GitHub release.
pub(crate) const GENERIC_UNREAL_ASSET: &str = "Luma-Unreal_Engine.zip";
/// Shared Generic Unity (64-bit) release asset.
pub(crate) const GENERIC_UNITY_ASSET: &str = "Luma-Unity_Engine.zip";
/// Shared Generic Unity (32-bit) release asset.
pub(crate) const GENERIC_UNITY_ASSET_X32: &str = "Luma-Unity_Engine-x32.zip";

#[must_use]
pub(crate) fn is_generic_unreal_asset(asset: &str) -> bool {
    asset == GENERIC_UNREAL_ASSET
}

#[must_use]
pub(crate) fn is_generic_unity_asset(asset: &str) -> bool {
    matches!(asset, GENERIC_UNITY_ASSET | GENERIC_UNITY_ASSET_X32)
}

/// Top-level Luma Framework manifest document.
#[derive(Debug, Clone)]
pub struct LumaManifest {
    /// Schema version used to interpret this document.
    pub schema_version: u32,
    /// RFC 3339 timestamp recording when the manifest was generated.
    pub generated_at: String,
    /// Minimum ReShade host version Luma's current builds require.
    pub min_reshade_version: String,
    /// The curated catalogue of games Luma is known to work with.
    pub titles: Vec<LumaTitle>,
}

impl LumaManifest {
    /// Parses `min_reshade_version` into a comparable [`Version`]. The manifest
    /// parser already validates this is well-formed (see
    /// `validate::ensure_semver`), so this only fails in practice for a
    /// manifest built outside that path (e.g. a hand-constructed test fixture) —
    /// callers still get a proper error rather than a panic, since a manifest
    /// crossing the CDN-fetch boundary is never fully trusted twice.
    pub(crate) fn min_reshade_version_parsed(
        &self,
    ) -> Result<renderpilot_domain::Version, crate::ServiceError> {
        renderpilot_domain::Version::parse(&self.min_reshade_version).map_err(|error| {
            crate::ServiceError::command_failed(format!(
                "manifest min_reshade_version `{}` is invalid: {error}",
                self.min_reshade_version
            ))
        })
    }
}

/// Upstream wiki verdict for an individual Luma feature in a specific game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LumaFeatureStatus {
    /// The wiki marks the feature as available for this game.
    Supported,
    /// The wiki marks the feature as unavailable for this game.
    Unsupported,
    /// The wiki marks the feature as still in development for this game.
    Experimental,
    /// The wiki has no explicit status for this feature and game.
    Unknown,
}

/// Explicit per-game Luma feature availability curated from the upstream wiki.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LumaFeatures {
    /// DLSS / FSR availability reported by the Luma wiki.
    pub dlss_fsr: LumaFeatureStatus,
    /// Native HDR availability reported by the Luma wiki.
    pub hdr: LumaFeatureStatus,
}

/// Category of one manually edited, user-facing Luma instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LumaGuidanceKind {
    /// A setting selected in the game's own UI.
    GameSetting,
    /// A manual edit to an engine INI file.
    EngineIni,
    /// A launch argument the user must add in their launcher.
    LaunchArgument,
    /// A stability or usability warning.
    Warning,
    /// An interaction with another game/mod configuration.
    Compatibility,
    /// A manually used third-party application or utility.
    ExternalTool,
}

/// A reviewed instruction from the curated catalogue.  Its stable `id` is the
/// translation key; `fallback_text` is intentionally English so a catalogue
/// release never depends on a simultaneous UI translation release.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LumaGuidance {
    /// Stable translation identifier, unique across the public catalogue.
    pub id: String,
    /// Presentation category, controlling the localized heading and icon.
    pub kind: LumaGuidanceKind,
    /// Reviewed English text used until a local translation is available.
    pub fallback_text: String,
    /// Exact copyable INI or argument text when this guidance kind uses code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// How a matched [`LumaTitle`] is routed. Deliberately narrower than RenoDX's
/// `Category`: every Luma asset lives on the GitHub Release (no `external`), and
/// a native-HDR title is simply absent from the catalogue rather than modelled.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LumaCategory {
    /// A standard Luma install (the common case; omitted from the manifest).
    #[default]
    Installable,
    /// Luma is known-broken / unsupported for this game.
    Blacklist {
        /// Localizable explanation supplied by the catalogue.
        message: CatalogMessage,
    },
}

/// Engine family a shared Luma payload targets. Narrower than the app-wide
/// detected-engine enum: Luma publishes only Unreal and Unity generics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LumaEngine {
    /// Shared Unreal Engine payload (`Luma-Unreal_Engine.zip`).
    Unreal,
    /// Shared Unity payload (`Luma-Unity_Engine.zip` / `-x32`).
    Unity,
}

/// How a curated title is scoped: a dedicated per-game profile or a shared
/// engine payload matched onto a specific game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum LumaProfile {
    /// Dedicated per-game Luma build.
    #[default]
    Game,
    /// Shared engine payload curated onto this title.
    Engine {
        /// Engine family the payload targets.
        engine: LumaEngine,
    },
}

impl LumaProfile {
    /// Whether this profile uses a shared engine payload rather than a dedicated build.
    #[must_use]
    pub const fn is_engine(self) -> bool {
        matches!(self, Self::Engine { .. })
    }

    /// Engine family when this is an engine profile.
    #[must_use]
    pub const fn engine(self) -> Option<LumaEngine> {
        match self {
            Self::Game => None,
            Self::Engine { engine } => Some(engine),
        }
    }

    /// Whether this is the Generic Unreal profile (features + DX11 callout apply).
    #[must_use]
    pub const fn is_generic_unreal(self) -> bool {
        matches!(
            self,
            Self::Engine {
                engine: LumaEngine::Unreal
            }
        )
    }
}

/// A curated game Luma Framework is known to work with.
#[derive(Debug, Clone)]
pub struct LumaTitle {
    /// Stable identifier of this title.
    pub id: String,
    /// Display name.
    pub name: String,
    /// GitHub Release asset file name (e.g. `Luma-Dishonored_2.zip`,
    /// `Luma-Generic_Mod.zip`). Validated to match Luma's own naming convention
    /// and to agree with `arch` (an `-x32` suffix iff
    /// [`renderpilot_domain::Architecture::X86`]).
    pub asset: String,
    /// Exact root add-on file name carried by `asset` (for example,
    /// `Luma-Dishonored 2.addon`). This is an install and recovery identity,
    /// not a name inferred from the release ZIP.
    pub addon_file: String,
    /// CPU architecture the asset targets.
    pub arch: renderpilot_domain::Architecture,
    /// Wiki test-map status: `working` (verified), `construction` (WIP), `unknown`.
    pub status: Status,
    /// How this game is routed once matched. Defaults to
    /// [`LumaCategory::Installable`], so the common case omits it.
    pub category: LumaCategory,
    /// Ordered match rules; resolution prefers the highest [`MatchRule::tier`].
    pub match_rules: Vec<MatchRule>,
    /// Per-game feature status from the UE matrix.  It deliberately exists
    /// only for Generic UE profiles: dedicated profiles must not imply HDR or
    /// upscaler support from a free-form Wiki note.
    pub features: Option<LumaFeatures>,
    /// Reviewed per-game instructions. Raw Wiki notes never cross this
    /// manifest boundary.
    pub guidance: Vec<LumaGuidance>,
    /// Required launch arguments (e.g. `-dx11`, `-nod3d9ex`), shown to the user as
    /// a copyable callout rather than written automatically (see the project
    /// decision log — no launcher-config automation in v1).
    pub launch_args: Vec<String>,
    /// Managed external dependency for this title. RenderPilot downloads,
    /// verifies, installs, and configures it alongside Luma.
    pub external_requirement: Option<LumaExternalRequirement>,
    /// Dedicated game profile vs shared engine payload.
    pub profile: LumaProfile,
}

impl LumaTitle {
    /// Whether this title is a Generic Unreal profile with the matching shared
    /// release asset. Single gate for features validation, the D3D12→DX11 matcher
    /// exception, and the advisory manual `-dx11` launch-arg callout.
    #[must_use]
    pub(crate) fn is_generic_unreal(&self) -> bool {
        self.profile.is_generic_unreal() && is_generic_unreal_asset(&self.asset)
    }
}
