//! Locate DLSS DLLs inside a game install directory.
//!
//! Walks the directory tree (bounded depth, skipping noisy folders)
//! and returns every match for `nvngx_dlss.dll`, `nvngx_dlssg.dll`,
//! or `nvngx_dlssd.dll`. Callers can have several copies of the same
//! family in one game folder (mods, backups), so we return every hit
//! and let the orchestration layer pick (usually the shallowest path).

use std::path::{Path, PathBuf};

use renderpilot_nvapi::DlssDllKind;

use crate::fs_walk;

/// One DLL match found under the install directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DllSearchResult {
    /// Which DLSS DLL family this file belongs to.
    pub kind: DlssDllKind,
    /// Absolute path to the DLL file on disk.
    pub path: PathBuf,
    /// Depth relative to the install dir root (0 = directly in root).
    pub depth: u32,
}

/// All DLSS DLL families we currently recognise.
const ALL_KINDS: [DlssDllKind; 3] = [
    DlssDllKind::Sr,
    DlssDllKind::FrameGen,
    DlssDllKind::RayReconstruction,
];

/// Recursively searches `install_dir` for known DLSS DLLs.
/// Returns every match sorted by `(depth ASC, path ASC)` so callers
/// can pick the shallowest, lexicographically smallest hit as the
/// "primary" copy.
pub fn find_dlss_dlls(install_dir: &Path) -> Vec<DllSearchResult> {
    let mut results = Vec::new();
    fs_walk::walk_files(install_dir, 0, &mut |_entry, path, depth| {
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            return;
        };
        for kind in ALL_KINDS {
            if file_name.eq_ignore_ascii_case(kind.file_name()) {
                results.push(DllSearchResult {
                    kind,
                    path: path.to_path_buf(),
                    depth,
                });
                break;
            }
        }
    });
    results.sort_by(|a, b| (a.depth, &a.path).cmp(&(b.depth, &b.path)));
    results
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};

    use tempfile::TempDir;

    use super::*;

    fn touch(path: &Path) {
        File::create(path).expect("create test file");
    }

    #[test]
    fn finds_each_dll_family_at_root() {
        let tmp = TempDir::new().unwrap();
        touch(&tmp.path().join("nvngx_dlss.dll"));
        touch(&tmp.path().join("nvngx_dlssg.dll"));
        touch(&tmp.path().join("nvngx_dlssd.dll"));
        touch(&tmp.path().join("game.exe"));

        let results = find_dlss_dlls(tmp.path());
        let kinds: Vec<_> = results.iter().map(|r| r.kind).collect();
        assert_eq!(kinds.len(), 3);
        assert!(kinds.contains(&DlssDllKind::Sr));
        assert!(kinds.contains(&DlssDllKind::FrameGen));
        assert!(kinds.contains(&DlssDllKind::RayReconstruction));
        for r in &results {
            assert_eq!(r.depth, 0);
        }
    }

    #[test]
    fn detects_case_insensitive_dll_names() {
        let tmp = TempDir::new().unwrap();
        touch(&tmp.path().join("NVNGX_DLSS.DLL"));

        let results = find_dlss_dlls(tmp.path());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].kind, DlssDllKind::Sr);
    }

    #[test]
    fn descends_into_subdirectories_up_to_max_depth() {
        let tmp = TempDir::new().unwrap();
        let mut nested = tmp.path().to_path_buf();
        for i in 0..(fs_walk::MAX_DEPTH as usize) {
            nested = nested.join(format!("level_{i}"));
            fs::create_dir(&nested).unwrap();
        }
        touch(&nested.join("nvngx_dlss.dll"));

        let results = find_dlss_dlls(tmp.path());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].depth, fs_walk::MAX_DEPTH);
    }

    #[test]
    fn ignores_directories_below_max_depth() {
        let tmp = TempDir::new().unwrap();
        let mut nested = tmp.path().to_path_buf();
        for i in 0..((fs_walk::MAX_DEPTH + 2) as usize) {
            nested = nested.join(format!("level_{i}"));
            fs::create_dir(&nested).unwrap();
        }
        touch(&nested.join("nvngx_dlss.dll"));

        let results = find_dlss_dlls(tmp.path());
        assert!(results.is_empty());
    }

    #[test]
    fn skips_filtered_directories() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("_dlsswapper_backups")).unwrap();
        touch(
            &tmp.path()
                .join("_dlsswapper_backups")
                .join("nvngx_dlss.dll"),
        );
        fs::create_dir(tmp.path().join(".git")).unwrap();
        touch(&tmp.path().join(".git").join("nvngx_dlssg.dll"));
        // A real DLL outside the skipped dirs survives.
        touch(&tmp.path().join("nvngx_dlssd.dll"));

        let results = find_dlss_dlls(tmp.path());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].kind, DlssDllKind::RayReconstruction);
    }

    #[test]
    fn returns_results_sorted_by_depth_then_path() {
        let tmp = TempDir::new().unwrap();
        // Deeper copy first to make sure sorting actually fires.
        fs::create_dir_all(tmp.path().join("bin/win64")).unwrap();
        touch(&tmp.path().join("bin/win64/nvngx_dlss.dll"));
        touch(&tmp.path().join("nvngx_dlss.dll"));

        let results = find_dlss_dlls(tmp.path());
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].depth, 0);
        assert_eq!(results[1].depth, 2);
    }

    #[test]
    fn returns_empty_when_no_dlls_present() {
        let tmp = TempDir::new().unwrap();
        touch(&tmp.path().join("game.exe"));
        touch(&tmp.path().join("readme.txt"));
        assert!(find_dlss_dlls(tmp.path()).is_empty());
    }
}
