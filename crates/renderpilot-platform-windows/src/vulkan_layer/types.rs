use std::path::PathBuf;

/// Detector state used by orchestration before mapping to its public DTO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulkanLayerState {
    /// No ReShade Vulkan layer is registered.
    Absent,
    /// A compatible ReShade layer is visible in the standard location and active.
    Installed,
    /// The standard ReShade layer is registered, but disabled in the loader registry.
    InstalledDisabled,
    /// A compatible ReShade layer is visible, but in a non-standard location.
    External,
    /// The loader-visible ReShade state is broken or ambiguous.
    Conflict,
    /// A layer is visible but unsupported for the target architecture.
    Unsupported,
}

/// Closed detector diagnostics. Orchestration maps these one-for-one into the
/// public RenoDX DTO without exposing private proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulkanLayerDiagnostic {
    /// The standard manifest is present on disk but not registered with the loader.
    RegistryMissing,
    /// The loader registry entry exists but is disabled (DWORD data is not `0`).
    RegistryDisabled,
    /// More than one ReShade-looking Vulkan manifest is visible to the loader.
    DuplicateLayerManifest,
    /// Loader visibility cannot be described by a single reliable registration.
    AmbiguousLoaderVisibility,
    /// A ReShade manifest points to a DLL that is missing.
    MissingLayerDll,
    /// A ReShade manifest points to a DLL that exists but cannot be read
    /// (permission denied, locked, etc.).
    UnreadableDll,
    /// A registry entry points to a manifest that is not present.
    MissingManifest,
    /// The visible layer DLL is not usable for the supported target bitness.
    UnsupportedArchitecture,
    /// HKCU layer registration may not be visible to elevated games.
    HkcuNotVisibleWhenElevated,
    /// A ReShade-looking manifest exists but cannot be parsed or trusted.
    ManifestMalformed,
    /// Backend validation could not prove that the layer is usable.
    BackendValidationFailed,
    /// The registry scope used by the official ReShade layout cannot be written.
    RegistryScopeNotWritable,
    /// Windows denied a required registry or filesystem operation.
    PermissionDenied,
    /// The actual DLL digest does not match the expected upstream digest.
    HashMismatch,
    /// The DLL is missing/unreadable and only an advisory DB digest is available.
    DbOnlyFallback,
}

/// Architecture of a visible layer DLL, when known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulkanLayerArchitecture {
    /// 64-bit layer DLL.
    X64,
    /// 32-bit layer DLL.
    X86,
    /// DLL architecture could not be determined.
    Unknown,
}

/// Loader visibility caveat for a visible layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulkanLoaderVisibility {
    /// Normal loader discovery applies.
    Normal,
    /// HKCU registration may not be visible to elevated games.
    HkcuNotVisibleWhenElevated,
    /// Visibility is ambiguous across hives/views.
    Ambiguous,
}

/// Observable shared-layer facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VulkanLayerFacts {
    /// Path to the visible layer manifest, when known.
    pub manifest_path: Option<PathBuf>,
    /// Path to the visible layer DLL, when known.
    pub dll_path: Option<PathBuf>,
    /// Display version, when readable.
    pub version: Option<String>,
    /// Detected DLL architecture.
    pub architecture: VulkanLayerArchitecture,
    /// Loader visibility caveat.
    pub loader_visibility: VulkanLoaderVisibility,
}

impl Default for VulkanLayerFacts {
    fn default() -> Self {
        Self {
            manifest_path: None,
            dll_path: None,
            version: None,
            architecture: VulkanLayerArchitecture::Unknown,
            loader_visibility: VulkanLoaderVisibility::Normal,
        }
    }
}

/// Full detector report for orchestration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VulkanLayerReport {
    /// Overall detector state.
    pub state: VulkanLayerState,
    /// Observable layer paths/version/architecture facts.
    pub facts: VulkanLayerFacts,
    /// Closed diagnostics that explain conflicts or read-only states.
    pub diagnostics: Vec<VulkanLayerDiagnostic>,
}

/// Which Windows registry hive a layer registration was found in.
///
/// The Vulkan loader consults both HKLM and HKCU for implicit-layer
/// registrations. HKLM registrations are visible to all processes including
/// elevated ones; HKCU registrations may not be visible to elevated games.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryHive {
    /// `HKEY_LOCAL_MACHINE` — visible to all processes.
    Hklm,
    /// `HKEY_CURRENT_USER` — may not be visible to elevated games.
    Hkcu,
}

/// A Vulkan implicit-layer registry value as consumed by the detector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerRegistryEntry {
    /// Manifest path stored in the registry value name.
    pub manifest_path: PathBuf,
    /// `true` only when the registry value data is a DWORD `0`.
    pub active: bool,
    /// Which hive this entry was read from.
    pub hive: RegistryHive,
}

impl LayerRegistryEntry {
    /// Creates an active loader entry (HKLM by default).
    #[must_use]
    pub fn active(manifest_path: impl Into<PathBuf>) -> Self {
        Self {
            manifest_path: manifest_path.into(),
            active: true,
            hive: RegistryHive::Hklm,
        }
    }

    /// Creates a disabled loader entry (HKLM by default).
    #[must_use]
    pub fn disabled(manifest_path: impl Into<PathBuf>) -> Self {
        Self {
            manifest_path: manifest_path.into(),
            active: false,
            hive: RegistryHive::Hklm,
        }
    }
}

// -----------------------------------------------------------------------------
// Directory resolution
// -----------------------------------------------------------------------------
