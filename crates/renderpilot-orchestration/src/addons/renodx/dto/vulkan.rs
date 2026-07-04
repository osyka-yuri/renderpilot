/// Public DTOs for the shared ReShade Vulkan layer.
use std::path::PathBuf;

use serde::Serialize;

use crate::addons::reshade::dto::ActionDescriptor;
use crate::addons::reshade::types::ReshadeChannel;

/// Public Vulkan layer detection state. This never encodes install origin; action
/// rights are expressed only via [`VulkanLayerActions`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VulkanLayerDetection {
    /// No compatible shared layer is visible.
    NotInstalled,
    /// Exactly one compatible shared layer is visible and backend actions are available.
    Installed,
    /// The standard layer is registered but disabled in the loader registry (DWORD != 0).
    InstalledDisabled,
    /// Exactly one compatible shared layer is visible, but only observation is safe.
    ExternalReadOnly,
    /// The visible layer state is ambiguous or broken.
    Conflict,
    /// Vulkan layer management is unsupported in this environment.
    Unsupported,
}

/// Loader visibility caveats the UI may explain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VulkanLoaderVisibility {
    /// Normal loader discovery applies.
    Normal,
    /// HKCU discovery may not apply to elevated games.
    HkcuNotVisibleWhenElevated,
    /// The resolver cannot make one clear visibility claim.
    Ambiguous,
}

/// Closed diagnostics for read-only/conflict layer states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerDiagnosticReason {
    /// A compatible existing layer is visible but action rights are unavailable.
    ExternalLayerDetected,
    /// More than one matching layer manifest or layer name is visible.
    DuplicateLayerManifest,
    /// Loader visibility depends on context or cannot be resolved safely.
    AmbiguousLoaderVisibility,
    /// A manifest points at a DLL that is not present.
    MissingLayerDll,
    /// A manifest points at a DLL that exists but cannot be read.
    UnreadableDll,
    /// A registry entry points to a manifest that is not present.
    MissingManifest,
    /// The standard manifest is present on disk but not registered with the loader.
    RegistryMissing,
    /// The loader registry entry exists but is disabled (DWORD data is not `0`).
    RegistryDisabled,
    /// The visible layer architecture is unsupported for the target.
    UnsupportedArchitecture,
    /// HKCU layer registration may be skipped for elevated games.
    HkcuNotVisibleWhenElevated,
    /// A layer manifest could not be parsed or trusted.
    ManifestMalformed,
    /// The required registry scope cannot be written.
    RegistryScopeNotWritable,
    /// The operating system denied a required operation.
    PermissionDenied,
    /// Backend validation failed without exposing private proof details.
    BackendValidationFailed,
    /// The actual DLL digest does not match the expected upstream digest.
    HashMismatch,
    /// The DLL is missing/unreadable and only an advisory DB digest is available.
    DbOnlyFallback,
}

/// Architecture of the visible shared layer, when known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VulkanLayerArchitecture {
    /// 64-bit layer.
    X64,
    /// 32-bit layer.
    X86,
    /// Architecture is not known yet.
    Unknown,
}

/// Observable facts about the shared Vulkan layer.
#[derive(Debug, Clone, Serialize)]
pub struct VulkanLayerFacts {
    /// Path to the visible manifest, when known.
    pub manifest_path: Option<PathBuf>,
    /// Path to the layer DLL, when known.
    pub dll_path: Option<PathBuf>,
    /// Display version, when readable.
    pub version: Option<String>,
    /// Detected architecture.
    pub architecture: VulkanLayerArchitecture,
    /// Loader visibility caveat.
    pub loader_visibility: VulkanLoaderVisibility,
}

/// Backend-authored actions for the shared Vulkan layer.
#[derive(Debug, Clone, Serialize)]
pub struct VulkanLayerActions {
    /// Install the shared layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install: Option<ActionDescriptor>,
    /// Update the shared layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<ActionDescriptor>,
    /// Switch the shared layer channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switch_channel: Option<ActionDescriptor>,
    /// Remove the shared layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove: Option<ActionDescriptor>,
    /// Resolve a layer conflict.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolve_conflict: Option<ActionDescriptor>,
}

/// Public shared Vulkan layer report.
#[derive(Debug, Clone, Serialize)]
pub struct VulkanLayerReport {
    /// Detection state.
    pub layer_detection: VulkanLayerDetection,
    /// Observable facts.
    pub layer_facts: VulkanLayerFacts,
    /// Closed diagnostics ordered by display priority.
    pub diagnostic_reasons: Vec<LayerDiagnosticReason>,
    /// Backend-authored actions.
    pub actions: VulkanLayerActions,
}

impl VulkanLayerReport {
    /// Returns the detection state.
    pub fn detection(&self) -> VulkanLayerDetection {
        self.layer_detection
    }
}

use crate::addons::update::UpdateStatus;

/// Settings-facing shared Vulkan layer report. This wraps the platform report
/// with manifest/advisory channel facts so UI channel controls do not infer
/// backend capabilities from paths or diagnostics.
#[derive(Debug, Clone, Serialize)]
pub struct VulkanLayerManagementReport {
    /// Current shared layer report.
    pub layer: VulkanLayerReport,
    /// Whether the current RenoDX manifest can provide Stable ReShade.
    pub reshade_stable_supported: bool,
    /// Channel recorded in the advisory shared-artifact record, when known.
    pub recorded_channel: Option<ReshadeChannel>,
    /// Effective channel for maintenance operations when no recorded channel is known.
    pub default_channel: ReshadeChannel,
    /// Status of the ReShade update check, if checked.
    pub update_status: Option<UpdateStatus>,
}
