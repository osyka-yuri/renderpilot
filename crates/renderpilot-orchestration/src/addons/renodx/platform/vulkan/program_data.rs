/// ProgramData installation paths.
use crate::ServiceError;
use crate::addons::renodx::errors;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

/// Returns the `ProgramData` directory where the ReShade Vulkan global layer
/// resides, if available on this platform.
pub fn layer_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        renderpilot_platform_windows::vulkan_layer::reshade_common_dir()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

pub(crate) fn standard_paths() -> Option<(PathBuf, PathBuf, PathBuf)> {
    let dir = layer_dir()?;
    let manifest = dir.join("ReShade64.json");
    let dll = dir.join("ReShade64.dll");
    Some((dir, manifest, dll))
}

/// Installs the ReShade Vulkan layer DLL.
///
/// # Errors
/// Fails if the platform installation fails or the host is not Windows.
#[cfg(windows)]
pub fn install_layer(dll_bytes: &[u8]) -> Result<(), ServiceError> {
    use renderpilot_platform_windows::vulkan_layer::{
        self, LayerInstallError, WindowsLayerRegistry,
    };

    let dir = vulkan_layer::reshade_common_dir()
        .ok_or_else(|| errors::failed("no ProgramData directory".to_owned()))?;
    vulkan_layer::install(&WindowsLayerRegistry, &dir, dll_bytes).map_err(|error| {
        let msg = match error {
            LayerInstallError::PermissionDenied => {
                "the ReShade shared Vulkan layer is not writable - run RenderPilot as administrator to install the Vulkan layer".to_owned()
            }
            LayerInstallError::RegistryScopeNotWritable => {
                "the loader registry is not writable - run RenderPilot as administrator to install the Vulkan layer".to_owned()
            }
            LayerInstallError::Io(error) => {
                format!("failed to install the shared Vulkan layer: {error}")
            }
        };
        errors::failed(msg)
    })
}

/// Non-Windows stub.
#[cfg(not(windows))]
pub fn install_layer(_dll_bytes: &[u8]) -> Result<(), ServiceError> {
    Err(errors::vulkan_unsupported_platform())
}

/// Removes the shared ReShade Vulkan layer unconditionally (regardless of
/// `ReShadeApps.ini` state). A user maintenance action.
///
/// # Errors
/// Fails if the platform removal fails or the host is not Windows.
#[cfg(windows)]
pub fn remove_layer() -> Result<(), ServiceError> {
    use renderpilot_platform_windows::vulkan_layer::{self, WindowsLayerRegistry};

    let Some(dir) = vulkan_layer::reshade_common_dir() else {
        return Ok(());
    };
    vulkan_layer::uninstall(&WindowsLayerRegistry, &dir).map_err(|error| {
        errors::failed(format!("failed to remove the shared Vulkan layer: {error}"))
    })
}

/// Non-Windows stub.
#[cfg(not(windows))]
pub fn remove_layer() -> Result<(), ServiceError> {
    Err(errors::vulkan_unsupported_platform())
}

pub(crate) fn current_layer_digest() -> Option<String> {
    let (_, _, dll_path) = standard_paths()?;
    let bytes = std::fs::read(dll_path).ok()?;
    Some(sha256_hex(&bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}
