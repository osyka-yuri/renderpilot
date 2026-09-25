//! Streaming body readers and payload integrity validation.

use reqwest::Response;
use reqwest::header::CONTENT_RANGE;

use crate::ServiceError;

use super::{DownloadProgress, HttpValidators, ProgressObserver, transport::HttpFetchFailure};
/// Streams a manifest-pinned payload, rejecting both a contradictory response
/// header and a body whose final length differs from the pinned size.
pub(super) async fn read_exact_body(
    response: Response,
    expected_size_bytes: u64,
    operation: &str,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<Vec<u8>, HttpFetchFailure> {
    let capacity = usize::try_from(expected_size_bytes).unwrap_or(0);
    let mut bytes = Vec::with_capacity(capacity);
    append_exact_body(
        response,
        &mut bytes,
        expected_size_bytes,
        operation,
        progress,
    )
    .await?;
    Ok(bytes)
}

pub(super) async fn append_exact_body(
    mut response: Response,
    bytes: &mut Vec<u8>,
    expected_size_bytes: u64,
    operation: &str,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<(), HttpFetchFailure> {
    let downloaded = u64::try_from(bytes.len()).map_err(|_| {
        HttpFetchFailure::Permanent(crate::failed(format!(
            "{operation} size exceeds supported platform limits"
        )))
    })?;
    let remaining = expected_size_bytes.checked_sub(downloaded).ok_or_else(|| {
        HttpFetchFailure::Permanent(crate::failed(format!(
            "{operation} size mismatch: expected {expected_size_bytes} bytes, got {downloaded} bytes"
        )))
    })?;
    ensure_exact_content_length(operation, response.content_length(), remaining)
        .map_err(HttpFetchFailure::Permanent)?;

    let mut downloaded = downloaded;
    let report = |downloaded: u64| {
        if let Some(observe) = progress {
            observe(DownloadProgress {
                downloaded_bytes: downloaded,
                total_bytes: expected_size_bytes,
                phase: Some(operation),
            });
        }
    };

    report(downloaded);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| HttpFetchFailure::Body(error.to_string()))?
    {
        bytes.extend_from_slice(&chunk);
        downloaded += chunk.len() as u64;
        if downloaded > expected_size_bytes {
            return Err(HttpFetchFailure::Permanent(crate::failed(format!(
                "{operation} size mismatch: expected {expected_size_bytes} bytes, got {downloaded} bytes"
            ))));
        }
        report(downloaded);
    }

    if bytes.len() as u64 != expected_size_bytes {
        return Err(HttpFetchFailure::Permanent(crate::failed(format!(
            "{operation} size mismatch: expected {expected_size_bytes} bytes, got {} bytes",
            bytes.len()
        ))));
    }
    Ok(())
}

pub(super) fn ensure_exact_range_response(
    response: &Response,
    start: u64,
    expected_size_bytes: u64,
    operation: &str,
) -> Result<(), ServiceError> {
    if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(crate::failed(format!(
            "{operation} server does not support resuming at byte {start} (status {})",
            response.status()
        )));
    }
    let content_range = response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            crate::failed(format!(
                "{operation} resume response is missing a valid Content-Range"
            ))
        })?;
    validate_exact_content_range(content_range, start, expected_size_bytes).map_err(|reason| {
        crate::failed(format!(
            "{operation} resume response has invalid Content-Range: {reason}"
        ))
    })
}

pub(super) fn validate_exact_content_range(
    value: &str,
    start: u64,
    expected_size_bytes: u64,
) -> Result<(), &'static str> {
    let (unit, range_and_total) = value.split_once(' ').ok_or("missing unit separator")?;
    if !unit.eq_ignore_ascii_case("bytes") {
        return Err("unit is not bytes");
    }
    let (range, total) = range_and_total.split_once('/').ok_or("missing total")?;
    let (actual_start, actual_end) = range.split_once('-').ok_or("missing byte range")?;
    let actual_start = actual_start
        .parse::<u64>()
        .map_err(|_| "invalid range start")?;
    let actual_end = actual_end.parse::<u64>().map_err(|_| "invalid range end")?;
    let total = total.parse::<u64>().map_err(|_| "invalid total")?;
    let expected_end = expected_size_bytes.checked_sub(1).ok_or("empty payload")?;
    if actual_start != start || actual_end != expected_end || total != expected_size_bytes {
        return Err("range does not match the manifest-pinned payload");
    }
    Ok(())
}

pub(super) fn same_payload_validator(first: &HttpValidators, resumed: &HttpValidators) -> bool {
    match (first.cache_validator(), resumed.cache_validator()) {
        (Some(first), Some(resumed)) => first == resumed,
        _ => true,
    }
}

/// Streams a response body into memory, enforcing `max_size_bytes` as it arrives
/// and reporting download progress when the total size is known. The shared core
/// of every capped download helper.
pub(super) async fn read_capped_body(
    mut response: Response,
    max_size_bytes: u64,
    operation: &str,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<Vec<u8>, ServiceError> {
    ensure_content_length_at_most(operation, response.content_length(), max_size_bytes)?;

    let total = response.content_length().unwrap_or(0);
    let report = |downloaded: u64| {
        if total > 0
            && let Some(observe) = progress
        {
            observe(DownloadProgress {
                downloaded_bytes: downloaded,
                total_bytes: total,
                phase: Some(operation),
            });
        }
    };

    let capacity = usize::try_from(total.min(max_size_bytes)).unwrap_or(0);
    let mut bytes = Vec::with_capacity(capacity);
    let mut downloaded: u64 = 0;
    report(0);

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| crate::failed(format!("failed to read {operation} chunk: {error}")))?
    {
        bytes.extend_from_slice(&chunk);
        downloaded += chunk.len() as u64;
        if downloaded > max_size_bytes {
            return Err(crate::failed(format!(
                "{operation} response is too large: expected at most {max_size_bytes} bytes"
            )));
        }
        report(downloaded);
    }

    Ok(bytes)
}

pub(super) fn validators_of(response: &Response) -> HttpValidators {
    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    HttpValidators {
        etag: header("etag"),
        last_modified: header("last-modified"),
    }
}

pub(super) fn ensure_content_length_at_most(
    operation: &str,
    content_length: Option<u64>,
    max_size_bytes: u64,
) -> Result<(), ServiceError> {
    if let Some(content_length) = content_length
        && content_length > max_size_bytes
    {
        return Err(crate::failed(format!(
            "{operation} response is too large: expected at most {max_size_bytes} bytes, got {content_length} bytes"
        )));
    }

    Ok(())
}

pub(super) fn ensure_exact_content_length(
    operation: &str,
    content_length: Option<u64>,
    expected_size_bytes: u64,
) -> Result<(), ServiceError> {
    if let Some(content_length) = content_length
        && content_length != expected_size_bytes
    {
        return Err(crate::failed(format!(
            "{operation} size mismatch: expected {expected_size_bytes} bytes, got {content_length} bytes"
        )));
    }

    Ok(())
}
