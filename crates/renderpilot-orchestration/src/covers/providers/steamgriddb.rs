//! SteamGridDB API: autocomplete search and vertical grid candidates.

use reqwest::{
    blocking::{Client, Response},
    StatusCode, Url,
};
use serde::{de::DeserializeOwned, Deserialize};

use super::super::validation::validate_cover_bytes;
use super::download::download_unvalidated_cover;
use crate::ServiceError;

const STEAMGRIDDB_API_BASE: &str = "https://www.steamgriddb.com/api/v2";

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SteamGridDbAutocompleteBody {
    Wrapped {
        #[serde(default)]
        data: Option<Vec<SteamGridDbAutocompleteItem>>,
    },
    Bare(Vec<SteamGridDbAutocompleteItem>),
}

impl SteamGridDbAutocompleteBody {
    fn into_items(self) -> Vec<SteamGridDbAutocompleteItem> {
        match self {
            Self::Wrapped { data } => data.unwrap_or_default(),
            Self::Bare(items) => items,
        }
    }
}

#[derive(Debug, Deserialize)]
struct SteamGridDbAutocompleteItem {
    id: SteamGridDbId,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SteamGridDbId {
    Number(serde_json::Number),
    String(String),
}

impl SteamGridDbId {
    fn into_grid_lookup_key(self) -> Option<String> {
        match self {
            Self::Number(number) => number_as_non_negative_integer_string(&number),
            Self::String(value) => string_as_non_negative_integer_string(&value),
        }
    }
}

#[derive(Debug, Deserialize)]
struct SteamGridDbGridsResponse {
    #[serde(default)]
    success: bool,

    #[serde(default)]
    data: Vec<SteamGridDbGridEntry>,
}

#[derive(Debug, Deserialize)]
struct SteamGridDbGridEntry {
    #[serde(default)]
    mime: Option<String>,

    #[serde(default)]
    url: Option<String>,
}

impl SteamGridDbGridEntry {
    fn image_url(&self) -> Option<&str> {
        let url = self.url.as_deref()?.trim();

        if url.is_empty() || self.looks_like_svg(url) {
            return None;
        }

        Some(url)
    }

