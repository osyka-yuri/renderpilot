//! Provider-neutral cache observation and publication contract.

use crate::ServiceError;

/// Opaque identity for one cache-path observation. Consumers retain it solely
/// as the capability required to publish a fetched candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CacheGeneration(pub(super) CacheGenerationState);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CacheGenerationState {
    Absent,
    Present {
        file_identity: CacheFileIdentity,
        length: u64,
        sha256: [u8; 32],
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CacheFileIdentity {
    #[cfg(windows)]
    Windows {
        volume_serial_number: u32,
        file_index: u64,
    },
    #[cfg(target_os = "linux")]
    Linux { device: u64, inode: u64 },
    #[cfg(all(
        not(any(windows, target_os = "linux")),
        feature = "development-host-fallback"
    ))]
    /// Unsupported development fallback. The digest and length are carried by
    /// a present generation; mtime is the extra replacement signal.
    MtimeFallback {
        modified: Option<std::time::SystemTime>,
    },
}

/// A cache value and the generation it came from. Invalid values are already
/// quarantined while the lease is held, so a later publisher cannot move a
/// concurrent valid replacement aside.
#[derive(Debug)]
pub(crate) enum CacheObservation<T> {
    Absent {
        generation: CacheGeneration,
    },
    Valid {
        generation: CacheGeneration,
        value: T,
    },
    Invalid {
        generation: CacheGeneration,
        error: ServiceError,
    },
}

impl<T> CacheObservation<T> {
    pub(crate) fn generation(&self) -> &CacheGeneration {
        match self {
            Self::Absent { generation }
            | Self::Valid { generation, .. }
            | Self::Invalid { generation, .. } => generation,
        }
    }
}

/// The platform-neutral result of a generation-aware cache publication.
///
/// `PreservedUnclassified` means the platform could prove only that a present
/// pathname must not be touched. It deliberately carries no value: callers
/// must make their existing durable-state decision explicitly.
#[derive(Debug)]
pub(crate) enum CachePublication<T> {
    /// The fetched candidate was atomically written.
    Published,
    /// Another process published a valid value first. Its bytes and mtime were
    /// left untouched and this is the value the caller must return.
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    Current(T),
    /// A present pathname must not be classified or replaced by this commit.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    PreservedUnclassified,
}

impl<T> CachePublication<T> {
    #[cfg(test)]
    pub(crate) fn published(&self) -> bool {
        matches!(self, Self::Published)
    }

    pub(crate) fn into_candidate_or_current(self, candidate: T) -> T {
        match self {
            Self::Current(current) => current,
            Self::Published | Self::PreservedUnclassified => candidate,
        }
    }
}

/// States whether an already matching authoritative cache entry must keep its
/// mtime or be refreshed by the validated candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MatchingCurrentPolicy {
    PreserveCurrent,
    RefreshCandidate,
}
