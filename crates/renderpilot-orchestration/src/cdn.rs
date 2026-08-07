//! The public content-delivery host and generic JSON-manifest fetch policy.
//!
//! RenderPilot serves its manifests (the graphics-library catalogue, the RenoDX
//! overrides document, ...) from one anonymous CDN bucket. This module owns the
//! single host literal -- [`CDN_HOST`] -- so URL construction ([`cdn_url`]) and the
//! host-pinning check in `libraries::validate` can never desync. It also owns the
//! network and TTL policy for generic manifest caches. The filesystem transaction,
//! corruption quarantine, and atomic cache publication primitives live in
//! [`crate::fs`].

use std::fs;
#[cfg(test)]
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::ServiceError;
use crate::fs::strip_utf8_bom;

/// The host every manifest, preset, and archive download is pinned to. The one
/// literal: [`cdn_url`] builds from it and `libraries::validate` pins against it.
pub(crate) const CDN_HOST: &str = "pub-48612a35034d40f88f42b4181547925a.r2.dev";

/// Builds the HTTPS URL for `path` on the CDN host.
pub(crate) fn cdn_url(path: &str) -> String {
    format!("https://{CDN_HOST}/{path}")
}

/// Describes a cached CDN manifest: where it lives, where to fetch it, its size
/// cap, and how long a cached copy stays fresh (`None` = never auto-expires --
/// refresh only on an explicit [`fetch`]).
pub(crate) struct CdnManifestSpec {
    /// Cache file name under the app data dir.
    pub file_name: &'static str,
    /// Full CDN URL to download from (build with [`cdn_url`]).
    pub url: String,
    /// Maximum accepted document size.
    pub max_size_bytes: u64,
    /// Freshness window; `None` means a present cache is always fresh.
    pub ttl: Option<Duration>,
}

/// Classification of a cached manifest read.
#[derive(Debug)]
enum CachedManifest<T> {
    /// Present, parsed, and within the TTL.
    Fresh(T),
    /// Present and parsed but past the TTL -- usable as an offline fallback.
    Stale(T),
    /// No cache on disk. The production observation path uses
    /// [`crate::fs::CacheObservation::Absent`]; this variant supports the
    /// direct filesystem classifier tests below.
    #[cfg(test)]
    Absent,
}

/// Returns the cached manifest if fresh; otherwise refreshes from the CDN, falling
/// back to a stale cache when the network is unavailable. Corrupt cache files are
/// quarantined by the generic filesystem abstraction before the refresh attempt.
pub(crate) async fn get_or_fetch<T, F>(spec: &CdnManifestSpec, parse: F) -> Result<T, ServiceError>
where
    F: Fn(&[u8]) -> Result<T, ServiceError>,
{
    let observed = observe_cached(spec, &parse)?;
    match observed {
        crate::fs::CacheObservation::Valid {
            value: CachedManifest::Fresh(manifest),
            ..
        } => Ok(manifest),
        crate::fs::CacheObservation::Valid {
            generation,
            value: CachedManifest::Stale(stale),
        } => match fetch_observed(spec, &parse, &generation).await {
            Ok(fresh) => Ok(fresh),
            Err(error) => {
                log::warn!(
                    "CDN manifest `{}` refresh failed ({error}); using the stale cache",
                    spec.file_name
                );
                Ok(stale)
            }
        },
        #[cfg(test)]
        crate::fs::CacheObservation::Valid {
            generation,
            value: CachedManifest::Absent,
        } => fetch_observed(spec, &parse, &generation).await,
        crate::fs::CacheObservation::Absent { generation } => {
            fetch_observed(spec, &parse, &generation).await
        }
        crate::fs::CacheObservation::Invalid { generation, error } => {
            log::warn!(
                "CDN manifest cache `{}` is unreadable ({error}); refreshing",
                spec.file_name
            );
            fetch_observed(spec, &parse, &generation).await
        }
    }
}

