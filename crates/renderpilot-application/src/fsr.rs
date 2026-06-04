//! AMD FSR split-set knowledge — the single source of truth for the FidelityFX
//! SDK 2.0 DLL names and the rules that distinguish and place them.
//!
//! FSR 3.1.4+ and FSR 4 ship as three core split DLLs (loader, upscaler, frame
//! generation). The loader is the game's entry point: an FSR 3.1 game (or one we
//! upgraded) loads it as `amd_fidelityfx_dx12.dll`, while a natively split game
//! loads it under its own `amd_fidelityfx_loader_dx12.dll`. Candidate filtering,
//! plan building, and the executor all reason about these names — they live here
//! once so they can never drift apart.
//!
//! Two further members — the denoiser (Ray Regeneration) and the radiance cache —
//! are *optional effects*. They are developer-integrated neural-rendering features:
//! the loader only loads them when the game's own code requests them, so dropping
//! the DLL into a game that does not implement the feature does nothing. A swap may
//! therefore **replace** an optional effect the game already ships, but must never
//! **add** one the game lacks — unlike the core members, which the 3→4 upgrade
//! installs as a set.

use renderpilot_domain::ComponentFile;

/// The FSR entry-point DLL an FSR 3.1 game (or one we already upgraded) loads. The
/// FSR 4 loader is installed under this name on such games.
pub const ENTRY_POINT_FILE: &str = "amd_fidelityfx_dx12.dll";

/// The FSR 4 loader's own file name, used by a natively split game as its entry
/// point.
pub const LOADER_FILE: &str = "amd_fidelityfx_loader_dx12.dll";

/// The FSR 4 upscaler — the member whose presence marks a split set.
pub const UPSCALER_FILE: &str = "amd_fidelityfx_upscaler_dx12.dll";

/// The FSR 4 frame-generation member.
pub const FRAMEGEN_FILE: &str = "amd_fidelityfx_framegeneration_dx12.dll";

/// The FSR Ray Regeneration (denoiser) effect member — an *optional* effect (see
/// [`is_optional_effect`]).
pub const DENOISER_FILE: &str = "amd_fidelityfx_denoiser_dx12.dll";

/// The FSR radiance-cache effect member — an *optional* effect (see
/// [`is_optional_effect`]).
pub const RADIANCE_CACHE_FILE: &str = "amd_fidelityfx_radiancecache_dx12.dll";

/// Whether `file_name` is a split-set member shipped only inside a package: a core
/// member (loader, upscaler, frame generation) or an optional effect (denoiser,
/// radiance cache). Such members are never offered as standalone single-file
/// artifacts — only inside a version-matched package.
#[must_use]
pub fn is_split_member(file_name: &str) -> bool {
    file_name.eq_ignore_ascii_case(LOADER_FILE)
        || file_name.eq_ignore_ascii_case(UPSCALER_FILE)
        || file_name.eq_ignore_ascii_case(FRAMEGEN_FILE)
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
    file_name.eq_ignore_ascii_case(DENOISER_FILE)
        || file_name.eq_ignore_ascii_case(RADIANCE_CACHE_FILE)
}

/// Whether `file_name` is the upscaler — the marker that distinguishes an FSR
/// 3.1.4+/FSR 4 *split* set from the unified single-file FSR 3.x backend.
#[must_use]
pub fn is_split_marker(file_name: &str) -> bool {
    file_name.eq_ignore_ascii_case(UPSCALER_FILE)
}

/// Whether a component's files form a split FSR set (they contain the upscaler).
#[must_use]
pub fn is_split_set(files: &[ComponentFile]) -> bool {
    files
        .iter()
        .any(|file| file.path().file_name().is_some_and(is_split_marker))
}

/// Whether a component still loads the FSR 3.1 entry point `amd_fidelityfx_dx12.dll`.
///
/// This is the ironclad marker of the **FSR 3.1 lineage**: such a game can always
/// return to a unified FSR 3.x backend, and one we upgraded keeps the FSR 4 loader
/// installed under this very name. Contrast [`is_native_fsr4`].
#[must_use]
pub fn has_entry_point(files: &[ComponentFile]) -> bool {
    files.iter().any(|file| {
        file.path()
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(ENTRY_POINT_FILE))
    })
}

/// Whether a component is a **native** FSR 4 set: it loads its own
/// `amd_fidelityfx_loader_dx12.dll` and has no `amd_fidelityfx_dx12.dll` entry point,
/// so there is no FSR 3 to return to (a unified downgrade must not be offered).
#[must_use]
pub fn is_native_fsr4(files: &[ComponentFile]) -> bool {
    let has_loader = files.iter().any(|file| {
        file.path()
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(LOADER_FILE))
    });
    has_loader && !has_entry_point(files)
}

/// Returns the file that should represent an FSR component to users.
///
/// For cohesive dx12-lineage FSR, the entry point is the user-facing path even
/// though another member (usually the upscaler) may be the version
/// representative. Native FSR 4 has no dx12 entry point, so it falls back to the
/// first file.
#[must_use]
pub fn display_component_file(files: &[ComponentFile]) -> Option<&ComponentFile> {
    files
        .iter()
        .find(|file| {
            file.path()
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case(ENTRY_POINT_FILE))
        })
        .or_else(|| files.first())
}

