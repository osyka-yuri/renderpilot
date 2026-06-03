//! AMD FSR split-set knowledge — the single source of truth for the FidelityFX
//! SDK 2.0 DLL names and the rules that distinguish and place them.
//!
//! FSR 3.1.4+ and FSR 4 ship as three split DLLs (loader, upscaler, frame
//! generation). The loader is the game's entry point: an FSR 3.1 game (or one we
//! upgraded) loads it as `amd_fidelityfx_dx12.dll`, while a natively split game
//! loads it under its own `amd_fidelityfx_loader_dx12.dll`. Candidate filtering,
//! plan building, and the executor all reason about these names — they live here
//! once so they can never drift apart.

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

/// Whether `file_name` is a split-set member shipped only inside a package
/// (the loader, upscaler, or frame generation).
#[must_use]
pub fn is_split_member(file_name: &str) -> bool {
    file_name.eq_ignore_ascii_case(LOADER_FILE)
        || file_name.eq_ignore_ascii_case(UPSCALER_FILE)
        || file_name.eq_ignore_ascii_case(FRAMEGEN_FILE)
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

#[cfg(test)]
mod tests {
    use super::*;
    use renderpilot_domain::PathRef;

    fn file(path: &str) -> ComponentFile {
        ComponentFile::new(PathRef::new(path).expect("path should be valid"))
    }

    #[test]
    fn recognizes_split_members_and_marker() {
        assert!(is_split_member(LOADER_FILE));
        assert!(is_split_member("AMD_FIDELITYFX_UPSCALER_DX12.DLL")); // case-insensitive
        assert!(is_split_member(FRAMEGEN_FILE));
        // The installed entry point is not itself a packaged member name.
        assert!(!is_split_member(ENTRY_POINT_FILE));

        assert!(is_split_marker(UPSCALER_FILE));
        assert!(!is_split_marker(LOADER_FILE));
        assert!(!is_split_marker(ENTRY_POINT_FILE));
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
}