/// Downloads, validates, and caches the manifest, returning the parsed value.
/// It always captures the cache generation before starting network I/O, even
/// when called directly rather than through [`get_or_fetch`].
pub(crate) async fn fetch<T, F>(spec: &CdnManifestSpec, parse: F) -> Result<T, ServiceError>
where
    F: Fn(&[u8]) -> Result<T, ServiceError>,
{
    let observed = observe_cached(spec, &parse)?;
    fetch_observed(spec, &parse, observed.generation()).await
}

async fn fetch_observed<T, F>(
    spec: &CdnManifestSpec,
    parse: &F,
    observed_generation: &crate::fs::CacheGeneration,
) -> Result<T, ServiceError>
where
    F: Fn(&[u8]) -> Result<T, ServiceError>,
{
    let bytes =
        crate::net::download_limited_bytes(&spec.url, spec.max_size_bytes, "manifest fetch")
            .await?;
    let manifest = parse(strip_utf8_bom(&bytes))?;
    let path = cache_path(spec.file_name)?;
    commit_manifest_candidate_at(&path, observed_generation, &bytes, manifest, parse)
}

/// Classifies the on-disk cache without touching the network.
fn observe_cached<T, F>(
    spec: &CdnManifestSpec,
    parse: &F,
) -> Result<crate::fs::CacheObservation<CachedManifest<T>>, ServiceError>
where
    F: Fn(&[u8]) -> Result<T, ServiceError>,
{
    let path = cache_path(spec.file_name)?;
    observe_cached_at(&path, spec.ttl, parse)
}

fn observe_cached_at<T, F>(
    path: &Path,
    ttl: Option<Duration>,
    parse: &F,
) -> Result<crate::fs::CacheObservation<CachedManifest<T>>, ServiceError>
where
    F: Fn(&[u8]) -> Result<T, ServiceError>,
{
    crate::fs::observe_cache_file(path, |bytes, metadata| {
        classify_cached_bytes(metadata, bytes, ttl, parse)
    })
}

fn commit_manifest_candidate_at<T, F>(
    path: &Path,
    observed_generation: &crate::fs::CacheGeneration,
    bytes: &[u8],
    manifest: T,
    parse: &F,
) -> Result<T, ServiceError>
where
    F: Fn(&[u8]) -> Result<T, ServiceError>,
{
    let publication = crate::fs::commit_cache_candidate(
        path,
        observed_generation,
        bytes,
        crate::fs::MatchingCurrentPolicy::RefreshCandidate,
        |current| parse(strip_utf8_bom(current)),
    )?;
    Ok(publication.into_candidate_or_current(manifest))
}

/// Reads, parses, and classifies the cache file at `path` by freshness. Split from
/// [`observe_cached`] so the classification can be exercised against an explicit temp
/// file without touching the process-wide app data dir.
#[cfg(test)]
fn read_cached_at<T, F>(
    path: &Path,
    ttl: Option<Duration>,
    parse: &F,
) -> Result<CachedManifest<T>, ServiceError>
where
    F: Fn(&[u8]) -> Result<T, ServiceError>,
{
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(CachedManifest::Absent),
        Err(error) => {
            return Err(crate::failed(format!(
                "failed to stat manifest cache `{}`: {error}",
                path.display()
            )));
        }
    };

    let bytes = crate::fs::read_file(path)?;
    classify_cached_bytes(&metadata, &bytes, ttl, parse)
}

fn classify_cached_bytes<T, F>(
    metadata: &fs::Metadata,
    bytes: &[u8],
    ttl: Option<Duration>,
    parse: &F,
) -> Result<CachedManifest<T>, ServiceError>
where
    F: Fn(&[u8]) -> Result<T, ServiceError>,
{
    let manifest = parse(strip_utf8_bom(bytes))?;
    if is_cache_expired(metadata, ttl) {
        Ok(CachedManifest::Stale(manifest))
    } else {
        Ok(CachedManifest::Fresh(manifest))
    }
}

fn is_cache_expired(metadata: &fs::Metadata, ttl: Option<Duration>) -> bool {
    let Some(ttl) = ttl else {
        return false;
    };
    let Ok(modified) = metadata.modified() else {
        // No mtime available: treat as fresh rather than thrashing the network.
        return false;
    };
    SystemTime::now()
        .duration_since(modified)
        .map(|elapsed| elapsed > ttl)
        .unwrap_or(false)
}

