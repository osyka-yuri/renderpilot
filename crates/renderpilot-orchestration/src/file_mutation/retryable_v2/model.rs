use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ServiceError;

pub(super) const FORMAT_VERSION: u32 = 2;

/// A no-follow observation used as an operation's compare-and-swap token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum V2DiskObservation {
    Absent,
    Regular { digest: String },
    NonRegular,
    Unreadable,
}

impl V2DiskObservation {
    pub(super) fn can_mutate(&self) -> bool {
        matches!(self, Self::Absent | Self::Regular { .. })
    }
}

/// An entirely enumerated target operation. `expected` is captured before the
/// unlocked phase and must match immediately before the forward operation.
#[derive(Debug)]
pub(crate) enum RetryableFileOperation {
    Write {
        path: PathBuf,
        bytes: Vec<u8>,
        expected: V2DiskObservation,
    },
    Delete {
        path: PathBuf,
        expected: V2DiskObservation,
    },
}

impl RetryableFileOperation {
    pub(crate) fn path(&self) -> &Path {
        match self {
            Self::Write { path, .. } | Self::Delete { path, .. } => path,
        }
    }

    pub(super) fn expected(&self) -> &V2DiskObservation {
        match self {
            Self::Write { expected, .. } | Self::Delete { expected, .. } => expected,
        }
    }
}

/// Prevalidated, fully enumerated v2 operation set.
#[derive(Debug)]
pub(crate) struct RetryableFilePlan {
    pub(crate) operations: Vec<RetryableFileOperation>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct ManifestV2 {
    pub(super) format_version: u32,
    pub(super) roots: Vec<String>,
    pub(super) transaction_dir: String,
    pub(super) operations: Vec<ManifestOperationV2>,
    pub(super) snapshots: Vec<SnapshotV2>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum ManifestOperationV2 {
    Write {
        path: String,
        expected: V2DiskObservation,
        post_digest: String,
    },
    Delete {
        path: String,
        expected: V2DiskObservation,
    },
}

impl ManifestOperationV2 {
    pub(super) fn path(&self) -> &str {
        match self {
            Self::Write { path, .. } | Self::Delete { path, .. } => path,
        }
    }

    pub(super) fn expected(&self) -> &V2DiskObservation {
        match self {
            Self::Write { expected, .. } | Self::Delete { expected, .. } => expected,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct SnapshotV2 {
    pub(super) path: String,
    pub(super) before: V2DiskObservation,
    pub(super) snapshot: Option<String>,
}

pub(super) fn serialize(manifest: &ManifestV2) -> Result<String, ServiceError> {
    serde_json::to_string(manifest)
        .map_err(|error| crate::failed(format!("failed to serialize v2 file mutation: {error}")))
}

pub(super) fn digest_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub(super) fn matches_regular_digest(observation: &V2DiskObservation, digest: &str) -> bool {
    matches!(observation, V2DiskObservation::Regular { digest: actual } if actual == digest)
}

pub(super) fn is_sha256_digest(digest: &str) -> bool {
    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}
