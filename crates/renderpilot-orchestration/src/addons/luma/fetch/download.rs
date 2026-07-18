//! HTTP download of a Luma release ZIP into a [`LumaPayload`].

use renderpilot_domain::Architecture;

use crate::ServiceError;
use crate::addons::reshade::fetch::sha256_hex;
use crate::net::{ProgressObserver, download_with_validators_and_final_url};

use super::super::source;
use super::extract::{MAX_ZIP_BYTES, extract_luma_payload};
use super::types::LumaPayload;

/// Downloads and extracts the release asset for its catalogue identity.
pub(crate) async fn fetch_luma_payload(
    asset: &str,
    addon_file: &str,
    arch: Architecture,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<LumaPayload, ServiceError> {
    let url = source::asset_url(asset);
    let label = format!("Luma Framework {asset}");
    download_and_extract_luma_zip(&url, addon_file, arch, progress, &label).await
}

/// Downloads and extracts a Luma release ZIP from an absolute catalogue URL.
async fn download_and_extract_luma_zip(
    url: &str,
    addon_file: &str,
    arch: Architecture,
    progress: Option<&ProgressObserver<'_>>,
    label: &str,
) -> Result<LumaPayload, ServiceError> {
    let (zip_bytes, validators, final_url) =
        download_with_validators_and_final_url(url, MAX_ZIP_BYTES, label, progress).await?;
    let zip_digest = sha256_hex(&zip_bytes);
    let build_number = source::parse_build_number(&final_url);
    let (files, main_addon_rel) = extract_luma_payload(&zip_bytes, addon_file, arch)?;
    Ok(LumaPayload {
        files,
        main_addon_rel,
        zip_digest,
        etag: validators.cache_validator(),
        last_modified: validators.last_modified,
        build_number,
    })
}
