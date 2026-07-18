//! Loading the shared ReShade source catalogue and raw per-tool catalogues.
//!
//! ReShade download locations have one wire-v1 source: `addons/v1/reshade.json`.
//! CDN/cache copies and the release-pinned bundled snapshot are parsed by the
//! same parser. Tool catalogues never embed or overlay ReShade sources.

use std::time::Duration;

use crate::ServiceError;
use crate::cdn::{self, CdnManifestSpec};

use super::manifest::parse_reshade_manifest;
use super::types::ReshadeSourceCatalog;

const MANIFEST_FILE_NAME: &str = "reshade_manifest_v1.json";
const MANIFEST_REMOTE_PATH: &str = "addons/v1/reshade.json";
const MAX_MANIFEST_SIZE_BYTES: u64 = 64 * 1024;
const MANIFEST_CACHE_TTL: Duration = Duration::from_hours(24);
const BUNDLED_MANIFEST: &[u8] = include_bytes!("../../../assets/reshade-v1-fallback.json");

/// Where an operation obtained its complete ReShade source catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReshadeCatalogOrigin {
    /// A fresh or stale copy managed by the CDN cache.
    CdnOrCache,
    /// The release-pinned last-resort snapshot shipped with the application.
    Bundled,
}

/// A complete source catalogue plus its provenance.
#[derive(Debug, Clone)]
pub struct ResolvedReshadeCatalog {
    /// Complete sources selected for this operation.
    pub sources: ReshadeSourceCatalog,
    /// Resolver tier that supplied the complete document.
    pub origin: ReshadeCatalogOrigin,
}

/// Per-command composition of a raw tool catalogue and shared ReShade sources.
#[derive(Debug, Clone)]
pub struct AddonCatalogBundle<M> {
    /// Raw tool catalogue, without ReShade source overlays.
    pub tool: M,
    /// Independently resolved ReShade source catalogue.
    pub reshade: ResolvedReshadeCatalog,
}

fn manifest_spec() -> CdnManifestSpec {
    CdnManifestSpec {
        file_name: MANIFEST_FILE_NAME,
        url: cdn::cdn_url(MANIFEST_REMOTE_PATH),
        max_size_bytes: MAX_MANIFEST_SIZE_BYTES,
        ttl: Some(MANIFEST_CACHE_TTL),
    }
}

fn bundled_catalog() -> Result<ResolvedReshadeCatalog, ServiceError> {
    Ok(ResolvedReshadeCatalog {
        sources: parse_reshade_manifest(BUNDLED_MANIFEST)?.into_sources(),
        origin: ReshadeCatalogOrigin::Bundled,
    })
}

fn resolve_with_bundled_fallback(
    cdn_or_cache: Result<super::manifest::ReshadeManifest, ServiceError>,
) -> Result<ResolvedReshadeCatalog, ServiceError> {
    match cdn_or_cache {
        Ok(manifest) => Ok(ResolvedReshadeCatalog {
            sources: manifest.into_sources(),
            origin: ReshadeCatalogOrigin::CdnOrCache,
        }),
        Err(error) => {
            log::warn!(
                "shared ReShade manifest unavailable ({error}); using the bundled release snapshot"
            );
            bundled_catalog()
        }
    }
}

/// Resolves the shared catalogue in strict priority order: CDN/fresh cache,
/// stale cache after a failed refresh, then the bundled snapshot.
pub(crate) async fn get_or_fetch_catalog() -> Result<ResolvedReshadeCatalog, ServiceError> {
    resolve_with_bundled_fallback(cdn::get_or_fetch(&manifest_spec(), parse_reshade_manifest).await)
}

/// Force-fetches the shared catalogue. This is intentionally strict: callers
/// reporting refresh health must preserve a CDN failure even though ordinary
/// operations can continue with [`get_or_fetch_catalog`]'s bundled fallback.
pub(crate) async fn fetch_catalog() -> Result<ResolvedReshadeCatalog, ServiceError> {
    cdn::fetch(&manifest_spec(), parse_reshade_manifest)
        .await
        .map(|manifest| ResolvedReshadeCatalog {
            sources: manifest.into_sources(),
            origin: ReshadeCatalogOrigin::CdnOrCache,
        })
}

