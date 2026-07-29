//! PathRef-style install relation helpers shared by catalog use cases.
//!
//! Domain `PathRef` values use `/` as a separator even on Windows.
//! Because of that, this module intentionally works with normalized path strings
//! instead of `std::path::Path`.
//!
//! **Not** the same as [`crate::paths`]: that module normalizes OS paths
//! (`\\` → `/`, case-fold, `\\?\` strip) for filesystem mutation keys.
//! Scan helpers assume PathRef-normalized input and add scope/drive-root
//! boundary checks for library discovery.

use renderpilot_domain::{InstallKey, InstallRoot, PathRef};

/// Returns `true` when `path` is equal to `scope_root` or lies under it.
///
/// The check is boundary-safe:
///
/// ```text
/// C:/Games/Game      matches C:/Games/Game/bin/x.dll
/// C:/Games/Game      does not match C:/Games/GameExtra/bin/x.dll
/// ```
///
/// Windows drive roots are treated as whole-volume scopes:
///
/// ```text
/// D:/ matches D:/SteamLibrary/steam.exe
/// ```
///
/// The function expects already normalized `PathRef`-style strings:
///
/// ```text
/// C:/Games/Game/bin/x.dll
/// ```
///
/// Not raw platform paths:
///
/// ```text
/// C:\Games\Game\bin\x.dll
/// ```
pub(super) fn normalized_path_within_scope(path: &str, scope_root: &str) -> bool {
    let Ok(path) = PathRef::new(path) else {
        return false;
    };
    let Ok(scope_root) = PathRef::new(scope_root) else {
        return false;
    };

    InstallRoot::new(scope_root).contains_path(&path)
}

/// Normalized PathRef-style comparison key for install-path matching.
///
/// Lower-cases ASCII and strips trailing `/` so case-only and trailing-slash
/// differences do not block matching catalog install paths against auto-scan
/// roots or a later manual re-scan of the same folder.
pub(super) fn install_path_match_key(path: &str) -> Option<InstallKey> {
    PathRef::new(path)
        .ok()
        .map(|path| InstallKey::from_path(&path))
}

/// Compares two validated install-path spellings without conflating invalid
/// inputs into one synthetic key.
pub(super) fn same_install_path(left: &str, right: &str) -> bool {
    install_path_match_key(left)
        .zip(install_path_match_key(right))
        .is_some_and(|(left, right)| left == right)
}

#[cfg(test)]
mod tests {
    use super::{install_path_match_key, normalized_path_within_scope};

    #[test]
    fn install_path_match_key_unifies_case_and_trailing_slash() {
        assert_eq!(
            install_path_match_key("D:/SteamLibrary/steamapps/common/Game/"),
            install_path_match_key("d:/steamlibrary/steamapps/common/Game"),
        );
    }

    #[test]
    fn same_path_is_within_scope() {
        assert!(normalized_path_within_scope(
            "C:/Games/GameA",
            "C:/Games/GameA"
        ));
    }

    #[test]
    fn child_path_under_directory_scope() {
        assert!(normalized_path_within_scope(
            "C:/Games/GameA/nvngx_dlss.dll",
            "C:/Games/GameA"
        ));
    }

    #[test]
    fn sibling_directory_name_is_not_a_prefix_match() {
        assert!(!normalized_path_within_scope(
            "C:/Games/GameExtra/bin/x.dll",
            "C:/Games/Game"
        ));
    }

    #[test]
    fn sibling_install_is_not_under_child_scope() {
        assert!(!normalized_path_within_scope(
            "C:/parent/GameB/x.dll",
            "C:/parent/GameA"
        ));
    }

    #[test]
    fn scope_with_trailing_separator_matches_same_path() {
        assert!(normalized_path_within_scope(
            "C:/Games/GameA",
            "C:/Games/GameA/"
        ));
    }

    #[test]
    fn scope_with_trailing_separator_matches_child_path() {
        assert!(normalized_path_within_scope(
            "C:/Games/GameA/bin/nvngx_dlss.dll",
            "C:/Games/GameA/"
        ));
    }

    #[test]
    fn path_with_trailing_separator_matches_scope_without_trailing_separator() {
        assert!(normalized_path_within_scope(
            "C:/Games/GameA/",
            "C:/Games/GameA"
        ));
    }

    #[test]
    fn both_paths_with_trailing_separators_match() {
        assert!(normalized_path_within_scope(
            "C:/Games/GameA/",
            "C:/Games/GameA/"
        ));
    }

    #[test]
    fn windows_drive_root_covers_volume_paths() {
        assert!(normalized_path_within_scope(
            "D:/SteamLibrary/steam.exe",
            "D:/"
        ));
    }

    #[test]
    fn windows_drive_root_does_not_cover_another_volume() {
        assert!(!normalized_path_within_scope(
            "E:/SteamLibrary/steam.exe",
            "D:/"
        ));
    }

    #[test]
    fn windows_drive_root_is_case_insensitive() {
        assert!(normalized_path_within_scope(
            "d:/SteamLibrary/steam.exe",
            "D:/"
        ));
    }

    #[test]
    fn regular_windows_paths_are_ascii_case_insensitive() {
        assert!(normalized_path_within_scope(
            "C:/Games/GameA/bin/x.dll",
            "c:/games/gamea"
        ));
    }

    #[test]
    fn empty_scope_never_matches() {
        assert!(!normalized_path_within_scope(
            "C:/Games/GameA/bin/x.dll",
            ""
        ));
    }

    #[test]
    fn empty_path_never_matches_non_empty_scope() {
        assert!(!normalized_path_within_scope("", "C:/Games/GameA"));
    }

    #[test]
    fn unix_root_covers_absolute_paths() {
        assert!(normalized_path_within_scope("/home/user/game/x.dll", "/"));
    }

    #[test]
    fn unix_root_does_not_cover_windows_style_path() {
        assert!(!normalized_path_within_scope("C:/Games/GameA/x.dll", "/"));
    }

    #[test]
    fn unix_sibling_directory_name_is_not_a_prefix_match() {
        assert!(!normalized_path_within_scope(
            "/games/GameExtra/bin/x.dll",
            "/games/Game"
        ));
    }

    #[test]
    fn unc_child_path_under_share_scope() {
        assert!(normalized_path_within_scope(
            "//server/share/GameA/bin/x.dll",
            "//server/share/GameA"
        ));
    }

    #[test]
    fn unc_sibling_share_path_is_not_prefix_match() {
        assert!(!normalized_path_within_scope(
            "//server/share-extra/GameA/bin/x.dll",
            "//server/share"
        ));
    }
}
