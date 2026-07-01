/// ReShadeApps.ini management.
use crate::ServiceError;
use crate::addons::renodx::errors;

/// Registers a game executable in `ReShadeApps.ini` so the shared layer knows
/// which apps are using it.
///
/// # Errors
/// Fails on filesystem errors or non-Windows.
#[cfg(windows)]
pub fn register_app(exe_path: &std::path::Path) -> Result<(), ServiceError> {
    use renderpilot_platform_windows::vulkan_layer;

    let dir = vulkan_layer::reshade_common_dir()
        .ok_or_else(|| errors::failed("no ProgramData directory".to_owned()))?;
    vulkan_layer::register_app(&dir, exe_path)
        .map_err(|error| errors::failed(format!("failed to register Vulkan layer app: {error}")))
}

/// Non-Windows stub.
#[cfg(not(windows))]
pub fn register_app(_exe_path: &std::path::Path) -> Result<(), ServiceError> {
    Err(errors::vulkan_unsupported_platform())
}

/// Unregisters a game executable from `ReShadeApps.ini`. If no apps remain,
/// removes the shared layer entirely (deletes directory + unregisters from
/// HKLM).
///
/// # Errors
/// Fails on filesystem/registry errors or non-Windows.
#[cfg(windows)]
pub fn unregister_app(exe_path: &std::path::Path) -> Result<bool, ServiceError> {
    use renderpilot_platform_windows::vulkan_layer::{self, WindowsLayerRegistry};

    let dir = vulkan_layer::reshade_common_dir()
        .ok_or_else(|| errors::failed("no ProgramData directory".to_owned()))?;
    let is_empty = vulkan_layer::unregister_app(&dir, exe_path).map_err(|error| {
        errors::failed(format!("failed to unregister Vulkan layer app: {error}"))
    })?;
    if is_empty {
        vulkan_layer::uninstall(&WindowsLayerRegistry, &dir).map_err(|error| {
            errors::failed(format!("failed to remove the shared Vulkan layer: {error}"))
        })?;
    }
    Ok(is_empty)
}

/// Non-Windows stub.
#[cfg(not(windows))]
pub fn unregister_app(_exe_path: &std::path::Path) -> Result<bool, ServiceError> {
    Err(errors::vulkan_unsupported_platform())
}
