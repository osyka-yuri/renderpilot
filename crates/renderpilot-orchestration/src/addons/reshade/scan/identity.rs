//! Identifies ReShade (and recognized non-ReShade builds sharing its proxy-DLL
//! naming convention) from a candidate DLL's PE version-resource strings.

use std::fs;
use std::path::Path;

use renderpilot_detection::VersionIdentityStrings;

use crate::addons::reshade::types::ReshadeChannel;

const RESHADE_ENGINE_DLLS: &[&str] = &["reshade64.dll", "reshade32.dll"];
const PROXY_DLL_SLOTS: &[&str] = &[
    "dxgi.dll",
    "d3d9.dll",
    "d3d10.dll",
    "d3d10_1.dll",
    "d3d11.dll",
    "d3d12.dll",
    "opengl32.dll",
];

/// Whether `name` is one of the proxy-DLL slots a ReShade host can occupy. The
/// single source of truth reused by the install/update record helpers.
#[must_use]
pub(crate) fn is_proxy_slot(name: &str) -> bool {
    PROXY_DLL_SLOTS
        .iter()
        .any(|slot| name.eq_ignore_ascii_case(slot))
}

pub(super) fn is_reshade_engine_dll(name: &str) -> bool {
    RESHADE_ENGINE_DLLS
        .iter()
        .any(|slot| name.eq_ignore_ascii_case(slot))
}

pub(super) fn version_strings_point_to_reshade(strings: &VersionIdentityStrings) -> bool {
    let values = [
        strings.product_name.as_deref(),
        strings.file_description.as_deref(),
        strings.original_filename.as_deref(),
        strings.company_name.as_deref(),
    ];
    values.into_iter().flatten().any(|value| {
        let value = value.to_ascii_lowercase();
        value.contains("reshade") || value.contains("crosire")
    })
}

/// Runtime DLL names that reliably identify a specific non-ReShade injector
/// framework sharing ReShade's proxy-DLL naming convention — so the proxy
/// stub's own PE identity can't be trusted to tell them apart, but the presence
/// of the framework's real runtime binary next to it can. GShade (a maintained
/// ReShade fork) documents this exact aliasing in its changelog: it hooks the
/// same slots ReShade does (`dxgi.dll`, `d3d11.dll`, …) while its actual runtime
/// is always named `GShade64.dll`/`GShade32.dll` regardless of which slot.
const KNOWN_CUSTOM_RUNTIME_DLLS: &[&str] = &["gshade64.dll", "gshade32.dll"];

/// Whether `game_dir` shows the on-disk signature of a recognized non-ReShade
/// build (currently: GShade) that a tool must never silently replace or check
/// upstream for updates against — its versioning and update cadence are its own
/// maintainer's concern. `host_identity`, when available, is an independent
/// secondary signal: the proxy DLL's own PE version-resource strings mentioning
/// the framework by name.
#[must_use]
pub fn is_known_custom_build(
    game_dir: &Path,
    host_identity: Option<&VersionIdentityStrings>,
) -> bool {
    let has_custom_runtime = fs::read_dir(game_dir).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_file())
                && KNOWN_CUSTOM_RUNTIME_DLLS.contains(
                    &entry
                        .file_name()
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .as_str(),
                )
        })
    });
    has_custom_runtime || host_identity.is_some_and(identity_mentions_a_known_custom_build)
}

fn identity_mentions_a_known_custom_build(identity: &VersionIdentityStrings) -> bool {
    let values = [
        identity.product_name.as_deref(),
        identity.file_description.as_deref(),
        identity.company_name.as_deref(),
        identity.original_filename.as_deref(),
    ];
    values
        .into_iter()
        .flatten()
        .any(|value| value.to_ascii_lowercase().contains("gshade"))
}

/// Determines an advisory channel (Stable or Nightly) from a PE's identity
/// strings, used when adopting orphaned installs. Stable ReShade builds do not
/// contain the "unofficial" marker in their identity strings.
pub(crate) fn guess_advisory_channel(identity: &VersionIdentityStrings) -> ReshadeChannel {
    let values = [
        identity.product_name.as_deref(),
        identity.file_description.as_deref(),
        identity.original_filename.as_deref(),
        identity.product_version.as_deref(),
    ];
    let is_unofficial = values
        .into_iter()
        .flatten()
        .any(|value| value.to_ascii_lowercase().contains("unofficial"));

    if is_unofficial {
        ReshadeChannel::Nightly
    } else {
        ReshadeChannel::Stable
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn guess_advisory_channel_defaults_to_stable_for_clean_identity() {
        assert_eq!(
            guess_advisory_channel(&VersionIdentityStrings::default()),
            ReshadeChannel::Stable
        );
    }

    #[test]
    fn guess_advisory_channel_detects_unofficial_marker_case_insensitively() {
        let identity = VersionIdentityStrings {
            product_version: Some("1.0.0 UNOFFICIAL".to_owned()),
            ..Default::default()
        };
        assert_eq!(guess_advisory_channel(&identity), ReshadeChannel::Nightly);
    }

    #[test]
    fn guess_advisory_channel_checks_product_name_too() {
        let identity = VersionIdentityStrings {
            product_name: Some("ReShade (unofficial build)".to_owned()),
            ..Default::default()
        };
        assert_eq!(guess_advisory_channel(&identity), ReshadeChannel::Nightly);
    }

    #[test]
    fn is_known_custom_build_detects_gshade_runtime_dll_case_insensitively() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("dxgi.dll"), b"stub").expect("write");
        fs::write(dir.path().join("GShade64.dll"), b"runtime").expect("write");

        assert!(is_known_custom_build(dir.path(), None));
    }

    #[test]
    fn is_known_custom_build_detects_identity_strings_mentioning_gshade() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("dxgi.dll"), b"stub").expect("write");
        let identity = VersionIdentityStrings {
            product_name: Some("GShade".to_owned()),
            ..Default::default()
        };

        assert!(is_known_custom_build(dir.path(), Some(&identity)));
    }

    #[test]
    fn is_known_custom_build_is_false_for_a_plain_reshade_folder() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("dxgi.dll"), b"stub").expect("write");
        fs::write(dir.path().join("ReShade.ini"), b"[GENERAL]\r\n").expect("write");

        assert!(!is_known_custom_build(dir.path(), None));
        let reshade_identity = VersionIdentityStrings {
            company_name: Some("crosire".to_owned()),
            ..Default::default()
        };
        assert!(!is_known_custom_build(dir.path(), Some(&reshade_identity)));
    }
}
