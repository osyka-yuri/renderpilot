//! Fetching and caching the RenoDX manifest.
//!
//! A thin adapter over the shared
//! [`get_or_fetch_tool_manifest`](reshade_manifest_store::get_or_fetch_tool_manifest)
//! skeleton: it pins the RenoDX manifest's file name and supplies the parse
//! and shared-ReShade-overlay steps. Everything else — the CDN cache spec,
//! fresh reuse, the stale-on-failure offline fallback, and corrupt-file
//! quarantine — lives in `cdn` / `reshade::manifest_store` and is shared with other tool manifests.
//!
//! After parsing, the manifest's own embedded `reshade` block is overlaid with
//! the standalone, shared `reshade_manifest.json` when that document is
//! reachable (see [`reshade_manifest_store::shared_config`]) — purely in
//! memory; the cached `renodx_manifest.json` file on disk is never rewritten.
//! When the shared document is unavailable, the embedded block is left
//! untouched, which is exactly what an app version that predates the shared
//! document already does.

use crate::ServiceError;
use crate::addons::reshade::manifest_store as reshade_manifest_store;
use crate::addons::reshade::types::ReshadeConfig;

use super::parse_manifest;
use super::types::RenoDxManifest;

/// Cache file name beside the other manifests in the app data directory.
const MANIFEST_FILE_NAME: &str = "renodx_manifest.json";

/// Returns the cached manifest when fresh, otherwise refreshes from the CDN —
/// degrading to a still-parseable stale cache if the refresh fails. The
/// returned manifest's `reshade` block is overlaid with the shared, standalone
/// ReShade manifest when that document is reachable (see the module doc).
pub async fn get_or_fetch_manifest() -> Result<RenoDxManifest, ServiceError> {
    reshade_manifest_store::get_or_fetch_tool_manifest(
        MANIFEST_FILE_NAME,
        parse_manifest,
        overlay_shared_reshade,
    )
    .await
}

/// Force-fetches the RenoDX manifest from the CDN (ignores TTL).
///
/// When `shared_reshade` is `Some`, that config is used for the in-memory
/// overlay (avoids a second ReShade CDN hit after a coordinated force). When
/// `None`, falls back to [`reshade_manifest_store::shared_config`].
pub async fn fetch_manifest(
    shared_reshade: Option<ReshadeConfig>,
) -> Result<RenoDxManifest, ServiceError> {
    reshade_manifest_store::fetch_tool_manifest(
        MANIFEST_FILE_NAME,
        parse_manifest,
        shared_reshade,
        overlay_shared_reshade,
    )
    .await
}

/// Replaces the manifest's embedded `reshade` block (both `stable` and
/// `nightly`) with the shared document's — RenoDX supports both channels, so
/// the overlay is a full replacement. Pure and in-memory only: the cached
/// manifest file on disk is never touched.
fn overlay_shared_reshade(manifest: &mut RenoDxManifest, shared: ReshadeConfig) {
    manifest.reshade = shared;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::renodx::test_support;
    use crate::addons::reshade::types::{ReshadeNightly, ReshadeStable};

    #[test]
    fn manifest_file_name_is_renodx_specific() {
        assert_eq!(MANIFEST_FILE_NAME, "renodx_manifest.json");
    }

    #[test]
    fn overlay_shared_reshade_replaces_both_stable_and_nightly() {
        let mut manifest = test_support::manifest(Vec::new());
        let shared = ReshadeConfig {
            stable: Some(ReshadeStable {
                url: "https://reshade.me/downloads/ReShade_Setup_9.9.9_Addon.exe".to_owned(),
            }),
            nightly: ReshadeNightly {
                url64: "https://nightly.link/x64-overlay.zip".to_owned(),
                url32: "https://nightly.link/x32-overlay.zip".to_owned(),
            },
        };

        overlay_shared_reshade(&mut manifest, shared);

        assert_eq!(
            manifest.reshade.stable.expect("stable").url,
            "https://reshade.me/downloads/ReShade_Setup_9.9.9_Addon.exe"
        );
        assert_eq!(
            manifest.reshade.nightly.url64,
            "https://nightly.link/x64-overlay.zip"
        );
        assert_eq!(
            manifest.reshade.nightly.url32,
            "https://nightly.link/x32-overlay.zip"
        );
    }
}