/// Resolves the basename the FSR loader must be installed under for a given game.
///
/// A package targets the FSR 3.1 entry point [`ENTRY_POINT_FILE`] by default — what
/// an FSR 3.1 game (or one we already upgraded) loads. A natively split game instead
/// loads the loader under its own [`LOADER_FILE`]; updating it must overwrite that
/// file rather than strand it behind a fresh entry point. Non-loader targets (and
/// games without the native loader file) pass through unchanged.
#[must_use]
pub fn resolve_loader_install_target<'a>(
    default_target: &str,
    component_file_names: impl IntoIterator<Item = &'a str>,
) -> String {
    if default_target.eq_ignore_ascii_case(ENTRY_POINT_FILE)
        && component_file_names
            .into_iter()
            .any(|name| name.eq_ignore_ascii_case(LOADER_FILE))
    {
        return LOADER_FILE.to_owned();
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
        assert!(is_split_member(LOADER_FILE));
        assert!(is_split_member("AMD_FIDELITYFX_UPSCALER_DX12.DLL")); // case-insensitive
        assert!(is_split_member(FRAMEGEN_FILE));
        // Optional effects are package-only members too.
        assert!(is_split_member(DENOISER_FILE));
        assert!(is_split_member(RADIANCE_CACHE_FILE));
        // The installed entry point is not itself a packaged member name.
        assert!(!is_split_member(ENTRY_POINT_FILE));

        assert!(is_split_marker(UPSCALER_FILE));
        assert!(!is_split_marker(LOADER_FILE));
        assert!(!is_split_marker(ENTRY_POINT_FILE));
    }

    #[test]
    fn optional_effects_are_the_denoiser_and_radiance_cache_only() {
        assert!(is_optional_effect(DENOISER_FILE));
        assert!(is_optional_effect("AMD_FIDELITYFX_RADIANCECACHE_DX12.DLL")); // case-insensitive
                                                                              // Core members and the entry point are never optional.
        assert!(!is_optional_effect(LOADER_FILE));
        assert!(!is_optional_effect(UPSCALER_FILE));
        assert!(!is_optional_effect(FRAMEGEN_FILE));
        assert!(!is_optional_effect(ENTRY_POINT_FILE));
    }

    #[test]
    fn is_split_set_is_marked_by_the_upscaler() {
        assert!(is_split_set(&[
            file("C:/game/amd_fidelityfx_dx12.dll"),
            file("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
        ]));
        // Unified FSR 3.x: a single dx12 backend, no upscaler member.
        assert!(!is_split_set(&[file("C:/game/amd_fidelityfx_dx12.dll")]));
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
    }

    #[test]
    fn loader_target_adapts_to_the_games_entry_point() {
        // FSR 3.1 game (or one we already upgraded): loads `amd_fidelityfx_dx12.dll`.
        assert_eq!(
            resolve_loader_install_target(ENTRY_POINT_FILE, [ENTRY_POINT_FILE]),
            ENTRY_POINT_FILE,
        );
        // Natively split game: loads the loader under its own name — install there.
        assert_eq!(
            resolve_loader_install_target(ENTRY_POINT_FILE, [LOADER_FILE, UPSCALER_FILE]),
            LOADER_FILE,
        );
        // A non-loader target (e.g. the upscaler) is never retargeted.
        assert_eq!(
            resolve_loader_install_target(UPSCALER_FILE, [LOADER_FILE]),
            UPSCALER_FILE,
        );
    }

    #[test]
    fn display_file_prefers_dx12_entry_point_when_present() {
        let cohesive = [
            file("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
            file("C:/game/amd_fidelityfx_dx12.dll"),
            file("C:/game/amd_fidelityfx_framegeneration_dx12.dll"),
        ];
        assert_eq!(
            display_component_file(&cohesive).and_then(|file| file.path().file_name()),
            Some(ENTRY_POINT_FILE),
        );

        let native = [
            file("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
            file("C:/game/amd_fidelityfx_loader_dx12.dll"),
        ];
        assert_eq!(
            display_component_file(&native).and_then(|file| file.path().file_name()),
            Some(UPSCALER_FILE),
        );
    }

    #[test]
    fn artifact_install_target_uses_install_as_and_component_lineage() {
        let loader = artifact_file(
            "C:/lib/amd_fidelityfx_loader_dx12.dll",
            Some(ENTRY_POINT_FILE),
        );
        let upscaler = artifact_file("C:/lib/amd_fidelityfx_upscaler_dx12.dll", None);

        let cohesive_component = [file("C:/game/amd_fidelityfx_dx12.dll")];
        assert_eq!(
            resolve_artifact_install_target(&loader, &cohesive_component),
            ENTRY_POINT_FILE,
        );

        let native_component = [
            file("C:/game/amd_fidelityfx_loader_dx12.dll"),
            file("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
        ];
        assert_eq!(
            resolve_artifact_install_target(&loader, &native_component),
            LOADER_FILE,
        );
        assert_eq!(
            resolve_artifact_install_target(&upscaler, &native_component),
            UPSCALER_FILE,
        );
    }
}
