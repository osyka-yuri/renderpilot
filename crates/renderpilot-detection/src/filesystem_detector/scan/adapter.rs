//! Private operating-system adapter for installation-tree traversal.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use renderpilot_application::AppResult;

use crate::error::detection_context_error;

/// Filesystem entry kind needed by installation traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InstallTreeEntryKind {
    /// Regular file.
    File,
    /// Directory.
    Directory,
    /// Symbolic link.
    Symlink,
    /// Device or another unsupported entry kind.
    Other,
}

/// Metadata projection used by the injectable traversal filesystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct InstallTreeMetadata {
    kind: InstallTreeEntryKind,
    reparse_point: bool,
}

impl InstallTreeMetadata {
    /// Creates metadata for a deterministic filesystem adapter.
    #[must_use]
    pub(super) const fn new(kind: InstallTreeEntryKind, reparse_point: bool) -> Self {
        Self {
            kind,
            reparse_point,
        }
    }

    pub(super) const fn kind(self) -> InstallTreeEntryKind {
        self.kind
    }

    pub(super) const fn is_reparse_point(self) -> bool {
        self.reparse_point || matches!(self.kind, InstallTreeEntryKind::Symlink)
    }
}

/// One directory entry returned by an [`InstallTreeFileSystem`].
#[derive(Debug)]
pub(super) struct InstallTreeDirectoryEntry {
    pub(super) path: PathBuf,
    pub(super) file_name: std::ffi::OsString,
    pub(super) file_type: io::Result<InstallTreeEntryKind>,
}

impl InstallTreeDirectoryEntry {
    /// Creates an entry for a deterministic filesystem adapter.
    #[must_use]
    pub(super) fn new(
        path: PathBuf,
        file_name: std::ffi::OsString,
        file_type: io::Result<InstallTreeEntryKind>,
    ) -> Self {
        Self {
            path,
            file_name,
            file_type,
        }
    }
}

/// Injectable filesystem boundary for deterministic installation-walker tests.
pub(super) trait InstallTreeFileSystem: Send + Sync {
    /// Reads non-following metadata for one path.
    fn symlink_metadata(&self, path: &Path) -> io::Result<InstallTreeMetadata>;

    /// Enumerates a directory while preserving per-entry failures.
    fn read_directory(&self, path: &Path)
    -> io::Result<Vec<io::Result<InstallTreeDirectoryEntry>>>;
}

#[derive(Debug)]
pub(super) struct SystemInstallTreeFileSystem;

pub(super) static SYSTEM_INSTALL_TREE_FILE_SYSTEM: SystemInstallTreeFileSystem =
    SystemInstallTreeFileSystem;

impl InstallTreeFileSystem for SystemInstallTreeFileSystem {
    fn symlink_metadata(&self, path: &Path) -> io::Result<InstallTreeMetadata> {
        fs::symlink_metadata(path).map(|metadata| project_metadata(&metadata))
    }

    fn read_directory(
        &self,
        path: &Path,
    ) -> io::Result<Vec<io::Result<InstallTreeDirectoryEntry>>> {
        Ok(fs::read_dir(path)?
            .map(|entry| {
                entry.map(|entry| {
                    let path = entry.path();
                    let file_name = entry.file_name();
                    let file_type = entry.file_type().map(project_file_type);
                    InstallTreeDirectoryEntry::new(path, file_name, file_type)
                })
            })
            .collect())
    }
}

pub(super) fn read_symlink_metadata(
    file_system: &dyn InstallTreeFileSystem,
    path: &Path,
) -> AppResult<InstallTreeMetadata> {
    file_system.symlink_metadata(path).map_err(|error| {
        detection_context_error(format_args!("could not read {}", path.display()), error)
    })
}

/// Returns `Ok(None)` when the entry vanished between `read_dir` and the
/// follow-up syscall (Steam updates, AV scanner, search indexer). The walker
/// records this as an incomplete diagnostic instead of aborting the whole scan.
#[cfg(test)]
pub(super) fn read_symlink_metadata_tolerant(
    path: &Path,
) -> AppResult<Option<InstallTreeMetadata>> {
    read_symlink_metadata_tolerant_with(&SYSTEM_INSTALL_TREE_FILE_SYSTEM, path)
}

pub(super) fn read_symlink_metadata_tolerant_with(
    file_system: &dyn InstallTreeFileSystem,
    path: &Path,
) -> AppResult<Option<InstallTreeMetadata>> {
    match file_system.symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(detection_context_error(
            format_args!("could not read {}", path.display()),
            error,
        )),
    }
}

/// Resolves a `DirEntry::file_type`, falling back to a tolerant
/// `symlink_metadata` read when `file_type()` itself fails (rare on Windows
/// when the entry already disappeared). Returns `Ok(None)` when neither call
/// can see the entry anymore.
pub(super) fn read_entry_file_type_tolerant(
    file_system: &dyn InstallTreeFileSystem,
    file_type: &io::Result<InstallTreeEntryKind>,
    path: &Path,
) -> AppResult<Option<InstallTreeEntryKind>> {
    match file_type {
        Ok(file_type) => Ok(Some(*file_type)),
        Err(_) => {
            Ok(read_symlink_metadata_tolerant_with(file_system, path)?
                .map(InstallTreeMetadata::kind))
        }
    }
}

fn project_file_type(file_type: fs::FileType) -> InstallTreeEntryKind {
    if file_type.is_symlink() {
        InstallTreeEntryKind::Symlink
    } else if file_type.is_file() {
        InstallTreeEntryKind::File
    } else if file_type.is_dir() {
        InstallTreeEntryKind::Directory
    } else {
        InstallTreeEntryKind::Other
    }
}

fn project_metadata(metadata: &fs::Metadata) -> InstallTreeMetadata {
    InstallTreeMetadata::new(
        project_file_type(metadata.file_type()),
        is_system_reparse_point(metadata),
    )
}

fn is_system_reparse_point(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }

    #[cfg(not(windows))]
    false
}

/// Reads a directory's entries in OS order.
///
/// Entries are intentionally not sorted here: the walker pushes only the
/// filtered files into `self.files`, and `into_sorted_files` imposes the final
/// deterministic order once — a per-directory sort would be redundant work
/// (and `sort_unstable_by_key(|e| e.file_name())` reallocates an `OsString` on
/// every comparison, which is expensive on large game folders).
pub(super) fn read_dir_entries(
    file_system: &dyn InstallTreeFileSystem,
    path: &Path,
) -> AppResult<Vec<AppResult<InstallTreeDirectoryEntry>>> {
    let entries = file_system.read_directory(path).map_err(|error| {
        detection_context_error(
            format_args!("could not read directory {}", path.display()),
            error,
        )
    })?;

    Ok(entries
        .into_iter()
        .map(|entry| {
            entry.map_err(|error| {
                detection_context_error(
                    format_args!("could not enumerate directory {}", path.display()),
                    error,
                )
            })
        })
        .collect())
}
