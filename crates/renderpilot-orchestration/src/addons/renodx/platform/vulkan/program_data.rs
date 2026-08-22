/// ProgramData installation paths.
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
