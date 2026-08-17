//! Projects catalogued [`LibraryComponent`]s into the per-family DLSS DLL map
//! the NVAPI layer consumes.
//!
//! This is the single place that turns the catalog's view of installed DLSS DLLs
//! into [`SettingContext`](renderpilot_nvapi::setting::SettingContext)`::dlls`.
//! The NVAPI layer no longer walks the filesystem itself: the global catalog
//! (`renderpilot-detection`) already discovered every `nvngx_dlss*.dll`, read its
//! PE version, and persisted it, so we read that instead of duplicating the scan.

use std::collections::HashMap;
use std::path::PathBuf;

use renderpilot_domain::{LibraryComponent, LibraryTechnology, Version, normalized_path_key};
use renderpilot_nvapi::setting::DllInfo;
use renderpilot_nvapi::{DlssDllKind, DlssVersion};

/// Maps a catalog technology to its NVAPI DLL family, when it is a DLSS DLL.
///
/// DLSS technologies are each their own `family()`, so every `nvngx_dlss*.dll`
/// is its own single-file component and maps 1:1 onto a [`DlssDllKind`].
fn dlss_dll_kind_for_technology(technology: LibraryTechnology) -> Option<DlssDllKind> {
    match technology {
        LibraryTechnology::DlssSuperResolution => Some(DlssDllKind::Sr),
        LibraryTechnology::DlssFrameGeneration => Some(DlssDllKind::FrameGen),
        LibraryTechnology::DlssRayReconstruction => Some(DlssDllKind::RayReconstruction),
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

fn strip_root(file_path: &str, root: &str) -> Option<String> {
    file_path
        .strip_prefix(root)
        .and_then(|path| path.strip_prefix('/'))
        .map(|path| path.to_owned())
}

fn relative_install_path(file_path: &str, install_root: &std::path::Path) -> Option<String> {
    let normalized_file = renderpilot_domain::normalized_path_key(file_path);
    let canonical_root = crate::paths::canonicalize_best_effort(install_root);
    let canonical_root = crate::paths::normalized_key(&canonical_root);
    let raw_root = crate::paths::normalized_key(install_root);

    strip_root(&normalized_file, canonical_root.trim_end_matches('/'))
        .or_else(|| strip_root(&normalized_file, raw_root.trim_end_matches('/')))
}

/// Builds the per-family DLL map from catalogued components.
///
/// Only files inside `install_root` participate. For each DLSS family, a known
/// version always wins over an unknown version; within that partition, the
/// shallowest normalized relative path wins, with the normalized path as a
/// deterministic tie-breaker. A family with only unknown versions is present
/// with `DllInfo::version == None`, not absent.
pub fn installed_dlls_from_components(
    components: &[LibraryComponent],
    install_root: &std::path::Path,
) -> HashMap<DlssDllKind, DllInfo> {
    let mut best: HashMap<DlssDllKind, (bool, usize, String, DllInfo)> = HashMap::new();

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

            let Some(relative_path) = relative_install_path(file.path().as_str(), install_root)
            else {
                continue;
            };

            let normalized_path = normalized_path_key(file.path().as_str());
            let depth = relative_path.bytes().filter(|&byte| byte == b'/').count();
            let info = DllInfo {
                path: PathBuf::from(file.path().as_str()),
                version: file.version().map(dlss_version_from_domain),
            };

            // `false` sorts before `true`, so versioned candidates are always
            // considered before unknown-version candidates.
            let is_unknown = info.version.is_none();
            let replace = match best.get(&kind) {
                Some((unknown, existing_depth, existing_path, _)) => {
                    (is_unknown, depth, normalized_path.as_str())
                        < (*unknown, *existing_depth, existing_path.as_str())
                }
                None => true,
            };
            if replace {
                best.insert(kind, (is_unknown, depth, normalized_path, info));
            }
        }
    }

    best.into_iter()
        .map(|(kind, (_, _, _, info))| (kind, info))
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
        technology: LibraryTechnology,
        path: &str,
        version: Option<&str>,
    ) -> LibraryComponent {
        let mut file = ComponentFile::new(PathRef::new(path).expect("valid path"));
        if let Some(version) = version {
            file = file.with_version(Version::parse(version).expect("valid version"));
        }

        LibraryComponent::new(
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
                LibraryTechnology::DlssSuperResolution,
                "C:/Games/G/nvngx_dlss.dll",
                Some("3.7.20.0"),
            ),
            dlss_component(
                "fg",
                LibraryTechnology::DlssFrameGeneration,
                "C:/Games/G/nvngx_dlssg.dll",
                Some("3.8.0.0"),
            ),
            dlss_component(
                "rr",
                LibraryTechnology::DlssRayReconstruction,
                "C:/Games/G/nvngx_dlssd.dll",
                Some("3.5.0.0"),
            ),
        ];

        let dlls = installed_dlls_from_components(&components, std::path::Path::new("C:/Games/G"));

        assert_eq!(dlls.len(), 3);
        assert_eq!(
            dlls[&DlssDllKind::Sr].version,
            Some(DlssVersion::new(3, 7, 20, 0))
        );
        assert_eq!(
            dlls[&DlssDllKind::Sr].path,
            PathBuf::from("C:/Games/G/nvngx_dlss.dll")
        );
        assert_eq!(
            dlls[&DlssDllKind::FrameGen].version,
            Some(DlssVersion::new(3, 8, 0, 0))
        );
        assert_eq!(
            dlls[&DlssDllKind::RayReconstruction].version,
            Some(DlssVersion::new(3, 5, 0, 0))
        );
    }

    #[test]
    fn non_dlss_technologies_are_ignored() {
        let components = [dlss_component(
            "sl",
            LibraryTechnology::NvidiaStreamline,
            "C:/Games/G/sl.interposer.dll",
            Some("2.0.0.0"),
        )];

        assert!(
            installed_dlls_from_components(&components, std::path::Path::new("C:/Games/G"))
                .is_empty()
        );
    }

    #[test]
    fn shallowest_copy_wins_for_a_family() {
        let components = [
            dlss_component(
                "deep",
                LibraryTechnology::DlssSuperResolution,
                "C:/Games/G/Engine/Binaries/ThirdParty/NVIDIA/nvngx_dlss.dll",
                Some("3.1.0.0"),
            ),
            dlss_component(
                "shallow",
                LibraryTechnology::DlssSuperResolution,
                "C:/Games/G/nvngx_dlss.dll",
                Some("3.7.20.0"),
            ),
        ];

        let dlls = installed_dlls_from_components(&components, std::path::Path::new("C:/Games/G"));

        assert_eq!(dlls.len(), 1);
        assert_eq!(
            dlls[&DlssDllKind::Sr].path,
            PathBuf::from("C:/Games/G/nvngx_dlss.dll")
        );
        assert_eq!(
            dlls[&DlssDllKind::Sr].version,
            Some(DlssVersion::new(3, 7, 20, 0))
        );
    }

    #[test]
    fn versioned_copy_wins_over_a_shallower_unknown_copy() {
        let components = [
            // Shallowest copy has no version: it must not shadow a deeper, versioned one.
            dlss_component(
                "shallow_no_version",
                LibraryTechnology::DlssSuperResolution,
                "C:/Games/G/nvngx_dlss.dll",
                None,
            ),
            dlss_component(
                "deep_versioned",
                LibraryTechnology::DlssSuperResolution,
                "C:/Games/G/bin/nvngx_dlss.dll",
                Some("3.7.20.0"),
            ),
        ];

        let dlls = installed_dlls_from_components(&components, std::path::Path::new("C:/Games/G"));

        assert_eq!(dlls.len(), 1);
        assert_eq!(
            dlls[&DlssDllKind::Sr].path,
            PathBuf::from("C:/Games/G/bin/nvngx_dlss.dll")
        );
    }

    #[test]
    fn family_with_no_versioned_copy_is_present_with_unknown_version() {
        let components = [dlss_component(
            "sr_no_version",
            LibraryTechnology::DlssSuperResolution,
            "C:/Games/G/nvngx_dlss.dll",
            None,
        )];

        let dlls = installed_dlls_from_components(&components, std::path::Path::new("C:/Games/G"));
        assert_eq!(dlls[&DlssDllKind::Sr].version, None);
    }

    #[test]
    fn files_outside_the_normalized_install_root_are_ignored() {
        let components = [dlss_component(
            "outside",
            LibraryTechnology::DlssSuperResolution,
            "C:/Games/Other/nvngx_dlss.dll",
            Some("3.7.20.0"),
        )];

        assert!(
            installed_dlls_from_components(&components, std::path::Path::new("C:/Games/G"))
                .is_empty()
        );
    }

    #[test]
    fn normalized_path_breaks_same_depth_ties_deterministically() {
        let components = [
            dlss_component(
                "z",
                LibraryTechnology::DlssSuperResolution,
                "C:/Games/G/Z/nvngx_dlss.dll",
                Some("3.7.20.0"),
            ),
            dlss_component(
                "a",
                LibraryTechnology::DlssSuperResolution,
                "C:/Games/G/A/nvngx_dlss.dll",
                Some("3.7.20.0"),
            ),
        ];

        let dlls = installed_dlls_from_components(&components, std::path::Path::new("C:/Games/G"));
        assert_eq!(
            dlls[&DlssDllKind::Sr].path,
            PathBuf::from("C:/Games/G/A/nvngx_dlss.dll")
        );
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

    #[test]
    fn matches_canonical_file_path_against_uncanonicalized_root_alias() {
        let temp = tempfile::tempdir().expect("temp dir");
        let canonical_root = crate::paths::canonicalize_best_effort(temp.path());
        let canonical_dll = canonical_root.join("nvngx_dlss.dll");
        let components = [dlss_component(
            "sr",
            LibraryTechnology::DlssSuperResolution,
            &canonical_dll.to_string_lossy().replace('\\', "/"),
            Some("3.7.20.0"),
        )];

        let dlls = installed_dlls_from_components(&components, temp.path());
        assert_eq!(dlls.len(), 1);
        assert_eq!(
            dlls[&DlssDllKind::Sr].version,
            Some(DlssVersion::new(3, 7, 20, 0))
        );
        assert_eq!(
            dlls[&DlssDllKind::Sr].path,
            PathBuf::from(canonical_dll.to_string_lossy().replace('\\', "/"))
        );
    }
}
