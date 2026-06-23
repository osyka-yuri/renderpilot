//! Fetching and caching the RenoDX manifest.
//!
//! A thin adapter over the generic [`crate::cdn`] manifest cache: it pins the
//! RenoDX manifest's file name, CDN path, size cap, and 24-hour freshness window,
//! and parses/validates the document via [`parse_manifest`](super::parse_manifest).
//! Everything else — fresh reuse, the stale-on-failure offline fallback, and
//! corrupt-file quarantine — lives in `cdn` and is shared with the library manifest.

use std::time::Duration;

use crate::cdn::{self, CdnManifestSpec};
use crate::ServiceError;

use super::parse_manifest;
use super::types::RenoDxManifest;

/// Cache file name beside the library manifest in the app data directory.
const MANIFEST_FILE_NAME: &str = "renodx_manifest.json";
/// Upper bound on the manifest document size.
const MAX_MANIFEST_SIZE_BYTES: u64 = 4 * 1024 * 1024;
/// How long a cached manifest stays fresh before it is refreshed from the CDN.
const MANIFEST_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// The CDN cache spec for the RenoDX manifest: a 24-hour TTL atop the generic
/// stale-fallback/quarantine behavior in [`crate::cdn`].
fn manifest_spec() -> CdnManifestSpec {
    CdnManifestSpec {
        file_name: MANIFEST_FILE_NAME,
        url: cdn::cdn_url(MANIFEST_FILE_NAME),
        max_size_bytes: MAX_MANIFEST_SIZE_BYTES,
        ttl: Some(MANIFEST_CACHE_TTL),
    }
}

/// Returns the cached manifest when fresh, otherwise refreshes from the CDN —
/// degrading to a still-parseable stale cache if the refresh fails.
pub async fn get_or_fetch_manifest() -> Result<RenoDxManifest, ServiceError> {
    cdn::get_or_fetch(&manifest_spec(), parse_manifest).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_is_pinned_to_the_cdn_host_with_a_day_long_ttl() {
        let spec = manifest_spec();
        assert_eq!(spec.file_name, "renodx_manifest.json");
        assert_eq!(spec.url, cdn::cdn_url("renodx_manifest.json"));
        assert!(spec.url.starts_with("https://"));
        assert_eq!(spec.ttl, Some(MANIFEST_CACHE_TTL));
    }
}
