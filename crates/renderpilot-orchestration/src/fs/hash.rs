//! Non-empty regular-file SHA-256 verification.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use renderpilot_domain::Sha256Hash;

use crate::ServiceError;

/// Error returned by [`sha256_of_non_empty_file`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NonEmptyFileError {
    /// `std::fs::metadata` failed (missing, permission denied, ...).
    Unreadable { path: PathBuf, detail: String },
    /// The path exists but is not a regular file.
    NotAFile(PathBuf),
    /// The path is a regular file but is empty.
    Empty(PathBuf),
    /// The file was a valid non-empty regular file but hashing failed.
    HashFailed(String),
}

impl fmt::Display for NonEmptyFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable { path, detail } => {
                write!(formatter, "cannot read `{}`: {detail}", path.display())
            }
            Self::NotAFile(path) => {
                write!(formatter, "`{}` is not a regular file", path.display())
            }
            Self::Empty(path) => write!(formatter, "`{}` is empty", path.display()),
            Self::HashFailed(detail) => write!(formatter, "hashing failed: {detail}"),
        }
    }
}

impl std::error::Error for NonEmptyFileError {}

impl From<NonEmptyFileError> for ServiceError {
    fn from(error: NonEmptyFileError) -> Self {
        crate::failed(error.to_string())
    }
}

/// Verifies that `path` is a non-empty regular file and returns its SHA-256 hash.
///
/// This is the single hashing entry point for non-empty file verification.
/// Callers must not reimplement the metadata -> `is_file` + `len > 0` -> hash
/// sequence; map [`NonEmptyFileError`] into domain vocabulary at the call site
/// (`BaselineConflict`, repair-required `ServiceError`, optional missing sidecar,
/// ...). Thin wrappers that only perform that mapping are intentional.
pub(crate) fn sha256_of_non_empty_file(path: &Path) -> Result<Sha256Hash, NonEmptyFileError> {
    let metadata = fs::metadata(path).map_err(|error| NonEmptyFileError::Unreadable {
        path: path.to_path_buf(),
        detail: error.to_string(),
    })?;
    if !metadata.is_file() {
        return Err(NonEmptyFileError::NotAFile(path.to_path_buf()));
    }
    if metadata.len() == 0 {
        return Err(NonEmptyFileError::Empty(path.to_path_buf()));
    }
    renderpilot_detection::sha256_file(path)
        .map_err(|error| NonEmptyFileError::HashFailed(error.to_string()))
}
