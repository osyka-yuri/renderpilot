use std::fmt;
use std::io;
use std::path::Path;

use super::manifest::layer_manifest_json;
use super::registry::LayerRegistry;
use super::{LAYER_DLL_NAME, LAYER_JSON_NAME};

/// Structured install error distinguishing registry-scope, permission, and
/// generic IO failures so the caller can produce a precise user message
/// instead of collapsing them into a single "not installed".
#[derive(Debug)]
pub enum LayerInstallError {
    /// The HKLM registry scope cannot be written (process not elevated).
    RegistryScopeNotWritable,
    /// The OS denied access to a file or directory.
    PermissionDenied,
    /// A generic filesystem or registry IO error.
    Io(io::Error),
}

impl fmt::Display for LayerInstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RegistryScopeNotWritable => {
                write!(
                    f,
                    "the HKLM registry scope is not writable (process not elevated)"
                )
            }
            Self::PermissionDenied => write!(f, "access denied"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for LayerInstallError {}

impl From<io::Error> for LayerInstallError {
    fn from(error: io::Error) -> Self {
        if error.kind() == io::ErrorKind::PermissionDenied || error.raw_os_error() == Some(5) {
            Self::PermissionDenied
        } else {
            Self::Io(error)
        }
    }
}

/// Installs the shared ReShade Vulkan layer into `layer_dir`: writes the host
/// DLL and the generated manifest, then registers the manifest with the Vulkan
/// loader via HKLM. Requires elevation.
///
/// Matches the official ReShade installer — same file names, same manifest
/// format, same registry location. The caller is responsible for elevation
/// and for only invoking this when no compatible layer is already present.
///
/// # Errors
/// Returns a structured [`LayerInstallError`] distinguishing registry-scope,
/// permission, and generic IO failures.
pub fn install(
    registry: &impl LayerRegistry,
    layer_dir: &Path,
    dll_bytes: &[u8],
) -> Result<(), LayerInstallError> {
    std::fs::create_dir_all(layer_dir).map_err(LayerInstallError::from)?;

    let dll_path = layer_dir.join(LAYER_DLL_NAME);
    std::fs::write(&dll_path, dll_bytes).map_err(LayerInstallError::from)?;

    let manifest_path = layer_dir.join(LAYER_JSON_NAME);
    std::fs::write(&manifest_path, layer_manifest_json()).map_err(LayerInstallError::from)?;

    registry.register(&manifest_path).map_err(|error| {
        if error.kind() == io::ErrorKind::PermissionDenied || error.raw_os_error() == Some(5) {
            LayerInstallError::RegistryScopeNotWritable
        } else {
            LayerInstallError::Io(error)
        }
    })
}

/// Removes the shared ReShade Vulkan layer: unregisters the manifest and
/// deletes the layer directory. Safe to call when nothing is installed.
///
/// Also removes any `ReShadeApps.ini` since the whole directory is deleted.
///
/// # Errors
/// Propagates registry and filesystem errors (a missing directory is not one).
pub fn uninstall(registry: &impl LayerRegistry, layer_dir: &Path) -> io::Result<()> {
    registry.unregister(&layer_dir.join(LAYER_JSON_NAME))?;
    match std::fs::remove_dir_all(layer_dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

// -----------------------------------------------------------------------------
// App tracking (ReShadeApps.ini)
// -----------------------------------------------------------------------------
