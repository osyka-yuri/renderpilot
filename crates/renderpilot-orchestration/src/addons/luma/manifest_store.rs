//! Fetching and caching the raw Luma tool catalogue.

use crate::ServiceError;
use crate::addons::reshade::manifest_store::{self as reshade_store, AddonCatalogBundle};

use super::parse_manifest;
use super::types::LumaManifest;

const MANIFEST_FILE_NAME: &str = "luma_manifest_v1.json";
const MANIFEST_REMOTE_PATH: &str = "addons/v1/luma.json";

/// Loads only the Luma tool catalogue. It contains no ReShade source URLs.
pub async fn get_or_fetch_manifest() -> Result<LumaManifest, ServiceError> {
    reshade_store::get_or_fetch_tool_catalog(
        MANIFEST_FILE_NAME,
        MANIFEST_REMOTE_PATH,
        parse_manifest,
    )
    .await
}

/// Force-fetches only the raw Luma tool catalogue.
pub async fn fetch_manifest() -> Result<LumaManifest, ServiceError> {
    reshade_store::fetch_tool_catalog(MANIFEST_FILE_NAME, MANIFEST_REMOTE_PATH, parse_manifest)
        .await
}

/// Resolves both independent catalogues once for a command that needs a host.
pub async fn get_or_fetch_bundle() -> Result<AddonCatalogBundle<LumaManifest>, ServiceError> {
    let (tool, reshade) = tokio::join!(
        get_or_fetch_manifest(),
        reshade_store::get_or_fetch_catalog()
    );
    Ok(AddonCatalogBundle {
        tool: tool?,
        reshade: reshade?,
    })
}
