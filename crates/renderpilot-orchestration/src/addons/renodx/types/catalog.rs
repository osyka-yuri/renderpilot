//! Runtime RenoDX catalogue model.

use renderpilot_domain::{Architecture, GraphicsApi, RenoDxManagedConfigKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::addons::CatalogMessage;
use crate::addons::engine_config::{EngineIniEntry, EngineIniRecipe};
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
    /// Curated guidance keyed by stable game id (v2; empty for v1).
    pub title_guidance: BTreeMap<String, Vec<RenoDxGuidance>>,
    /// Curated page-level guidance (v2; empty for v1).
    pub page_guidance: Vec<RenoDxGuidance>,
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
    /// Stable v2 profile identifier.
    pub profile_id: Option<String>,
    /// Whether this profile can be selected as an engine fallback.
    pub generic_fallback: bool,
    /// Closed install-time processing route selected by this engine profile.
    pub processing_path: RenoDxProcessingPath,
    /// Engine-wide curated guidance.
    pub guidance: Vec<RenoDxGuidance>,
}

/// RenoDX's closed processing-path policy.  This is deliberately independent
/// from user-facing guidance: remote text can never select an INI mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RenoDxProcessingPath {
    /// Enable RenoDX's resource-upgrade processing route.
    Upgrade,
    /// Use the engine/native HDR processing route.
    Native,
    /// Leave the existing RenoDX processing-path setting untouched.
    #[default]
    Unmanaged,
}

impl RenoDxProcessingPath {
    /// Maps the processing path into the typed ReShade.ini processing value,
    /// or `None` if the configuration should not be managed.
    #[must_use]
    pub const fn desired_set_path(self) -> Option<renderpilot_domain::RenoDxSetPathValue> {
        match self {
            Self::Upgrade => Some(renderpilot_domain::RenoDxSetPathValue::One),
            Self::Native => Some(renderpilot_domain::RenoDxSetPathValue::Zero),
            Self::Unmanaged => None,
        }
    }
}

/// User-facing identity of an engine-level generic match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenoDxGenericProfile {
    /// Engine matched by the fallback.
    pub engine: Engine,
    /// Localizable catalogue label.
    pub message: CatalogMessage,
    /// Stable v2 profile identifier when supplied by the manifest.
    pub profile_id: Option<String>,
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
    /// Stable v2 profile reference for an exact title.
    pub profile_id: Option<String>,
    /// Optional title-specific processing route.  When absent, the referenced
    /// engine profile supplies the route.
    pub processing_path: Option<RenoDxProcessingPath>,
    /// Strict, reviewed add-on settings to apply to ReShade.ini.
    pub renodx_config: Option<RenoDxConfig>,
    /// An otherwise valid v2 title uses an add-on setting this client cannot apply.
    pub has_unsupported_settings: bool,
    /// Whether page-wide guidance is composed into this title's guidance.
    pub inherit_page_guidance: bool,
    /// Launch arguments associated with this title.
    pub launch: Option<RenoDxLaunchRequirement>,
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

/// A manually curated RenoDX recommendation or caveat.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenoDxGuidance {
    /// Stable catalogue guidance id.
    pub id: String,
    /// Rendering kind used by the UI.
    pub kind: RenoDxGuidanceKind,
    /// External i18n message key.
    pub message_id: String,
    /// Safe fallback when a translation is unavailable.
    pub fallback_text: String,
    /// Optional copyable code (required for Engine.ini entries).
    pub code: Option<String>,
    /// Optional structured setting values.
    pub settings: Vec<RenoDxSetting>,
    /// Typed Engine.ini instructions.  When present, this is the sole source
    /// of truth for future file mutation; `code` remains presentation only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine_ini: Option<RenoDxEngineIniRecipe>,
    /// Optional reviewed source URL.
    pub url: Option<String>,
    /// Optional engine/version gate.
    pub condition: Option<RenoDxGuidanceCondition>,
}

/// A closed, versioned Engine.ini mutation recipe published by RenoDX v2.
///
/// This model intentionally contains only scalar section/key/value data.  It
/// is not a general INI parser and never derives authority from the rendered
/// guidance `code` string.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenoDxEngineIniRecipe {
    /// Recipe format version, currently fixed at one.
    pub schema_version: u32,
    /// Curated revision of this recipe's semantics.
    pub revision: u32,
    /// Ordered INI sections.
    pub sections: Vec<RenoDxEngineIniSection>,
}

