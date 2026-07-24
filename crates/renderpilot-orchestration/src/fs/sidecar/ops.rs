//! Sidecar verify / create / restore against the live filesystem.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use renderpilot_domain::Sha256Hash;

use crate::ServiceError;
use crate::fs::{NonEmptyFileError, copy_file_atomically, sha256_of_non_empty_file};

/// Failure to verify a sidecar (`.bak`) file against an expected baseline hash.
///
/// Returned by [`verify_sidecar`]. Each calling engine maps the variants to its
/// own error type so that domain-specific error messages stay at the call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SidecarVerifyError {
    /// `std::fs::metadata` failed (missing, permission denied, ...).
    Unreadable { path: PathBuf, detail: String },
    /// The path exists but is not a regular file.
    NotAFile(PathBuf),
    /// The sidecar is an empty file -- a baseline must carry real bytes.
    Empty(PathBuf),
    /// The sidecar's SHA-256 does not match the expected baseline hash.
    HashMismatch {
        path: PathBuf,
        expected: Sha256Hash,
        actual: Sha256Hash,
    },
}

impl fmt::Display for SidecarVerifyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable { path, detail } => {
                write!(
                    formatter,
                    "cannot read sidecar `{}`: {detail}",
                    path.display()
                )
            }
            Self::NotAFile(path) => {
                write!(
                    formatter,
                    "sidecar `{}` is not a regular file",
                    path.display()
                )
            }
            Self::Empty(path) => write!(formatter, "sidecar `{}` is empty", path.display()),
            Self::HashMismatch {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "sidecar hash mismatch for `{}`: expected {expected}, got {actual}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for SidecarVerifyError {}

/// Verifies that the sidecar at `path` is a non-empty regular file whose
/// SHA-256 matches `expected`.
///
/// Consolidates the `sha256_file` + compare pattern shared by
/// `coordinated_files::execute_file_plan` and `catalog::execute::fs_ops`.
/// Callers map [`SidecarVerifyError`] to their own error type so that
/// domain-specific messages remain at the call site.
pub(crate) fn verify_sidecar(path: &Path, expected: &Sha256Hash) -> Result<(), SidecarVerifyError> {
    let actual = sha256_of_non_empty_file(path).map_err(|error| match error {
        NonEmptyFileError::Unreadable { path, detail } => {
            SidecarVerifyError::Unreadable { path, detail }
        }
        NonEmptyFileError::NotAFile(path) => SidecarVerifyError::NotAFile(path),
        NonEmptyFileError::Empty(path) => SidecarVerifyError::Empty(path),
        NonEmptyFileError::HashFailed(detail) => SidecarVerifyError::Unreadable {
            path: path.to_path_buf(),
            detail,
        },
    })?;
    if &actual != expected {
        return Err(SidecarVerifyError::HashMismatch {
            path: path.to_path_buf(),
            expected: expected.clone(),
            actual,
        });
    }
    Ok(())
}

/// Creates a baseline sidecar without ever replacing an existing path.
///
/// `create_new` makes path ownership atomic and fails when `sidecar` already
/// exists, enforcing the immutable capture-once contract even if another
/// process creates the path after preflight. Callers execute inside the durable
/// file transaction, whose absent-path snapshot removes an interrupted partial
/// creation during recovery.
pub(crate) fn create_sidecar(live: &Path, sidecar: &Path) -> Result<(), ServiceError> {
    sidecar.parent().ok_or_else(|| {
        crate::failed(format!(
            "cannot create sidecar `{}` because it has no parent directory",
            sidecar.display()
        ))
    })?;
    let mut source = fs::File::open(live).map_err(|error| {
        crate::failed(format!(
            "failed to open sidecar source `{}`: {error}",
            live.display()
        ))
    })?;
    let mut destination = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(sidecar)
        .map_err(|error| {
            crate::failed(format!(
                "failed to create immutable sidecar `{}`: {error}",
                sidecar.display()
            ))
        })?;
    let result = io::copy(&mut source, &mut destination)
        .and_then(|_| destination.sync_all())
        .map_err(|error| {
            crate::failed(format!(
                "failed to persist immutable sidecar `{}`: {error}",
                sidecar.display()
            ))
        });
    drop(destination);
    if result.is_ok() {
        crate::fs::sync_parent_directory_best_effort(sidecar);
    } else if let Err(error) = fs::remove_file(sidecar)
        && error.kind() != io::ErrorKind::NotFound
    {
        log::warn!(
            "failed to remove partial sidecar {}: {error}",
            sidecar.display()
        );
    }
    result
}

/// Restores `live` from `sidecar` crash-atomically, then removes the sidecar.
///
/// The copy runs first so a failure leaves the sidecar in place as a rollback
/// net. The sidecar is removed with `fs::remove_file` (strict -- a missing
/// sidecar at this point is an unexpected concurrent mutation, not a
/// benign no-op).
pub(crate) fn restore_from_sidecar(live: &Path, sidecar: &Path) -> Result<(), ServiceError> {
    copy_file_atomically(sidecar, live)?;
    fs::remove_file(sidecar).map_err(|error| {
        crate::failed(format!(
            "failed to remove restored baseline {}: {error}",
            sidecar.display()
        ))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_capture_is_complete_and_never_replaces_an_existing_backup() {
        let directory = tempfile::tempdir().expect("temp dir");
        let live = directory.path().join("runtime.dll");
        let sidecar = directory.path().join("runtime.dll.bak");
        fs::write(&live, b"original-runtime").expect("live");

        create_sidecar(&live, &sidecar).expect("first capture");
        assert_eq!(fs::read(&sidecar).expect("sidecar"), b"original-runtime");

        fs::write(&live, b"later-runtime").expect("changed live");
        assert!(
            create_sidecar(&live, &sidecar).is_err(),
            "capture-once must reject an existing path"
        );
        assert_eq!(
            fs::read(&sidecar).expect("immutable sidecar"),
            b"original-runtime"
        );
    }
}
