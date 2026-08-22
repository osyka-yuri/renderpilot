//! Shared ReShade Vulkan implicit-layer management.
//!
//! Matches the official ReShade installer exactly: layer files live in
//! `C:\ProgramData\ReShade\`, registration is under
//! `HKLM\Software\Khronos\Vulkan\ImplicitLayers`, and per-app tracking uses
//! `ReShadeApps.ini` in the same directory. This makes our install
//! indistinguishable from the official one - the official installer can update
//! our install, and we can update theirs.

/// Vulkan loader implicit-layer registry key (relative to a hive root).
#[cfg(windows)]
const IMPLICIT_LAYERS_KEY: &str = r"Software\Khronos\Vulkan\ImplicitLayers";
/// 32-bit view of the same key on 64-bit Windows.
#[cfg(windows)]
const IMPLICIT_LAYERS_KEY_WOW64: &str = r"Software\Wow6432Node\Khronos\Vulkan\ImplicitLayers";

/// Layer name in the manifest (matches ReShade's official layer name).
const LAYER_NAME: &str = "VK_LAYER_reshade";
/// The ReShade host DLL file name.
pub const LAYER_DLL_NAME: &str = "ReShade64.dll";
/// The Vulkan layer manifest file name (matches ReShade's official name).
const LAYER_JSON_NAME: &str = "ReShade64.json";
/// App tracking file name (matches ReShade's official name).
const APPS_INI_NAME: &str = "ReShadeApps.ini";
const APPS_KEY: &str = "Apps";

mod apps_ini;
mod detection;
#[cfg(test)]
mod install;
mod manifest;
mod paths;
mod pe;
mod planner;
mod registry;
mod types;
mod util;

pub use apps_ini::{
    AppListChange, AppListPlan, AppListPlanError, parse_app_list, plan_register_app,
    plan_unregister_app, read_app_list, read_app_list_bytes,
};
pub use detection::detect_report;
pub use paths::reshade_common_dir;
#[cfg(windows)]
pub use planner::observe_standard_shared_vulkan_layer;
pub use planner::{
    AppUnregisterOutcome, DirectoryEntryKind, DirectoryEntryObservation, DirectoryMutation,
    DirectoryObservation, FileMutation, FileObservation, LayerPlanOperation, LayerPlannerError,
    RegistryMutation, SharedVulkanLayerObservation, SharedVulkanLayerPlan, active_registry_value,
    canonical_manifest_bytes, observe_shared_vulkan_layer, plan_install_and_register, plan_refresh,
    plan_register_app_only, plan_settings_remove, plan_unregister_app_only, unregister_app_outcome,
};
#[cfg(windows)]
pub use registry::WindowsLayerRegistry;
pub use registry::{LayerRegistry, RegistryValueState};
pub use types::{
    LayerRegistryEntry, RegistryHive, VulkanLayerArchitecture, VulkanLayerDiagnostic,
    VulkanLayerFacts, VulkanLayerReport, VulkanLayerState, VulkanLoaderVisibility,
};

#[cfg(test)]
pub(crate) use apps_ini::{register_app, unregister_app, write_app_list};
#[cfg(test)]
pub(crate) use detection::detect;
#[cfg(test)]
pub(crate) use install::{LayerInstallError, install, uninstall};
#[cfg(test)]
pub(crate) use manifest::{inspect_manifest, layer_manifest_json, resolve_library_path};
#[cfg(test)]
pub(crate) use util::same_path;

#[cfg(test)]
mod tests;
