//! AMD FSR split-set knowledge — the single source of truth for the FidelityFX
//! SDK 2.0 DLL names and the rules that distinguish and place them.
//!
//! FSR 3.1.4+ and FSR 4 ship as three core split DLLs (loader, upscaler, frame
//! generation). The loader is the game's entry point: an FSR 3.1 DX12 game (or
//! one we upgraded) loads it as `amd_fidelityfx_dx12.dll`, and a Vulkan game
//! loads it as `amd_fidelityfx_vk.dll`. A natively split game loads the loader
//! under its own `amd_fidelityfx_loader_<api>.dll`. Candidate filtering, plan
//! building, and the executor all reason about these names — they live here once
//! so they can never drift apart.
//!
//! Two further members — the denoiser (Ray Regeneration) and the radiance cache —
//! are *optional effects*. They are developer-integrated neural-rendering features:
//! the loader only loads them when the game's own code requests them, so dropping
//! the DLL into a game that does not implement the feature does nothing. A swap may
//! therefore **replace** an optional effect the game already ships, but must never
//! **add** one the game lacks — unlike the core members, which the 3→4 upgrade
//! installs as a set.

use renderpilot_domain::ComponentFile;

/// The FSR entry-point DLL an FSR 3.1 DX12 game (or one we already upgraded) loads.
pub const ENTRY_POINT_FILE_DX12: &str = "amd_fidelityfx_dx12.dll";

/// The FSR entry-point DLL an FSR 3.1 Vulkan game (or one we already upgraded) loads.
pub const ENTRY_POINT_FILE_VK: &str = "amd_fidelityfx_vk.dll";

fn starts_with_ignore_ascii_case(s: &str, prefix: &str) -> bool {
    let prefix_len = prefix.len();
    s.len() >= prefix_len && s.as_bytes()[..prefix_len].eq_ignore_ascii_case(prefix.as_bytes())
}

fn ends_with_ignore_ascii_case(s: &str, suffix: &str) -> bool {
    let len = s.len();
    let suf_len = suffix.len();
    len >= suf_len && s.as_bytes()[len - suf_len..].eq_ignore_ascii_case(suffix.as_bytes())
}

/// Whether `file_name` is a split-set member shipped only inside a package: a core
/// member (loader, upscaler, frame generation) or an optional effect (denoiser,
/// radiance cache). Such members are never offered as standalone single-file
/// artifacts — only inside a version-matched package.
#[must_use]
pub fn is_split_member(file_name: &str) -> bool {
    starts_with_ignore_ascii_case(file_name, "amd_fidelityfx_loader_")
        || starts_with_ignore_ascii_case(file_name, "amd_fidelityfx_upscaler_")
        || starts_with_ignore_ascii_case(file_name, "amd_fidelityfx_framegeneration_")
        || is_optional_effect(file_name)
}

/// Whether `file_name` is an *optional* FSR effect — the denoiser (Ray
/// Regeneration) or the radiance cache.
///
/// These are developer-integrated features the loader loads only when the game
/// implements them. The swap engine therefore **replaces** such a DLL when the game
/// already has it, but never **adds** one to a game that lacks it. The core members
/// (loader, upscaler, frame generation) are not optional — the 3→4 upgrade installs
/// them together.
#[must_use]
pub fn is_optional_effect(file_name: &str) -> bool {
    starts_with_ignore_ascii_case(file_name, "amd_fidelityfx_denoiser_")
        || starts_with_ignore_ascii_case(file_name, "amd_fidelityfx_radiancecache_")
}

/// Whether `file_name` is the upscaler — the marker that distinguishes an FSR
/// 3.1.4+/FSR 4 *split* set from the unified single-file FSR 3.x backend.
#[must_use]
pub fn is_split_marker(file_name: &str) -> bool {
    starts_with_ignore_ascii_case(file_name, "amd_fidelityfx_upscaler_")
}

/// Whether a component's files form a split FSR set (they contain the upscaler).
#[must_use]
pub fn is_split_set(files: &[ComponentFile]) -> bool {
    files
        .iter()
        .any(|file| file.path().file_name().is_some_and(is_split_marker))
}

