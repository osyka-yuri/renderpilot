//! Byte download with size cap.
//! Image semantics are validated by callers.

use std::io::Read;

use reqwest::{
    blocking::{Client, Response},
    StatusCode,
};

use super::super::paths::MAX_COVER_BYTES;
use crate::error::CliError;

pub(super) fn download_unvalidated_cover(client: &Client, url: &str) -> Result<Vec<u8>, CliError> {
    let response = client.get(url).send().map_err(cover_download_failed)?;

    ensure_success_status(response.status())?;
    ensure_declared_size_is_allowed(&response)?;

    read_body_with_size_limit(response)
}

fn ensure_success_status(status: StatusCode) -> Result<(), CliError> {
    if status.is_success() {
        return Ok(());
    }

    match status {
        StatusCode::NOT_FOUND | StatusCode::GONE => Err(CliError::CoverNotFound),
        _ => Err(CliError::CoverDownloadFailed(format!(
            "cover download failed with HTTP status {status}"
        ))),
    }
}

fn ensure_declared_size_is_allowed(response: &Response) -> Result<(), CliError> {
    if let Some(content_length) = response.content_length() {
        if content_length > MAX_COVER_BYTES {
            return Err(cover_too_large());
        }
    }

    Ok(())
}

fn read_body_with_size_limit(response: Response) -> Result<Vec<u8>, CliError> {
    let max_len = usize::try_from(MAX_COVER_BYTES).map_err(|_| {
        CliError::CoverDownloadFailed("maximum cover size is not supported on this platform".into())
    })?;

    let read_limit = MAX_COVER_BYTES
        .checked_add(1)
        .ok_or_else(|| CliError::CoverDownloadFailed("maximum cover size is too large".into()))?;

    let initial_capacity = response
        .content_length()
        .and_then(|len| usize::try_from(len.min(MAX_COVER_BYTES)).ok())
        .unwrap_or(0);

    let mut bytes = Vec::with_capacity(initial_capacity);
    let mut limited_response = response.take(read_limit);

    limited_response
        .read_to_end(&mut bytes)
        .map_err(cover_download_failed)?;

    if bytes.len() > max_len {
        return Err(cover_too_large());
    }

    Ok(bytes)
}

fn cover_download_failed(error: impl ToString) -> CliError {
    CliError::CoverDownloadFailed(error.to_string())
}

fn cover_too_large() -> CliError {
    CliError::CoverDownloadFailed(format!(
        "downloaded cover exceeds maximum size of {MAX_COVER_BYTES} bytes"
    ))
}
