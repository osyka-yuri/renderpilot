use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, str::FromStr};

/// Error returned when parsing a stable enum string fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseEnumError {
    type_name: &'static str,
    value: String,
}

impl ParseEnumError {
    #[must_use]
    pub fn new(type_name: &'static str, value: impl Into<String>) -> Self {
        Self {
            type_name,
            value: value.into(),
        }
    }

    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        self.type_name
    }

    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Display for ParseEnumError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid {} value: {:?}",
            self.type_name, self.value
        )
    }
}

impl Error for ParseEnumError {}

// Implementation detail: only invoked from `stable_enum!`. Expands to the enum with serde
// `rename` per wire literal, `ALL`, `as_str` (same literals), `from_stable_str` (trimmed
// exact match on those literals), plus `Display`, `AsRef<str>`, and `FromStr`.
macro_rules! define_stable_wire_enum {
    (
        derives = [$($derive:ident),+ $(,)?],
        $(#[$enum_meta:meta])*
        pub enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident = $stable:literal
            ),+ $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[derive($($derive),+)]
        pub enum $name {
            $(
                $(#[$variant_meta])*
                #[serde(rename = $stable)]
                $variant,
            )+
        }

        impl $name {
            /// All known variants in a stable order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// Returns the stable string representation.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $stable,)+
                }
            }

            /// Parses the stable string representation.
            #[must_use]
            pub fn from_stable_str(value: &str) -> Option<Self> {
                match value.trim() {
                    $($stable => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str((*self).as_str())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                (*self).as_str()
            }
        }

        impl FromStr for $name {
            type Err = ParseEnumError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::from_stable_str(value)
                    .ok_or_else(|| ParseEnumError::new(stringify!($name), value))
            }
        }
    };
}

