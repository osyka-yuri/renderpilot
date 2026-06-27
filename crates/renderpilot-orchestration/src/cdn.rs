//! The public content-delivery host and a generic, host-pinned JSON-manifest cache.
//!
//! RenderPilot serves its manifests (the graphics-library catalogue, the RenoDX
//! overrides document, …) from one anonymous CDN bucket. This module owns the
//! single host literal — [`CDN_HOST`] — so URL construction ([`cdn_url`]) and the
//! host-pinning check in `libraries::validate` can never desync, and a generic
//! [`get_or_fetch`] cache that every manifest reuses: download (HTTPS, size-capped),
//! strip any UTF-8 BOM, parse + validate, and cache under the app data dir with an
//! optional TTL, a stale-on-failure offline fallback, and corrupt-file quarantine.

use std::fs;
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

fn err(message: impl Into<String>) -> ServiceError {
    ServiceError::CommandFailed(message.into())
}

/// Describes a cached CDN manifest: where it lives, where to fetch it, its size
/// cap, and how long a cached copy stays fresh (`None` = never auto-expires —
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
enum CachedManifest<T> {
    /// Present, parsed, and within the TTL.
    Fresh(T),
    /// Present and parsed but past the TTL — usable as an offline fallback.
    Stale(T),
    /// No cache on disk.
    Absent,
}

/// Returns the cached manifest if fresh; otherwise refreshes from the CDN, falling
/// back to a stale cache when the network is unavailable, and quarantining (rather
/// than deleting) a corrupt cache so a clean copy can be written while the bad one
/// remains for diagnosis. A network failure with no cache surfaces as an error.
pub(crate) async fn get_or_fetch<T, F>(spec: &CdnManifestSpec, parse: F) -> Result<T, ServiceError>
where
    F: Fn(&[u8]) -> Result<T, ServiceError>,
{
    match read_cached(spec, &parse) {
        Ok(CachedManifest::Fresh(manifest)) => Ok(manifest),
        Ok(CachedManifest::Stale(stale)) => match fetch(spec, &parse).await {
            Ok(fresh) => Ok(fresh),
            Err(error) => {
                log::warn!(
                    "CDN manifest `{}` refresh failed ({error}); using the stale cache",
                    spec.file_name
                );
                Ok(stale)
            }
        },
        Ok(CachedManifest::Absent) => fetch(spec, &parse).await,
        Err(error) => {
            log::warn!(
                "CDN manifest cache `{}` is unreadable ({error}); refreshing",
                spec.file_name
            );
            quarantine_corrupt(spec.file_name);
            fetch(spec, &parse).await
        }
    }
}

/// Returns a present, parseable cache (ignoring its TTL), or `None` if it is
/// absent or corrupt — for callers that reuse any cache they have and run their
/// own fetch (with extra side effects) on a miss.
pub(crate) fn cached<T, F>(spec: &CdnManifestSpec, parse: F) -> Option<T>
where
    F: Fn(&[u8]) -> Result<T, ServiceError>,
{
    match read_cached(spec, &parse) {
        Ok(CachedManifest::Fresh(manifest) | CachedManifest::Stale(manifest)) => Some(manifest),
        Ok(CachedManifest::Absent) | Err(_) => None,
    }
}

/// Downloads, validates, and caches the manifest, returning the parsed value.
pub(crate) async fn fetch<T, F>(spec: &CdnManifestSpec, parse: F) -> Result<T, ServiceError>
where
    F: Fn(&[u8]) -> Result<T, ServiceError>,
{
    let bytes =
        crate::net::download_limited_bytes(&spec.url, spec.max_size_bytes, "manifest fetch")
            .await?;
    let manifest = parse(strip_utf8_bom(&bytes))?;
    crate::fs::write_file_atomically(&cache_path(spec.file_name)?, &bytes)?;
    Ok(manifest)
}

/// Classifies the on-disk cache without touching the network.
fn read_cached<T, F>(spec: &CdnManifestSpec, parse: &F) -> Result<CachedManifest<T>, ServiceError>
where
    F: Fn(&[u8]) -> Result<T, ServiceError>,
{
    read_cached_at(&cache_path(spec.file_name)?, spec.ttl, parse)
}

