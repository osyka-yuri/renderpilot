//! Generic cache transactions and corrupt-file diagnostics.
//!
//! A cache read, validation, quarantine, and subsequent publication must be
//! serialized across processes. This module owns that protocol independently of
//! any provider or document format.

use std::{fs, path::Path};

use crate::ServiceError;

mod contract;
mod lease;
mod observation;
mod publication;

use observation::{cache_churn_error, read_cache_file_locked, stable_cache_snapshot};
#[cfg(not(target_os = "linux"))]
use publication::CacheRetirement;
#[cfg(windows)]
use publication::publish_after_exact_retirement;
use publication::quarantine_snapshot_at_locked;
#[cfg(all(windows, test))]
use publication::{CachePublicationTestHook, inject_cache_publication_test_hook};
#[cfg(all(test, target_os = "linux"))]
use publication::{
    LinuxCacheConflictTestHook, cache_linux_conflict_test_after_snapshot_proof,
    inject_linux_cache_conflict_test_hook,
};

pub(crate) use lease::with_cache_file_transaction;

use contract::CacheGenerationState;
pub(crate) use contract::{
    CacheGeneration, CacheObservation, CachePublication, MatchingCurrentPolicy,
};

/// Keep enough failed documents to compare a recent sequence without allowing a
/// repeatedly corrupt cache to consume unbounded disk space.
const MAX_CORRUPT_DIAGNOSTICS: usize = 3;
const MAX_CORRUPT_DIAGNOSTIC_BYTES: usize = 1024 * 1024;
const CACHE_CHURN_RETRIES: usize = 2;

/// Captures the active cache bytes and generation under the short cache lease.
/// The validation callback runs while that lease is held so an invalid observed
/// value can be quarantined before another publisher is allowed to proceed.
/// Network work must happen only after this function returns.
pub(crate) fn observe_cache_file<T, F>(
    path: &Path,
    validate: F,
) -> Result<CacheObservation<T>, ServiceError>
where
    F: FnOnce(&[u8], &fs::Metadata) -> Result<T, ServiceError>,
{
    with_cache_file_transaction(path, || {
        let Some(snapshot) = read_cache_file_locked(path)? else {
            return Ok(CacheObservation::Absent {
                generation: CacheGeneration(CacheGenerationState::Absent),
            });
        };
        match validate(&snapshot.bytes, &snapshot.metadata) {
            Ok(value) => Ok(CacheObservation::Valid {
                generation: snapshot.generation,
                value,
            }),
            Err(error) => {
                let generation = snapshot.generation.clone();
                quarantine_snapshot_at_locked(path, snapshot)?;
                Ok(CacheObservation::Invalid { generation, error })
            }
        }
    })
}

/// Rechecks the cache generation and publishes a fetched candidate only when
/// the originally observed value is still current. A newer valid cache wins.
/// On Windows and development fallbacks, a newer absent or invalid cache is
/// repaired by quarantining it (when present) and publishing the
/// already-validated fetched candidate under this same short lease. Linux
/// preserves every present occupant without classifying, quarantining, or
/// replacing it, because pathname replacement cannot prove ownership of a late
/// successor.
///
/// `MatchingCurrentPolicy::PreserveCurrent` is for callers whose existing
/// contract treats equal serialized bytes as a no-op. CDN refreshes use
/// `RefreshCandidate` so a successful fetch refreshes its TTL even for
/// identical bytes; catalog activation preserves an equal authoritative
/// snapshot's mtime.
pub(crate) fn commit_cache_candidate<T, F>(
    path: &Path,
    observed_generation: &CacheGeneration,
    candidate_bytes: &[u8],
    matching_current_policy: MatchingCurrentPolicy,
    mut validate_current: F,
) -> Result<CachePublication<T>, ServiceError>
where
    F: FnMut(&[u8]) -> Result<T, ServiceError>,
{
    with_cache_file_transaction(path, || {
        #[cfg(target_os = "linux")]
        let _ = (
            observed_generation,
            matching_current_policy,
            &mut validate_current,
        );
        #[cfg(not(target_os = "linux"))]
        let mut retained_rejected_generation = None;
        for _ in 0..=CACHE_CHURN_RETRIES {
            let current = stable_cache_snapshot(path)?;
            let Some(current) = current else {
                match super::atomic::write_file_atomically_no_replace(path, candidate_bytes)? {
                    super::atomic::NoReplaceWrite::Published => {
                        return Ok(CachePublication::Published);
                    }
                    super::atomic::NoReplaceWrite::Occupied => {
                        #[cfg(not(target_os = "linux"))]
                        {
                            retained_rejected_generation = None;
                        }
                        continue;
                    }
                }
            };

            // Every present snapshot keeps its exact open owner alive through
            // classification. Linux must never replace that pathname after a
            // generation proof because an external winner could arrive before
            // a replacement rename.
            current.owner.retain();
            #[cfg(target_os = "linux")]
            {
                return Ok(CachePublication::PreservedUnclassified);
            }

            #[cfg(not(target_os = "linux"))]
            {
                if retained_rejected_generation
                    .as_ref()
                    .is_some_and(|generation| *generation == current.generation)
                {
                    #[cfg(windows)]
                    {
                        match publish_after_exact_retirement(path, current, candidate_bytes)? {
                            super::atomic::NoReplaceWrite::Published => {
                                return Ok(CachePublication::Published);
                            }
                            super::atomic::NoReplaceWrite::Occupied => {
                                retained_rejected_generation = None;
                                continue;
                            }
                        }
                    }
                    #[cfg(all(
                        not(any(windows, target_os = "linux")),
                        feature = "development-host-fallback"
                    ))]
                    {
                        if snapshot_still_current(path, &current)? {
                            crate::fs::write_file_atomically(path, candidate_bytes)?;
                            return Ok(CachePublication::Published);
                        }
                        retained_rejected_generation = None;
                        continue;
                    }
                }

                if current.generation == *observed_generation {
                    if matching_current_policy == MatchingCurrentPolicy::PreserveCurrent
                        && current.bytes == candidate_bytes
                    {
                        return validate_current(&current.bytes).map(CachePublication::Current);
                    }
                    #[cfg(windows)]
                    {
                        match publish_after_exact_retirement(path, current, candidate_bytes)? {
                            super::atomic::NoReplaceWrite::Published => {
                                return Ok(CachePublication::Published);
                            }
                            super::atomic::NoReplaceWrite::Occupied => continue,
                        }
                    }
                    #[cfg(all(
                        not(any(windows, target_os = "linux")),
                        feature = "development-host-fallback"
                    ))]
                    {
                        if snapshot_still_current(path, &current)? {
                            crate::fs::write_file_atomically(path, candidate_bytes)?;
                            return Ok(CachePublication::Published);
                        }
                        continue;
                    }
                }

                match validate_current(&current.bytes) {
                    Ok(value) => return Ok(CachePublication::Current(value)),
                    Err(error) => {
                        log::debug!(
                            "cache CAS: current cache `{}` became invalid while fetching; replacing it: {error}",
                            path.display()
                        );
                        let generation = current.generation.clone();
                        retained_rejected_generation =
                            match quarantine_snapshot_at_locked(path, current)? {
                                #[cfg(windows)]
                                CacheRetirement::Retired => None,
                                CacheRetirement::Retained => Some(generation),
                            };
                    }
                }
            }
        }
        Err(cache_churn_error(path))
    })
}

#[cfg(test)]
mod tests;