/// Declares a wire-stable enum: serde and `as_str` use the given string literals;
/// `from_stable_str` trims input then matches those literals exactly.
///
/// With a leading `default,`, `Default` is also derived. Mark **exactly one** variant with
/// `#[default]` (the compiler rejects zero or multiple).
macro_rules! stable_enum {
    (
        default,
        $(#[$enum_meta:meta])*
        pub enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident = $stable:literal
            ),+ $(,)?
        }
    ) => {
        define_stable_wire_enum! {
            derives = [Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default],
            $(#[$enum_meta])*
            pub enum $name {
                $(
                    $(#[$variant_meta])*
                    $variant = $stable
                ),+
            }
        }
    };
    (
        $(#[$enum_meta:meta])*
        pub enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident = $stable:literal
            ),+ $(,)?
        }
    ) => {
        define_stable_wire_enum! {
            derives = [Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize],
            $(#[$enum_meta])*
            pub enum $name {
                $(
                    $(#[$variant_meta])*
                    $variant = $stable
                ),+
            }
        }
    };
}

stable_enum! {
    /// Operating system family targeted by a game installation or adapter.
    pub enum Platform {
        /// Microsoft Windows.
        Windows = "Windows",
        /// Linux distributions.
        Linux = "Linux",
        /// Apple macOS.
        MacOs = "MacOs",
    }
}

stable_enum! {
    /// Known launcher or distribution source for a game installation.
    pub enum Launcher {
        /// Valve Steam.
        Steam = "Steam",
        /// Epic Games Store.
        Epic = "Epic",
        /// GOG Galaxy or local GOG installation metadata.
        Gog = "Gog",
        /// Ubisoft Connect.
        Ubisoft = "Ubisoft",
        /// EA app.
        Ea = "Ea",
        /// Battle.net.
        BattleNet = "BattleNet",
        /// Xbox app or Microsoft Store installation.
        Xbox = "Xbox",
        /// User-provided path outside a known launcher.
        Manual = "Manual",
        /// Steam Proton compatibility runtime.
        Proton = "Proton",
        /// CodeWeavers CrossOver runtime.
        CrossOver = "CrossOver",
        /// Whisky runtime.
        Whisky = "Whisky",
    }
}

stable_enum! {
    /// Runtime used by a game executable.
    pub enum GameRuntime {
        /// Native Windows executable.
        NativeWindows = "NativeWindows",
        /// Native Linux executable.
        NativeLinux = "NativeLinux",
        /// Steam Proton compatibility runtime.
        Proton = "Proton",
        /// Generic Wine runtime.
        Wine = "Wine",
        /// CodeWeavers CrossOver runtime.
        CrossOver = "CrossOver",
        /// Whisky runtime.
        Whisky = "Whisky",
    }
}

/// Graphics or presentation technology detected in a game installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum GraphicsTechnology {
    /// NVIDIA DLSS Super Resolution.
    #[serde(rename = "dlss_super_resolution")]
    DlssSuperResolution,

    /// NVIDIA DLSS Frame Generation.
    #[serde(rename = "dlss_frame_generation")]
    DlssFrameGeneration,

    /// NVIDIA DLSS Ray Reconstruction.
    #[serde(rename = "dlss_ray_reconstruction")]
    DlssRayReconstruction,

    /// NVIDIA Streamline integration.
    #[serde(rename = "nvidia_streamline")]
    NvidiaStreamline,

    /// NVIDIA Reflex.
    #[serde(rename = "nvidia_reflex")]
    NvidiaReflex,

    /// Intel XeSS Super Resolution.
    #[serde(rename = "intel_xess")]
    IntelXeSs,

    /// Intel Xe Frame Generation.
    #[serde(rename = "intel_xefg")]
    IntelXeFg,

    /// Intel Xe Low Latency.
    #[serde(rename = "intel_xell")]
    IntelXeLl,

    /// AMD FidelityFX Super Resolution.
    #[serde(rename = "amd_fsr")]
    AmdFsr,

    /// AMD FidelityFX Frame Generation.
    #[serde(rename = "amd_fsr_frame_generation")]
    AmdFsrFrameGeneration,

    /// AMD FSR Ray Regeneration (denoiser).
    #[serde(rename = "amd_fsr_ray_regeneration")]
    AmdFsrRayRegeneration,

    /// Microsoft DirectStorage runtime.
    #[serde(rename = "direct_storage")]
    DirectStorage,

    /// Technology is present but not classified yet.
    #[default]
    #[serde(rename = "unknown")]
    Unknown,
}

impl GraphicsTechnology {
    /// All known variants in a stable order.
    pub const ALL: &'static [Self] = &[
        Self::DlssSuperResolution,
        Self::DlssFrameGeneration,
        Self::DlssRayReconstruction,
        Self::NvidiaStreamline,
        Self::NvidiaReflex,
        Self::IntelXeSs,
        Self::IntelXeFg,
        Self::IntelXeLl,
        Self::AmdFsr,
        Self::AmdFsrFrameGeneration,
        Self::AmdFsrRayRegeneration,
        Self::DirectStorage,
        Self::Unknown,
    ];

    /// Returns the stable snake_case identifier used by CLI filters and output.
    #[must_use]
    pub const fn as_slug(self) -> &'static str {
        match self {
            Self::DlssSuperResolution => "dlss_super_resolution",
            Self::DlssFrameGeneration => "dlss_frame_generation",
            Self::DlssRayReconstruction => "dlss_ray_reconstruction",
            Self::NvidiaStreamline => "nvidia_streamline",
            Self::NvidiaReflex => "nvidia_reflex",
            Self::IntelXeSs => "intel_xess",
            Self::IntelXeFg => "intel_xefg",
            Self::IntelXeLl => "intel_xell",
            Self::AmdFsr => "amd_fsr",
            Self::AmdFsrFrameGeneration => "amd_fsr_frame_generation",
            Self::AmdFsrRayRegeneration => "amd_fsr_ray_regeneration",
            Self::DirectStorage => "direct_storage",
            Self::Unknown => "unknown",
        }
    }

    /// Parses a stable snake_case identifier.
    #[must_use]
    pub fn from_slug(value: &str) -> Option<Self> {
        let value = value.trim();

        Self::ALL
            .iter()
            .copied()
            .find(|technology| technology.as_slug().eq_ignore_ascii_case(value))
    }
}

impl fmt::Display for GraphicsTechnology {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str((*self).as_slug())
    }
}

impl AsRef<str> for GraphicsTechnology {
    fn as_ref(&self) -> &str {
        (*self).as_slug()
    }
}

impl FromStr for GraphicsTechnology {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_slug(value).ok_or_else(|| ParseEnumError::new("GraphicsTechnology", value))
    }
}

stable_enum! {
    /// Kind of configurable or replaceable component.
    pub enum ComponentKind {
        /// Native library such as a DLL or shared object.
        NativeLibrary = "NativeLibrary",
        /// Component that belongs to an NVIDIA Streamline bundle.
        StreamlineComponent = "StreamlineComponent",
        /// Driver-level profile setting.
        DriverProfileSetting = "DriverProfileSetting",
        /// Game configuration setting.
        GameConfigSetting = "GameConfigSetting",
        /// Launch argument or environment option.
        LaunchOption = "LaunchOption",
        /// Runtime layer such as Proton, DXVK, or VKD3D.
        RuntimeLayer = "RuntimeLayer",
    }
}

