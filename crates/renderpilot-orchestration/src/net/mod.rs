//! Shared HTTP client and download helpers — provider-neutral foundation used by
//! every subsystem that fetches over the network (the graphics-library swapper, the
//! add-on installers, …). Nothing here knows about a specific host or payload type.
//!
//! All public helpers stream the body, enforce a size cap as bytes arrive, and
//! report progress when the total size is known; the cap and progress logic live in
//! exactly one place (`read_capped_body`). A single process-wide [`Client`] is
//! reused across calls.

use std::sync::OnceLock;
use std::time::Duration;

use reqwest::{Client, Response, Url};

use crate::ServiceError;

const HTTP_TIMEOUT: Duration = Duration::from_mins(1);
const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

static HTTP_CLIENT: OnceLock<Result<Client, String>> = OnceLock::new();

// ---------------------------------------------------------------------------
// Download-progress contract
// ---------------------------------------------------------------------------

/// Cumulative progress of a download, in bytes.
#[derive(Clone, Copy, Debug)]
pub struct DownloadProgress<'a> {
    /// Number of bytes received so far.
    pub downloaded_bytes: u64,
    /// Total expected size in bytes.
    pub total_bytes: u64,
    /// Optional label for the download phase (e.g. "RenoDX add-on ...").
    pub phase: Option<&'a str>,
}

/// Observer invoked as bytes arrive; must be cheap and non-blocking.
///
/// The lifetime parameter keeps the alias usable for non-`'static` observers,
/// e.g. per-member wrappers that borrow an outer observer.
pub type ProgressObserver<'a> = dyn Fn(DownloadProgress<'_>) + Send + Sync + 'a;

/// HTTP cache validators captured from a response, used for change detection.
#[derive(Debug, Clone, Default)]
pub(crate) struct HttpValidators {
    /// Strong/weak `ETag`, when present.
    pub etag: Option<String>,
    /// `Last-Modified`, when present.
    pub last_modified: Option<String>,
}

impl HttpValidators {
    /// The single cache validator used for the cheap "did it change?" pre-check:
    /// the `ETag` when present, otherwise `Last-Modified`. Centralized so the value
    /// stored at install time and the value compared at update time are always
    /// derived the same way (a drift would make the fast-path misfire).
    #[must_use]
    pub(crate) fn cache_validator(&self) -> Option<String> {
        self.etag.clone().or_else(|| self.last_modified.clone())
    }
}

fn err(message: impl Into<String>) -> ServiceError {
    ServiceError::CommandFailed(message.into())
}

/// The process-wide HTTPS client (lazily built, then reused). A failure to build
/// the client (e.g. a malformed TLS backend) surfaces as a `ServiceError` at the
/// first download rather than a process panic.
pub(crate) fn http_client() -> Result<&'static Client, ServiceError> {
    let result = HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(HTTP_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| format!("failed to create global HTTP client: {e}"))
    });
    result.as_ref().map_err(|e| err(e.clone()))
}

// ---------------------------------------------------------------------------
// Public download helpers (auto-supply the shared client)
// ---------------------------------------------------------------------------

/// Downloads up to `max_size_bytes` from `url`. For payloads whose final size is
/// not known up front (integrity is then established by the caller).
pub(crate) async fn download_limited_bytes(
    url: &str,
    max_size_bytes: u64,
    operation: &str,
) -> Result<Vec<u8>, ServiceError> {
    let response = get_successful_response(url, operation).await?;
    read_capped_body(response, max_size_bytes, operation, None).await
}

/// Downloads exactly `expected_size_bytes`, reporting progress. For uncompressed
/// payloads whose final size is known up front.
pub(crate) async fn download_exact_bytes(
    url: &str,
    expected_size_bytes: u64,
    operation: &str,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<Vec<u8>, ServiceError> {
    let mut response = get_successful_response(url, operation).await?;
    ensure_exact_content_length(operation, response.content_length(), expected_size_bytes)?;

    let capacity = usize::try_from(expected_size_bytes).unwrap_or(0);
    let mut bytes = Vec::with_capacity(capacity);
    let mut downloaded: u64 = 0;

    let report = |dl: u64| {
        if let Some(cb) = progress {
            cb(DownloadProgress {
                downloaded_bytes: dl,
                total_bytes: expected_size_bytes,
                phase: Some(operation),
            });
        }
    };

    report(0);

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| err(format!("failed to read {operation} chunk: {error}")))?
    {
        bytes.extend_from_slice(&chunk);
        downloaded += chunk.len() as u64;

        // Guard against a server that sends more data than declared; the final
        // length check below catches the exact mismatch.
        if downloaded > expected_size_bytes {
            break;
        }

        report(downloaded);
    }

    if bytes.len() as u64 != expected_size_bytes {
        return Err(err(format!(
            "{operation} size mismatch: expected {expected_size_bytes} bytes, got {} bytes",
            bytes.len()
        )));
    }

    Ok(bytes)
}

