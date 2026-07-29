//! Shared install-path normalization used by auto-discovery and folder scan.
//!
//! `fs::canonicalize` on Windows often returns verbatim paths (`\\?\C:\…`).
//! Install paths must use the short form so auto-discovery and a manual
//! re-scan resolve to the same [`renderpilot_domain::InstallKey`]. `GameId`
//! remains opaque and independent of the path.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Canonicalizes an existing installation path and strips a Windows verbatim prefix.
///
/// Callers must not persist a lexical fallback when canonicalization fails:
/// junction aliases would then receive different installation identities.
pub fn canonicalize_install_path(path: &Path) -> io::Result<PathBuf> {
    fs::canonicalize(path).map(strip_verbatim_prefix)
}

/// Strips `\\?\` / `\\?\UNC\` prefixes returned by Windows `canonicalize`.
pub(crate) fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    let value = path.to_string_lossy();

    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = value.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_verbatim_drive_prefix() {
        let stripped = strip_verbatim_prefix(PathBuf::from(r"\\?\C:\Games\Foo"));
        assert_eq!(stripped, PathBuf::from(r"C:\Games\Foo"));
    }

    #[test]
    fn strip_verbatim_unc_prefix() {
        let stripped = strip_verbatim_prefix(PathBuf::from(r"\\?\UNC\server\share\Game"));
        assert_eq!(stripped, PathBuf::from(r"\\server\share\Game"));
    }

    #[test]
    fn leaves_non_verbatim_path_unchanged() {
        let path = PathBuf::from(r"D:\SteamLibrary\steamapps\common\Game");
        assert_eq!(strip_verbatim_prefix(path.clone()), path);
    }

    #[test]
    fn canonicalization_never_falls_back_to_a_lexical_alias() {
        let missing = std::env::temp_dir().join(format!(
            "renderpilot-missing-install-{}",
            std::process::id()
        ));

        assert!(canonicalize_install_path(&missing).is_err());
    }
}