/// Whether `file_name` is an FSR 3.1 unified entry point (`amd_fidelityfx_dx12.dll` or `amd_fidelityfx_vk.dll`).
#[must_use]
pub fn is_entry_point(file_name: &str) -> bool {
    file_name.eq_ignore_ascii_case("amd_fidelityfx_dx12.dll")
        || file_name.eq_ignore_ascii_case("amd_fidelityfx_vk.dll")
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
        .any(|file| file.path().file_name().is_some_and(is_entry_point))
}

/// Whether `file_name` is the FSR loader's own file name.
#[must_use]
pub fn is_loader(file_name: &str) -> bool {
    starts_with_ignore_ascii_case(file_name, "amd_fidelityfx_loader_")
}

/// Whether a component is a **native** FSR 4 set: it loads its own
/// `amd_fidelityfx_loader_` and has no entry point,
/// so there is no FSR 3 to return to (a unified downgrade must not be offered).
#[must_use]
pub fn is_native_fsr4(files: &[ComponentFile]) -> bool {
    let has_loader = files
        .iter()
        .any(|file| file.path().file_name().is_some_and(is_loader));
    has_loader && !has_entry_point(files)
}

/// Returns the file that should represent an FSR component to users.
///
/// For cohesive FSR, the entry point is the user-facing path even
/// though another member (usually the upscaler) may be the version
/// representative. Native FSR 4 has no entry point, so it falls back to the
/// first file.
#[must_use]
pub fn display_component_file(files: &[ComponentFile]) -> Option<&ComponentFile> {
    files
        .iter()
        .find(|file| file.path().file_name().is_some_and(is_entry_point))
        .or_else(|| files.first())
}

/// Resolves the basename the FSR loader must be installed under for a given game.
///
/// A package targets the FSR 3.1 entry point by default — what
/// an FSR 3.1 game (or one we already upgraded) loads. A natively split game instead
/// loads the loader under its own `amd_fidelityfx_loader_*`; updating it must overwrite that
/// file rather than strand it behind a fresh entry point. Non-loader targets (and
/// games without the native loader file) pass through unchanged.
#[must_use]
pub fn resolve_loader_install_target<'a>(
    default_target: &str,
    component_file_names: impl IntoIterator<Item = &'a str>,
) -> String {
    if is_entry_point(default_target) {
        if let Some(native_loader) = component_file_names
            .into_iter()
            .find(|name| is_loader(name))
        {
            return native_loader.to_owned();
        }
    }

    default_target.to_owned()
}

/// Resolves the basename an artifact member should install under for the given
/// component.
///
/// This folds together the artifact file's default target selection (`install_as`
/// when present, else its own file name) and the FSR loader's lineage-aware
/// retargeting via [`resolve_loader_install_target`]. Shared by both plan
/// building and apply execution so they cannot drift.
#[must_use]
pub fn resolve_artifact_install_target(
    artifact_file: &ComponentFile,
    component_files: &[ComponentFile],
) -> String {
    let default_target = artifact_file
        .install_as()
        .or_else(|| artifact_file.path().file_name())
        .unwrap_or("");

    resolve_loader_install_target(
        default_target,
        component_files
            .iter()
            .filter_map(|file| file.path().file_name()),
    )
}

/// Identifies the graphics API an FSR DLL is bound to, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsrApi {
    /// DirectX 12 backend (`_dx12.dll`).
    Dx12,
    /// Vulkan backend (`_vk.dll`).
    Vulkan,
}

