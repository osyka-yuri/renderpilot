//! Projects catalogued [`GraphicsComponent`]s into the per-family DLSS DLL map
//! the NVAPI layer consumes.
//!
//! This is the single place that turns the catalog's view of installed DLSS DLLs
//! into [`SettingContext`](renderpilot_nvapi::setting::SettingContext)`::dlls`.
//! The NVAPI layer no longer walks the filesystem itself: the global catalog
//! (`renderpilot-detection`) already discovered every `nvngx_dlss*.dll`, read its
//! PE version, and persisted it, so we read that instead of duplicating the scan.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::PathBuf;

use renderpilot_domain::{GraphicsComponent, GraphicsTechnology, Version};
use renderpilot_nvapi::setting::DllInfo;
use renderpilot_nvapi::{DlssDllKind, DlssVersion};

/// Maps a catalog technology to its NVAPI DLL family, when it is a DLSS DLL.
///
/// DLSS technologies are each their own `family()`, so every `nvngx_dlss*.dll`
/// is its own single-file component and maps 1:1 onto a [`DlssDllKind`].
fn dlss_dll_kind_for_technology(technology: GraphicsTechnology) -> Option<DlssDllKind> {
    match technology {
        GraphicsTechnology::DlssSuperResolution => Some(DlssDllKind::Sr),
        GraphicsTechnology::DlssFrameGeneration => Some(DlssDllKind::FrameGen),
        GraphicsTechnology::DlssRayReconstruction => Some(DlssDllKind::RayReconstruction),
        _ => None,
    }
}

/// Adapts a domain [`Version`] (variable segment count) to the four-part
/// [`DlssVersion`] used by the preset manifests.
///
/// Missing trailing components default to `0` and any extras are dropped. This
/// is correct for manifest matching, which compares `entry <= version`
/// component-wise (see [`crate::dlss::preset_manifest::resolve_entry`]).
fn dlss_version_from_domain(version: &Version) -> DlssVersion {
    let segments = version.segments();
    let part =
        |index: usize| u32::try_from(segments.get(index).copied().unwrap_or(0)).unwrap_or(u32::MAX);

    DlssVersion::new(part(0), part(1), part(2), part(3))
}

