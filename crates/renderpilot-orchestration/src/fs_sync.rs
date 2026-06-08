//! Filesystem durability helpers shared across orchestration features.
//!
//! Writing a file with `fs::copy` / `fs::write` only schedules the data for the
//! OS page cache; a crash or power loss before the cache is flushed can leave a
//! torn or zero-length file on disk. To make a write durable, two things must be
//! flushed: the file's *contents* (`File::sync_all`) and the *directory entry*
//! that points at it (an `fsync` on the parent directory). These helpers provide
//! both, with the Windows-specific dance required to open a directory handle.

use std::io;
use std::path::Path;

/// Flushes a file's contents to durable storage.
///
/// Opens the already-written `path` for **write** — `sync_all` maps to
/// `FlushFileBuffers` on Windows, which requires a write handle and fails with
/// `ERROR_ACCESS_DENIED` on a read-only one. Returns the underlying I/O error so
/// callers can decide whether a durability failure should abort the operation.
pub(crate) fn sync_file(path: &Path) -> io::Result<()> {
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)?
        .sync_all()
}

/// Best-effort flush of the directory entry for `path` (i.e. its parent).
///
/// Renames and freshly created files only become durable once the *directory*
/// is fsynced. Failures are swallowed: the data write itself has already been
/// made durable by [`sync_file`], and a parent-dir sync failure must not turn an
/// otherwise-successful operation into an error.
pub(crate) fn sync_parent_directory_best_effort(path: &Path) {
    if let Some(parent) = path.parent() {
        sync_directory_best_effort(parent);
    }
}

#[cfg(not(windows))]
pub(crate) fn sync_directory_best_effort(path: &Path) {
    if let Ok(dir) = std::fs::File::open(path) {
        let _ = dir.sync_all();
    }
}

#[cfg(windows)]
pub(crate) fn sync_directory_best_effort(path: &Path) {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    // FILE_FLAG_BACKUP_SEMANTICS — required to open a directory handle on Windows
    // so that `sync_all` (FlushFileBuffers) can be issued against it.
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    if let Ok(dir) = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
    {
        let _ = dir.sync_all();
    }
}