fn cache_path(file_name: &str) -> Result<PathBuf, ServiceError> {
    Ok(crate::app_dir::app_dir()?.join(file_name))
}

#[cfg(test)]
mod tests {
    use std::assert_matches;
    use std::fs::OpenOptions;
    #[cfg(not(target_os = "linux"))]
    use std::sync::{Arc, Barrier};
    use std::time::{Duration, SystemTime};

    use tempfile::tempdir;

    use super::*;

    const TTL: Duration = Duration::from_hours(24);

    #[derive(Debug, PartialEq, Eq)]
    struct Doc(String);

    /// A toy parser standing in for any manifest parser: it rejects a document
    /// containing `bad` (a corrupt cache) and otherwise echoes the trimmed text,
    /// so the BOM strip is observable in the returned value.
    fn parse_doc(bytes: &[u8]) -> Result<Doc, ServiceError> {
        let text = std::str::from_utf8(bytes).map_err(|e| crate::failed(e.to_string()))?;
        if text.contains("bad") {
            return Err(crate::failed("invalid doc"));
        }
        Ok(Doc(text.trim().to_owned()))
    }

    fn write_cache(dir: &Path, contents: &str) -> PathBuf {
        let path = dir.join("manifest.json");
        fs::write(&path, contents).expect("write cache");
        path
    }

    /// Backdates a file's mtime so freshness checks see it as `age` old. Windows
    /// needs a write-capable handle to change the mtime.
    fn age_file(path: &Path, age: Duration) {
        let modified = SystemTime::now()
            .checked_sub(age)
            .expect("representable timestamp");
        OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open cache")
            .set_modified(modified)
            .expect("set mtime");
    }

    #[test]
    fn cdn_url_builds_from_the_single_host() {
        assert_eq!(
            cdn_url("manifest.json"),
            format!("https://{CDN_HOST}/manifest.json")
        );
    }

    #[test]
    fn strips_a_utf8_bom() {
        // `strip_utf8_bom` lives in `crate::fs::io`; this test verifies the
        // import alias still resolves in the CDN module's scope.
        let mut bytes = b"\xEF\xBB\xBF".to_vec();
        bytes.extend_from_slice(b"{}");
        assert_eq!(strip_utf8_bom(&bytes), b"{}");
    }

    #[test]
    fn fresh_within_ttl_stale_past_it_and_none_never_expires() {
        let fresh = tempfile::NamedTempFile::new().expect("temp");
        let meta = fresh.as_file().metadata().expect("meta");

        // A 1-hour TTL: a just-written file is fresh.
        assert!(!is_cache_expired(&meta, Some(Duration::from_hours(1))));
        // A zero TTL: anything already written is stale.
        assert!(is_cache_expired(&meta, Some(Duration::ZERO)));
        // No TTL: never expires, even at zero age.
        assert!(!is_cache_expired(&meta, None));
    }