    fn looks_like_svg(&self, url: &str) -> bool {
        self.mime
            .as_deref()
            .is_some_and(|mime| mime.to_ascii_lowercase().contains("svg"))
            || url.to_ascii_lowercase().contains(".svg")
    }
}

fn number_as_non_negative_integer_string(number: &serde_json::Number) -> Option<String> {
    if let Some(value) = number.as_u64() {
        return Some(value.to_string());
    }

    number
        .as_i64()
        .filter(|value| *value >= 0)
        .map(|value| value.to_string())
}

fn string_as_non_negative_integer_string(value: &str) -> Option<String> {
    value
        .trim()
        .parse::<u64>()
        .ok()
        .map(|value| value.to_string())
}

fn steamgriddb_url(path_segments: &[&str]) -> Result<Url, ServiceError> {
    let mut url = Url::parse(STEAMGRIDDB_API_BASE).map_err(download_failed)?;

    {
        let mut segments = url.path_segments_mut().map_err(|_| {
            ServiceError::CoverDownloadFailed("invalid SteamGridDB base URL".into())
        })?;

        segments.extend(path_segments);
    }

    Ok(url)
}

fn steamgriddb_get_json<T>(client: &Client, api_key: &str, url: Url) -> Result<T, ServiceError>
where
    T: DeserializeOwned,
{
    let response = client
        .get(url)
        .bearer_auth(api_key)
        .send()
        .map_err(download_failed)?;

    parse_success_json(response)
}

fn parse_success_json<T>(response: Response) -> Result<T, ServiceError>
where
    T: DeserializeOwned,
{
    let response = require_success_status(response)?;

    response.json().map_err(download_failed)
}

fn require_success_status(response: Response) -> Result<Response, ServiceError> {
    let status = response.status();

    if status.is_success() {
        return Ok(response);
    }

    if status == StatusCode::NOT_FOUND {
        return Err(ServiceError::CoverNotFound);
    }

    Err(ServiceError::CoverDownloadFailed(format!(
        "SteamGridDB request failed with status {status}"
    )))
}

fn download_failed(error: impl ToString) -> ServiceError {
    ServiceError::CoverDownloadFailed(error.to_string())
}

fn steamgriddb_autocomplete_first_id(
    client: &Client,
    api_key: &str,
    title: &str,
) -> Result<String, ServiceError> {
    let url = steamgriddb_url(&["search", "autocomplete", title])?;

    let body: SteamGridDbAutocompleteBody = steamgriddb_get_json(client, api_key, url)?;

    body.into_items()
        .into_iter()
        .find_map(|item| item.id.into_grid_lookup_key())
        .ok_or(ServiceError::CoverNotFound)
}

fn steamgriddb_download_first_grid(
    client: &Client,
    api_key: &str,
    game_ref: &str,
) -> Result<Vec<u8>, ServiceError> {
    let url = steamgriddb_url(&["grids", "game", game_ref])?;

    let parsed: SteamGridDbGridsResponse = steamgriddb_get_json(client, api_key, url)?;

    if !parsed.success {
        return Err(ServiceError::CoverNotFound);
    }

    download_first_valid_grid(client, parsed.data)
}

fn download_first_valid_grid(
    client: &Client,
    entries: Vec<SteamGridDbGridEntry>,
) -> Result<Vec<u8>, ServiceError> {
    let mut last_download_error = None;

    for image_url in entries.iter().filter_map(SteamGridDbGridEntry::image_url) {
        match download_validated_cover(client, image_url) {
            Ok(bytes) => return Ok(bytes),
            Err(ServiceError::CoverNotFound) => {
                // Invalid candidate, missing URL, unsupported format, or failed validation.
                continue;
            }
            Err(error) => {
                // Keep trying other candidates, but do not completely hide real download failures.
                last_download_error = Some(error);
            }
        }
    }

    Err(last_download_error.unwrap_or(ServiceError::CoverNotFound))
}

fn download_validated_cover(client: &Client, image_url: &str) -> Result<Vec<u8>, ServiceError> {
    let bytes = download_unvalidated_cover(client, image_url)?;

    validate_cover_bytes(&bytes).map_err(|_| ServiceError::CoverNotFound)?;

    Ok(bytes)
}

/// SteamGridDB lookup by stable slug only, e.g. `steam-{app_id}`.
/// No title fallback.
pub(super) fn try_download_grid_for_slug(
    client: &Client,
    api_key: &str,
    slug: &str,
) -> Result<Vec<u8>, ServiceError> {
    steamgriddb_download_first_grid(client, api_key, slug)
}

/// Try GridDB `slug` first, e.g. `gog-{product_id}`,
/// then autocomplete by `title` and fetch grids again.
pub(super) fn try_download_grid_slug_then_autocomplete(
    client: &Client,
    api_key: &str,
    slug: &str,
    title: &str,
) -> Result<Vec<u8>, ServiceError> {
    match steamgriddb_download_first_grid(client, api_key, slug) {
        Ok(bytes) => Ok(bytes),
        Err(ServiceError::CoverNotFound) => {
            let grid_key = steamgriddb_autocomplete_first_id(client, api_key, title)?;
            steamgriddb_download_first_grid(client, api_key, &grid_key)
        }
        Err(error) => Err(error),
    }
}

/// Title-only GridDB path for non-Steam/non-GOG launchers.
pub(super) fn try_download_grid_autocomplete_only(
    client: &Client,
    api_key: &str,
    title: &str,
) -> Result<Vec<u8>, ServiceError> {
    let grid_key = steamgriddb_autocomplete_first_id(client, api_key, title)?;
    steamgriddb_download_first_grid(client, api_key, &grid_key)
}
