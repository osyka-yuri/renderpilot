//! Backup / sidecar path naming (syntax only -- not filesystem-kind validation).

use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};

/// Failure to build a distinct sidecar path from a candidate original path.
///
/// This is a **naming** error only: missing final path component. It does not
/// mean the path is not a regular file, directory, or symlink -- callers that
/// care about artifact kind must check the filesystem (or a validated type)
/// separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SidecarPathError {
    /// `original` has no final file-name component, so no distinct sidecar name
    /// can be formed (`PathBuf::add_extension` returns `false`).
    MissingFileName(PathBuf),
}

impl fmt::Display for SidecarPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFileName(path) => write!(
                f,
                "cannot build sidecar path for `{}`: missing file name component",
                path.display()
            ),
        }
    }
}

impl std::error::Error for SidecarPathError {}

/// Appends a lowercase `.bak` extension using [`PathBuf::add_extension`].
///
/// # Naming contract
///
/// - Requires a final file-name component; otherwise returns
///   [`SidecarPathError::MissingFileName`].
/// - Always appends: `foo.dll.bak` -> `foo.dll.bak.bak`.
/// - Does **not** inspect the filesystem (file vs directory vs symlink).
///
/// # Workflow contract
///
/// Call sites map errors by role -- this helper never panics:
/// - **Mutating apply** (engine place/remove, catalog overlay/downgrade, renodx
///   adopt): surface as service/provider errors so a bad path cannot abort the
///   process mid-mutation without a failure signal.
/// - **Best-effort discovery** (scan recovery, swap shadow plan): `log::warn!`
///   and skip / proceed without shadow when the name cannot be formed.
/// - **Restore / uninstall** (`revert_to_baseline_fs`, addon uninstall):
///   `log::warn!` and skip -- never silent.
/// - **Tests:** `.expect` is fine for fixture paths that always have a name.
///
/// Re-processing a path already classified as a backup is a **workflow**
/// decision, not this helper's.
pub(crate) fn backup_path(original: &Path) -> Result<PathBuf, SidecarPathError> {
    with_added_extension(original, "bak")
}

/// Expands a set of live paths to include their `.bak` sidecars (when derivable).
/// Each path is yielded first, then its sidecar -- so callers snapshot/validate
/// the live file before touching the backup.
pub(crate) fn expand_with_sidecars(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    paths
        .into_iter()
        .flat_map(|path| {
            let sidecar = backup_path(&path).ok();
            std::iter::once(path).chain(sidecar)
        })
        .collect()
}

/// Appends `extension` (without a leading dot) via [`PathBuf::add_extension`].
///
/// Prefer this over [`Path::with_added_extension`], which ignores the `bool`
/// from `add_extension` and can return an unchanged path when there is no
/// file name.
pub(crate) fn with_added_extension(
    original: &Path,
    extension: &str,
) -> Result<PathBuf, SidecarPathError> {
    let mut sidecar = original.to_path_buf();
    if !sidecar.add_extension(extension) {
        return Err(SidecarPathError::MissingFileName(original.to_path_buf()));
    }
    debug_assert_ne!(sidecar.as_path(), original);
    Ok(sidecar)
}

/// Inverse of [`backup_path`]: strip a single final extension that is **exactly**
/// lowercase `bak` (`OsStr::new("bak")`).
///
/// - `foo.dll.bak` -> `Some(foo.dll)`
/// - `foo.dll.bak.bak` -> `Some(foo.dll.bak)` (one layer)
/// - `foo.bak.tmp` -> `None` (final extension is not `bak`)
/// - `foo.dll.BAK` / `foo.dll.Bak` -> `None` (exact lowercase only)
/// - root / no file name -> `None`
///
/// Lossless for non-UTF-8 names: uses `file_stem` / `extension` OsStr APIs only.
#[must_use]
pub(crate) fn original_path_from_backup(backup: &Path) -> Option<PathBuf> {
    if backup.extension() != Some(OsStr::new("bak")) {
        return None;
    }
    let stem = backup.file_stem()?;
    if stem.is_empty() {
        return None;
    }
    Some(match backup.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(stem),
        _ => PathBuf::from(stem),
    })
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::*;

    #[test]
    fn backup_path_appends_bak_for_common_names() {
        assert_eq!(
            backup_path(Path::new("foo.dll")).expect("name"),
            PathBuf::from("foo.dll.bak")
        );
        assert_eq!(
            backup_path(Path::new("foo.tar.dll")).expect("name"),
            PathBuf::from("foo.tar.dll.bak")
        );
        assert_eq!(
            backup_path(Path::new("foo")).expect("name"),
            PathBuf::from("foo.bak")
        );
        assert_eq!(
            backup_path(Path::new(".hidden")).expect("name"),
            PathBuf::from(".hidden.bak")
        );
        // Helper always appends; workflow must reject re-backup of classified backups.
        assert_eq!(
            backup_path(Path::new("foo.dll.bak")).expect("name"),
            PathBuf::from("foo.dll.bak.bak")
        );
    }

    #[test]
    fn backup_path_rejects_missing_file_name() {
        assert_matches!(
            backup_path(Path::new("")),
            Err(SidecarPathError::MissingFileName(_))
        );
        // On Windows `C:\` / Unix `/` have no file name component.
        #[cfg(windows)]
        assert_matches!(
            backup_path(Path::new(r"C:\")),
            Err(SidecarPathError::MissingFileName(_))
        );
        #[cfg(unix)]
        assert_matches!(
            backup_path(Path::new("/")),
            Err(SidecarPathError::MissingFileName(_))
        );
    }

    #[test]
    fn original_path_from_backup_strips_one_lowercase_bak_layer() {
        assert_eq!(
            original_path_from_backup(Path::new("foo.dll.bak")),
            Some(PathBuf::from("foo.dll"))
        );
        assert_eq!(
            original_path_from_backup(Path::new("foo.dll.bak.bak")),
            Some(PathBuf::from("foo.dll.bak"))
        );
        assert_eq!(
            original_path_from_backup(Path::new(r"C:\Games\nvngx_dlss.dll.bak")),
            Some(PathBuf::from(r"C:\Games\nvngx_dlss.dll"))
        );
        assert_eq!(original_path_from_backup(Path::new("foo.bak.tmp")), None);
        assert_eq!(original_path_from_backup(Path::new("foo.dll.BAK")), None);
        assert_eq!(original_path_from_backup(Path::new("foo.dll.Bak")), None);
        assert_eq!(original_path_from_backup(Path::new("foo.dll")), None);
    }

    #[test]
    fn backup_path_round_trips_with_original_path_from_backup() {
        for sample in [
            "foo.dll",
            "foo.tar.dll",
            "foo",
            ".hidden",
            r"C:\Games\a\b.dll",
        ] {
            let original = Path::new(sample);
            let bak = backup_path(original).expect("sample has a file name");
            assert_eq!(
                original_path_from_backup(&bak).as_deref(),
                Some(original),
                "round-trip failed for {sample}"
            );
        }
    }

    #[test]
    fn with_added_extension_builds_sha256_sidecar() {
        assert_eq!(
            with_added_extension(Path::new("foo.dll"), "sha256").expect("name"),
            PathBuf::from("foo.dll.sha256")
        );
    }
}
