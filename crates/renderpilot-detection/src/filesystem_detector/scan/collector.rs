//! Deterministic, cancellation-aware installation-tree collection.

use std::path::{Path, PathBuf};

use renderpilot_application::AppResult;

use super::adapter::*;
use super::policy::is_skipped_directory;
use super::{InstallTreeReport, InstallWalkMode, WalkDiagnostic, WalkDiagnosticKind};

pub(in crate::filesystem_detector) trait FileNameFilter {
    fn should_consider(&self, file_name: &str) -> bool;
}

impl FileNameFilter for () {
    fn should_consider(&self, _file_name: &str) -> bool {
        true
    }
}

impl<F: Fn(&str) -> bool> FileNameFilter for F {
    fn should_consider(&self, file_name: &str) -> bool {
        self(file_name)
    }
}

/// Walks `root` (up to `max_depth` levels deep) and returns every file the
/// caller should consider during detection.
///
/// `name_filter` rejects leaf files using only [`fs::DirEntry::file_name`] and
/// [`fs::DirEntry::file_type`] when the entry is clearly a regular file, so
/// most non-matching assets never incur `fs::symlink_metadata`. Directories
/// are still opened and recursed; symlink targets and ambiguous entries fall
/// back to `symlink_metadata` for correctness.
pub(in crate::filesystem_detector) fn collect_files_filtered(
    root: &Path,
    max_depth: Option<usize>,
    name_filter: impl FileNameFilter,
) -> AppResult<InstallTreeReport> {
    collect_files_filtered_with_mode(
        root,
        InstallWalkMode::Full,
        max_depth,
        None,
        &SYSTEM_INSTALL_TREE_FILE_SYSTEM,
        name_filter,
    )
}

pub(super) fn collect_files_filtered_with_mode(
    root: &Path,
    mode: InstallWalkMode,
    max_depth: Option<usize>,
    max_entries: Option<usize>,
    file_system: &dyn InstallTreeFileSystem,
    name_filter: impl FileNameFilter,
) -> AppResult<InstallTreeReport> {
    collect_files_filtered_with_cancel(
        root,
        mode,
        max_depth,
        max_entries,
        file_system,
        name_filter,
        || false,
    )
}

pub(super) fn collect_files_filtered_with_cancel(
    root: &Path,
    mode: InstallWalkMode,
    max_depth: Option<usize>,
    max_entries: Option<usize>,
    file_system: &dyn InstallTreeFileSystem,
    name_filter: impl FileNameFilter,
    is_cancelled: impl Fn() -> bool,
) -> AppResult<InstallTreeReport> {
    let mut collector = FileCollector::new(
        mode,
        max_depth,
        max_entries,
        file_system,
        name_filter,
        is_cancelled,
    );

    collector.collect(root)?;

    Ok(collector.into_report())
}

struct FileCollector<'filesystem, F: FileNameFilter, C: Fn() -> bool> {
    mode: InstallWalkMode,
    max_depth: Option<usize>,
    max_entries: Option<usize>,
    visited_entries: usize,
    budget_exhausted: bool,
    file_system: &'filesystem dyn InstallTreeFileSystem,
    name_filter: F,
    files: Vec<PathBuf>,
    diagnostics: Vec<WalkDiagnostic>,
    is_cancelled: C,
}

impl<'filesystem, F: FileNameFilter, C: Fn() -> bool> FileCollector<'filesystem, F, C> {
    fn new(
        mode: InstallWalkMode,
        max_depth: Option<usize>,
        max_entries: Option<usize>,
        file_system: &'filesystem dyn InstallTreeFileSystem,
        name_filter: F,
        is_cancelled: C,
    ) -> Self {
        Self {
            mode,
            max_depth,
            max_entries,
            visited_entries: 0,
            budget_exhausted: false,
            file_system,
            name_filter,
            files: Vec::new(),
            diagnostics: Vec::new(),
            is_cancelled,
        }
    }

    fn collect(&mut self, root: &Path) -> AppResult<()> {
        if self.cancelled(root) {
            return Ok(());
        }
        let metadata = read_symlink_metadata(self.file_system, root)?;

        if metadata.is_reparse_point() {
            if self.rejects_reparse_points() {
                self.record_reparse_point(root);
            }
            return Ok(());
        }

        if metadata.kind() == InstallTreeEntryKind::File {
            self.visit_file_path(root, 0);
            return Ok(());
        }

        if metadata.kind() == InstallTreeEntryKind::Directory {
            self.visit_directory(root, 0)?;
        }

        Ok(())
    }

    fn into_report(mut self) -> InstallTreeReport {
        self.files.sort_unstable();
        InstallTreeReport {
            files: self.files,
            diagnostics: self.diagnostics,
            visited_entries: self.visited_entries,
        }
    }

    fn visit_directory(&mut self, path: &Path, dir_depth: usize) -> AppResult<()> {
        if self.cancelled(path) {
            return Ok(());
        }
        if self.should_skip_directory(path, dir_depth) {
            return Ok(());
        }

        let entries = match read_dir_entries(self.file_system, path) {
            Ok(entries) => entries,
            Err(error) => {
                self.record_error(path, &error);
                return Ok(());
            }
        };
        for entry_result in entries {
            if self.cancelled(path) || self.budget_exhausted {
                break;
            }
            match entry_result {
                Ok(entry) => {
                    if !self.consume_entry(&entry.path) {
                        break;
                    }
                    self.visit_child_entry(entry, dir_depth)?;
                }
                Err(error) => {
                    if !self.consume_entry(path) {
                        break;
                    }
                    self.record_error(path, &error);
                }
            }
        }

        Ok(())
    }

