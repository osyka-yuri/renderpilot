//! Orchestration glue for the global ReShade Vulkan layer.
//!
//! Wraps the Windows-only platform layer
//! ([`renderpilot_platform_windows::vulkan_layer`]) in a cross-platform service API:
//! a status the UI can render, and consent-gated install / removal. The layer is a
//! single shared resource (one ReShade Vulkan overlay system-wide), so the service
//! detects an existing one and reuses it, installing its own only when none is
//! present. On non-Windows every operation reports [`VulkanLayerStatus::Unsupported`]
//! / errors — RenoDX for Vulkan is a Windows feature.

use serde::Serialize;

use super::errors;
use crate::ServiceError;

/// The global ReShade Vulkan layer state, as the service and UI see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VulkanLayerStatus {
    /// No ReShade Vulkan layer is registered; a Vulkan install must add one.
    Absent,
    /// A foreign ReShade layer is present and will be reused untouched.
    Foreign,
    /// A layer RenderPilot installed is present.
    Managed,
    /// Not a Windows host — RenoDX for Vulkan is unavailable.
    Unsupported,
}

impl VulkanLayerStatus {
    /// Whether a Vulkan install can reuse an existing layer (no new install, so no
    /// fresh consent is needed).
    #[must_use]
    pub fn is_present(self) -> bool {
        matches!(
            self,
            VulkanLayerStatus::Foreign | VulkanLayerStatus::Managed
        )
    }
}

/// The current global ReShade Vulkan layer state.
#[cfg(windows)]
#[must_use]
pub fn layer_status() -> VulkanLayerStatus {
    use renderpilot_platform_windows::vulkan_layer::{
        self, VulkanLayerState, WindowsLayerRegistry,
    };

    let Some(dir) = vulkan_layer::default_layer_dir() else {
        return VulkanLayerStatus::Absent;
    };
    match vulkan_layer::detect(&WindowsLayerRegistry, &dir) {
        VulkanLayerState::Absent => VulkanLayerStatus::Absent,
        VulkanLayerState::Foreign => VulkanLayerStatus::Foreign,
        VulkanLayerState::Managed => VulkanLayerStatus::Managed,
    }
}

/// The current global ReShade Vulkan layer state (always `Unsupported` off Windows).
#[cfg(not(windows))]
#[must_use]
pub fn layer_status() -> VulkanLayerStatus {
    VulkanLayerStatus::Unsupported
}

/// Installs RenderPilot's global ReShade Vulkan layer from the given host DLL bytes.
/// The caller installs only when [`layer_status`] is [`VulkanLayerStatus::Absent`]
/// and the user has consented.
///
/// # Errors
/// Fails if there is no local data directory, the platform write fails, or the host
/// is not Windows.
#[cfg(windows)]
pub fn install_layer(dll_bytes: &[u8], reshade_version: Option<&str>) -> Result<(), ServiceError> {
    use renderpilot_platform_windows::vulkan_layer::{self, WindowsLayerRegistry};

    let dir = vulkan_layer::default_layer_dir().ok_or_else(|| {
        errors::failed("no local data directory to install the Vulkan layer into".to_owned())
    })?;
    vulkan_layer::install(&WindowsLayerRegistry, &dir, dll_bytes, reshade_version).map_err(
        |error| {
            errors::failed(format!(
                "failed to install the global Vulkan layer: {error}"
            ))
        },
    )
}

/// Non-Windows stub: installing the Vulkan layer is unsupported.
///
/// # Errors
/// Always errors — RenoDX for Vulkan is a Windows feature.
#[cfg(not(windows))]
pub fn install_layer(
    _dll_bytes: &[u8],
    _reshade_version: Option<&str>,
) -> Result<(), ServiceError> {
    Err(errors::invalid(
        "RenoDX for Vulkan games is only supported on Windows".to_owned(),
    ))
}

/// Removes RenderPilot's global ReShade Vulkan layer. A foreign layer is never
/// touched. Safe to call when nothing is installed.
///
/// # Errors
/// Fails if the platform removal fails or the host is not Windows.
#[cfg(windows)]
pub fn remove_layer() -> Result<(), ServiceError> {
    use renderpilot_platform_windows::vulkan_layer::{self, WindowsLayerRegistry};

    let Some(dir) = vulkan_layer::default_layer_dir() else {
        return Ok(());
    };
    vulkan_layer::uninstall(&WindowsLayerRegistry, &dir).map_err(|error| {
        errors::failed(format!("failed to remove the global Vulkan layer: {error}"))
    })
}

/// Non-Windows stub: removing the Vulkan layer is unsupported.
///
/// # Errors
/// Always errors — RenoDX for Vulkan is a Windows feature.
#[cfg(not(windows))]
pub fn remove_layer() -> Result<(), ServiceError> {
    Err(errors::invalid(
        "RenoDX for Vulkan games is only supported on Windows".to_owned(),
    ))
}
