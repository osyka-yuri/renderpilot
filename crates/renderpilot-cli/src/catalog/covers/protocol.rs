//! `rp-cover:` custom protocol: map URL path → catalog row → file bytes.

use std::fs;
use std::path::Path;

use http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use http::{HeaderName, HeaderValue, Response, StatusCode};
use renderpilot_domain::GameId;

use super::basename::cover_basename_is_safe;
use super::paths::covers_directory;
use super::validation::mime_for_bytes;
use crate::catalog::{open_catalog_storage, storage};

const X_CONTENT_TYPE_OPTIONS: HeaderName = HeaderName::from_static("x-content-type-options");

#[derive(Debug)]
enum CoverProtocolError {
    EmptyPath,
    InvalidPathEncoding,
    InvalidGameId,
    CatalogUnavailable,
    CoverNotFound,
    UnsafeCoverFileName,
    CatalogPathUnavailable,
    InvalidCoverFile,
    CoverReadFailed,
}

struct CoverPayload {
    bytes: Vec<u8>,
    content_type: HeaderValue,
}

pub(crate) fn cover_protocol_http_response(request_path: &str) -> Response<Vec<u8>> {
    match load_cover_payload(request_path) {
        Ok(payload) => ok_response(payload),
        Err(_err) => {
            // Если в проекте есть tracing/log, здесь удобно оставить debug-лог:
            // tracing::debug!(?err, request_path, "failed to load cover");
            empty_response(StatusCode::NOT_FOUND)
        }
    }
}

fn load_cover_payload(request_path: &str) -> Result<CoverPayload, CoverProtocolError> {
    let game_id = game_id_from_request_path(request_path)?;

    let sqlite = open_catalog_storage().map_err(|_| CoverProtocolError::CatalogUnavailable)?;

    let record = sqlite
        .find_game_cover(&game_id)
        .map_err(|_| CoverProtocolError::CatalogUnavailable)?
        .ok_or(CoverProtocolError::CoverNotFound)?;

    if !cover_basename_is_safe(&record.file_name) {
        return Err(CoverProtocolError::UnsafeCoverFileName);
    }

    let catalog_path =
        storage::catalog_database_path().map_err(|_| CoverProtocolError::CatalogPathUnavailable)?;

    let cover_path = covers_directory(&catalog_path).join(&record.file_name);
    let bytes = read_regular_file(&cover_path)?;

    let content_type = HeaderValue::from_str(mime_for_bytes(&bytes))
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));

    Ok(CoverPayload {
        bytes,
        content_type,
    })
}

fn game_id_from_request_path(request_path: &str) -> Result<GameId, CoverProtocolError> {
    let trimmed = request_path.trim_start_matches('/');

    if trimmed.is_empty() {
        return Err(CoverProtocolError::EmptyPath);
    }

    let decoded = urlencoding::decode(trimmed)
        .map_err(|_| CoverProtocolError::InvalidPathEncoding)?
        .into_owned();

    GameId::new(decoded).map_err(|_| CoverProtocolError::InvalidGameId)
}

fn read_regular_file(path: &Path) -> Result<Vec<u8>, CoverProtocolError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| CoverProtocolError::CoverReadFailed)?;

    if !metadata.file_type().is_file() {
        return Err(CoverProtocolError::InvalidCoverFile);
    }

    fs::read(path).map_err(|_| CoverProtocolError::CoverReadFailed)
}

fn ok_response(payload: CoverPayload) -> Response<Vec<u8>> {
    let mut response = Response::new(payload.bytes);

    *response.status_mut() = StatusCode::OK;

    response
        .headers_mut()
        .insert(CONTENT_TYPE, payload.content_type);

    response
        .headers_mut()
        .insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));

    if let Ok(content_length) = HeaderValue::from_str(&response.body().len().to_string()) {
        response
            .headers_mut()
            .insert(CONTENT_LENGTH, content_length);
    }

    response
}

fn empty_response(status: StatusCode) -> Response<Vec<u8>> {
    let mut response = Response::new(Vec::new());
    *response.status_mut() = status;
    response
}