impl RenoDxEngineIniRecipe {
    /// Renders the recipe in the canonical presentation form.  Callers that
    /// need to apply a recipe must use the typed fields, not this text.
    #[must_use]
    pub fn canonical_code(&self) -> String {
        use std::fmt::Write;

        let mut out = String::new();
        for (i, section) in self.sections.iter().enumerate() {
            if i > 0 {
                out.push_str("\n\n");
            }
            let _ = write!(out, "[{}]", section.name);
            for entry in &section.entries {
                let _ = write!(out, "\n{}={}", entry.key, entry.value);
            }
        }
        out
    }

    /// Converts the RenoDX wire recipe into the shared runtime authority.
    pub fn to_engine_config_recipe(
        &self,
        id: impl Into<String>,
    ) -> Result<EngineIniRecipe, crate::addons::engine_config::EngineIniError> {
        let entries = self
            .sections
            .iter()
            .flat_map(|section| {
                section.entries.iter().map(|entry| EngineIniEntry {
                    section: section.name.clone(),
                    key: entry.key.clone(),
                    value: entry.value.clone(),
                })
            })
            .collect();
        EngineIniRecipe::new(id, self.revision, entries)
    }
}

/// One ordered section of a RenoDx Engine.ini recipe.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenoDxEngineIniSection {
    /// Section name without surrounding brackets.
    pub name: String,
    /// Ordered key/value entries in this section.
    pub entries: Vec<RenoDxEngineIniEntry>,
}

/// One scalar key/value assignment in an Engine.ini recipe.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenoDxEngineIniEntry {
    /// INI key.
    pub key: String,
    /// INI scalar value.
    pub value: String,
}

/// Guidance category. Serialized names are owned by the v2 wire adapter/schema.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenoDxGuidanceKind {
    /// Setting changed in the game UI.
    GameSetting,
    /// Structured RenoDX setting/value pair.
    AddonSetting,
    /// Copyable `Engine.ini` fragment.
    EngineIni,
    /// Important operational warning.
    Warning,
    /// Compatibility behavior or limitation.
    Compatibility,
    /// External helper or prerequisite.
    ExternalTool,
}

/// A typed setting shown alongside a guidance item.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenoDxSetting {
    /// User-facing setting name.
    pub name: String,
    /// Reviewed value to apply.
    pub value: String,
}

/// Version/engine condition for guidance materialization.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenoDxGuidanceCondition {
    /// Engine required for the guidance.
    pub engine: Option<Engine>,
    /// Required Unreal Engine major version.
    pub unreal_major: Option<u32>,
    /// Inclusive minimum Unreal Engine minor version.
    pub unreal_minor_min: Option<u32>,
    /// Inclusive maximum Unreal Engine minor version.
    pub unreal_minor_max: Option<u32>,
}

/// Strict machine-readable RenoDX configuration compiled from reviewed catalog
/// data. Presentation guidance is deliberately not an authority for writes.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenoDxConfig {
    /// Canonical typed values to apply under `[renodx]`.
    pub settings: Vec<RenoDxConfigSetting>,
}

/// One canonical RenoDX key/value pair.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenoDxConfigSetting {
    /// Closed RenoDX key name.
    pub key: RenoDxConfigKey,
    /// Canonical integer value accepted by the selected profile.
    pub value: i32,
}

/// Closed set of RenoDX configuration keys RenderPilot may manage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum RenoDxConfigKey {
    /// B8G8R8A8 typeless resource upgrade mode.
    #[serde(rename = "Upgrade_B8G8R8A8_TYPELESS")]
    UpgradeB8G8R8A8Typeless,
    /// B8G8R8A8 UNORM resource upgrade mode.
    #[serde(rename = "Upgrade_B8G8R8A8_UNORM")]
    UpgradeB8G8R8A8Unorm,
    /// R8G8B8A8 typeless resource upgrade mode.
    #[serde(rename = "Upgrade_R8G8B8A8_TYPELESS")]
    UpgradeR8G8B8A8Typeless,
    /// R8G8B8A8 UNORM resource upgrade mode.
    #[serde(rename = "Upgrade_R8G8B8A8_UNORM")]
    UpgradeR8G8B8A8Unorm,
    /// R10G10B10A2 UNORM resource upgrade mode.
    #[serde(rename = "Upgrade_R10G10B10A2_UNORM")]
    UpgradeR10G10B10A2Unorm,
    /// R10G10B10A2 typeless resource upgrade mode.
    #[serde(rename = "Upgrade_R10G10B10A2_TYPELESS")]
    UpgradeR10G10B10A2Typeless,
    /// R11G11B10 float resource upgrade mode.
    #[serde(rename = "Upgrade_R11G11B10_FLOAT")]
    UpgradeR11G11B10Float,
    /// R16G16B16A16 typeless resource upgrade mode.
    #[serde(rename = "Upgrade_R16G16B16A16_TYPELESS")]
    UpgradeR16G16B16A16Typeless,
    /// Copy-destination resource upgrade mode.
    #[serde(rename = "Upgrade_CopyDestinations")]
    UpgradeCopyDestinations,
    /// Force-borderless mode.
    #[serde(rename = "ForceBorderless")]
    ForceBorderless,
    /// Use scRGB swap-chain mode.
    #[serde(rename = "Upgrade_UseSCRGB")]
    UpgradeUseScrgb,
    /// Swap-chain encoding mode.
    #[serde(rename = "Swapchain_Encoding")]
    SwapchainEncoding,
    /// Compatibility scaling offset.
    #[serde(rename = "Scaling_Offset")]
    ScalingOffset,
    /// Compatibility tonemap offset.
    #[serde(rename = "Tonemap_Offset")]
    TonemapOffset,
    /// Compatibility blit-copy workaround mode.
    #[serde(rename = "Blit_Copy_Hack")]
    BlitCopyHack,
    /// Swap-chain proxy mode.
    #[serde(rename = "Use_Swapchain_Proxy")]
    UseSwapchainProxy,
    /// Force pipeline cloning.
    #[serde(rename = "Force_Pipeline_Cloning")]
    ForcePipelineCloning,
    /// Color-grade contrast preset value.
    #[serde(rename = "ColorGradeContrast")]
    ColorGradeContrast,
    /// Color-grade saturation preset value.
    #[serde(rename = "ColorGradeSaturation")]
    ColorGradeSaturation,
    /// Color-grade blowout preset value.
    #[serde(rename = "ColorGradeBlowout")]
    ColorGradeBlowout,
}