pub(crate) const TOOL_MANIFEST_MAX_SIZE_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const TOOL_MANIFEST_CACHE_TTL: Duration = Duration::from_hours(24);

fn tool_manifest_spec(file_name: &'static str, remote_path: &'static str) -> CdnManifestSpec {
    CdnManifestSpec {
        file_name,
        url: cdn::cdn_url(remote_path),
        max_size_bytes: TOOL_MANIFEST_MAX_SIZE_BYTES,
        ttl: Some(TOOL_MANIFEST_CACHE_TTL),
    }
}

/// Loads a raw tool catalogue without consulting or mutating ReShade sources.
pub(crate) async fn get_or_fetch_tool_catalog<M>(
    file_name: &'static str,
    remote_path: &'static str,
    parse: impl Fn(&[u8]) -> Result<M, ServiceError>,
) -> Result<M, ServiceError> {
    cdn::get_or_fetch(&tool_manifest_spec(file_name, remote_path), parse).await
}

/// Force-fetches a raw tool catalogue without consulting ReShade sources.
pub(crate) async fn fetch_tool_catalog<M>(
    file_name: &'static str,
    remote_path: &'static str,
    parse: impl Fn(&[u8]) -> Result<M, ServiceError>,
) -> Result<M, ServiceError> {
    cdn::fetch(&tool_manifest_spec(file_name, remote_path), parse).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::reshade::types::ReshadeChannel;

    #[test]
    fn bundled_snapshot_uses_the_wire_v1_parser_and_allowlist() {
        let resolved = bundled_catalog().expect("bundled ReShade manifest");
        assert_eq!(resolved.origin, ReshadeCatalogOrigin::Bundled);
        assert!(resolved.sources.supports_channel(ReshadeChannel::Stable));
        assert!(resolved.sources.supports_channel(ReshadeChannel::Nightly));
    }

    #[test]
    fn valid_cdn_or_cache_document_replaces_the_snapshot_without_merging_stable() {
        let remote_without_stable = parse_reshade_manifest(
            br#"{
                "schema_version": 1,
                "generated_at": "2026-07-14T00:00:00Z",
                "channels": {
                    "nightly": {
                        "url64": "https://nightly.link/crosire/reshade/workflows/build/main/remote64.zip",
                        "url32": "https://nightly.link/crosire/reshade/workflows/build/main/remote32.zip"
                    }
                }
            }"#,
        )
        .expect("valid remote");

        let resolved = resolve_with_bundled_fallback(Ok(remote_without_stable)).expect("resolve");

        assert_eq!(resolved.origin, ReshadeCatalogOrigin::CdnOrCache);
        assert!(resolved.sources.stable.is_none());
        assert!(resolved.sources.nightly.url64.ends_with("remote64.zip"));
    }

    #[test]
    fn failed_cdn_and_cache_resolution_uses_the_bundled_snapshot() {
        let resolved = resolve_with_bundled_fallback(Err(ServiceError::command_failed(
            "network and cache unavailable",
        )))
        .expect("bundled fallback");

        assert_eq!(resolved.origin, ReshadeCatalogOrigin::Bundled);
        assert!(resolved.sources.supports_channel(ReshadeChannel::Stable));
        assert!(resolved.sources.supports_channel(ReshadeChannel::Nightly));
    }

    #[test]
    fn spec_is_pinned_to_the_cdn_host_with_a_day_long_ttl() {
        let spec = manifest_spec();
        assert_eq!(spec.file_name, "reshade_manifest_v1.json");
        assert_eq!(spec.url, cdn::cdn_url("addons/v1/reshade.json"));
        assert!(spec.url.starts_with("https://"));
        assert_eq!(spec.ttl, Some(MANIFEST_CACHE_TTL));
    }

    #[test]
    fn tool_manifest_spec_is_pinned_to_the_cdn_host_with_a_day_long_ttl() {
        let spec = tool_manifest_spec("example_manifest_v1.json", "addons/v1/example.json");
        assert_eq!(spec.file_name, "example_manifest_v1.json");
        assert_eq!(spec.url, cdn::cdn_url("addons/v1/example.json"));
        assert!(spec.url.starts_with("https://"));
        assert_eq!(spec.ttl, Some(TOOL_MANIFEST_CACHE_TTL));
    }
}
