use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::model::{SnapshotV2, V2DiskObservation, digest_bytes, matches_regular_digest};
use crate::ServiceError;

/// No-follow structural observation. A symlink/reparse point is non-regular
/// even when its target is a readable regular file, and is never acceptable
/// for a v2 mutation. The narrow post-check race remains an explicit hostile
/// residual.
pub(crate) fn observe(path: &Path) -> V2DiskObservation {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return V2DiskObservation::Absent,
        Err(_) => return V2DiskObservation::Unreadable,
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() || is_reparse(&metadata)
    {
        return V2DiskObservation::NonRegular;
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return V2DiskObservation::Unreadable,
    };
    V2DiskObservation::Regular {
        digest: digest_bytes(&bytes),
    }
}

pub(super) fn snapshot_preimage(
    transaction_dir: &Path,
    index: usize,
    path: &Path,
    current: &V2DiskObservation,
) -> Result<Option<String>, ServiceError> {
    match current {
        V2DiskObservation::Regular { .. } => {
            let snapshot = transaction_dir.join(format!("{index}.before"));
            crate::fs::copy_file_atomically(path, &snapshot)?;
            crate::fs::sync_parent_directory_best_effort(&snapshot);
            #[cfg(test)]
            if super::test_support::take_corrupt_next_preimage_snapshot(path) {
                fs::write(&snapshot, b"test-preimage-mismatch").map_err(|error| {
                    crate::failed(format!(
                        "failed to corrupt v2 preimage snapshot for test {}: {error}",
                        snapshot.display()
                    ))
                })?;
            }
            if observe(&snapshot) != *current {
                return Err(crate::failed(format!(
                    "v2 preimage snapshot does not match original observation at {}",
                    path.display()
                )));
            }
            if observe(path) != *current {
                return Err(crate::failed(format!(
                    "v2 target changed while snapshotting {}",
                    path.display()
                )));
            }
            Ok(Some(snapshot.to_string_lossy().into_owned()))
        }
        V2DiskObservation::Absent => Ok(None),
        V2DiskObservation::NonRegular | V2DiskObservation::Unreadable => Err(crate::failed(
            "v2 operation was prepared with an unsafe target observation",
        )),
    }
}

/// Reads the transaction artifact and confirms the published manifest still
/// describes its exact bytes before any forward write can touch a live target.
pub(super) fn read_verified_payload(
    transaction_dir: &Path,
    index: usize,
    post_digest: &str,
) -> Result<Vec<u8>, ServiceError> {
    let payload = transaction_dir.join(format!("{index}.payload"));
    if !matches_regular_digest(&observe(&payload), post_digest) {
        return Err(crate::failed(format!(
            "v2 payload digest does not match its prepared manifest at {}",
            payload.display()
        )));
    }
    let bytes = fs::read(&payload).map_err(|error| {
        crate::failed(format!(
            "failed to read v2 payload {}: {error}",
            payload.display()
        ))
    })?;
    if digest_bytes(&bytes) != post_digest {
        return Err(crate::failed(format!(
            "v2 payload digest does not match its prepared manifest at {}",
            payload.display()
        )));
    }
    Ok(bytes)
}

pub(super) fn write_forward(
    path: &Path,
    expected: &V2DiskObservation,
    bytes: &[u8],
) -> Result<(), ServiceError> {
    match expected {
        V2DiskObservation::Absent => write_new_no_clobber(path, bytes),
        V2DiskObservation::Regular { .. } => crate::fs::write_file_atomically(path, bytes),
        V2DiskObservation::NonRegular | V2DiskObservation::Unreadable => Err(crate::failed(
            "v2 operation was prepared with an unsafe target observation",
        )),
    }?;
    crate::fs::sync_parent_directory_best_effort(path);
    Ok(())
}

