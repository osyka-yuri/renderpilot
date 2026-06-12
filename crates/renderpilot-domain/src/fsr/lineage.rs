//! Component-level lineage predicates for AMD FSR split sets.
//!
//! These functions examine the complete set of files in a component to
//! determine whether it is a split set, whether it has an FSR 3.1 entry point
//! (assigning its lineage), and whether its upscaler represents the set.

use crate::{ComponentFile, Version};

use super::naming;

/// Whether a component's files form a split FSR set (they contain the upscaler).
#[must_use]
pub fn is_split_set(files: &[ComponentFile]) -> bool {
    files
        .iter()
        .any(|file| file.path().file_name().is_some_and(naming::is_split_marker))
}

/// Whether a component still loads an FSR 3.1 entry point.
///
/// This is the ironclad marker of the **FSR 3.1 lineage**: such a game can always
/// return to a unified FSR 3.x backend, and one we upgraded keeps the FSR 4 loader
/// installed under this very name. Contrast [`is_native_fsr4`].
#[must_use]
pub fn has_entry_point(files: &[ComponentFile]) -> bool {
    files
        .iter()
        .any(|file| file.path().file_name().is_some_and(naming::is_entry_point))
}

/// Whether a component is a **native** FSR 4 set: it loads its own
/// `amd_fidelityfx_loader_` and has no entry point,
/// so there is no FSR 3 to return to (a unified downgrade must not be offered).
#[must_use]
pub fn is_native_fsr4(files: &[ComponentFile]) -> bool {
    let has_loader = files
        .iter()
        .any(|file| file.path().file_name().is_some_and(naming::is_loader));
    has_loader && !has_entry_point(files)
}

/// Whether two PE versions belong to one FSR release: they share the **build**,
/// the last version segment. Within one FidelityFX release every DLL carries
/// the same build (loader `2.1.0.604` ↔ upscaler `4.0.3.604` ↔ denoiser
/// `1.0.0.604`), while a unified FSR 3.1 backend's build (`1.0.1.41314`) never
/// matches a split set's. Mirrors the manifest-side release grouping by the
/// last `sort_key` segment.
#[must_use]
pub fn same_release_build(left: &Version, right: &Version) -> bool {
    match (left.segments().last(), right.segments().last()) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

/// Whether the upscaler is the set's **version representative**.
///
/// In a cohesive split set the entry point is the loader (a `1.x`/`2.x` PE
/// version that would mislead users), so the upscaler carries the FSR version
/// (e.g. `4.0.3`). But an upscaler can also sit next to a **real** unified FSR
/// 3.1 entry point as a developer leftover — then the entry point is what the
/// game loads and must represent the set.
///
/// The arbiter is release-build cohesion: the upscaler represents the set iff
/// one exists and, for every entry point in the set, a same-API upscaler with a
/// matching release build (see [`same_release_build`]) is present. Unknown
/// versions fail the match — when in doubt, the file the game loads wins.
#[must_use]
pub fn upscaler_represents_set<'a>(
    files: impl IntoIterator<Item = (&'a str, Option<&'a Version>)>,
) -> bool {
    let mut entry_points: Vec<(Option<naming::FsrApi>, Option<&Version>)> = Vec::new();
    let mut upscalers: Vec<(Option<naming::FsrApi>, Option<&Version>)> = Vec::new();

    for (name, version) in files {
        if naming::is_entry_point(name) {
            entry_points.push((naming::fsr_graphics_api(name), version));
        } else if naming::is_split_marker(name) {
            upscalers.push((naming::fsr_graphics_api(name), version));
        }
    }

    if upscalers.is_empty() {
        return false;
    }

    entry_points.iter().all(|(entry_api, entry_version)| {
        upscalers.iter().any(|(upscaler_api, upscaler_version)| {
            entry_api == upscaler_api
                && match (entry_version, upscaler_version) {
                    (Some(entry), Some(upscaler)) => same_release_build(entry, upscaler),
                    _ => false,
                }
        })
    })
}
