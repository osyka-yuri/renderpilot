//! Sidecar verify / create / restore against the live filesystem.

use std::fmt;
use std::fs;
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

/// Creates a baseline sidecar by crash-atomically copying `live` onto
/// `sidecar`.
///
/// Thin named alias for [`copy_file_atomically`] so call sites declare the
/// intent ("create a `.bak`") at the call site rather than via a comment.
/// Pre- and post-conditions (live-hash verification, sidecar-absence checks,
/// post-copy re-hash) stay with each engine.
pub(crate) fn create_sidecar(live: &Path, sidecar: &Path) -> Result<(), ServiceError> {
    copy_file_atomically(live, sidecar)
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
