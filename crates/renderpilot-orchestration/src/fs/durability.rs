//! Directory-entry durability (parent-dir fsync).

use std::path::Path;

/// Best-effort flush of the directory entry for `path` (i.e. its parent).
///
/// Renames and freshly created files only become durable once the *directory* is
/// fsynced. Failures are swallowed: the data write has already been made durable by
/// [`crate::fs::write_file_atomically`], and a parent-dir sync failure must not
/// turn an otherwise-successful operation into an error.
pub(crate) fn sync_parent_directory_best_effort(path: &Path) {
    if let Some(parent) = path.parent() {
        sync_directory_best_effort(parent);
    }
}

#[cfg(not(windows))]
pub(crate) fn sync_directory(path: &Path) -> std::io::Result<()> {
    std::fs::File::open(path)?.sync_all()
}

#[cfg(not(windows))]
pub(crate) fn sync_directory_best_effort(path: &Path) {
    let _ = sync_directory(path);
}

#[cfg(windows)]
pub(crate) fn sync_directory(path: &Path) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    // FILE_FLAG_BACKUP_SEMANTICS -- required to open a directory handle on Windows
    // so that `sync_all` (FlushFileBuffers) can be issued against it.
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .and_then(|dir| dir.sync_all())
}

#[cfg(windows)]
pub(crate) fn sync_directory_best_effort(path: &Path) {
    let _ = sync_directory(path);
}
