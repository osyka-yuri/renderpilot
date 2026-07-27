//! First-party Steam artwork from the public CDN and official store search.

use reqwest::blocking::Client;

use super::{steam_cdn, steam_store};
use crate::ServiceError;

/// Resolves Steam's vertical artwork without accepting horizontal capsules.
///
/// The canonical CDN is authoritative. Regional package IDs are resolved
/// through Steam's store search using an exact normalized title match.
pub(super) fn try_steam_artwork(
    client: &Client,
    app_id: &str,
    title: &str,
) -> Result<Vec<u8>, ServiceError> {
    let direct_error = match steam_cdn::try_steam_cdn(client, app_id) {
        Ok(bytes) => return Ok(bytes),
        Err(error) => error,
    };

    if let Ok(bytes) = steam_store::try_canonical_steam_cover(client, app_id, title) {
        return Ok(bytes);
    }

    Err(direct_error)
}
