use serde::{Deserialize, Serialize};

/// Operating system family targeted by a game installation or adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    /// Microsoft Windows.
    Windows,
    /// Linux distributions.
    Linux,
    /// Apple macOS.
    MacOs,
}

/// Known launcher or distribution source for a game installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Launcher {
    /// Valve Steam.
    Steam,
    /// Epic Games Store.
    Epic,
    /// GOG Galaxy or local GOG installation metadata.
    Gog,
    /// Ubisoft Connect.
    Ubisoft,
    /// EA app.
    Ea,
    /// Battle.net.
    BattleNet,
    /// Xbox app or Microsoft Store installation.
    Xbox,
    /// User-provided path outside a known launcher.
    Manual,
    /// Steam Proton compatibility runtime.
    Proton,
    /// CodeWeavers CrossOver runtime.
    CrossOver,
    /// Whisky runtime.
    Whisky,
}

/// Runtime used by a game executable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameRuntime {
    /// Native Windows executable.
    NativeWindows,
    /// Native Linux executable.
    NativeLinux,
    /// Steam Proton compatibility runtime.
    Proton,
    /// Generic Wine runtime.
    Wine,
    /// CodeWeavers CrossOver runtime.
    CrossOver,
    /// Whisky runtime.
    Whisky,
}

/// Graphics or presentation technology detected in a game installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GraphicsTechnology {
    /// NVIDIA DLSS Super Resolution.
    DlssSuperResolution,
    /// NVIDIA DLSS Frame Generation.
    DlssFrameGeneration,
    /// NVIDIA DLSS Ray Reconstruction.
    DlssRayReconstruction,
    /// NVIDIA Streamline integration.
    NvidiaStreamline,
    /// NVIDIA Reflex.
    NvidiaReflex,
    /// Intel XeSS Super Resolution.
    IntelXeSs,
    /// Intel Xe Frame Generation.
    IntelXeFg,
    /// AMD FidelityFX Super Resolution.
    AmdFsr,
    /// AMD FidelityFX Frame Generation.
    AmdFsrFrameGeneration,
    /// OptiScaler integration.
    OptiScaler,
    /// Technology is present but not classified yet.
    Unknown,
}

impl GraphicsTechnology {
    /// Returns the stable snake_case identifier used by CLI filters and output.
    pub const fn as_slug(self) -> &'static str {
        match self {
            Self::DlssSuperResolution => "dlss_super_resolution",
            Self::DlssFrameGeneration => "dlss_frame_generation",
            Self::DlssRayReconstruction => "dlss_ray_reconstruction",
            Self::NvidiaStreamline => "nvidia_streamline",
            Self::NvidiaReflex => "nvidia_reflex",
            Self::IntelXeSs => "intel_xess",
            Self::IntelXeFg => "intel_xefg",
            Self::AmdFsr => "amd_fsr",
            Self::AmdFsrFrameGeneration => "amd_fsr_frame_generation",
            Self::OptiScaler => "optiscaler",
            Self::Unknown => "unknown",
        }
    }

    /// Parses a stable snake_case identifier.
    pub fn from_slug(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "dlss_super_resolution" => Some(Self::DlssSuperResolution),
            "dlss_frame_generation" => Some(Self::DlssFrameGeneration),
            "dlss_ray_reconstruction" => Some(Self::DlssRayReconstruction),
            "nvidia_streamline" => Some(Self::NvidiaStreamline),
            "nvidia_reflex" => Some(Self::NvidiaReflex),
            "intel_xess" => Some(Self::IntelXeSs),
            "intel_xefg" => Some(Self::IntelXeFg),
            "amd_fsr" => Some(Self::AmdFsr),
            "amd_fsr_frame_generation" => Some(Self::AmdFsrFrameGeneration),
            "optiscaler" => Some(Self::OptiScaler),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// Kind of configurable or replaceable component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComponentKind {
    /// Native library such as a DLL or shared object.
    NativeLibrary,
    /// Component that belongs to an NVIDIA Streamline bundle.
    StreamlineComponent,
    /// Driver-level profile setting.
    DriverProfileSetting,
    /// Game configuration setting.
    GameConfigSetting,
    /// Launch argument or environment option.
    LaunchOption,
    /// Runtime layer such as Proton, DXVK, or VKD3D.
    RuntimeLayer,
}

/// Replacement policy for a detected component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Swappability {
    /// Component can be replaced independently.
    Swappable,
    /// Component must be handled as part of a bundle.
    BundleOnly,
    /// Component should only be inspected.
    ReadOnly,
    /// Component is integrated into the game engine and cannot be swapped safely.
    IntegratedIntoEngine,
    /// Replacement policy is not known yet.
    Unknown,
    /// Replacement is known to be unsafe.
    Unsafe,
}

#[cfg(test)]
mod tests {
    use super::GraphicsTechnology;

    #[test]
    fn graphics_technology_slug_round_trips() {
        let technology = GraphicsTechnology::from_slug("dlss_super_resolution")
            .expect("technology should parse");

        assert_eq!(technology, GraphicsTechnology::DlssSuperResolution);
        assert_eq!(technology.as_slug(), "dlss_super_resolution");
    }

    #[test]
    fn graphics_technology_rejects_unknown_slug() {
        assert_eq!(GraphicsTechnology::from_slug("does_not_exist"), None);
    }
}