impl RenoDxConfigKey {
    /// Returns the corresponding durable planner key.
    pub(crate) const fn managed_key(self) -> RenoDxManagedConfigKey {
        match self {
            Self::UpgradeB8G8R8A8Typeless => RenoDxManagedConfigKey::UpgradeB8G8R8A8Typeless,
            Self::UpgradeB8G8R8A8Unorm => RenoDxManagedConfigKey::UpgradeB8G8R8A8Unorm,
            Self::UpgradeR8G8B8A8Typeless => RenoDxManagedConfigKey::UpgradeR8G8B8A8Typeless,
            Self::UpgradeR8G8B8A8Unorm => RenoDxManagedConfigKey::UpgradeR8G8B8A8Unorm,
            Self::UpgradeR10G10B10A2Unorm => RenoDxManagedConfigKey::UpgradeR10G10B10A2Unorm,
            Self::UpgradeR10G10B10A2Typeless => RenoDxManagedConfigKey::UpgradeR10G10B10A2Typeless,
            Self::UpgradeR11G11B10Float => RenoDxManagedConfigKey::UpgradeR11G11B10Float,
            Self::UpgradeR16G16B16A16Typeless => {
                RenoDxManagedConfigKey::UpgradeR16G16B16A16Typeless
            }
            Self::UpgradeCopyDestinations => RenoDxManagedConfigKey::UpgradeCopyDestinations,
            Self::ForceBorderless => RenoDxManagedConfigKey::ForceBorderless,
            Self::UpgradeUseScrgb => RenoDxManagedConfigKey::UpgradeUseScrgb,
            Self::SwapchainEncoding => RenoDxManagedConfigKey::SwapchainEncoding,
            Self::ScalingOffset => RenoDxManagedConfigKey::ScalingOffset,
            Self::TonemapOffset => RenoDxManagedConfigKey::TonemapOffset,
            Self::BlitCopyHack => RenoDxManagedConfigKey::BlitCopyHack,
            Self::UseSwapchainProxy => RenoDxManagedConfigKey::UseSwapchainProxy,
            Self::ForcePipelineCloning => RenoDxManagedConfigKey::ForcePipelineCloning,
            Self::ColorGradeContrast => RenoDxManagedConfigKey::ColorGradeContrast,
            Self::ColorGradeSaturation => RenoDxManagedConfigKey::ColorGradeSaturation,
            Self::ColorGradeBlowout => RenoDxManagedConfigKey::ColorGradeBlowout,
        }
    }

    /// Returns the canonical INI key string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.managed_key().as_str()
    }
}

/// Launch argument recommendation attached to an exact title.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenoDxLaunchRequirement {
    /// Arguments in launcher order.
    pub arguments: Vec<String>,
    /// Whether the arguments are mandatory or advisory.
    pub requirement: RenoDxLaunchRequirementLevel,
}

/// Whether launch arguments are required for the curated path or merely advised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenoDxLaunchRequirementLevel {
    /// The curated RenoDX path requires these arguments.
    Required,
    /// The arguments are recommended but not mandatory.
    Recommended,
}