    /// `parent_dir_depth` is the depth of the directory whose children we are visiting.
    fn visit_child_entry(
        &mut self,
        entry: InstallTreeDirectoryEntry,
        parent_dir_depth: usize,
    ) -> AppResult<()> {
        let InstallTreeDirectoryEntry {
            path,
            file_name,
            file_type,
        } = entry;
        if self.cancelled(&path) {
            return Ok(());
        }
        let child_depth = parent_dir_depth + 1;

        let file_type = match read_entry_file_type_tolerant(self.file_system, &file_type, &path) {
            Ok(Some(file_type)) => file_type,
            Ok(None) => {
                self.record_vanished(&path);
                return Ok(());
            }
            Err(error) => {
                self.record_error(&path, &error);
                return Ok(());
            }
        };

        if self.rejects_reparse_points() {
            let metadata = match read_symlink_metadata_tolerant_with(self.file_system, &path) {
                Ok(Some(metadata)) => metadata,
                Ok(None) => {
                    self.record_vanished(&path);
                    return Ok(());
                }
                Err(error) => {
                    self.record_error(&path, &error);
                    return Ok(());
                }
            };
            if metadata.is_reparse_point() {
                self.record_reparse_point(&path);
                return Ok(());
            }
        }

        if file_type == InstallTreeEntryKind::Symlink {
            return Ok(());
        }

        if file_type == InstallTreeEntryKind::File {
            if self
                .max_depth
                .is_some_and(|max_depth| child_depth > max_depth)
            {
                return Ok(());
            }

            let Some(name) = file_name.to_str() else {
                return Ok(());
            };

            if !self.name_filter.should_consider(name) {
                return Ok(());
            }

            self.files.push(path);
            return Ok(());
        }

        if file_type == InstallTreeEntryKind::Directory {
            if self.should_skip_directory(&path, child_depth) {
                return Ok(());
            }

            // Re-check symlinks/junctions that can look like directories in `file_type`.
            let md = match read_symlink_metadata_tolerant_with(self.file_system, &path) {
                Ok(Some(metadata)) => metadata,
                Ok(None) => {
                    self.record_vanished(&path);
                    return Ok(());
                }
                Err(error) => {
                    self.record_error(&path, &error);
                    return Ok(());
                }
            };

            if md.is_reparse_point() {
                if self.rejects_reparse_points() {
                    self.record_reparse_point(&path);
                }
                return Ok(());
            }
            if md.kind() == InstallTreeEntryKind::Directory {
                self.visit_directory(&path, child_depth)?;
            }
        }

        Ok(())
    }

    fn visit_file_path(&mut self, path: &Path, depth: usize) {
        if self.max_depth.is_some_and(|max_depth| depth > max_depth) {
            return;
        }

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            return;
        };

        if !self.name_filter.should_consider(file_name) {
            return;
        }

        self.files.push(path.to_path_buf());
    }

    fn should_skip_directory(&mut self, path: &Path, depth: usize) -> bool {
        if self.max_depth.is_some_and(|max_depth| depth >= max_depth) {
            if !self
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind == WalkDiagnosticKind::DepthLimit)
            {
                self.diagnostics.push(WalkDiagnostic {
                    kind: WalkDiagnosticKind::DepthLimit,
                    path: path.to_path_buf(),
                    message: "advisory probe reached its directory depth limit".to_owned(),
                });
            }
            return true;
        }
        // Name-based directory exclusion policy applies to descendant directories only.
        depth > 0 && is_skipped_directory(path, self.mode)
    }

    fn record_error(&mut self, path: &Path, error: &renderpilot_application::AppError) {
        self.diagnostics.push(WalkDiagnostic {
            kind: WalkDiagnosticKind::Io,
            path: path.to_path_buf(),
            message: error.to_string(),
        });
    }

    fn record_vanished(&mut self, path: &Path) {
        self.diagnostics.push(WalkDiagnostic {
            kind: WalkDiagnosticKind::Io,
            path: path.to_path_buf(),
            message: "filesystem entry disappeared while the installation was being inspected"
                .to_owned(),
        });
    }

    fn record_reparse_point(&mut self, path: &Path) {
        self.diagnostics.push(WalkDiagnostic {
            kind: WalkDiagnosticKind::ReparsePoint,
            path: path.to_path_buf(),
            message: "installation traversal encountered a reparse point".to_owned(),
        });
    }

    fn rejects_reparse_points(&self) -> bool {
        self.mode == InstallWalkMode::FullStrict
    }

    fn consume_entry(&mut self, path: &Path) -> bool {
        if self
            .max_entries
            .is_some_and(|max_entries| self.visited_entries >= max_entries)
        {
            self.budget_exhausted = true;
            if !self
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind == WalkDiagnosticKind::BudgetExceeded)
            {
                self.diagnostics.push(WalkDiagnostic {
                    kind: WalkDiagnosticKind::BudgetExceeded,
                    path: path.to_path_buf(),
                    message: "installation inspection exhausted its traversal budget".to_owned(),
                });
            }
            return false;
        }
        self.visited_entries += 1;
        true
    }

    fn cancelled(&mut self, path: &Path) -> bool {
        if !(self.is_cancelled)() {
            return false;
        }
        if !self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == WalkDiagnosticKind::Cancelled)
        {
            self.diagnostics.push(WalkDiagnostic {
                kind: WalkDiagnosticKind::Cancelled,
                path: path.to_path_buf(),
                message: "installation tree traversal was cancelled".to_owned(),
            });
        }
        true
    }
}
