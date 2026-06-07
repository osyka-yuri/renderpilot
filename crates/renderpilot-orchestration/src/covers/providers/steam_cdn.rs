//! Steam CDN vertical library asset only (`library_600x900.jpg`).

use reqwest::blocking::Client;

use super::super::validation::validate_cover_bytes;
use super::download::download_unvalidated_cover;
use crate::ServiceError;

const STEAM_CDN_BASE_URL: &str = "https://cdn.akamai.steamstatic.com/steam/apps";
const STEAM_LIBRARY_COVER_FILENAME: &str = "library_600x900.jpg";

/// Tries to download the Steam vertical library cover for the given app id.
///
/// Steam exposes multiple capsule/header images, but only `library_600x900.jpg`
/// matches the vertical cover format expected by the UI. Horizontal fallbacks are
/// intentionally not used because they can produce misleading artwork.
pub(super) fn try_steam_cdn(client: &Client, app_id: &str) -> Result<Vec<u8>, ServiceError> {
    let url = steam_library_cover_url(app_id);
    let bytes = download_unvalidated_cover(client, &url)?;

    validate_steam_library_cover(&bytes)?;

    Ok(bytes)
}

fn steam_library_cover_url(app_id: &str) -> String {
    format!("{STEAM_CDN_BASE_URL}/{app_id}/{STEAM_LIBRARY_COVER_FILENAME}")
}

fn validate_steam_library_cover(bytes: &[u8]) -> Result<(), ServiceError> {
    validate_cover_bytes(bytes)
        .map(|_| ())
        .map_err(|_| ServiceError::CoverNotFound)
}
