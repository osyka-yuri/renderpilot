//! Lease-scoped file observation used by detection and persistence.
//!
//! A successful observation holds the file object while hashing and PE facts
//! are read. Reuse is deliberately stricter than observation: when the host
//! cannot supply a durable strong key, we still return the stable bytes but
//! publish no cache key.

use std::path::Path;

use renderpilot_application::AppResult;
use renderpilot_domain::Sha256Hash;

mod common;
#[cfg(not(windows))]
mod portable;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
use portable::{observe_system_file, probe_system_identity};
#[cfg(windows)]
use windows::{observe_system_file, probe_system_identity};

/// Persisted observation rows produced by older algorithms must never be
/// reused. The schema is intentionally unchanged; this revision is part of
/// the cache equality boundary.
pub const FILE_OBSERVATION_ALGORITHM_REVISION: u16 = 2;

/// A reusable, platform-specific cache key for one stable filesystem object.
///
/// This key is never synthesized from timestamps. Windows requires a complete
/// `FILE_ID_INFO` and an USN journal generation/token; unsupported filesystems
/// and journal failures are observed successfully but have no key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrongFileCacheKey {
    /// Platform-specific key format.
    pub kind: String,
    /// Stable object identity within the volume.
    pub object_identity: String,
    /// Generation/change token bound to the object identity.
    pub change_token: String,
    /// Byte length observed while the lease was held.
    pub size: u64,
}

/// Compatibility spelling retained for existing internal adapters.
pub use StrongFileCacheKey as StrongFileIdentity;

/// Bytes read from one stable object, with an optional reusable key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StableFileSnapshot {
    /// `Some` only when the platform supplied a durable strong cache key.
    pub cache_key: Option<StrongFileCacheKey>,
    /// SHA-256 of exactly these bytes.
    pub sha256: Sha256Hash,
    /// Exact bytes read while the object lease was held.
    pub bytes: Vec<u8>,
}

/// Result of a full stable observation attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileObservationResult {
    /// The path did not exist when observation began.
    Missing,
    /// I/O, sharing, or identity instability made the path unsafe to read.
    Unavailable,
    /// Fully stable bytes are available; `cache_key` can be `None`.
    Available(StableFileSnapshot),
}

/// Result of an identity-only reuse probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileIdentityProbeResult {
    /// The path did not exist when the lease was acquired.
    Missing,
    /// I/O or path identity instability made reuse unsafe for this scan.
    Unavailable,
    /// The file is safely observable but cannot provide a reusable key.
    Uncacheable,
    /// A stable durable key was observed without reading file contents.
    Available(StrongFileCacheKey),
}

/// Send+Sync boundary for filesystem observation. Tests can inject a source
/// that deterministically reports replacement, unavailability, or an
/// uncacheable successful full read.
pub trait FileObservationSource: Send + Sync {
    /// Reads one stable snapshot or reports a fail-closed unavailable outcome.
    fn observe(&self, path: &Path) -> AppResult<FileObservationResult>;

    /// Acquires and validates a reusable key without reading contents.
    ///
    /// Custom sources inherit a conservative implementation that may observe
    /// content. The system source overrides it so an exact strong hit performs
    /// no content read.
    fn probe_identity(&self, path: &Path) -> AppResult<FileIdentityProbeResult> {
        Ok(match self.observe(path)? {
            FileObservationResult::Missing => FileIdentityProbeResult::Missing,
            FileObservationResult::Unavailable => FileIdentityProbeResult::Unavailable,
            FileObservationResult::Available(snapshot) => match snapshot.cache_key {
                Some(cache_key) => FileIdentityProbeResult::Available(cache_key),
                None => FileIdentityProbeResult::Uncacheable,
            },
        })
    }
}

/// Platform source used by normal detection.
#[derive(Debug, Default)]
pub struct SystemFileObservationSource;

impl FileObservationSource for SystemFileObservationSource {
    fn observe(&self, path: &Path) -> AppResult<FileObservationResult> {
        observe_system_file(path)
    }

    fn probe_identity(&self, path: &Path) -> AppResult<FileIdentityProbeResult> {
        probe_system_identity(path)
    }
}