/// Reads, parses, and classifies the cache file at `path` by freshness. Split from
/// [`read_cached`] so the classification can be exercised against an explicit temp
/// file without touching the process-wide app data dir.
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
            return Err(err(format!(
                "failed to stat manifest cache `{}`: {error}",
                path.display()
            )));
        }
    };

    let bytes = crate::fs::read_file(path)?;
    let manifest = parse(strip_utf8_bom(&bytes))?;

    if is_cache_expired(&metadata, ttl) {
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

/// Renames a corrupt cache aside (`.corrupt`) so the next fetch writes a clean file
/// while the bad document remains for diagnosis. Best-effort.
fn quarantine_corrupt(file_name: &str) {
    let Ok(path) = cache_path(file_name) else {
        return;
    };
    quarantine_at(&path);
}

/// Renames the file at `path` aside to `*.corrupt` (best-effort). Split from
/// [`quarantine_corrupt`] so it can be exercised against an explicit temp file.
fn quarantine_at(path: &Path) {
    let quarantined = path.with_extension("corrupt");
    let _ = fs::rename(path, &quarantined);
}

fn cache_path(file_name: &str) -> Result<PathBuf, ServiceError> {
    Ok(crate::app_dir::app_dir()?.join(file_name))
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::time::{Duration, SystemTime};

    use tempfile::tempdir;

    use super::*;

    const TTL: Duration = Duration::from_secs(24 * 60 * 60);

    #[derive(Debug, PartialEq, Eq)]
    struct Doc(String);

    /// A toy parser standing in for any manifest parser: it rejects a document
    /// containing `bad` (a corrupt cache) and otherwise echoes the trimmed text,
    /// so the BOM strip is observable in the returned value.
    fn parse_doc(bytes: &[u8]) -> Result<Doc, ServiceError> {
        let text = std::str::from_utf8(bytes).map_err(|e| err(e.to_string()))?;
        if text.contains("bad") {
            return Err(err("invalid doc"));
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
        // `strip_utf8_bom` now lives in `crate::fs`; this test verifies the
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
        assert!(!is_cache_expired(&meta, Some(Duration::from_secs(3600))));
        // A zero TTL: anything already written is stale.
        assert!(is_cache_expired(&meta, Some(Duration::ZERO)));
        // No TTL: never expires, even at zero age.
        assert!(!is_cache_expired(&meta, None));
    }

    #[test]
    fn missing_cache_classifies_as_absent() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("manifest.json");
        assert!(matches!(
            read_cached_at(&path, Some(TTL), &parse_doc).expect("classify"),
            CachedManifest::Absent
        ));
    }

    #[test]
    fn fresh_cache_classifies_as_fresh() {
        let dir = tempdir().expect("temp dir");
        let path = write_cache(dir.path(), "ok");
        assert!(matches!(
            read_cached_at(&path, Some(TTL), &parse_doc).expect("classify"),
            CachedManifest::Fresh(_)
        ));
    }

    #[test]
    fn past_ttl_cache_is_kept_as_a_stale_fallback() {
        // The offline-fallback contract: a past-TTL cache must still be parsed and
        // surfaced (as Stale), not silently dropped.
        let dir = tempdir().expect("temp dir");
        let path = write_cache(dir.path(), "ok");
        age_file(&path, TTL + Duration::from_secs(3600));
        assert!(matches!(
            read_cached_at(&path, Some(TTL), &parse_doc).expect("classify"),
            CachedManifest::Stale(_)
        ));
    }

    #[test]
    fn no_ttl_cache_never_goes_stale() {
        let dir = tempdir().expect("temp dir");
        let path = write_cache(dir.path(), "ok");
        age_file(&path, Duration::from_secs(10 * 365 * 24 * 60 * 60));
        assert!(matches!(
            read_cached_at(&path, None, &parse_doc).expect("classify"),
            CachedManifest::Fresh(_)
        ));
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

    #[test]
    fn quarantine_renames_the_bad_file_aside() {
        let dir = tempdir().expect("temp dir");
        let path = write_cache(dir.path(), "bad");
        quarantine_at(&path);
        assert!(!path.exists(), "the corrupt file is moved aside");
        assert!(
            path.with_extension("corrupt").exists(),
            "it is preserved as *.corrupt for diagnosis"
        );
    }
}
