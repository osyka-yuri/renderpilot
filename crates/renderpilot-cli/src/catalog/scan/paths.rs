//! PathRef-style string helpers.
//!
//! Domain `PathRef` values use `/` as a separator even on Windows.
//! Because of that, this module intentionally works with normalized path strings
//! instead of `std::path::Path`.

const PATH_SEPARATOR: u8 = b'/';
const WINDOWS_DRIVE_ROOT_LEN: usize = 3;

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
    let path = trim_trailing_separators_except_root(path);
    let scope_root = trim_trailing_separators_except_root(scope_root);

    if path.is_empty() || scope_root.is_empty() {
        return false;
    }

    if normalized_path_eq(path, scope_root) {
        return true;
    }

    if !normalized_path_starts_with(path, scope_root) {
        return false;
    }

    is_root_scope(scope_root) || has_path_boundary_after_prefix(path, scope_root.len())
}

fn trim_trailing_separators_except_root(path: &str) -> &str {
    let mut trimmed = path;

    while trimmed.len() > 1
        && trimmed.ends_with('/')
        && !is_windows_drive_root(trimmed)
        && !is_unix_root(trimmed)
    {
        trimmed = &trimmed[..trimmed.len() - 1];
    }

    trimmed
}

fn normalized_path_eq(left: &str, right: &str) -> bool {
    left.as_bytes().eq_ignore_ascii_case(right.as_bytes())
}

fn normalized_path_starts_with(path: &str, prefix: &str) -> bool {
    path.len() >= prefix.len()
        && path.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

fn has_path_boundary_after_prefix(path: &str, prefix_len: usize) -> bool {
    matches!(
        path.as_bytes().get(prefix_len),
        None | Some(&PATH_SEPARATOR)
    )
}

fn is_root_scope(path: &str) -> bool {
    is_windows_drive_root(path) || is_unix_root(path)
}

fn is_unix_root(path: &str) -> bool {
    path == "/"
}

fn is_windows_drive_root(path: &str) -> bool {
    let bytes = path.as_bytes();

    bytes.len() == WINDOWS_DRIVE_ROOT_LEN
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes[2] == PATH_SEPARATOR
}

#[cfg(test)]
mod tests {
    use super::normalized_path_within_scope;

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