/// Builds the per-family DLL map from catalogued components.
///
/// For each DLSS family it keeps the shallowest catalogued copy that has a known
/// version, mirroring the previous filesystem walk's "sort by `(depth, path)`,
/// first readable version wins" rule. A copy whose version was never resolved is
/// skipped; a family with no usable copy is simply absent from the map (treated
/// downstream as "DLL not present", imposing no preset constraints).
pub fn installed_dlls_from_components(
    components: &[GraphicsComponent],
) -> HashMap<DlssDllKind, DllInfo> {
    let mut best: HashMap<DlssDllKind, (usize, DllInfo)> = HashMap::new();

    for component in components {
        let Some(kind) = dlss_dll_kind_for_technology(component.technology()) else {
            continue;
        };

        for file in component.files() {
            let matches_kind = file
                .path()
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case(kind.file_name()));
            if !matches_kind {
                continue;
            }

            let Some(version) = file.version() else {
                continue;
            };

            let path = file.path().as_str();
            // Catalog paths are forward-slash normalized, so the separator count
            // orders identically to the old install-relative recursion depth.
            let depth = path.bytes().filter(|&byte| byte == b'/').count();
            let info = DllInfo {
                path: PathBuf::from(path),
                version: dlss_version_from_domain(version),
            };

            match best.entry(kind) {
                Entry::Occupied(mut existing) => {
                    let (existing_depth, existing_info) = existing.get();
                    if (depth, &info.path) < (*existing_depth, &existing_info.path) {
                        existing.insert((depth, info));
                    }
                }
                Entry::Vacant(slot) => {
                    slot.insert((depth, info));
                }
            }
        }
    }

    best.into_iter()
        .map(|(kind, (_depth, info))| (kind, info))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use renderpilot_domain::{
        ComponentFile, ComponentId, ComponentKind, GameId, PathRef, Swappability,
    };

    fn game_id() -> GameId {
        GameId::new("game:test").expect("valid game id")
    }

    fn dlss_component(
        suffix: &str,
        technology: GraphicsTechnology,
        path: &str,
        version: Option<&str>,
    ) -> GraphicsComponent {
        let mut file = ComponentFile::new(PathRef::new(path).expect("valid path"));
        if let Some(version) = version {
            file = file.with_version(Version::parse(version).expect("valid version"));
        }

        GraphicsComponent::new(
            ComponentId::new(format!("component:test:{suffix}")).expect("valid component id"),
            game_id(),
            ComponentKind::NativeLibrary,
            technology,
            Swappability::Swappable,
        )
        .with_file(file)
    }

    #[test]
    fn maps_each_dlss_family_to_its_kind() {
        let components = [
            dlss_component(
                "sr",
                GraphicsTechnology::DlssSuperResolution,
                "C:/Games/G/nvngx_dlss.dll",
                Some("3.7.20.0"),
            ),
            dlss_component(
                "fg",
                GraphicsTechnology::DlssFrameGeneration,
                "C:/Games/G/nvngx_dlssg.dll",
                Some("3.8.0.0"),
            ),
            dlss_component(
                "rr",
                GraphicsTechnology::DlssRayReconstruction,
                "C:/Games/G/nvngx_dlssd.dll",
                Some("3.5.0.0"),
            ),
        ];

        let dlls = installed_dlls_from_components(&components);

        assert_eq!(dlls.len(), 3);
        assert_eq!(
            dlls[&DlssDllKind::Sr].version,
            DlssVersion::new(3, 7, 20, 0)
        );
        assert_eq!(
            dlls[&DlssDllKind::Sr].path,
            PathBuf::from("C:/Games/G/nvngx_dlss.dll")
        );
        assert_eq!(
            dlls[&DlssDllKind::FrameGen].version,
            DlssVersion::new(3, 8, 0, 0)
        );
        assert_eq!(
            dlls[&DlssDllKind::RayReconstruction].version,
            DlssVersion::new(3, 5, 0, 0)
        );
    }

    #[test]
    fn non_dlss_technologies_are_ignored() {
        let components = [dlss_component(
            "sl",
            GraphicsTechnology::NvidiaStreamline,
            "C:/Games/G/sl.interposer.dll",
            Some("2.0.0.0"),
        )];

        assert!(installed_dlls_from_components(&components).is_empty());
    }

    #[test]
    fn shallowest_copy_wins_for_a_family() {
        let components = [
            dlss_component(
                "deep",
                GraphicsTechnology::DlssSuperResolution,
                "C:/Games/G/Engine/Binaries/ThirdParty/NVIDIA/nvngx_dlss.dll",
                Some("3.1.0.0"),
            ),
            dlss_component(
                "shallow",
                GraphicsTechnology::DlssSuperResolution,
                "C:/Games/G/nvngx_dlss.dll",
                Some("3.7.20.0"),
            ),
        ];

        let dlls = installed_dlls_from_components(&components);

        assert_eq!(dlls.len(), 1);
        assert_eq!(
            dlls[&DlssDllKind::Sr].path,
            PathBuf::from("C:/Games/G/nvngx_dlss.dll")
        );
        assert_eq!(
            dlls[&DlssDllKind::Sr].version,
            DlssVersion::new(3, 7, 20, 0)
        );
    }

    #[test]
    fn copies_without_a_version_are_skipped() {
        let components = [
            // Shallowest copy has no version: it must not shadow a deeper, versioned one.
            dlss_component(
                "shallow_no_version",
                GraphicsTechnology::DlssSuperResolution,
                "C:/Games/G/nvngx_dlss.dll",
                None,
            ),
            dlss_component(
                "deep_versioned",
                GraphicsTechnology::DlssSuperResolution,
                "C:/Games/G/bin/nvngx_dlss.dll",
                Some("3.7.20.0"),
            ),
        ];

        let dlls = installed_dlls_from_components(&components);

        assert_eq!(dlls.len(), 1);
        assert_eq!(
            dlls[&DlssDllKind::Sr].path,
            PathBuf::from("C:/Games/G/bin/nvngx_dlss.dll")
        );
    }

    #[test]
    fn family_with_no_versioned_copy_is_absent() {
        let components = [dlss_component(
            "sr_no_version",
            GraphicsTechnology::DlssSuperResolution,
            "C:/Games/G/nvngx_dlss.dll",
            None,
        )];

        assert!(installed_dlls_from_components(&components).is_empty());
    }

    #[test]
    fn version_conversion_pads_and_truncates() {
        assert_eq!(
            dlss_version_from_domain(&Version::parse("3.7.20").unwrap()),
            DlssVersion::new(3, 7, 20, 0)
        );
        assert_eq!(
            dlss_version_from_domain(&Version::parse("310.1.0.0").unwrap()),
            DlssVersion::new(310, 1, 0, 0)
        );
        assert_eq!(
            dlss_version_from_domain(&Version::parse("3.7.20.0.99").unwrap()),
            DlssVersion::new(3, 7, 20, 0)
        );
    }
}
