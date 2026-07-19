//! HTTP download of a Luma release ZIP into a [`LumaPayload`].

use renderpilot_domain::Architecture;

use crate::ServiceError;
use crate::addons::reshade::fetch::sha256_hex;
use crate::net::{ProgressObserver, download_with_url_chain};

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
    let download = download_with_url_chain(url, MAX_ZIP_BYTES, label, progress).await?;
    let zip_digest = sha256_hex(&download.bytes);
    // GitHub's final hop is a CDN URL without `latest-<n>`; scan the full chain.
    let build_number = source::parse_build_number_from_chain(&download.url_chain);
    let (files, main_addon_rel) = extract_luma_payload(&download.bytes, addon_file, arch)?;
    Ok(LumaPayload {
        files,
        main_addon_rel,
        zip_digest,
        etag: download.validators.cache_validator(),
        last_modified: download.validators.last_modified,
        build_number,
    })
}
