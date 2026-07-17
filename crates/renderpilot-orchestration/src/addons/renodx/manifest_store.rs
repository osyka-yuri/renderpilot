//! Fetching and caching the raw RenoDX tool catalogue.

use crate::ServiceError;
use crate::addons::reshade::manifest_store::{self as reshade_store, AddonCatalogBundle};

use super::parse_manifest;
use super::types::RenoDxManifest;

const MANIFEST_FILE_NAME: &str = "renodx_manifest_v1.json";
const MANIFEST_REMOTE_PATH: &str = "addons/v1/renodx.json";

/// Loads only the RenoDX tool catalogue. It contains no ReShade source URLs.
pub async fn get_or_fetch_manifest() -> Result<RenoDxManifest, ServiceError> {
    reshade_store::get_or_fetch_tool_catalog(
        MANIFEST_FILE_NAME,
        MANIFEST_REMOTE_PATH,
        parse_manifest,
    )
    .await
}

/// Force-fetches only the raw RenoDX tool catalogue.
pub async fn fetch_manifest() -> Result<RenoDxManifest, ServiceError> {
    reshade_store::fetch_tool_catalog(MANIFEST_FILE_NAME, MANIFEST_REMOTE_PATH, parse_manifest)
        .await
}

/// Resolves both independent catalogues once for a command that needs a host.
pub async fn get_or_fetch_bundle() -> Result<AddonCatalogBundle<RenoDxManifest>, ServiceError> {
    let (tool, reshade) = tokio::join!(
        get_or_fetch_manifest(),
        reshade_store::get_or_fetch_catalog()
    );
    Ok(AddonCatalogBundle {
        tool: tool?,
        reshade: reshade?,
    })
}
