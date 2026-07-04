//! Fetching and caching the standalone, shared `reshade_manifest.json`, plus
//! the shared skeleton every per-tool manifest store (`renodx`, `luma`) builds
//! its own store on.
//!
//! A thin adapter over the generic [`crate::cdn`] manifest cache (the same one
//! RenoDX's and Luma's own manifests use): it pins this document's file name,
//! CDN path, size cap, and 24-hour freshness window, and parses/validates it
//! via [`super::manifest::parse_reshade_manifest`].
//!
//! Unlike the per-tool manifest stores, [`shared_config`] never propagates a
//! failure: this document is a pure *overlay* on top of each tool's own fallback
//! ReShade sources. A missing/unreachable/corrupt copy just means the caller
//! keeps using its bundled or tool-manifest fallback. See [`super::manifest`]'s
//! module doc for the full RenoDX compatibility story.

use std::time::Duration;

use crate::ServiceError;
use crate::cdn::{self, CdnManifestSpec};

use super::manifest::parse_reshade_manifest;
use super::types::ReshadeConfig;

/// Cache file name beside the other manifests in the app data directory.
const MANIFEST_FILE_NAME: &str = "reshade_manifest.json";
/// Upper bound on the manifest document size — this document is tiny (a
/// handful of URLs), so the cap is far smaller than a tool manifest's.
const MAX_MANIFEST_SIZE_BYTES: u64 = 64 * 1024;
/// How long a cached manifest stays fresh before it is refreshed from the CDN.
const MANIFEST_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// The CDN cache spec for the shared ReShade manifest: a 24-hour TTL atop the
/// generic stale-fallback/quarantine behavior in [`crate::cdn`].
fn manifest_spec() -> CdnManifestSpec {
    CdnManifestSpec {
        file_name: MANIFEST_FILE_NAME,
        url: cdn::cdn_url(MANIFEST_FILE_NAME),
        max_size_bytes: MAX_MANIFEST_SIZE_BYTES,
        ttl: Some(MANIFEST_CACHE_TTL),
    }
}

/// Returns the shared ReShade host sources, or `None` if the document could
/// not be fetched/read/parsed — never an error. A caller that gets `None`
/// simply keeps using its own fallback ReShade sources.
pub(crate) async fn shared_config() -> Option<ReshadeConfig> {
    match cdn::get_or_fetch(&manifest_spec(), parse_reshade_manifest).await {
        Ok(manifest) => Some(manifest.into_config()),
        Err(error) => {
            log::warn!(
                "shared ReShade manifest unavailable ({error}); falling back to the tool manifest's own embedded sources"
            );
            None
        }
    }
}

/// Prefer an already-resolved config; otherwise load via [`shared_config`].
pub(crate) async fn resolve_shared_config(
    preferred: Option<ReshadeConfig>,
) -> Option<ReshadeConfig> {
    match preferred {
        Some(config) => Some(config),
        None => shared_config().await,
    }
}

/// Upper bound accepted for a tool's own manifest document — larger than the
/// shared, ReShade-only document above, since a tool manifest carries a whole
/// title catalog.
pub(crate) const TOOL_MANIFEST_MAX_SIZE_BYTES: u64 = 4 * 1024 * 1024;
/// How long a cached tool manifest stays fresh before it is refreshed from the CDN.
pub(crate) const TOOL_MANIFEST_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// The standard CDN cache spec for a tool's own manifest document (4 MiB cap,
/// 24-hour TTL) — every per-tool manifest store (`renodx`, `luma`) builds its
/// spec from this instead of repeating the constants.
fn tool_manifest_spec(file_name: &'static str) -> CdnManifestSpec {
    CdnManifestSpec {
        file_name,
        url: cdn::cdn_url(file_name),
        max_size_bytes: TOOL_MANIFEST_MAX_SIZE_BYTES,
        ttl: Some(TOOL_MANIFEST_CACHE_TTL),
    }
}

/// Shared skeleton for a tool manifest store: cached/CDN fetch via
/// [`cdn::get_or_fetch`], then an in-memory overlay of the shared, standalone
/// ReShade document (see the module doc) when it is reachable.
/// `overlay_shared_reshade` mutates the parsed manifest's own ReShade source
/// fields; the on-disk tool-manifest cache file is never rewritten.
pub(crate) async fn get_or_fetch_tool_manifest<M>(
    file_name: &'static str,
    parse: impl Fn(&[u8]) -> Result<M, ServiceError>,
    overlay_shared_reshade: impl FnOnce(&mut M, ReshadeConfig),
) -> Result<M, ServiceError> {
    let mut manifest = cdn::get_or_fetch(&tool_manifest_spec(file_name), parse).await?;
    if let Some(shared) = resolve_shared_config(None).await {
        overlay_shared_reshade(&mut manifest, shared);
    }
    Ok(manifest)
}

/// Force-fetch variant of [`get_or_fetch_tool_manifest`] (ignores TTL). When
/// `shared_reshade` is `Some`, that config is used for the overlay instead of
/// an extra ReShade CDN hit (see [`resolve_shared_config`]).
pub(crate) async fn fetch_tool_manifest<M>(
    file_name: &'static str,
    parse: impl Fn(&[u8]) -> Result<M, ServiceError>,
    shared_reshade: Option<ReshadeConfig>,
    overlay_shared_reshade: impl FnOnce(&mut M, ReshadeConfig),
) -> Result<M, ServiceError> {
    let mut manifest = cdn::fetch(&tool_manifest_spec(file_name), parse).await?;
    if let Some(shared) = resolve_shared_config(shared_reshade).await {
        overlay_shared_reshade(&mut manifest, shared);
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_is_pinned_to_the_cdn_host_with_a_day_long_ttl() {
        let spec = manifest_spec();
        assert_eq!(spec.file_name, "reshade_manifest.json");
        assert_eq!(spec.url, cdn::cdn_url("reshade_manifest.json"));
        assert!(spec.url.starts_with("https://"));
        assert_eq!(spec.ttl, Some(MANIFEST_CACHE_TTL));
    }

    #[test]
    fn tool_manifest_spec_is_pinned_to_the_cdn_host_with_a_day_long_ttl() {
        let spec = tool_manifest_spec("example_manifest.json");
        assert_eq!(spec.file_name, "example_manifest.json");
        assert_eq!(spec.url, cdn::cdn_url("example_manifest.json"));
        assert!(spec.url.starts_with("https://"));
        assert_eq!(spec.ttl, Some(TOOL_MANIFEST_CACHE_TTL));
    }
}
