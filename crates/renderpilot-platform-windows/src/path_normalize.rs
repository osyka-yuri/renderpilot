//! Shared install-path normalization used by auto-discovery and folder scan.
//!
//! `fs::canonicalize` on Windows often returns verbatim paths (`\\?\C:\…`).
//! Catalog ids and install paths must use the short form so auto-scan and
//! manual re-scan of the same folder produce the same `manual:<path>` id.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// Canonicalizes an existing path best-effort and strips a Windows verbatim prefix.
pub(crate) fn canonicalize_install_dir(path: &Path) -> PathBuf {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    strip_verbatim_prefix(canonical)
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
}
