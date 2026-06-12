//! Install-target resolution for artifact files.
//!
//! Determines the basename under which an artifact member should be written
//! to the game directory, taking into account the component's lineage and
//! the artifact's `install_as` metadata.

use crate::ComponentFile;

use super::{lineage, naming};

/// Resolves the basename an artifact member should install under for the given
/// component: the artifact file's `install_as` when present (else its own file
/// name), with one lineage-aware adjustment for the FSR loader. Shared by both
/// plan building and apply execution so they cannot drift.
///
/// A package's loader targets the FSR 3.1 entry point by default — what an FSR
/// 3.1 game (or one we already upgraded) loads. That stands whenever the
/// component still has an entry point, **even when a loader file also exists
/// under its own name**: such a file belongs to a separate effect stack (e.g.
/// loader + denoiser for Ray Regeneration) that pairs with its own loader and
/// must never be overwritten by an upscaling
/// swap. Only a natively split game (no entry point) loads the loader under its
/// own `amd_fidelityfx_loader_*` name, so only then is the install retargeted
/// to overwrite that file in place.
#[must_use]
pub fn resolve_artifact_install_target(
    artifact_file: &ComponentFile,
    component_files: &[ComponentFile],
) -> String {
    let default_target = artifact_file
        .install_as()
        .or_else(|| artifact_file.path().file_name())
        .unwrap_or("");

    if naming::is_entry_point(default_target) && !lineage::has_entry_point(component_files) {
        if let Some(native_loader) = component_files
            .iter()
            .filter_map(|file| file.path().file_name())
            .find(|name| naming::is_loader(name))
        {
            return native_loader.to_owned();
        }
    }

    default_target.to_owned()
}
