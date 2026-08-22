//! Replace-existing atomic write and copy primitives.

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

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

/// Publishes an already synced, same-parent stage file by native atomic
/// replacement. The stage pathname is consumed only after the rename succeeds.
pub(crate) fn publish_staged_replace(stage: &Path, destination: &Path) -> Result<(), ServiceError> {
    replace_with_temp_file(stage, destination)
}

/// Moves a file without replacing an existing destination. Supported
/// production hosts use their native no-replace rename, so this works on
/// filesystems that do not support hard links.
pub(crate) fn move_file_no_replace(source: &Path, destination: &Path) -> Result<(), ServiceError> {
    #[cfg(windows)]
    return super::windows::move_windows_no_replace(source, destination);

    #[cfg(target_os = "linux")]
    return super::linux::move_linux_no_replace(source, destination);

    #[cfg(all(
        not(any(windows, target_os = "linux")),
        feature = "development-host-fallback"
    ))]
    {
        fs::hard_link(source, destination).map_err(|error| {
            crate::failed(format!(
                "failed to claim no-replace destination `{}` for `{}`: {error}",
                destination.display(),
                source.display()
            ))
        })?;
        fs::remove_file(source).map_err(|error| {
            crate::failed(format!(
                "failed to remove source `{}` after no-replace destination claim: {error}",
                source.display()
            ))
        })
    }
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

pub(super) fn temporary_file_path(path: &Path, marker: &str) -> PathBuf {
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
mod staged_tomb_tests {
    use super::*;

    #[test]
    fn staged_tomb_move_is_no_replace() {
        let directory = tempfile::tempdir().expect("tempdir");
        let live = directory.path().join("live.bin");
        let tomb = directory.path().join("tomb.bin");
        fs::write(&live, b"live").expect("live");
        fs::write(&tomb, b"foreign").expect("tomb");

        assert!(move_file_no_replace(&live, &tomb).is_err());
        assert_eq!(fs::read(&live).expect("live remains"), b"live");
        assert_eq!(fs::read(&tomb).expect("tomb remains"), b"foreign");
    }

    #[test]
    fn staged_tomb_move_consumes_the_live_name() {
        let directory = tempfile::tempdir().expect("tempdir");
        let live = directory.path().join("live.bin");
        let tomb = directory.path().join("tomb.bin");
        fs::write(&live, b"live").expect("live");

        move_file_no_replace(&live, &tomb).expect("move");
        assert!(!live.exists());
        assert_eq!(fs::read(&tomb).expect("tomb"), b"live");
    }
}
