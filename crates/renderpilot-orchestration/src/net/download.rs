//! Public download operations and the resumable exact-size download state machine.

use std::time::Duration;

use reqwest::{Method, Url};

use crate::{
    ServiceError,
    github_auth::{GitHubToken, is_github_auth_target},
};

use super::{
    HttpValidators, ProgressObserver, ValidatedDownload,
    body::{
        append_exact_body, ensure_exact_range_response, read_capped_body, read_exact_body,
        same_payload_validator, validators_of,
    },
    http_client, parse_https_url,
    transport::{
        HttpFetchFailure, follow_redirects, follow_redirects_classified, get_successful_response,
        head_successful_response, send_with_anonymous_retry,
    },
};

pub(super) const RESUME_RETRY_INITIAL_DELAY_MS: u64 = 250;
pub(super) const RESUME_RETRY_MAX_DELAY: Duration = Duration::from_secs(8);

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
    let response = get_successful_response(url, operation).await?;
    read_exact_body(response, expected_size_bytes, operation, progress)
        .await
        .map_err(|failure| failure.into_service_error(operation))
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

/// Like [`download_with_validators`], but records every redirect hop (start → … →
/// final). Use when identity is encoded on an intermediate URL that a CDN hop
/// would otherwise hide.
pub(crate) async fn download_with_url_chain(
    url: &str,
    max_size_bytes: u64,
    operation: &str,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<ValidatedDownload, ServiceError> {
    let followed = follow_redirects(Method::GET, url, operation).await?;
    let validators = followed.validators();
    let url_chain = followed.url_chain;
    let bytes = read_capped_body(followed.response, max_size_bytes, operation, progress).await?;
    Ok(ValidatedDownload {
        bytes,
        validators,
        url_chain,
    })
}

/// Like [`download_with_url_chain`], but requires the exact manifest-pinned
/// byte length. This keeps redirect provenance while allowing an honest
/// determinate progress indicator when a CDN omits `Content-Length`.
pub(crate) async fn download_exact_with_url_chain(
    url: &str,
    expected_size_bytes: u64,
    operation: &str,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<ValidatedDownload, ServiceError> {
    let capacity = usize::try_from(expected_size_bytes).unwrap_or(0);
    let mut bytes = Vec::with_capacity(capacity);
    let mut validators = None;
    let mut url_chain = Vec::new();
    let mut consecutive_resume_failures = 0;
    let parsed_url = parse_https_url(url, operation)?;
    let mut token = if is_github_auth_target(&parsed_url) {
        GitHubToken::from_local_machine_async().await
    } else {
        None
    };

    loop {
        // A body may fail after yielding every expected byte but before a clean
        // EOF. That response is not trusted as complete, so restart instead of
        // issuing the invalid range `bytes=<size>-`.
        if u64::try_from(bytes.len()).ok() == Some(expected_size_bytes) {
            bytes.clear();
            validators = None;
            url_chain.clear();
        }
        let range_start = u64::try_from(bytes.len()).ok().filter(|start| *start > 0);
        let followed = match follow_redirects_classified(
            Method::GET,
            &parsed_url,
            operation,
            range_start,
            token.as_ref(),
        )
        .await
        {
            Ok(followed) => followed,
            Err(HttpFetchFailure::AuthenticatedStatus(_)) if token.is_some() => {
                // The anonymous attempt starts at the original URL. Discard
                // bytes, validators, and provenance collected with the token
                // so a resumed body cannot mix authenticated and anonymous
                // responses.
                restart_anonymously(&mut token, &mut bytes, &mut validators, &mut url_chain);
                consecutive_resume_failures = 0;
                continue;
            }
            Err(failure) => {
                if let Some((message, delay)) =
                    resume_retry(&failure, bytes.len(), consecutive_resume_failures)
                {
                    consecutive_resume_failures = consecutive_resume_failures.saturating_add(1);
                    tracing::warn!(
                        "{operation} resume attempt {consecutive_resume_failures} failed: {message}; retrying from byte {}",
                        bytes.len(),
                    );
                    tokio::time::sleep(delay).await;
                    continue;
                }
                return Err(failure.into_service_error(operation));
            }
        };
        let mut bytes_before_body = bytes.len();
        if let Some(start) = range_start {
            if followed.response.status() == reqwest::StatusCode::OK {
                // The origin ignored Range and supplied a new full body. Drop
                // the incomplete prefix before consuming it; accepting it is
                // safe because it is not combined with prior response bytes.
                tracing::warn!(
                    "{operation} server ignored resume at byte {start}; restarting from this full response"
                );
                bytes.clear();
                validators = None;
                url_chain.clear();
                bytes_before_body = 0;
            } else {
                ensure_exact_range_response(
                    &followed.response,
                    start,
                    expected_size_bytes,
                    operation,
                )?;
            }
        }
        let response_validators = followed.validators();
        if let Some(first_validators) = &validators
            && !same_payload_validator(first_validators, &response_validators)
        {
            return Err(crate::failed(format!(
                "{operation} changed while a partial download was being resumed"
            )));
        }
        validators.get_or_insert(response_validators);
        url_chain.extend(followed.url_chain);

        match append_exact_body(
            followed.response,
            &mut bytes,
            expected_size_bytes,
            operation,
            progress,
        )
        .await
        {
            Ok(()) => {
                let validators = validators.ok_or_else(|| {
                    crate::failed(format!("{operation} completed without response validators"))
                })?;
                return Ok(ValidatedDownload {
                    bytes,
                    validators,
                    url_chain,
                });
            }
            Err(failure) => {
                if bytes.len() > bytes_before_body {
                    consecutive_resume_failures = 0;
                }
                if let Some((message, delay)) =
                    resume_retry(&failure, bytes.len(), consecutive_resume_failures)
                {
                    consecutive_resume_failures = consecutive_resume_failures.saturating_add(1);
                    tracing::warn!(
                        "{operation} resume attempt {consecutive_resume_failures} failed: {message}; retrying from byte {}",
                        bytes.len(),
                    );
                    tokio::time::sleep(delay).await;
                    continue;
                }
                return Err(failure.into_service_error(operation));
            }
        }
    }
}

pub(super) fn restart_anonymously<T>(
    token: &mut Option<T>,
    bytes: &mut Vec<u8>,
    validators: &mut Option<HttpValidators>,
    url_chain: &mut Vec<Url>,
) {
    *token = None;
    bytes.clear();
    *validators = None;
    url_chain.clear();
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
    let client = http_client()?;
    let token = if is_github_auth_target(&parsed) {
        GitHubToken::from_local_machine_async().await
    } else {
        None
    };
    let mut authorization = token
        .as_ref()
        .and_then(|token| token.authorization_header_for_url(&parsed));
    let response = send_with_anonymous_retry(authorization.is_some(), |use_authorization| {
        let mut request = client
            .get(parsed.clone())
            .header(reqwest::header::REFERER, referer);
        if use_authorization && let Some(header) = authorization.take() {
            request = request.header(crate::github_auth::authorization_header_name(), header);
        }
        request.send()
    })
    .await
    .map_err(|error| crate::failed(format!("{operation} failed: {error}")))?;
    if !response.status().is_success() {
        return Err(crate::failed(format!(
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
    let response = head_successful_response(url, operation).await?;
    Ok(validators_of(&response))
}

/// HEAD request that also returns every redirect hop (start → … → final) so
/// callers can recover identity encoded on an intermediate URL.
pub(crate) async fn head_with_url_chain(
    url: &str,
    operation: &str,
) -> Result<(HttpValidators, Vec<Url>), ServiceError> {
    let followed = follow_redirects(Method::HEAD, url, operation).await?;
    Ok((followed.validators(), followed.url_chain))
}

// ---------------------------------------------------------------------------
pub(super) fn resume_retry(
    failure: &HttpFetchFailure,
    prefix_len: usize,
    consecutive_failures: u32,
) -> Option<(&str, Duration)> {
    if prefix_len == 0 {
        return None;
    }
    let message = match failure {
        HttpFetchFailure::Request(message) | HttpFetchFailure::Body(message) => message,
        HttpFetchFailure::AuthenticatedStatus(_) | HttpFetchFailure::Permanent(_) => return None,
    };
    let exponent = consecutive_failures.min(5);
    let multiplier = 1_u64 << exponent;
    let delay = Duration::from_millis(RESUME_RETRY_INITIAL_DELAY_MS.saturating_mul(multiplier))
        .min(RESUME_RETRY_MAX_DELAY);
    Some((message, delay))
}
