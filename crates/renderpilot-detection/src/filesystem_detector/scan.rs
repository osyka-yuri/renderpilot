use std::path::{Path, PathBuf};

use renderpilot_application::AppResult;

const DEFAULT_PROBE_ENTRY_BUDGET: usize = 20_000;

/// Whether an installation traversal observed the complete reachable tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkCompleteness {
    /// Every reachable non-reparse directory was enumerated.
    Complete,
    /// One or more entries could not be enumerated or inspected.
    Incomplete,
}

/// Purpose and cost envelope of an installation-tree traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallWalkMode {
    /// Bounded filename-only advisory inspection.
    Probe,
    /// Authoritative traversal of the confirmed install tree.
    Full,
    /// Full traversal for proofs that must reject reparse descendants.
    FullStrict,
}

/// Stable class of a recoverable traversal diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkDiagnosticKind {
    /// Filesystem metadata or enumeration failure.
    Io,
    /// Cooperative cancellation requested by the caller.
    Cancelled,
    /// The caller's traversal-entry budget was exhausted.
    BudgetExceeded,
    /// Probe depth limit intentionally prevented a complete traversal.
    DepthLimit,
    /// A symbolic link, junction, or another reparse point was encountered.
    ReparsePoint,
}

/// One recoverable traversal failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkDiagnostic {
    kind: WalkDiagnosticKind,
    path: PathBuf,
    message: String,
}

impl WalkDiagnostic {
    /// Stable diagnostic category.
    pub fn kind(&self) -> WalkDiagnosticKind {
        self.kind
    }

    /// Path whose metadata or children could not be read.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// User-safe operating-system diagnostic.
    pub fn message(&self) -> &str {
        &self.message
    }
}

mod adapter;
use adapter::*;

mod collector;
pub(super) use collector::collect_files_filtered;
use collector::{collect_files_filtered_with_cancel, collect_files_filtered_with_mode};
mod policy;

/// Result of one installation-tree traversal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallTreeReport {
    files: Vec<PathBuf>,
    diagnostics: Vec<WalkDiagnostic>,
    visited_entries: usize,
}

impl InstallTreeReport {
    /// Completeness of this traversal.
    pub fn completeness(&self) -> WalkCompleteness {
        if self.diagnostics.is_empty() {
            WalkCompleteness::Complete
        } else {
            WalkCompleteness::Incomplete
        }
    }

    /// Candidate files in deterministic path order.
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    /// Recoverable failures that made the result incomplete.
    pub fn diagnostics(&self) -> &[WalkDiagnostic] {
        &self.diagnostics
    }

    /// Number of directory entries consumed from the caller's budget.
    pub fn visited_entries(&self) -> usize {
        self.visited_entries
    }

    /// Consumes the report and returns its candidate files.
    pub fn into_files(self) -> Vec<PathBuf> {
        self.files
    }
}

/// Shared traversal for executable probing and full component detection.
#[derive(Clone, Copy)]
pub struct InstallTreeWalker {
    mode: InstallWalkMode,
    max_depth: Option<usize>,
    max_entries: Option<usize>,
}

impl Default for InstallTreeWalker {
    fn default() -> Self {
        Self::full()
    }
}

impl InstallTreeWalker {
    /// Creates a full-tree walker with no arbitrary recursion cutoff.
    #[must_use]
    pub fn full() -> Self {
        Self {
            mode: InstallWalkMode::Full,
            max_depth: None,
            max_entries: None,
        }
    }

    /// Creates a full-tree walker for authority-sensitive proofs.
    ///
    /// Unlike [`Self::full`], this mode reports every encountered reparse
    /// point as incomplete instead of silently skipping it. This prevents a
    /// static-import proof from claiming coverage of a tree whose descendants
    /// may resolve outside the confirmed installation root.
    #[must_use]
    pub fn full_strict() -> Self {
        Self {
            mode: InstallWalkMode::FullStrict,
            max_depth: None,
            max_entries: None,
        }
    }

    /// Creates a cheap advisory probe with a bounded directory depth.
    #[must_use]
    pub fn probe() -> Self {
        Self {
            mode: InstallWalkMode::Probe,
            max_depth: Some(4),
            max_entries: Some(DEFAULT_PROBE_ENTRY_BUDGET),
        }
    }
    /// Purpose of this walker.
    pub fn mode(self) -> InstallWalkMode {
        self.mode
    }