    #[test]
    fn missing_cache_classifies_as_absent() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("manifest.json");
        assert_matches!(
            read_cached_at(&path, Some(TTL), &parse_doc).expect("classify"),
            CachedManifest::Absent
        );
    }

    #[test]
    fn fresh_cache_classifies_as_fresh() {
        let dir = tempdir().expect("temp dir");
        let path = write_cache(dir.path(), "ok");
        assert_matches!(
            read_cached_at(&path, Some(TTL), &parse_doc).expect("classify"),
            CachedManifest::Fresh(_)
        );
    }

    #[test]
    fn past_ttl_cache_is_kept_as_a_stale_fallback() {
        // The offline-fallback contract: a past-TTL cache must still be parsed and
        // surfaced (as Stale), not silently dropped.
        let dir = tempdir().expect("temp dir");
        let path = write_cache(dir.path(), "ok");
        age_file(&path, TTL + Duration::from_hours(1));
        assert_matches!(
            read_cached_at(&path, Some(TTL), &parse_doc).expect("classify"),
            CachedManifest::Stale(_)
        );
    }

    #[test]
    fn no_ttl_cache_never_goes_stale() {
        let dir = tempdir().expect("temp dir");
        let path = write_cache(dir.path(), "ok");
        age_file(&path, Duration::from_secs(10 * 365 * 24 * 60 * 60));
        assert_matches!(
            read_cached_at(&path, None, &parse_doc).expect("classify"),
            CachedManifest::Fresh(_)
        );
    }

    #[test]
    fn corrupt_cache_classifies_as_error() {
        let dir = tempdir().expect("temp dir");
        let path = write_cache(dir.path(), "bad");
        assert!(read_cached_at(&path, Some(TTL), &parse_doc).is_err());
    }

    #[test]
    fn a_bom_prefixed_cache_parses_after_strip() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("manifest.json");
        let mut bytes = b"\xEF\xBB\xBF".to_vec();
        bytes.extend_from_slice(b"ok");
        fs::write(&path, &bytes).expect("write cache");

        match read_cached_at(&path, Some(TTL), &parse_doc).expect("classify") {
            CachedManifest::Fresh(Doc(text)) => assert_eq!(text, "ok"),
            _ => panic!("expected a fresh BOM-stripped doc, got a different cache state"),
        }
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn delayed_cdn_fetch_returns_the_newer_valid_winner_without_touching_its_mtime() {
        let dir = tempdir().expect("temp dir");
        let path = write_cache(dir.path(), "seed");
        let delayed =
            observe_cached_at(&path, Some(TTL), &parse_doc).expect("observe delayed fetch");
        let delayed_generation = delayed.generation().clone();
        let barrier = Arc::new(Barrier::new(2));
        let publisher_path = path.clone();
        let publisher_barrier = Arc::clone(&barrier);

        let publisher = std::thread::spawn(move || {
            publisher_barrier.wait();
            let observed = observe_cached_at(&publisher_path, Some(TTL), &parse_doc)
                .expect("observe newer fetch");
            commit_manifest_candidate_at(
                &publisher_path,
                observed.generation(),
                b"new",
                Doc("new".to_owned()),
                &parse_doc,
            )
            .expect("publish newer manifest")
        });

        barrier.wait();
        assert_eq!(
            publisher.join().expect("newer fetch completes first"),
            Doc("new".to_owned())
        );
        let winner_mtime = fs::metadata(&path)
            .expect("newer manifest metadata")
            .modified()
            .expect("newer manifest mtime");

        let returned = commit_manifest_candidate_at(
            &path,
            &delayed_generation,
            b"old",
            Doc("old".to_owned()),
            &parse_doc,
        )
        .expect("delayed fetch observes winner");

        assert_eq!(returned, Doc("new".to_owned()));
        assert_eq!(fs::read_to_string(&path).expect("read winner"), "new");
        assert_eq!(
            fs::metadata(&path)
                .expect("winner metadata after delayed commit")
                .modified()
                .expect("winner mtime after delayed commit"),
            winner_mtime,
            "the losing delayed fetch must not reset the winner TTL"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_cache_refresh_returns_fetched_candidate_without_touching_stale_cache() {
        let dir = tempdir().expect("temp dir");
        let path = write_cache(dir.path(), "stale-a");
        age_file(&path, TTL + Duration::from_hours(1));
        let observed =
            observe_cached_at(&path, Some(TTL), &parse_doc).expect("observe stale cache A");
        let stale_metadata = fs::metadata(&path).expect("snapshot stale cache A metadata");

        let returned = commit_manifest_candidate_at(
            &path,
            observed.generation(),
            b"fetched-c",
            Doc("fetched-c".to_owned()),
            &parse_doc,
        )
        .expect("Linux keeps the validated fetched candidate in memory");

        assert_eq!(returned, Doc("fetched-c".to_owned()));
        assert_eq!(fs::read(&path).expect("read stale cache A"), b"stale-a");
        let retained = fs::metadata(&path).expect("read stale cache A metadata");
        assert_eq!(retained.len(), stale_metadata.len());
        assert_eq!(
            retained.modified().expect("read stale cache A mtime"),
            stale_metadata
                .modified()
                .expect("read stale cache A snapshot mtime"),
            "the unpublished CDN candidate must not refresh stale cache A"
        );
    }
}