pub(super) fn restore_snapshot(snapshot: &SnapshotV2) -> Result<(), ServiceError> {
    let path = Path::new(&snapshot.path);
    #[cfg(test)]
    if super::test_support::take_fail_restore_snapshot(path) {
        return Err(crate::failed(format!(
            "test-injected v2 restore snapshot failure at {}",
            path.display()
        )));
    }
    match (&snapshot.before, &snapshot.snapshot) {
        (V2DiskObservation::Regular { .. }, Some(before)) => {
            let before = Path::new(before);
            if observe(before) != snapshot.before {
                return Err(crate::failed(format!(
                    "v2 preimage snapshot no longer matches its recorded observation at {}",
                    before.display()
                )));
            }
            crate::fs::copy_file_atomically(before, path)?;
            crate::fs::sync_parent_directory_best_effort(path);
            if observe(path) != snapshot.before {
                return Err(crate::failed(format!(
                    "v2 restored target does not match its recorded observation at {}",
                    path.display()
                )));
            }
            Ok(())
        }
        (V2DiskObservation::Absent, None) => {
            fs::remove_file(path).map_err(|error| match error.kind() {
                io::ErrorKind::NotFound => crate::failed("unreachable missing v2 rollback target"),
                _ => crate::failed(format!(
                    "failed to remove v2 rollback target {}: {error}",
                    path.display()
                )),
            })?;
            crate::fs::sync_parent_directory_best_effort(path);
            if observe(path) != V2DiskObservation::Absent {
                return Err(crate::failed(format!(
                    "v2 restored target does not remain absent at {}",
                    path.display()
                )));
            }
            Ok(())
        }
        _ => Err(crate::failed("v2 manifest has an invalid preimage")),
    }
}

pub(super) fn write_new_no_clobber(path: &Path, bytes: &[u8]) -> Result<(), ServiceError> {
    let parent = path
        .parent()
        .ok_or_else(|| crate::failed("v2 target has no parent"))?;
    let staged = staged_sibling_path(path)?;
    let empty = V2DiskObservation::Regular {
        digest: digest_bytes(&[]),
    };
    let mut reservation_created = false;
    let result = (|| {
        let mut staged_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)
            .map_err(|error| {
                crate::failed(format!(
                    "failed to stage {} without clobbering: {error}",
                    staged.display()
                ))
            })?;
        staged_file.write_all(bytes).map_err(|error| {
            crate::failed(format!("failed to stage {}: {error}", staged.display()))
        })?;
        staged_file.sync_all().map_err(|error| {
            crate::failed(format!(
                "failed to flush staged {}: {error}",
                staged.display()
            ))
        })?;
        drop(staged_file);

        let reservation = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| {
                crate::failed(format!(
                    "failed to create {} without clobbering: {error}",
                    path.display()
                ))
            })?;
        // `create_new` is the point at which this transaction exclusively owns
        // an empty target. Record that authority before any fallible flush so
        // cleanup can remove exactly that reservation if the flush fails.
        reservation_created = true;
        #[cfg(test)]
        if super::test_support::take_fail_reservation_flush(path) {
            return Err(crate::failed("test-injected v2 reservation flush failure"));
        }
        #[cfg(test)]
        if super::test_support::take_drift_after_absent_reservation(path) {
            drop(reservation);
            fs::write(path, b"foreign-reservation").map_err(|error| {
                crate::failed(format!(
                    "failed to inject foreign reservation drift at {}: {error}",
                    path.display()
                ))
            })?;
            return Err(crate::failed("test-injected v2 reservation drift failure"));
        }
        reservation.sync_all().map_err(|error| {
            crate::failed(format!(
                "failed to flush empty v2 reservation {}: {error}",
                path.display()
            ))
        })?;
        drop(reservation);
        if observe(path) != empty {
            return Err(crate::failed(format!(
                "v2 empty reservation drifted before publish {}",
                path.display()
            )));
        }
        #[cfg(test)]
        if super::test_support::take_fail_after_absent_reservation(path) {
            return Err(crate::failed("test-injected absent v2 publish failure"));
        }
        fs::rename(&staged, path).map_err(|error| {
            crate::failed(format!(
                "failed to atomically publish staged v2 file {}: {error}",
                path.display()
            ))
        })?;
        crate::fs::sync_directory_best_effort(parent);
        Ok(())
    })();
    if result.is_err() {
        if reservation_created && observe(path) == empty {
            match fs::remove_file(path) {
                Ok(()) => crate::fs::sync_directory_best_effort(parent),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    log::warn!(
                        "failed to remove unchanged v2 empty reservation {}: {error}",
                        path.display()
                    );
                }
            }
        }
        match fs::remove_file(&staged) {
            Ok(()) => crate::fs::sync_directory_best_effort(parent),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                log::warn!(
                    "failed to remove staged v2 file {}: {error}",
                    staged.display()
                );
            }
        }
    }
    result
}

fn staged_sibling_path(path: &Path) -> Result<PathBuf, ServiceError> {
    let parent = path
        .parent()
        .ok_or_else(|| crate::failed("v2 target has no parent"))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| crate::failed("v2 target has no file name"))?;
    Ok(parent.join(format!(
        ".{name}.renderpilot-v2-{}.staged",
        ulid::Ulid::generate()
    )))
}

#[cfg(windows)]
fn is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse(_metadata: &fs::Metadata) -> bool {
    false
}