stable_enum! {
    default,
    /// Replacement policy for a detected component.
    pub enum Swappability {
        /// Component can be replaced independently.
        Swappable = "Swappable",
        /// Component must be handled as part of a bundle.
        BundleOnly = "BundleOnly",
        /// Component should only be inspected.
        ReadOnly = "ReadOnly",
        /// Component is integrated into the game engine and cannot be swapped safely.
        IntegratedIntoEngine = "IntegratedIntoEngine",
        /// Replacement policy is not known yet.
        #[default]
        Unknown = "Unknown",
        /// Replacement is known to be unsafe.
        Unsafe = "Unsafe",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_stable_strings_round_trip() {
        for platform in Platform::ALL {
            assert_eq!(platform.as_str().parse::<Platform>().unwrap(), *platform);
            assert_eq!(platform.to_string(), platform.as_str());
            assert_eq!(platform.as_ref(), platform.as_str());
        }
    }

    #[test]
    fn launcher_stable_strings_round_trip() {
        for launcher in Launcher::ALL {
            assert_eq!(launcher.as_str().parse::<Launcher>().unwrap(), *launcher);
            assert_eq!(launcher.to_string(), launcher.as_str());
            assert_eq!(launcher.as_ref(), launcher.as_str());
        }
    }

    #[test]
    fn game_runtime_stable_strings_round_trip() {
        for runtime in GameRuntime::ALL {
            assert_eq!(runtime.as_str().parse::<GameRuntime>().unwrap(), *runtime);
            assert_eq!(runtime.to_string(), runtime.as_str());
            assert_eq!(runtime.as_ref(), runtime.as_str());
        }
    }

    #[test]
    fn component_kind_stable_strings_round_trip() {
        for kind in ComponentKind::ALL {
            assert_eq!(kind.as_str().parse::<ComponentKind>().unwrap(), *kind);
            assert_eq!(kind.to_string(), kind.as_str());
            assert_eq!(kind.as_ref(), kind.as_str());
        }
    }

    #[test]
    fn swappability_stable_strings_round_trip() {
        for swappability in Swappability::ALL {
            assert_eq!(
                swappability.as_str().parse::<Swappability>().unwrap(),
                *swappability
            );
            assert_eq!(swappability.to_string(), swappability.as_str());
            assert_eq!(swappability.as_ref(), swappability.as_str());
        }
    }

    #[test]
    fn graphics_technology_slug_round_trips() {
        for technology in GraphicsTechnology::ALL {
            assert_eq!(
                GraphicsTechnology::from_slug(technology.as_slug()),
                Some(*technology)
            );
            assert_eq!(
                technology.as_slug().parse::<GraphicsTechnology>().unwrap(),
                *technology
            );
            assert_eq!(technology.to_string(), technology.as_slug());
            assert_eq!(technology.as_ref(), technology.as_slug());
        }
    }

    #[test]
    fn graphics_technology_slug_parse_is_trimmed_and_case_insensitive() {
        assert_eq!(
            GraphicsTechnology::from_slug(" DLSS_SUPER_RESOLUTION "),
            Some(GraphicsTechnology::DlssSuperResolution)
        );
    }

    #[test]
    fn graphics_technology_rejects_unknown_slug() {
        assert_eq!(GraphicsTechnology::from_slug("does_not_exist"), None);
        assert!("does_not_exist".parse::<GraphicsTechnology>().is_err());
    }

    #[test]
    fn defaults_are_unknown_when_available() {
        assert_eq!(GraphicsTechnology::default(), GraphicsTechnology::Unknown);
        assert_eq!(Swappability::default(), Swappability::Unknown);
    }

    #[test]
    fn serde_swappability_unknown_round_trips() {
        let json = serde_json::to_string(&Swappability::Unknown).unwrap();
        assert_eq!(json, "\"Unknown\"");

        let parsed: Swappability = serde_json::from_str("\"Unknown\"").unwrap();
        assert_eq!(parsed, Swappability::Unknown);
    }

    #[test]
    fn serde_swappability_default_matches_unknown_wire_value() {
        assert_eq!(
            serde_json::to_string(&Swappability::default()).unwrap(),
            serde_json::to_string(&Swappability::Unknown).unwrap()
        );
    }

    #[test]
    fn serde_uses_explicit_platform_wire_names() {
        let json = serde_json::to_string(&Platform::MacOs).unwrap();
        assert_eq!(json, "\"MacOs\"");

        let platform: Platform = serde_json::from_str("\"MacOs\"").unwrap();
        assert_eq!(platform, Platform::MacOs);
    }

    #[test]
    fn serde_uses_graphics_slugs_only() {
        let json = serde_json::to_string(&GraphicsTechnology::DlssSuperResolution).unwrap();
        assert_eq!(json, "\"dlss_super_resolution\"");

        let from_slug: GraphicsTechnology =
            serde_json::from_str("\"dlss_super_resolution\"").unwrap();
        assert_eq!(from_slug, GraphicsTechnology::DlssSuperResolution);
    }

    #[test]
    fn serde_rejects_legacy_graphics_variant_names() {
        let legacy_name = serde_json::from_str::<GraphicsTechnology>("\"DlssSuperResolution\"");

        assert!(legacy_name.is_err());
    }
}
