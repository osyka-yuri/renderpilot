//! Coordinated add-on file bindings shared with catalog baselines.
//!
//! Wire records may deserialize freely (including incomplete shapes). Domain
//! invariants are enforced when assembling an [`crate::InstalledAddon`] via
//! [`crate::InstalledAddon::try_with_managed_files`], which storage uses on load.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{PathRef, Sha256Hash};

/// Whether an add-on owns the current bytes at a coordinated game path or only
/// accepted an already suitable file without writing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedFileMode {
    /// The add-on wrote the current bytes and must unwind them on removal.
    Owned,
    /// A suitable file already existed and the add-on did not change it.
    Reused,
}

/// State of a coordinated path before the first owner replaced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ManagedFileBaseline {
    /// The path did not exist before the owner first wrote it.
    Absent,
    /// The path existed and its immutable sidecar must have this digest.
    Present {
        /// SHA-256 of the pre-mutation bytes.
        sha256: Sha256Hash,
    },
}

/// A file coordinated with another feature instead of being handled by the
/// generic add-on create/backup engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedAddonFile {
    path: PathRef,
    mode: ManagedFileMode,
    baseline: ManagedFileBaseline,
    installed_sha256: Sha256Hash,
}

impl ManagedAddonFile {
    /// Records a coordinated file whose current bytes were written by the add-on.
    #[must_use]
    pub fn owned(
        path: PathRef,
        baseline: ManagedFileBaseline,
        installed_sha256: Sha256Hash,
    ) -> Self {
        Self {
            path,
            mode: ManagedFileMode::Owned,
            baseline,
            installed_sha256,
        }
    }

    /// Records a suitable pre-existing file that the add-on accepted without
    /// changing it. A reused path necessarily existed, so its baseline and the
    /// accepted live hash are the same value.
    #[must_use]
    pub fn reused(path: PathRef, accepted_sha256: Sha256Hash) -> Self {
        Self {
            path,
            mode: ManagedFileMode::Reused,
            baseline: ManagedFileBaseline::Present {
                sha256: accepted_sha256.clone(),
            },
            installed_sha256: accepted_sha256,
        }
    }

    /// Returns the absolute coordinated path.
    #[must_use]
    pub fn path(&self) -> &PathRef {
        &self.path
    }

    /// Returns whether the add-on owns or merely reused the current file.
    #[must_use]
    pub fn mode(&self) -> ManagedFileMode {
        self.mode
    }

    /// Returns the state recorded before the first owned write.
    #[must_use]
    pub fn baseline(&self) -> &ManagedFileBaseline {
        &self.baseline
    }

    /// Returns the digest last installed or accepted by the add-on.
    #[must_use]
    pub fn installed_sha256(&self) -> &Sha256Hash {
        &self.installed_sha256
    }
}

/// A persisted add-on record violates coordinated-file ownership invariants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstalledAddonInvariantError {
    /// A reused binding cannot describe a path that was absent.
    ReusedFileHasAbsentBaseline(PathRef),
    /// A reused binding must identify the exact bytes it accepted.
    ReusedFileHashMismatch(PathRef),
    /// The same coordinated path was listed more than once.
    DuplicateManagedPath(PathRef),
    /// A coordinated path is also owned by the generic add-on file engine.
    ManagedPathOwnedByEngine(PathRef),
}

impl fmt::Display for InstalledAddonInvariantError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReusedFileHasAbsentBaseline(path) => write!(
                formatter,
                "reused managed file has an absent baseline: {path}"
            ),
            Self::ReusedFileHashMismatch(path) => write!(
                formatter,
                "reused managed file baseline does not match its accepted hash: {path}"
            ),
            Self::DuplicateManagedPath(path) => {
                write!(formatter, "managed file path is duplicated: {path}")
            }
            Self::ManagedPathOwnedByEngine(path) => write!(
                formatter,
                "managed file path is also tracked by the generic add-on engine: {path}"
            ),
        }
    }
}

impl std::error::Error for InstalledAddonInvariantError {}