/// Downloads up to `max_size_bytes`, reporting progress, and returns the bytes plus
/// the response's cache validators (for change-detection / update tracking).
pub(crate) async fn download_with_validators(
    url: &str,
    max_size_bytes: u64,
    operation: &str,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<(Vec<u8>, HttpValidators), ServiceError> {
    let (bytes, validators, _final_url) =
        download_with_validators_and_final_url(url, max_size_bytes, operation, progress).await?;
    Ok((bytes, validators))
}

/// Like [`download_with_validators`], but also returns the final response URL
/// after redirects so callers with stricter provenance requirements can validate
/// redirect targets.
pub(crate) async fn download_with_validators_and_final_url(
    url: &str,
    max_size_bytes: u64,
    operation: &str,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<(Vec<u8>, HttpValidators, Url), ServiceError> {
    let response = get_successful_response(url, operation).await?;
    let final_url = response.url().clone();
    let validators = validators_of(&response);
    let bytes = read_capped_body(response, max_size_bytes, operation, progress).await?;
    Ok((bytes, validators, final_url))
}

/// Downloads up to `max_size_bytes` with an explicit `Referer` header (some hosts,
/// e.g. the nightly.link GitHub-artifact proxy, gate downloads on it), reporting
/// progress, returning the bytes plus the response's cache validators.
pub(crate) async fn download_with_referer(
    url: &str,
    referer: &str,
    max_size_bytes: u64,
    operation: &str,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<(Vec<u8>, HttpValidators), ServiceError> {
    let parsed = parse_https_url(url, operation)?;
    let response = http_client()?
        .get(parsed)
        .header(reqwest::header::REFERER, referer)
        .send()
        .await
        .map_err(|error| err(format!("{operation} failed: {error}")))?;
    if !response.status().is_success() {
        return Err(err(format!(
            "{operation} failed with status {}",
            response.status()
        )));
    }
    let validators = validators_of(&response);
    let bytes = read_capped_body(response, max_size_bytes, operation, progress).await?;
    Ok((bytes, validators))
}

/// Fetches just the cache validators for `url` via a `HEAD` request (a cheap
/// "did it change?" pre-check).
pub(crate) async fn head_validators(
    url: &str,
    operation: &str,
) -> Result<HttpValidators, ServiceError> {
    let (validators, _final_url) = head_validators_and_final_url(url, operation).await?;
    Ok(validators)
}

/// Like [`head_validators`], but also returns the final response URL after
/// redirects — for a rolling-release host whose redirect target itself encodes
/// a version (e.g. Luma's `releases/latest/download/<asset>` → `releases/download/latest-<n>/<asset>`).
pub(crate) async fn head_validators_and_final_url(
    url: &str,
    operation: &str,
) -> Result<(HttpValidators, Url), ServiceError> {
    let parsed = parse_https_url(url, operation)?;
    let response = http_client()?
        .head(parsed)
        .send()
        .await
        .map_err(|error| err(format!("{operation} failed: {error}")))?;
    if !response.status().is_success() {
        return Err(err(format!(
            "{operation} failed with status {}",
            response.status()
        )));
    }
    let final_url = response.url().clone();
    Ok((validators_of(&response), final_url))
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

async fn get_successful_response(url: &str, operation: &str) -> Result<Response, ServiceError> {
    let url = parse_https_url(url, operation)?;
    let response = http_client()?
        .get(url)
        .send()
        .await
        .map_err(|error| err(format!("{operation} failed: {error}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(err(format!("{operation} failed with status {status}")));
    }

    Ok(response)
}

/// Streams a response body into memory, enforcing `max_size_bytes` as it arrives
/// and reporting download progress when the total size is known. The shared core
/// of every capped download helper.
async fn read_capped_body(
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
        .map_err(|error| err(format!("failed to read {operation} chunk: {error}")))?
    {
        bytes.extend_from_slice(&chunk);
        downloaded += chunk.len() as u64;
        if downloaded > max_size_bytes {
            return Err(err(format!(
                "{operation} response is too large: expected at most {max_size_bytes} bytes"
            )));
        }
        report(downloaded);
    }

    Ok(bytes)
}

fn validators_of(response: &Response) -> HttpValidators {
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

/// Parses `url` and requires it to be HTTPS.
pub(crate) fn parse_https_url(url: &str, operation: &str) -> Result<Url, ServiceError> {
    let url =
        Url::parse(url).map_err(|error| err(format!("invalid URL for {operation}: {error}")))?;

    if url.scheme() != "https" {
        return Err(err(format!(
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
    if let Some(content_length) = content_length
        && content_length > max_size_bytes
    {
        return Err(err(format!(
            "{operation} response is too large: expected at most {max_size_bytes} bytes, got {content_length} bytes"
        )));
    }

    Ok(())
}

fn ensure_exact_content_length(
    operation: &str,
    content_length: Option<u64>,
    expected_size_bytes: u64,
) -> Result<(), ServiceError> {
    if let Some(content_length) = content_length
        && content_length != expected_size_bytes
    {
        return Err(err(format!(
            "{operation} size mismatch: expected {expected_size_bytes} bytes, got {content_length} bytes"
        )));
    }

    Ok(())
}
