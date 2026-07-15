//! Content-durable write and copy (temp file + sync + atomic rename).

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ServiceError;

/// Writes `bytes` to `path` content-durably: into a temp file (synced), then an
/// atomic rename over the destination, creating parent directories as needed.
///
/// Does **not** fsync the parent directory -- that is a separate, batched
/// [`crate::fs::sync_directory_best_effort`] callers invoke once per group
/// of writes.
pub(crate) fn write_file_atomically(path: &Path, bytes: &[u8]) -> Result<(), ServiceError> {
    let parent = path.parent().ok_or_else(|| {
        crate::failed(format!(
            "cannot write file `{}` because it has no parent directory",
            path.display()
        ))
    })?;

    fs::create_dir_all(parent).map_err(|error| {
        crate::failed(format!(
            "failed to create directory `{}`: {error}",
            parent.display()
        ))
    })?;

    let temp_path = temporary_file_path(path, "tmp");
    write_temp_file(&temp_path, bytes)?;

    replace_with_temp_file(&temp_path, path)
}

/// Copies `source` onto `dest` content-durably and crash-atomically: streams into a
/// temp file **in the destination directory** (synced), then atomically replaces
/// `dest`. Unlike a bare `fs::copy`, a crash can never leave a torn/partial file at
/// `dest` (it is always either the old file or the complete new one). Creates parent
/// directories as needed; a no-op when `source` and `dest` are the same file.
///
/// Does **not** fsync the parent directory -- that is a separate, batched
/// [`crate::fs::sync_directory_best_effort`] callers invoke once per group
/// of writes.
pub(crate) fn copy_file_atomically(source: &Path, dest: &Path) -> Result<(), ServiceError> {
    // Copying a file onto itself would destroy it through the temp/replace dance.
    if is_same_file(source, dest) {
        return Ok(());
    }

    let parent = dest.parent().ok_or_else(|| {
        crate::failed(format!(
            "cannot copy onto `{}` because it has no parent directory",
            dest.display()
        ))
    })?;

    fs::create_dir_all(parent).map_err(|error| {
        crate::failed(format!(
            "failed to create directory `{}`: {error}",
            parent.display()
        ))
    })?;

    // The temp lives in the destination directory so the replace stays a
    // same-volume (atomic) rename rather than a cross-volume copy.
    let temp_path = temporary_file_path(dest, "copy");
    copy_into_temp(source, &temp_path)?;

    replace_with_temp_file(&temp_path, dest)
}

/// Streams `source` into a freshly created (never-clobbered) temp file and flushes
/// it to durable storage, removing the temp on any failure.
fn copy_into_temp(source: &Path, temp_path: &Path) -> Result<(), ServiceError> {
    let mut reader = fs::File::open(source).map_err(|error| {
        crate::failed(format!(
            "failed to open source file `{}`: {error}",
            source.display()
        ))
    })?;

    let mut temp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_path)
        .map_err(|error| {
            crate::failed(format!(
                "failed to create temporary file `{}`: {error}",
                temp_path.display()
            ))
        })?;

    io::copy(&mut reader, &mut temp_file).map_err(|error| {
        let _ = fs::remove_file(temp_path);
        crate::failed(format!(
            "failed to copy `{}` into `{}`: {error}",
            source.display(),
            temp_path.display()
        ))
    })?;

    temp_file.sync_all().map_err(|error| {
        let _ = fs::remove_file(temp_path);
        crate::failed(format!(
            "failed to flush temporary file `{}`: {error}",
            temp_path.display()
        ))
    })?;

    Ok(())
}

/// Whether two paths resolve to the same existing file. Conservatively `false` when
/// either path cannot be canonicalized (e.g. `dest` does not exist yet -- the common
/// install case), so a real copy proceeds.
fn is_same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn write_temp_file(temp_path: &Path, bytes: &[u8]) -> Result<(), ServiceError> {
    let mut temp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_path)
        .map_err(|error| {
            crate::failed(format!(
                "failed to create temporary file `{}`: {error}",
                temp_path.display()
            ))
        })?;

    temp_file.write_all(bytes).map_err(|error| {
        let _ = fs::remove_file(temp_path);
        crate::failed(format!(
            "failed to write temporary file `{}`: {error}",
            temp_path.display()
        ))
    })?;

    temp_file.sync_all().map_err(|error| {
        let _ = fs::remove_file(temp_path);
        crate::failed(format!(
            "failed to flush temporary file `{}`: {error}",
            temp_path.display()
        ))
    })?;

    Ok(())
}

/// Atomically replaces `destination_path` with the freshly written `temp_path` via a
/// same-directory rename. `std::fs::rename` replaces an existing destination on every
/// supported platform -- on Windows it maps to `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`
/// -- so this overwrites cleanly. Rename *durability* is the caller's separate
/// [`crate::fs::sync_directory_best_effort`] step, not a per-rename flush.
fn replace_with_temp_file(temp_path: &Path, destination_path: &Path) -> Result<(), ServiceError> {
    fs::rename(temp_path, destination_path).map_err(|error| {
        let _ = fs::remove_file(temp_path);
        crate::failed(format!(
            "failed to move temporary file `{}` to `{}`: {error}",
            temp_path.display(),
            destination_path.display()
        ))
    })
}

fn temporary_file_path(path: &Path, marker: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    path.with_file_name(format!(
        "{file_name}.{marker}-{}-{timestamp}",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_existing_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("payload.bin");
        fs::write(&path, b"old").expect("seed file");

        write_file_atomically(&path, b"new").expect("replace file");

        assert_eq!(fs::read(&path).expect("read replaced file"), b"new");
    }

    #[test]
    fn copy_file_atomically_replaces_existing_dest() {
        let dir = tempfile::tempdir().expect("temp dir");
        let source = dir.path().join("src.bin");
        let dest = dir.path().join("dest.bin");
        fs::write(&source, b"new-content").expect("seed source");
        fs::write(&dest, b"old").expect("seed dest");

        copy_file_atomically(&source, &dest).expect("atomic copy");

        assert_eq!(fs::read(&dest).expect("read dest"), b"new-content");
    }

    #[test]
    fn copy_file_atomically_failure_leaves_dest_unchanged() {
        let dir = tempfile::tempdir().expect("temp dir");
        let dest = dir.path().join("dest.bin");
        fs::write(&dest, b"original").expect("seed dest");
        let missing = dir.path().join("does-not-exist.bin");

        let result = copy_file_atomically(&missing, &dest);

        assert!(result.is_err(), "copying a missing source must fail");
        assert_eq!(
            fs::read(&dest).expect("read dest"),
            b"original",
            "a failed copy must leave the existing destination untouched"
        );
    }

    #[test]
    fn copy_file_atomically_is_a_noop_for_the_same_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("f.bin");
        fs::write(&path, b"data").expect("seed file");

        copy_file_atomically(&path, &path).expect("same-file no-op");

        assert_eq!(fs::read(&path).expect("read file"), b"data");
    }
}