/// Returns the specific graphics API this FSR file belongs to, based on its suffix.
#[must_use]
pub fn fsr_graphics_api(file_name: &str) -> Option<FsrApi> {
    if ends_with_ignore_ascii_case(file_name, "_dx12.dll") {
        Some(FsrApi::Dx12)
    } else if ends_with_ignore_ascii_case(file_name, "_vk.dll") {
        Some(FsrApi::Vulkan)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderpilot_domain::PathRef;

    fn file(path: &str) -> ComponentFile {
        ComponentFile::new(PathRef::new(path).expect("path should be valid"))
    }

    fn artifact_file(path: &str, install_as: Option<&str>) -> ComponentFile {
        let file = file(path);

        match install_as {
            Some(install_as) => file.with_install_as(install_as),
            None => file,
        }
    }

    #[test]
    fn recognizes_split_members_and_marker() {
        assert!(is_split_member("amd_fidelityfx_loader_dx12.dll"));
        assert!(is_split_member("AMD_FIDELITYFX_UPSCALER_DX12.DLL")); // case-insensitive
        assert!(is_split_member("amd_fidelityfx_framegeneration_dx12.dll"));
        // Optional effects are package-only members too.
        assert!(is_split_member("amd_fidelityfx_denoiser_dx12.dll"));
        assert!(is_split_member("amd_fidelityfx_radiancecache_dx12.dll"));
        // The installed entry point is not itself a packaged member name.
        assert!(!is_split_member("amd_fidelityfx_dx12.dll"));

        assert!(is_split_marker("amd_fidelityfx_upscaler_dx12.dll"));
        assert!(!is_split_marker("amd_fidelityfx_loader_dx12.dll"));
        assert!(!is_split_marker("amd_fidelityfx_dx12.dll"));

        // VK variants mirror DX12.
        assert!(is_split_member("amd_fidelityfx_loader_vk.dll"));
        assert!(is_split_member("AMD_FIDELITYFX_UPSCALER_VK.DLL")); // case-insensitive
        assert!(is_split_member("amd_fidelityfx_framegeneration_vk.dll"));
        assert!(is_split_member("amd_fidelityfx_denoiser_vk.dll"));
        assert!(is_split_member("amd_fidelityfx_radiancecache_vk.dll"));
        assert!(!is_split_member("amd_fidelityfx_vk.dll")); // entry point is not a member

        assert!(is_split_marker("amd_fidelityfx_upscaler_vk.dll"));
        assert!(!is_split_marker("amd_fidelityfx_loader_vk.dll"));
        assert!(!is_split_marker("amd_fidelityfx_vk.dll"));
    }

    #[test]
    fn optional_effects_are_the_denoiser_and_radiance_cache_only() {
        assert!(is_optional_effect("amd_fidelityfx_denoiser_dx12.dll"));
        assert!(is_optional_effect("AMD_FIDELITYFX_RADIANCECACHE_DX12.DLL")); // case-insensitive
                                                                              // Core members and the entry point are never optional.
        assert!(!is_optional_effect("amd_fidelityfx_loader_dx12.dll"));
        assert!(!is_optional_effect("amd_fidelityfx_upscaler_dx12.dll"));
        assert!(!is_optional_effect(
            "amd_fidelityfx_framegeneration_dx12.dll"
        ));
        assert!(!is_optional_effect("amd_fidelityfx_dx12.dll"));

        // VK variants mirror DX12.
        assert!(is_optional_effect("amd_fidelityfx_denoiser_vk.dll"));
        assert!(is_optional_effect("AMD_FIDELITYFX_RADIANCECACHE_VK.DLL")); // case-insensitive
        assert!(!is_optional_effect("amd_fidelityfx_loader_vk.dll"));
        assert!(!is_optional_effect("amd_fidelityfx_upscaler_vk.dll"));
        assert!(!is_optional_effect("amd_fidelityfx_framegeneration_vk.dll"));
        assert!(!is_optional_effect("amd_fidelityfx_vk.dll"));
    }

    #[test]
    fn is_split_set_is_marked_by_the_upscaler() {
        assert!(is_split_set(&[
            file("C:/game/amd_fidelityfx_dx12.dll"),
            file("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
        ]));
        // Unified FSR 3.x: a single dx12 backend, no upscaler member.
        assert!(!is_split_set(&[file("C:/game/amd_fidelityfx_dx12.dll")]));

        assert!(is_split_set(&[
            file("C:/game/amd_fidelityfx_vk.dll"),
            file("C:/game/amd_fidelityfx_upscaler_vk.dll"),
        ]));
        assert!(!is_split_set(&[file("C:/game/amd_fidelityfx_vk.dll")]));
    }

    #[test]
    fn entry_point_distinguishes_dx12_lineage_from_native_fsr4() {
        // dx12-lineage: pure FSR 3.1, or one we upgraded (loader installed as dx12).
        let unified = [file("C:/game/amd_fidelityfx_dx12.dll")];
        let upgraded = [
            file("C:/game/amd_fidelityfx_dx12.dll"),
            file("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
            file("C:/game/amd_fidelityfx_framegeneration_dx12.dll"),
        ];
        assert!(has_entry_point(&unified));
        assert!(has_entry_point(&upgraded));
        assert!(!is_native_fsr4(&unified));
        assert!(!is_native_fsr4(&upgraded));

        // Native FSR 4: loads its own loader, no dx12 entry point.
        let native = [
            file("C:/game/amd_fidelityfx_loader_dx12.dll"),
            file("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
        ];
        assert!(!has_entry_point(&native));
        assert!(is_native_fsr4(&native));

        // VK-lineage: pure FSR 3.1 VK, or one we upgraded (loader installed as vk).
        let unified_vk = [file("C:/game/amd_fidelityfx_vk.dll")];
        let upgraded_vk = [
            file("C:/game/amd_fidelityfx_vk.dll"),
            file("C:/game/amd_fidelityfx_upscaler_vk.dll"),
            file("C:/game/amd_fidelityfx_framegeneration_vk.dll"),
        ];
        assert!(has_entry_point(&unified_vk));
        assert!(has_entry_point(&upgraded_vk));
        assert!(!is_native_fsr4(&unified_vk));
        assert!(!is_native_fsr4(&upgraded_vk));

        // Native FSR 4 VK: loads its own loader, no vk entry point.
        let native_vk = [
            file("C:/game/amd_fidelityfx_loader_vk.dll"),
            file("C:/game/amd_fidelityfx_upscaler_vk.dll"),
        ];
        assert!(!has_entry_point(&native_vk));
        assert!(is_native_fsr4(&native_vk));
    }

    #[test]
    fn loader_target_adapts_to_the_games_entry_point() {
        // FSR 3.1 game (or one we already upgraded): loads `amd_fidelityfx_dx12.dll`.
        assert_eq!(
            resolve_loader_install_target("amd_fidelityfx_dx12.dll", ["amd_fidelityfx_dx12.dll"]),
            "amd_fidelityfx_dx12.dll",
        );
        // Natively split game: loads the loader under its own name — install there.
        assert_eq!(
            resolve_loader_install_target(
                "amd_fidelityfx_dx12.dll",
                [
                    "amd_fidelityfx_loader_dx12.dll",
                    "amd_fidelityfx_upscaler_dx12.dll"
                ]
            ),
            "amd_fidelityfx_loader_dx12.dll",
        );
        // A non-loader target (e.g. the upscaler) is never retargeted.
        assert_eq!(
            resolve_loader_install_target(
                "amd_fidelityfx_upscaler_dx12.dll",
                ["amd_fidelityfx_loader_dx12.dll"]
            ),
            "amd_fidelityfx_upscaler_dx12.dll",
        );

        // VK variants mirror DX12.
        assert_eq!(
            resolve_loader_install_target("amd_fidelityfx_vk.dll", ["amd_fidelityfx_vk.dll"]),
            "amd_fidelityfx_vk.dll",
        );
        assert_eq!(
            resolve_loader_install_target(
                "amd_fidelityfx_vk.dll",
                [
                    "amd_fidelityfx_loader_vk.dll",
                    "amd_fidelityfx_upscaler_vk.dll"
                ],
            ),
            "amd_fidelityfx_loader_vk.dll",
        );
        assert_eq!(
            resolve_loader_install_target(
                "amd_fidelityfx_upscaler_vk.dll",
                ["amd_fidelityfx_loader_vk.dll"],
            ),
            "amd_fidelityfx_upscaler_vk.dll",
        );
    }

    #[test]
    fn display_file_prefers_entry_point_when_present() {
        let cohesive = [
            file("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
            file("C:/game/amd_fidelityfx_dx12.dll"),
            file("C:/game/amd_fidelityfx_framegeneration_dx12.dll"),
        ];
        assert_eq!(
            display_component_file(&cohesive).and_then(|file| file.path().file_name()),
            Some("amd_fidelityfx_dx12.dll"),
        );

        let native = [
            file("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
            file("C:/game/amd_fidelityfx_loader_dx12.dll"),
        ];
        assert_eq!(
            display_component_file(&native).and_then(|file| file.path().file_name()),
            Some("amd_fidelityfx_upscaler_dx12.dll"),
        );

        let cohesive_vk = [
            file("C:/game/amd_fidelityfx_upscaler_vk.dll"),
            file("C:/game/amd_fidelityfx_vk.dll"),
            file("C:/game/amd_fidelityfx_framegeneration_vk.dll"),
        ];
        assert_eq!(
            display_component_file(&cohesive_vk).and_then(|f| f.path().file_name()),
            Some("amd_fidelityfx_vk.dll"),
        );
    }

    #[test]
    fn artifact_install_target_uses_install_as_and_component_lineage() {
        let loader = artifact_file(
            "C:/lib/amd_fidelityfx_loader_dx12.dll",
            Some("amd_fidelityfx_dx12.dll"),
        );
        let upscaler = artifact_file("C:/lib/amd_fidelityfx_upscaler_dx12.dll", None);

        let cohesive_component = [file("C:/game/amd_fidelityfx_dx12.dll")];
        assert_eq!(
            resolve_artifact_install_target(&loader, &cohesive_component),
            "amd_fidelityfx_dx12.dll",
        );

        let native_component = [
            file("C:/game/amd_fidelityfx_loader_dx12.dll"),
            file("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
        ];
        assert_eq!(
            resolve_artifact_install_target(&loader, &native_component),
            "amd_fidelityfx_loader_dx12.dll",
        );
        assert_eq!(
            resolve_artifact_install_target(&upscaler, &native_component),
            "amd_fidelityfx_upscaler_dx12.dll",
        );

        // VK variants mirror DX12.
        let loader_vk = artifact_file(
            "C:/lib/amd_fidelityfx_loader_vk.dll",
            Some("amd_fidelityfx_vk.dll"),
        );
        let upscaler_vk = artifact_file("C:/lib/amd_fidelityfx_upscaler_vk.dll", None);

        let cohesive_vk = [file("C:/game/amd_fidelityfx_vk.dll")];
        assert_eq!(
            resolve_artifact_install_target(&loader_vk, &cohesive_vk),
            "amd_fidelityfx_vk.dll",
        );

        let native_vk = [
            file("C:/game/amd_fidelityfx_loader_vk.dll"),
            file("C:/game/amd_fidelityfx_upscaler_vk.dll"),
        ];
        assert_eq!(
            resolve_artifact_install_target(&loader_vk, &native_vk),
            "amd_fidelityfx_loader_vk.dll",
        );
        assert_eq!(
            resolve_artifact_install_target(&upscaler_vk, &native_vk),
            "amd_fidelityfx_upscaler_vk.dll",
        );
    }

    #[test]
    fn api_predicates_match_suffix_not_infix() {
        assert_eq!(
            fsr_graphics_api("amd_fidelityfx_vk.dll"),
            Some(FsrApi::Vulkan)
        );
        assert_eq!(
            fsr_graphics_api("amd_fidelityfx_loader_vk.dll"),
            Some(FsrApi::Vulkan)
        );
        assert_eq!(
            fsr_graphics_api("AMD_FIDELITYFX_UPSCALER_VK.DLL"),
            Some(FsrApi::Vulkan)
        ); // case-insensitive
        assert_ne!(
            fsr_graphics_api("amd_fidelityfx_dx12.dll"),
            Some(FsrApi::Vulkan)
        );
        assert_ne!(
            fsr_graphics_api("nvidia_vk_compat.dll"),
            Some(FsrApi::Vulkan)
        ); // infix must NOT match

        assert_eq!(
            fsr_graphics_api("amd_fidelityfx_dx12.dll"),
            Some(FsrApi::Dx12)
        );
        assert_eq!(
            fsr_graphics_api("amd_fidelityfx_loader_dx12.dll"),
            Some(FsrApi::Dx12)
        );
        assert_eq!(
            fsr_graphics_api("AMD_FIDELITYFX_UPSCALER_DX12.DLL"),
            Some(FsrApi::Dx12)
        ); // case-insensitive
        assert_ne!(
            fsr_graphics_api("amd_fidelityfx_vk.dll"),
            Some(FsrApi::Dx12)
        );
        assert_ne!(
            fsr_graphics_api("some_dx12_shim_extra.dll"),
            Some(FsrApi::Dx12)
        ); // infix must NOT match
    }
}
