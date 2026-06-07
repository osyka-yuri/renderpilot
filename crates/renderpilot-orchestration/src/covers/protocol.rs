//! `rp-cover:` custom protocol: map URL path → catalog row → file bytes.

use std::fs;
use std::path::Path;

use http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use http::{HeaderName, HeaderValue, Response, StatusCode};
use renderpilot_domain::GameId;

use super::basename::cover_basename_is_safe;
use super::paths::covers_directory;
use super::validation::mime_for_bytes;
use crate::storage;

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

/// Handles an `rp-cover:` protocol request and returns an HTTP response with the cover bytes.
///
/// Takes the shared [`crate::Context`] so cover requests reuse the one managed
/// SQLite connection instead of opening a fresh one per image.
pub fn cover_protocol_http_response(
    context: &crate::Context,
    request_path: &str,
) -> Response<Vec<u8>> {
    match load_cover_payload(context, request_path) {
        Ok(payload) => ok_response(payload),
        Err(_) => empty_response(StatusCode::NOT_FOUND),
    }
}

fn load_cover_payload(
    context: &crate::Context,
    request_path: &str,
) -> Result<CoverPayload, CoverProtocolError> {
    let game_id = game_id_from_request_path(request_path)?;

    let record = context
        .storage()
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
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, payload.content_type)
        .header(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));

    if let Ok(content_length) = HeaderValue::from_str(&payload.bytes.len().to_string()) {
        builder = builder.header(CONTENT_LENGTH, content_length);
    }

    or_unavailable(builder.body(payload.bytes))
}

/// Upholds the `rp-cover` handler's "always answer, never panic" contract by
/// degrading a (theoretically) failed response build to a `NOT_FOUND` miss
/// instead of unwrapping.
fn or_unavailable(result: Result<Response<Vec<u8>>, http::Error>) -> Response<Vec<u8>> {
    result.unwrap_or_else(|_| empty_response(StatusCode::NOT_FOUND))
}

/// Infallible primitive: a body-less response carrying only the given status.
fn empty_response(status: StatusCode) -> Response<Vec<u8>> {
    let mut response = Response::new(Vec::new());
    *response.status_mut() = status;
    response
}

/// Response served when a cover cannot be produced (for example, when the shared
/// context is not yet managed). Callers outside this crate use this instead of
/// building HTTP responses themselves, keeping all response shaping in one place.
#[must_use]
pub fn cover_unavailable_response() -> Response<Vec<u8>> {
    empty_response(StatusCode::NOT_FOUND)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(bytes: Vec<u8>) -> CoverPayload {
        CoverPayload {
            bytes,
            content_type: HeaderValue::from_static("image/png"),
        }
    }

    #[test]
    fn request_path_rejects_empty_paths() {
        assert!(matches!(
            game_id_from_request_path(""),
            Err(CoverProtocolError::EmptyPath)
        ));
        assert!(matches!(
            game_id_from_request_path("/"),
            Err(CoverProtocolError::EmptyPath)
        ));
    }

    #[test]
    fn request_path_decodes_percent_encoding() {
        let game_id = game_id_from_request_path("/abc%2D1").expect("valid id");
        assert_eq!(game_id.as_str(), "abc-1");
    }

    #[test]
    fn request_path_rejects_blank_decoded_id() {
        // "%20" decodes to a single space, which `GameId` rejects after trimming.
        assert!(matches!(
            game_id_from_request_path("/%20"),
            Err(CoverProtocolError::InvalidGameId)
        ));
    }

    #[test]
    fn ok_response_carries_payload_and_security_headers() {
        let response = ok_response(payload(b"cover-bytes".to_vec()));

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CONTENT_TYPE], "image/png");
        assert_eq!(response.headers()[X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(response.headers()[CONTENT_LENGTH], "11");
        assert_eq!(response.body(), b"cover-bytes");
    }

    #[test]
    fn unavailable_response_is_an_empty_not_found() {
        let response = cover_unavailable_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(response.body().is_empty());
    }

    #[test]
    fn or_unavailable_passes_through_ok_builds() {
        let built = Response::builder()
            .status(StatusCode::OK)
            .body(b"ok".to_vec())
            .expect("valid response");

        let response = or_unavailable(Ok(built));

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.body(), b"ok");
    }

    #[test]
    fn or_unavailable_degrades_failed_builds_without_panicking() {
        // An invalid header name makes `body()` return an error; the handler must
        // still answer rather than panic.
        let failed = Response::builder()
            .header("inva\nlid", "x")
            .body(Vec::new());
        assert!(failed.is_err(), "expected the builder to capture an error");

        let response = or_unavailable(failed);

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(response.body().is_empty());
    }
}
