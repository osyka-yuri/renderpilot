//! Fetching and caching the raw RenoDX tool catalogue.

use crate::ServiceError;
use crate::addons::reshade::manifest_store::{self as reshade_store, AddonCatalogBundle};

use super::types::RenoDxManifest;
use super::{parse_manifest, parse_manifest_v2};

const MANIFEST_FILE_NAME: &str = "renodx_manifest_v2.json";
const MANIFEST_REMOTE_PATH: &str = "addons/v2/renodx.json";
const LEGACY_MANIFEST_FILE_NAME: &str = "renodx_manifest_v1.json";
const LEGACY_MANIFEST_REMOTE_PATH: &str = "addons/v1/renodx.json";

/// Loads the RenoDX v2 tool catalogue, with the CDN v1 compatibility document
/// used only when the v2 transport is unavailable. It contains no ReShade URLs.
pub async fn get_or_fetch_manifest() -> Result<RenoDxManifest, ServiceError> {
    match reshade_store::get_or_fetch_tool_catalog(
        MANIFEST_FILE_NAME,
        MANIFEST_REMOTE_PATH,
        parse_manifest_v2,
    )
    .await
    {
        Ok(manifest) => Ok(manifest),
        Err(error) if is_v2_contract_error(&error) => Err(error),
        Err(error) => {
            tracing::warn!(
                "RenoDX v2 manifest is unavailable ({error}); trying the legacy v1 catalogue"
            );
            reshade_store::get_or_fetch_tool_catalog(
                LEGACY_MANIFEST_FILE_NAME,
                LEGACY_MANIFEST_REMOTE_PATH,
                parse_manifest,
            )
            .await
        }
    }
}

fn is_v2_contract_error(error: &ServiceError) -> bool {
    matches!(
        error,
        ServiceError::ManifestContractRejected {
            contract: crate::ManifestContract::RenoDxV2,
            ..
        }
    )
}

/// Force-fetches only the raw RenoDX tool catalogue.
pub async fn fetch_manifest() -> Result<RenoDxManifest, ServiceError> {
    reshade_store::fetch_tool_catalog(MANIFEST_FILE_NAME, MANIFEST_REMOTE_PATH, parse_manifest_v2)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_v2_is_terminal_but_transport_unavailability_can_use_v1() {
        let malformed = parse_manifest_v2(b"not json").expect_err("malformed v2");
        assert!(is_v2_contract_error(&malformed));
        assert!(!is_v2_contract_error(&ServiceError::command_failed(
            "manifest fetch failed with status 404"
        )));
    }
}
