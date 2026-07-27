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
    let app_id = normalize_steam_app_id(app_id).ok_or(ServiceError::CoverNotFound)?;
    let url = steam_library_cover_url(app_id);
    let bytes = download_unvalidated_cover(client, &url)?;

    validate_steam_library_cover(&bytes)?;

    Ok(bytes)
}

fn normalize_steam_app_id(app_id: &str) -> Option<&str> {
    let app_id = app_id.trim();

    (!app_id.is_empty() && app_id.chars().all(|character| character.is_ascii_digit()))
        .then_some(app_id)
}

fn steam_library_cover_url(app_id: &str) -> String {
    format!("{STEAM_CDN_BASE_URL}/{app_id}/{STEAM_LIBRARY_COVER_FILENAME}")
}

fn validate_steam_library_cover(bytes: &[u8]) -> Result<(), ServiceError> {
    validate_cover_bytes(bytes)
        .map(|_| ())
        .map_err(|_| ServiceError::CoverNotFound)
}

#[cfg(test)]
mod tests {
    use super::{normalize_steam_app_id, steam_library_cover_url};

    #[test]
    fn accepts_only_numeric_app_ids() {
        assert_eq!(normalize_steam_app_id(" 22380 "), Some("22380"));
        assert_eq!(normalize_steam_app_id(""), None);
        assert_eq!(normalize_steam_app_id("../22380"), None);
        assert_eq!(normalize_steam_app_id("store-22380"), None);
    }

    #[test]
    fn builds_only_the_vertical_library_asset_url() {
        assert_eq!(
            steam_library_cover_url("22380"),
            "https://cdn.akamai.steamstatic.com/steam/apps/22380/library_600x900.jpg"
        );
    }
}
