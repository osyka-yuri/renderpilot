//! HTTP client and download helpers for library manifests and archives.

use std::sync::OnceLock;
use std::time::Duration;

use reqwest::{Client, Response, Url};

use crate::ServiceError;

use super::library_error;

const HTTP_TIMEOUT: Duration = Duration::from_secs(60);
const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

pub(super) fn http_client() -> &'static Client {
    HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(HTTP_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .expect("failed to create global HTTP client")
    })
}

pub(super) async fn get_successful_response(
    client: &Client,
    url: &str,
    operation: &str,
) -> Result<Response, ServiceError> {
    let url = parse_https_url(url, operation)?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| library_error(format!("{operation} failed: {error}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(library_error(format!(
            "{operation} failed with status {status}"
        )));
    }

    Ok(response)
}

pub(super) async fn download_limited_bytes(
    client: &Client,
    url: &str,
    max_size_bytes: u64,
    operation: &str,
) -> Result<Vec<u8>, ServiceError> {
    let response = get_successful_response(client, url, operation).await?;
    ensure_content_length_at_most(operation, response.content_length(), max_size_bytes)?;

    let bytes = response
        .bytes()
        .await
        .map_err(|error| library_error(format!("failed to read {operation} response: {error}")))?;

    if bytes.len() as u64 > max_size_bytes {
        return Err(library_error(format!(
            "{operation} response is too large: expected at most {max_size_bytes} bytes, got {} bytes",
            bytes.len()
        )));
    }

    Ok(bytes.to_vec())
}

pub(super) async fn download_exact_bytes(
    client: &Client,
    url: &str,
    expected_size_bytes: u64,
    operation: &str,
) -> Result<Vec<u8>, ServiceError> {
    let response = get_successful_response(client, url, operation).await?;
    ensure_exact_content_length(operation, response.content_length(), expected_size_bytes)?;

    let bytes = response
        .bytes()
        .await
        .map_err(|error| library_error(format!("failed to read {operation} response: {error}")))?;

    if bytes.len() as u64 != expected_size_bytes {
        return Err(library_error(format!(
            "{operation} size mismatch: expected {expected_size_bytes} bytes, got {} bytes",
            bytes.len()
        )));
    }

    Ok(bytes.to_vec())
}

pub(super) fn parse_https_url(url: &str, operation: &str) -> Result<Url, ServiceError> {
    let url = Url::parse(url)
        .map_err(|error| library_error(format!("invalid URL for {operation}: {error}")))?;

    if url.scheme() != "https" {
        return Err(library_error(format!(
            "invalid URL for {operation}: only HTTPS URLs are allowed"
        )));
    }

    Ok(url)
}

fn ensure_content_length_at_most(
    operation: &str,
    content_length: Option<u64>,
    max_size_bytes: u64,
) -> Result<(), ServiceError> {
    if let Some(content_length) = content_length {
        if content_length > max_size_bytes {
            return Err(library_error(format!(
                "{operation} response is too large: expected at most {max_size_bytes} bytes, got {content_length} bytes"
            )));
        }
    }

    Ok(())
}

fn ensure_exact_content_length(
    operation: &str,
    content_length: Option<u64>,
    expected_size_bytes: u64,
) -> Result<(), ServiceError> {
    if let Some(content_length) = content_length {
        if content_length != expected_size_bytes {
            return Err(library_error(format!(
                "{operation} size mismatch: expected {expected_size_bytes} bytes, got {content_length} bytes"
            )));
        }
    }

    Ok(())
}
