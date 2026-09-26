//! Strict RenoDX v2 adapter.  Runtime policy is represented by the same
//! normalized model as v1; this module only owns the public JSON shape.

use renderpilot_domain::{Architecture, GraphicsApi};
use serde::{Deserialize, Deserializer};
use std::collections::BTreeMap;

use crate::addons::catalog_message::WireCatalogMessage;
use crate::addons::matching::{Engine, MatchKind, MatchRule, Status};

use super::catalog::{
    RenoDxCategory, RenoDxCompatibility, RenoDxConfig, RenoDxConfigKey, RenoDxConfigSetting,
    RenoDxEngineIniEntry, RenoDxEngineIniRecipe, RenoDxEngineIniSection, RenoDxGeneric,
    RenoDxGuidance, RenoDxGuidanceCondition, RenoDxGuidanceKind, RenoDxLaunchRequirement,
    RenoDxLaunchRequirementLevel, RenoDxManifest, RenoDxProcessingPath, RenoDxSetting, RenoDxTitle,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WireManifestV2 {
    schema_version: u32,
    generated_at: String,
    games: Vec<WireGame>,
    engine_profiles: Vec<WireEngineProfile>,
    page_guidance: Vec<WireGuidance>,
}

impl WireManifestV2 {
    pub(crate) fn validate_raw_renodx_config(&self) -> Result<(), String> {
        for game in &self.games {
            let Some(config) = &game.renodx_config else {
                continue;
            };
            if config.settings.is_empty() {
                return Err(format!(
                    "title `{}` RenoDX config must contain at least one setting",
                    game.id
                ));
            }
            let mut keys = std::collections::HashSet::new();
            for setting in &config.settings {
                let key = setting.key.as_str();
                if key.trim().is_empty() {
                    return Err(format!(
                        "title `{}` RenoDX config contains an empty key",
                        game.id
                    ));
                }
                if key == "Set_Path" {
                    return Err(format!(
                        "title `{}` RenoDX config cannot set reserved key `Set_Path`",
                        game.id
                    ));
                }
                if !keys.insert(key) {
                    return Err(format!(
                        "title `{}` RenoDX config contains duplicate key `{key}`",
                        game.id
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireProfileId {
    UeExtended,
    UnrealLegacy,
    Unity,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireProcessingPath {
    Upgrade,
    Native,
    Unmanaged,
}

impl From<WireProcessingPath> for RenoDxProcessingPath {
    fn from(value: WireProcessingPath) -> Self {
        match value {
            WireProcessingPath::Upgrade => Self::Upgrade,
            WireProcessingPath::Native => Self::Native,
            WireProcessingPath::Unmanaged => Self::Unmanaged,
        }
    }
}

impl WireProfileId {
    const fn as_str(self) -> &'static str {
        match self {
            Self::UeExtended => "ue_extended",
            Self::UnrealLegacy => "unreal_legacy",
            Self::Unity => "unity",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireEngine {
    Unreal,
    Unity,
}

impl From<WireEngine> for Engine {
    fn from(value: WireEngine) -> Self {
        match value {
            WireEngine::Unreal => Self::Unreal,
            WireEngine::Unity => Self::Unity,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireMatchKind {
    SteamAppid,
    EpicId,
    GogId,
    XboxStoreId,
    ExeName,
}

impl From<WireMatchKind> for MatchKind {
    fn from(value: WireMatchKind) -> Self {
        match value {
            WireMatchKind::SteamAppid => Self::SteamAppid,
            WireMatchKind::EpicId => Self::EpicId,
            WireMatchKind::GogId => Self::GogId,
            WireMatchKind::XboxStoreId => Self::XboxStoreId,
            WireMatchKind::ExeName => Self::ExeName,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireMatchRule {
    kind: WireMatchKind,
    value: String,
    tier: u32,
}

impl From<WireMatchRule> for MatchRule {
    fn from(value: WireMatchRule) -> Self {
        Self {
            kind: value.kind.into(),
            value: value.value,
            tier: value.tier,
        }
    }
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireGame {
    id: String,
    name: String,
    architecture: Architecture,
    status: Status,
    r#match: Vec<WireMatchRule>,
    addon: WireAddon,
    #[serde(default)]
    profile_id: Option<WireProfileId>,
    #[serde(default)]
    processing_path: Option<WireProcessingPath>,
    #[serde(default)]
    renodx_config: Option<WireRenoDxConfig>,
    #[serde(default = "default_true")]
    inherit_page_guidance: bool,
    #[serde(default)]
    guidance: Vec<WireGuidance>,
    #[serde(default)]
    requirements: Option<WireRequirements>,
    #[serde(default)]
    availability: Option<WireAvailability>,
    #[serde(default)]
    constraints: WireCompatibility,
    #[serde(default)]
    proxy_dll: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRenoDxConfig {
    settings: Vec<WireRenoDxConfigSetting>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRenoDxConfigSetting {
    key: WireRenoDxConfigKey,
    value: i32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum WireRenoDxConfigKey {
    Known(WireKnownRenoDxConfigKey),
    Unknown(String),
}

impl WireRenoDxConfigKey {
    fn as_str(&self) -> &str {
        match self {
            Self::Known(key) => key.as_str(),
            Self::Unknown(key) => key,
        }
    }

    fn into_known(self) -> Option<RenoDxConfigKey> {
        match self {
            Self::Known(key) => Some(key.into()),
            Self::Unknown(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
enum WireKnownRenoDxConfigKey {
    #[serde(rename = "Upgrade_B8G8R8A8_TYPELESS")]
    UpgradeB8G8R8A8Typeless,
    #[serde(rename = "Upgrade_B8G8R8A8_UNORM")]
    UpgradeB8G8R8A8Unorm,
    #[serde(rename = "Upgrade_R8G8B8A8_TYPELESS")]
    UpgradeR8G8B8A8Typeless,
    #[serde(rename = "Upgrade_R8G8B8A8_UNORM")]
    UpgradeR8G8B8A8Unorm,
    #[serde(rename = "Upgrade_R10G10B10A2_UNORM")]
    UpgradeR10G10B10A2Unorm,
    #[serde(rename = "Upgrade_R10G10B10A2_TYPELESS")]
    UpgradeR10G10B10A2Typeless,
    #[serde(rename = "Upgrade_R11G11B10_FLOAT")]
    UpgradeR11G11B10Float,
    #[serde(rename = "Upgrade_R16G16B16A16_TYPELESS")]
    UpgradeR16G16B16A16Typeless,
    #[serde(rename = "Upgrade_CopyDestinations")]
    UpgradeCopyDestinations,
    #[serde(rename = "ForceBorderless")]
    ForceBorderless,
    #[serde(rename = "Upgrade_UseSCRGB")]
    UpgradeUseScrgb,
    #[serde(rename = "Swapchain_Encoding")]
    SwapchainEncoding,
    #[serde(rename = "Scaling_Offset")]
    ScalingOffset,
    #[serde(rename = "Tonemap_Offset")]
    TonemapOffset,
    #[serde(rename = "Blit_Copy_Hack")]
    BlitCopyHack,
    #[serde(rename = "Use_Swapchain_Proxy")]
    UseSwapchainProxy,
    #[serde(rename = "Force_Pipeline_Cloning")]
    ForcePipelineCloning,
    #[serde(rename = "ColorGradeContrast")]
    ColorGradeContrast,
    #[serde(rename = "ColorGradeSaturation")]
    ColorGradeSaturation,
    #[serde(rename = "ColorGradeBlowout")]
    ColorGradeBlowout,
}

impl WireKnownRenoDxConfigKey {
    fn as_str(self) -> &'static str {
        RenoDxConfigKey::from(self).as_str()
    }
}

impl From<WireKnownRenoDxConfigKey> for RenoDxConfigKey {
    fn from(value: WireKnownRenoDxConfigKey) -> Self {
        match value {
            WireKnownRenoDxConfigKey::UpgradeB8G8R8A8Typeless => Self::UpgradeB8G8R8A8Typeless,
            WireKnownRenoDxConfigKey::UpgradeB8G8R8A8Unorm => Self::UpgradeB8G8R8A8Unorm,
            WireKnownRenoDxConfigKey::UpgradeR8G8B8A8Typeless => Self::UpgradeR8G8B8A8Typeless,
            WireKnownRenoDxConfigKey::UpgradeR8G8B8A8Unorm => Self::UpgradeR8G8B8A8Unorm,
            WireKnownRenoDxConfigKey::UpgradeR10G10B10A2Unorm => Self::UpgradeR10G10B10A2Unorm,
            WireKnownRenoDxConfigKey::UpgradeR10G10B10A2Typeless => {
                Self::UpgradeR10G10B10A2Typeless
            }
            WireKnownRenoDxConfigKey::UpgradeR11G11B10Float => Self::UpgradeR11G11B10Float,
            WireKnownRenoDxConfigKey::UpgradeR16G16B16A16Typeless => {
                Self::UpgradeR16G16B16A16Typeless
            }
            WireKnownRenoDxConfigKey::UpgradeCopyDestinations => Self::UpgradeCopyDestinations,
            WireKnownRenoDxConfigKey::ForceBorderless => Self::ForceBorderless,
            WireKnownRenoDxConfigKey::UpgradeUseScrgb => Self::UpgradeUseScrgb,
            WireKnownRenoDxConfigKey::SwapchainEncoding => Self::SwapchainEncoding,
            WireKnownRenoDxConfigKey::ScalingOffset => Self::ScalingOffset,
            WireKnownRenoDxConfigKey::TonemapOffset => Self::TonemapOffset,
            WireKnownRenoDxConfigKey::BlitCopyHack => Self::BlitCopyHack,
            WireKnownRenoDxConfigKey::UseSwapchainProxy => Self::UseSwapchainProxy,
            WireKnownRenoDxConfigKey::ForcePipelineCloning => Self::ForcePipelineCloning,
            WireKnownRenoDxConfigKey::ColorGradeContrast => Self::ColorGradeContrast,
            WireKnownRenoDxConfigKey::ColorGradeSaturation => Self::ColorGradeSaturation,
            WireKnownRenoDxConfigKey::ColorGradeBlowout => Self::ColorGradeBlowout,
        }
    }
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
    id: WireProfileId,
    engine: WireEngine,
    generic_fallback: bool,
    status: Status,
    addon: WireEngineAddon,
    message: WireCatalogMessage,
    guidance: Vec<WireGuidance>,
    processing_path: WireProcessingPath,
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireGuidance {
    id: String,
    kind: RenoDxGuidanceKind,
    message_id: String,
    fallback_text: String,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    settings: Vec<RenoDxSetting>,
    #[serde(default, deserialize_with = "deserialize_engine_ini")]
    engine_ini: Option<WireEngineIniRecipe>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    condition: Option<WireCondition>,
}

/// Keep the distinction between an omitted recipe (manual guidance) and an
/// explicit JSON null (malformed contract).  Serde's default handles the
/// missing field; this deserializer only accepts a concrete recipe value.
fn deserialize_engine_ini<'de, D>(deserializer: D) -> Result<Option<WireEngineIniRecipe>, D::Error>
where
    D: Deserializer<'de>,
{
    WireEngineIniRecipe::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireEngineIniRecipe {
    schema_version: u32,
    revision: u32,
    sections: Vec<WireEngineIniSection>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireEngineIniSection {
    name: String,
    entries: Vec<WireEngineIniEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireEngineIniEntry {
    key: String,
    value: String,
}

impl From<WireEngineIniRecipe> for RenoDxEngineIniRecipe {
    fn from(value: WireEngineIniRecipe) -> Self {
        Self {
            schema_version: value.schema_version,
            revision: value.revision,
            sections: value.sections.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<WireEngineIniSection> for RenoDxEngineIniSection {
    fn from(value: WireEngineIniSection) -> Self {
        Self {
            name: value.name,
            entries: value.entries.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<WireEngineIniEntry> for RenoDxEngineIniEntry {
    fn from(value: WireEngineIniEntry) -> Self {
        Self {
            key: value.key,
            value: value.value,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCondition {
    #[serde(default)]
    engine: Option<WireEngine>,
    #[serde(default)]
    unreal_major: Option<u32>,
    #[serde(default)]
    unreal_minor_min: Option<u32>,
    #[serde(default)]
    unreal_minor_max: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRequirements {
    launch: WireLaunch,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireLaunch {
    arguments: Vec<String>,
    requirement: RenoDxLaunchRequirementLevel,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
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
    pub(crate) fn from_wire_v2(wire: WireManifestV2) -> Self {
        let mut title_guidance = BTreeMap::new();
        let titles = wire
            .games
            .into_iter()
            .map(|game| {
                let guidance = game.guidance.into_iter().map(Into::into).collect();
                title_guidance.insert(game.id.clone(), guidance);
                let mut has_unsupported_settings = false;
                let renodx_config = game.renodx_config.map(|config| RenoDxConfig {
                    settings: config
                        .settings
                        .into_iter()
                        .filter_map(|setting| {
                            let key = setting.key.into_known();
                            if let Some(key) = key {
                                Some(RenoDxConfigSetting {
                                    key,
                                    value: setting.value,
                                })
                            } else {
                                has_unsupported_settings = true;
                                None
                            }
                        })
                        .collect(),
                });
                RenoDxTitle {
                    id: game.id,
                    name: game.name,
                    category: game
                        .availability
                        .map_or(RenoDxCategory::Installable, Into::into),
                    slug: game.addon.slug,
                    arch: game.architecture,
                    status: game.status,
                    match_rules: game.r#match.into_iter().map(Into::into).collect(),
                    compatibility: RenoDxCompatibility {
                        required_api: game.constraints.required_api,
                        conflicts: game.constraints.conflicts,
                        source: game.constraints.source,
                    },
                    proxy_dll_override: game.proxy_dll,
                    download_url: game.addon.source,
                    profile_id: game.profile_id.map(|profile| profile.as_str().to_owned()),
                    processing_path: game.processing_path.map(Into::into),
                    renodx_config,
                    has_unsupported_settings,
                    inherit_page_guidance: game.inherit_page_guidance,
                    launch: game.requirements.map(|r| RenoDxLaunchRequirement {
                        arguments: r.launch.arguments,
                        requirement: r.launch.requirement,
                    }),
                }
            })
            .collect();

        let generics = wire
            .engine_profiles
            .into_iter()
            .map(|profile| {
                let (url64, url32) = profile
                    .addon
                    .sources
                    .map(|sources| (sources.x64, sources.x86))
                    .unzip();
                RenoDxGeneric {
                    engine: profile.engine.into(),
                    status: profile.status,
                    slug: profile.addon.slug,
                    url64,
                    url32,
                    message: profile.message.into(),
                    profile_id: Some(profile.id.as_str().to_owned()),
                    generic_fallback: profile.generic_fallback,
                    processing_path: profile.processing_path.into(),
                    guidance: profile.guidance.into_iter().map(Into::into).collect(),
                }
            })
            .collect();

        Self {
            schema_version: wire.schema_version,
            generated_at: wire.generated_at,
            generics,
            titles,
            title_guidance,
            page_guidance: wire.page_guidance.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<WireGuidance> for RenoDxGuidance {
    fn from(value: WireGuidance) -> Self {
        Self {
            id: value.id,
            kind: value.kind,
            message_id: value.message_id,
            fallback_text: value.fallback_text,
            code: value.code,
            settings: value.settings,
            engine_ini: value.engine_ini.map(Into::into),
            url: value.url,
            condition: value.condition.map(Into::into),
        }
    }
}

impl From<WireCondition> for RenoDxGuidanceCondition {
    fn from(value: WireCondition) -> Self {
        Self {
            engine: value.engine.map(Into::into),
            unreal_major: value.unreal_major,
            unreal_minor_min: value.unreal_minor_min,
            unreal_minor_max: value.unreal_minor_max,
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