    /// Limits recursion for narrowly scoped tests or callers with explicit policy.
    #[must_use]
    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = Some(max_depth);
        self
    }

    /// Caps the number of directory entries examined by this traversal.
    ///
    /// Exhausting the budget yields an incomplete report rather than a hard
    /// failure, so advisory callers cannot treat a truncated tree as proof.
    #[must_use]
    pub fn with_entry_budget(mut self, max_entries: usize) -> Self {
        self.max_entries = Some(max_entries);
        self
    }

    /// Traverses an installation tree with cheap filename filtering.
    pub fn walk_filtered(
        self,
        root: &Path,
        name_filter: impl Fn(&str) -> bool,
    ) -> AppResult<InstallTreeReport> {
        collect_files_filtered_with_mode(
            root,
            self.mode,
            self.max_depth,
            self.max_entries,
            &SYSTEM_INSTALL_TREE_FILE_SYSTEM,
            name_filter,
        )
    }

    /// Traverses with cooperative cancellation. A cancelled walk returns an
    /// incomplete report, so callers cannot accidentally prune from it.
    pub fn walk_filtered_cancellable(
        self,
        root: &Path,
        name_filter: impl Fn(&str) -> bool,
        is_cancelled: impl Fn() -> bool,
    ) -> AppResult<InstallTreeReport> {
        collect_files_filtered_with_cancel(
            root,
            self.mode,
            self.max_depth,
            self.max_entries,
            &SYSTEM_INSTALL_TREE_FILE_SYSTEM,
            name_filter,
            is_cancelled,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, fs, io, path::PathBuf};

    use super::{
        InstallTreeDirectoryEntry, InstallTreeEntryKind, InstallTreeFileSystem,
        InstallTreeMetadata, InstallTreeWalker, InstallWalkMode, WalkCompleteness,
        WalkDiagnosticKind, collect_files_filtered, collect_files_filtered_with_mode,
        read_symlink_metadata_tolerant,
    };

    #[test]
    fn probe_and_full_modes_are_explicit() {
        assert_eq!(InstallTreeWalker::probe().mode(), InstallWalkMode::Probe);
        assert_eq!(InstallTreeWalker::full().mode(), InstallWalkMode::Full);
        assert_eq!(
            InstallTreeWalker::full_strict().mode(),
            InstallWalkMode::FullStrict
        );
    }

    #[test]
    fn cancellation_returns_incomplete_without_files() {
        let temp = tempfile::tempdir().expect("temp dir");
        fs::write(temp.path().join("game.exe"), b"candidate").expect("fixture");

        let report = InstallTreeWalker::full()
            .walk_filtered_cancellable(temp.path(), |_| true, || true)
            .expect("cancelled report");

        assert_eq!(report.completeness(), WalkCompleteness::Incomplete);
        assert!(report.files().is_empty());
        assert_eq!(report.diagnostics().len(), 1);
        assert_eq!(
            report.diagnostics()[0].kind(),
            WalkDiagnosticKind::Cancelled
        );
    }

    #[test]
    fn entry_budget_returns_an_incomplete_prefix_and_exact_usage() {
        let temp = tempfile::tempdir().expect("temp dir");
        fs::write(temp.path().join("one.exe"), b"candidate").expect("fixture");
        fs::write(temp.path().join("two.exe"), b"candidate").expect("fixture");

        let report = InstallTreeWalker::full()
            .with_entry_budget(1)
            .walk_filtered(temp.path(), |_| true)
            .expect("bounded report");

        assert_eq!(report.completeness(), WalkCompleteness::Incomplete);
        assert_eq!(report.visited_entries(), 1);
        assert_eq!(report.files().len(), 1);
        assert_eq!(report.diagnostics().len(), 1);
        assert_eq!(
            report.diagnostics()[0].kind(),
            WalkDiagnosticKind::BudgetExceeded
        );
    }

    #[test]
    fn injected_filesystem_reports_a_vanished_entry_deterministically() {
        struct VanishingFileSystem;

        impl InstallTreeFileSystem for VanishingFileSystem {
            fn symlink_metadata(&self, path: &std::path::Path) -> io::Result<InstallTreeMetadata> {
                if path == std::path::Path::new("virtual-root") {
                    Ok(InstallTreeMetadata::new(
                        InstallTreeEntryKind::Directory,
                        false,
                    ))
                } else {
                    Err(io::Error::new(io::ErrorKind::NotFound, "vanished"))
                }
            }

            fn read_directory(
                &self,
                _path: &std::path::Path,
            ) -> io::Result<Vec<io::Result<InstallTreeDirectoryEntry>>> {
                Ok(vec![Ok(InstallTreeDirectoryEntry::new(
                    PathBuf::from("virtual-root/vanished.exe"),
                    OsString::from("vanished.exe"),
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "stale entry",
                    )),
                ))])
            }
        }

        fn accept_all(_: &str) -> bool {
            true
        }

        let file_system = VanishingFileSystem;
        let report = collect_files_filtered_with_mode(
            std::path::Path::new("virtual-root"),
            InstallWalkMode::Full,
            None,
            None,
            &file_system,
            accept_all,
        )
        .expect("injected walk");

        assert_eq!(report.completeness(), WalkCompleteness::Incomplete);
        assert_eq!(report.visited_entries(), 1);
        assert!(report.files().is_empty());
        assert_eq!(report.diagnostics().len(), 1);
        assert_eq!(report.diagnostics()[0].kind(), WalkDiagnosticKind::Io);
        assert!(report.diagnostics()[0].message().contains("disappeared"));
    }

    #[test]
    fn strict_full_walk_marks_reparse_descendants_incomplete() {
        struct ReparseFileSystem;

        impl InstallTreeFileSystem for ReparseFileSystem {
            fn symlink_metadata(&self, path: &std::path::Path) -> io::Result<InstallTreeMetadata> {
                let metadata = if path == std::path::Path::new("virtual-root") {
                    InstallTreeMetadata::new(InstallTreeEntryKind::Directory, false)
                } else {
                    InstallTreeMetadata::new(InstallTreeEntryKind::Directory, true)
                };
                Ok(metadata)
            }

            fn read_directory(
                &self,
                _path: &std::path::Path,
            ) -> io::Result<Vec<io::Result<InstallTreeDirectoryEntry>>> {
                Ok(vec![Ok(InstallTreeDirectoryEntry::new(
                    PathBuf::from("virtual-root/junction"),
                    OsString::from("junction"),
                    Ok(InstallTreeEntryKind::Directory),
                ))])
            }
        }

        let report = collect_files_filtered_with_mode(
            std::path::Path::new("virtual-root"),
            InstallWalkMode::FullStrict,
            None,
            None,
            &ReparseFileSystem,
            (),
        )
        .expect("strict walk");

        assert_eq!(report.completeness(), WalkCompleteness::Incomplete);
        assert!(report.files().is_empty());
        assert_eq!(report.diagnostics().len(), 1);
        assert_eq!(
            report.diagnostics()[0].kind(),
            WalkDiagnosticKind::ReparsePoint
        );
    }

    #[test]
    fn strict_full_walk_visits_directory_names_excluded_by_regular_full_scan() {
        let temp = tempfile::tempdir().expect("temp dir");
        let excluded = temp.path().join(".git");
        fs::create_dir_all(&excluded).expect("excluded-named directory");
        let candidate = excluded.join("loader.dll");
        fs::write(&candidate, b"candidate").expect("candidate file");

        let full = InstallTreeWalker::full()
            .walk_filtered(temp.path(), |_| true)
            .expect("regular full walk");
        let strict = InstallTreeWalker::full_strict()
            .walk_filtered(temp.path(), |_| true)
            .expect("strict full walk");

        assert!(
            !full.files().contains(&candidate),
            "ordinary full scans retain their established directory exclusions"
        );
        assert!(
            strict.files().contains(&candidate),
            "external-importer proof must cover all regular files below the root"
        );
    }

    #[test]
    fn read_symlink_metadata_tolerant_returns_none_for_missing_path() {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let missing = temp.path().join("vanished-during-scan.dll");

        let result = read_symlink_metadata_tolerant(&missing).expect("should not error");

        assert!(
            result.is_none(),
            "missing entry must surface as None instead of aborting the walk",
        );
    }

    #[test]
    fn name_filter_drops_files_with_other_extensions_before_metadata() {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let root = temp.path();

        fs::write(root.join("nvngx_dlss.dll"), b"fake-dll").expect("write dll");
        fs::write(root.join("config.ini"), b"key=value").expect("write ini");
        fs::write(root.join("video.bik"), b"bik").expect("write bik");
        fs::write(root.join("noext"), b"foo").expect("write no-ext");

        let allow_dll = |file_name: &str| {
            let lower = file_name.to_ascii_lowercase();
            lower.ends_with(".dll")
        };

        let collected = collect_files_filtered(root, Some(3), allow_dll)
            .expect("walk should succeed")
            .into_files();

        let names = collected
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["nvngx_dlss.dll".to_owned()]);
    }

    #[test]
    fn name_filter_skips_many_wrong_extensions_under_load() {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let root = temp.path();

        for index in 0..100 {
            fs::write(root.join(format!("asset_{index:03}.bik")), b"bik").expect("write bik");
        }
        fs::write(root.join("target.dll"), b"x").expect("write dll");
        fs::write(root.join("readme.ini"), b"ini").expect("write ini");

        let allow_dll = |file_name: &str| file_name.to_ascii_lowercase().ends_with(".dll");

        let collected = collect_files_filtered(root, Some(3), allow_dll)
            .expect("walk should succeed")
            .into_files();

        assert_eq!(collected.len(), 1);
        assert_eq!(
            collected[0].file_name().unwrap().to_string_lossy(),
            "target.dll"
        );
    }

    #[test]
    fn max_depth_includes_boundary_and_excludes_deeper_files() {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let root = temp.path();
        let depth_one = root.join("depth1");
        let depth_two = depth_one.join("depth2");
        let depth_three = depth_two.join("depth3");

        fs::create_dir_all(&depth_three).expect("nested directories should be created");

        let root_dll = root.join("root.dll");
        let depth_two_dll = depth_two.join("depth-two.dll");
        let depth_three_dll = depth_three.join("depth-three.dll");

        fs::write(&root_dll, b"root").expect("root dll");
        fs::write(&depth_two_dll, b"depth2").expect("depth2 dll");
        fs::write(&depth_three_dll, b"depth3").expect("depth3 dll");

        let collected = collect_files_filtered(root, Some(3), |name: &str| {
            name.to_ascii_lowercase().ends_with(".dll")
        })
        .expect("walk should succeed")
        .into_files();

        let collected_set = collected
            .into_iter()
            .collect::<std::collections::BTreeSet<PathBuf>>();

        assert!(collected_set.contains(&root_dll));
        assert!(collected_set.contains(&depth_two_dll));
        assert!(
            !collected_set.contains(&depth_three_dll),
            "files deeper than max_depth should be skipped",
        );
    }

    #[cfg(windows)]
    #[test]
    fn walker_skips_symlinked_directories() {
        use std::os::windows::fs::symlink_dir;

        let temp = tempfile::tempdir().expect("temp dir should be created");
        let root = temp.path();
        let real_dir = root.join("real");
        let link_dir = root.join("linked");

        fs::create_dir_all(&real_dir).expect("real dir should be created");
        fs::write(real_dir.join("nvngx_dlss.dll"), b"real").expect("real dll");

        if symlink_dir(&real_dir, &link_dir).is_err() {
            return;
        }

        let collected = collect_files_filtered(root, Some(4), |name: &str| {
            name.to_ascii_lowercase().ends_with(".dll")
        })
        .expect("walk should succeed")
        .into_files();

        let names = collected
            .iter()
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect::<Vec<_>>();

        assert_eq!(
            names.len(),
            1,
            "symlinked dir should not duplicate or add entries"
        );
        assert!(names[0].contains("/real/"));
        assert!(!names[0].contains("/linked/"));
    }

    #[test]
    fn walker_skips_dotted_and_development_directories() {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let root = temp.path();

        let shipping = root
            .join("Plugins")
            .join("DLSS")
            .join("Binaries")
            .join("ThirdParty")
            .join("Win64");
        let dev = shipping.join("Development");
        let dev_upper = root.join("DEVELOPMENT");
        let git_dir = root.join(".git");
        let vs_dir = root.join(".vs");

        fs::create_dir_all(&shipping).expect("shipping dir");
        fs::create_dir_all(&dev).expect("dev dir");
        fs::create_dir_all(&dev_upper).expect("upper dev dir");
        fs::create_dir_all(&git_dir).expect("git dir");
        fs::create_dir_all(&vs_dir).expect("vs dir");

        fs::write(shipping.join("nvngx_dlss.dll"), b"shipping-dlss").expect("shipping dll");
        fs::write(dev.join("nvngx_dlss.dll"), b"dev-dlss").expect("dev dll");
        fs::write(dev_upper.join("nvngx_dlss.dll"), b"dev-upper-dll").expect("dev upper dll");
        fs::write(git_dir.join("nvngx_dlss.dll"), b"git-dll").expect("git dll");
        fs::write(vs_dir.join("nvngx_dlss.dll"), b"vs-dll").expect("vs dll");

        let collected = collect_files_filtered(root, Some(10), |name: &str| {
            name.to_ascii_lowercase().ends_with(".dll")
        })
        .expect("walk should succeed")
        .into_files();

        assert_eq!(collected.len(), 1, "only shipping DLL should be collected");
        assert_eq!(
            collected[0].parent().unwrap(),
            shipping.as_path(),
            "collected DLL must be from the shipping directory"
        );
    }
}
