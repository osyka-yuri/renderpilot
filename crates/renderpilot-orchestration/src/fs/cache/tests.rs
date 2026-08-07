use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier, mpsc};
use std::time::Duration;
#[cfg(windows)]
use std::time::Instant;

#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    FILE_GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

use tempfile::tempdir;

use super::*;

/// Reads and validates a cache under the same lease that quarantines a rejected
/// entry. A concurrent publisher therefore cannot replace the observed bytes in
/// the interval between validation and the no-replace move.
#[cfg(test)]
pub(crate) fn read_with_corrupt_quarantine<T, F>(path: &Path, read: F) -> Result<T, ServiceError>
where
    F: FnOnce() -> Result<T, ServiceError>,
{
    with_cache_file_transaction(path, || match read() {
        Ok(value) => Ok(value),
        Err(error) => {
            quarantine_at_locked(path);
            Err(error)
        }
    })
}

/// Publishes the captured bytes from the file at `path` to the first available immutable diagnostic slot
/// (`manifest.json.corrupt` through `manifest.json.corrupt.2`). Existing slots
/// are never replaced or removed. Windows and Linux use one native atomic
/// no-replace move, so no writer can claim a slot between an availability check
/// and the rename. The fallback for other development targets uses an atomic
/// `create_new` snapshot and leaves the active cache for the subsequent refresh.
///
/// An available slot receives only a complete, durable copy of the captured
/// rejected bytes; the active snapshot is retired only after that publication.
/// When every slot is occupied, the active cache is left in place for the
/// subsequent atomic refresh without creating another artifact.
#[cfg(test)]
fn quarantine_at(path: &Path) {
    if let Err(error) = with_cache_file_transaction(path, || {
        quarantine_at_locked(path);
        Ok(())
    }) {
        log::debug!(
            "cache quarantine: could not acquire the cache transaction for `{}`: {error}",
            path.display()
        );
    }
}

#[cfg(test)]
fn quarantine_at_locked(path: &Path) {
    match read_cache_file_locked(path) {
        Ok(Some(snapshot)) => {
            let _ = quarantine_snapshot_at_locked(path, snapshot);
        }
        Ok(None) => {}
        Err(error) => log::debug!(
            "cache quarantine: could not capture cache `{}`: {error}",
            path.display()
        ),
    }
}

fn parse_doc(bytes: &[u8]) -> Result<(), ServiceError> {
    if bytes == b"bad" {
        return Err(crate::failed("invalid doc"));
    }
    Ok(())
}

fn write_cache(dir: &Path, contents: &str) -> PathBuf {
    let path = dir.join("manifest.json");
    fs::write(&path, contents).expect("write cache");
    path
}

fn corrupt_sidecar(path: &Path, suffix: Option<u32>) -> PathBuf {
    let base = crate::fs::with_added_extension(path, "corrupt").expect("cache has a file name");
    match suffix {
        Some(suffix) => crate::fs::with_added_extension(&base, &suffix.to_string())
            .expect("quarantine has a file name"),
        None => base,
    }
}

fn corrupt_diagnostic_paths(path: &Path) -> Vec<PathBuf> {
    (0..MAX_CORRUPT_DIAGNOSTICS)
        .map(|slot| {
            if slot == 0 {
                corrupt_sidecar(path, None)
            } else {
                corrupt_sidecar(path, Some(slot as u32))
            }
        })
        .filter(|diagnostic| diagnostic.is_file())
        .collect()
}

fn owned_publication_temp_paths(path: &Path) -> Vec<PathBuf> {
    let parent = path.parent().expect("cache path has a parent");
    let prefix = format!(
        "{}.publish-{}-",
        corrupt_sidecar(path, None)
            .file_name()
            .expect("diagnostic path has a file name")
            .to_string_lossy(),
        std::process::id()
    );
    fs::read_dir(parent)
        .expect("read diagnostic parent")
        .map(|entry| entry.expect("read diagnostic entry").path())
        .filter(|candidate| {
            candidate
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(&prefix))
        })
        .collect()
}

#[cfg(windows)]
fn windows_owned_publication_temp_paths(destination: &Path) -> Vec<PathBuf> {
    let parent = destination.parent().expect("cache path has a parent");
    let prefix = format!(
        "{}.publish-{}-",
        destination
            .file_name()
            .expect("cache path has a file name")
            .to_string_lossy(),
        std::process::id()
    );
    fs::read_dir(parent)
        .expect("read cache parent")
        .map(|entry| entry.expect("read cache entry").path())
        .filter(|candidate| {
            candidate
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(&prefix))
        })
        .collect()
}

mod contract;
mod generation;
#[cfg(target_os = "linux")]
mod linux;
mod quarantine;
#[cfg(windows)]
mod windows;
