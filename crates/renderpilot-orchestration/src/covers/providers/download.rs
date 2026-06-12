//! Byte download with size cap and transient-failure retry.
//!
//! Image semantics (PNG/JPG/WEBP/GIF magic bytes, validation) are enforced by
//! callers via [`super::super::validation::validate_cover_bytes`].
//!
//! ## Retry policy
//!
//! Cover fetches happen in batches in the background sync and one-off from the
//! UI. Both Steam CDN and `*.gog-statics.com` occasionally serve a transient
//! 5xx or close the connection mid-stream — the original Black-Myth-Wukong
//! "broken cover" investigation showed that a re-run picked the file up
//! cleanly. To make this self-healing without user intervention, every
//! attempt that looks transient (network error, 5xx, 408, 429, mid-body read
//! error) is retried up to [`COVER_DOWNLOAD_MAX_ATTEMPTS`] times with
//! exponential backoff. Permanent failures (404, 410, oversize, other 4xx)
//! bubble up immediately.

use std::io::Read;
use std::thread;
use std::time::Duration;

use reqwest::{
    blocking::{Client, Response},
    StatusCode,
};

use super::super::paths::MAX_COVER_BYTES;
use crate::ServiceError;

/// Total attempts for a single cover download (initial + retries).
const COVER_DOWNLOAD_MAX_ATTEMPTS: u32 = 3;

/// Initial back-off between attempts. Doubled on every retry, so worst-case
/// added latency before surfacing a final transient failure is roughly
/// 200 ms + 400 ms = 600 ms — well below any user-facing timeout while still
/// giving a stuck CDN edge time to recover.
const COVER_DOWNLOAD_INITIAL_BACKOFF: Duration = Duration::from_millis(200);

/// `usize`-typed cover size cap used for the in-memory body buffer.
///
/// `MAX_COVER_BYTES` is a small fixed constant (10 MiB), so the conversion to
/// `usize` cannot fail at runtime on any supported target. We assert that
/// here at compile time and use the result as a `const`, which lets the body
/// reader stay infallible on the size-conversion path.
const COVER_MAX_LEN: usize = {
    assert!(
        MAX_COVER_BYTES <= usize::MAX as u64,
        "MAX_COVER_BYTES must fit into usize on this target",
    );
    MAX_COVER_BYTES as usize
};

/// Read cap that lets us detect "one byte beyond the limit" so the body
/// reader can reject oversize responses deterministically.
const COVER_READ_LIMIT: u64 = MAX_COVER_BYTES + 1;

/// Internal classification of a single download attempt.
///
/// Keeping retry semantics next to the network code lets the rest of the
/// pipeline (validation, install) keep the simpler `Result<_, ServiceError>`
/// contract.
enum AttemptError {
    /// Definitive failure — do not retry. Examples: HTTP 404 / 410, oversize
    /// content, other 4xx where the server is telling us the request itself
    /// will never succeed.
    Permanent(ServiceError),
    /// Likely transient — retry up to [`COVER_DOWNLOAD_MAX_ATTEMPTS`].
    /// Examples: connect/read errors, HTTP 5xx, 408 Request Timeout,
    /// 429 Too Many Requests, body read interrupted partway through.
    Transient(ServiceError),
}

pub(super) fn download_unvalidated_cover(
    client: &Client,
    url: &str,
) -> Result<Vec<u8>, ServiceError> {
    download_unvalidated_cover_with(
        || download_unvalidated_cover_once(client, url),
        thread::sleep,
    )
}

/// Retry harness around a single download attempt.
///
/// Extracted so unit tests can drive deterministic attempt sequences without
/// real network I/O or real `thread::sleep` calls.
fn download_unvalidated_cover_with<Attempt, Sleep>(
    mut attempt: Attempt,
    mut sleep_for: Sleep,
) -> Result<Vec<u8>, ServiceError>
where
    Attempt: FnMut() -> Result<Vec<u8>, AttemptError>,
    Sleep: FnMut(Duration),
{
    let mut current_attempt: u32 = 1;
    let mut backoff = COVER_DOWNLOAD_INITIAL_BACKOFF;

    loop {
        match attempt() {
            Ok(bytes) => return Ok(bytes),
            Err(AttemptError::Permanent(error)) => return Err(error),
            Err(AttemptError::Transient(error)) => {
                if current_attempt >= COVER_DOWNLOAD_MAX_ATTEMPTS {
                    return Err(error);
                }

                sleep_for(backoff);
                backoff = backoff.saturating_mul(2);
                current_attempt += 1;
            }
        }
    }
}

fn download_unvalidated_cover_once(client: &Client, url: &str) -> Result<Vec<u8>, AttemptError> {
    let response = client
        .get(url)
        .send()
        .map_err(|error| AttemptError::Transient(cover_download_failed(&error)))?;

    classify_status(response.status())?;
    classify_declared_size(&response)?;

    read_body_with_size_limit(response)
}

fn classify_status(status: StatusCode) -> Result<(), AttemptError> {
    if status.is_success() {
        return Ok(());
    }

    if matches!(status, StatusCode::NOT_FOUND | StatusCode::GONE) {
        return Err(AttemptError::Permanent(ServiceError::CoverNotFound));
    }

    let cli_error = ServiceError::CoverDownloadFailed(format!(
        "cover download failed with HTTP status {status}"
    ));

    let outcome = if matches!(
        status,
        StatusCode::REQUEST_TIMEOUT | StatusCode::TOO_MANY_REQUESTS
    ) || status.is_server_error()
    {
        AttemptError::Transient(cli_error)
    } else {
        // 401/403/4xx-other: server is rejecting the request itself; retry
        // would just hit the same wall.
        AttemptError::Permanent(cli_error)
    };

    Err(outcome)
}

fn classify_declared_size(response: &Response) -> Result<(), AttemptError> {
    if let Some(content_length) = response.content_length() {
        if content_length > MAX_COVER_BYTES {
            return Err(AttemptError::Permanent(cover_too_large()));
        }
    }

    Ok(())
}

fn read_body_with_size_limit(response: Response) -> Result<Vec<u8>, AttemptError> {
    let initial_capacity = response
        .content_length()
        .map(|len| usize::try_from(len.min(MAX_COVER_BYTES)).unwrap_or(COVER_MAX_LEN))
        .unwrap_or(0);

    let mut bytes = Vec::with_capacity(initial_capacity);
    let mut limited_response = response.take(COVER_READ_LIMIT);

    limited_response
        .read_to_end(&mut bytes)
        // Mid-body read failures usually mean a dropped connection; retry.
        .map_err(|error| AttemptError::Transient(cover_download_failed(&error)))?;

    if bytes.len() > COVER_MAX_LEN {
        return Err(AttemptError::Permanent(cover_too_large()));
    }

    Ok(bytes)
}

fn cover_download_failed(error: &impl ToString) -> ServiceError {
    ServiceError::CoverDownloadFailed(error.to_string())
}

fn cover_too_large() -> ServiceError {
    ServiceError::CoverDownloadFailed(format!(
        "downloaded cover exceeds maximum size of {MAX_COVER_BYTES} bytes"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn permanent_cover_not_found() -> AttemptError {
        AttemptError::Permanent(ServiceError::CoverNotFound)
    }

    fn transient_failure(message: &str) -> AttemptError {
        AttemptError::Transient(ServiceError::CoverDownloadFailed(message.into()))
    }

    #[test]
    fn returns_immediately_on_success_without_sleeping() {
        let attempts = RefCell::new(0u32);
        let sleeps = RefCell::new(0u32);

        let result = download_unvalidated_cover_with(
            || {
                *attempts.borrow_mut() += 1;
                Ok(b"ok".to_vec())
            },
            |_| *sleeps.borrow_mut() += 1,
        );

        assert_eq!(result.expect("success path returns bytes"), b"ok".to_vec());
        assert_eq!(*attempts.borrow(), 1);
        assert_eq!(*sleeps.borrow(), 0);
    }

    #[test]
    fn retries_transient_failures_up_to_max_attempts() {
        let attempts = RefCell::new(0u32);
        let sleeps = RefCell::new(Vec::<Duration>::new());

        let result = download_unvalidated_cover_with(
            || {
                *attempts.borrow_mut() += 1;
                Err(transient_failure("network blip"))
            },
            |duration| sleeps.borrow_mut().push(duration),
        );

        let error =
            result.expect_err("persistent transient failures should bubble up after retries");
        assert!(matches!(error, ServiceError::CoverDownloadFailed(_)));
        assert_eq!(*attempts.borrow(), COVER_DOWNLOAD_MAX_ATTEMPTS);

        // Sleeps happen between attempts, never after the final failure.
        let recorded_sleeps = sleeps.borrow();
        assert_eq!(
            recorded_sleeps.len(),
            (COVER_DOWNLOAD_MAX_ATTEMPTS - 1) as usize,
        );
        assert_eq!(
            recorded_sleeps.as_slice(),
            &[
                COVER_DOWNLOAD_INITIAL_BACKOFF,
                COVER_DOWNLOAD_INITIAL_BACKOFF.saturating_mul(2),
            ],
        );
    }

    #[test]
    fn recovers_after_first_transient_failure_records_expected_backoff() {
        let attempts = RefCell::new(0u32);
        let sleeps = RefCell::new(Vec::<Duration>::new());

        let result = download_unvalidated_cover_with(
            || {
                let n = {
                    let mut a = attempts.borrow_mut();
                    *a += 1;
                    *a
                };
                if n < 2 {
                    Err(transient_failure("flake"))
                } else {
                    Ok(b"healed".to_vec())
                }
            },
            |duration| sleeps.borrow_mut().push(duration),
        );

        assert_eq!(result.expect("retry should succeed"), b"healed".to_vec());
        assert_eq!(*attempts.borrow(), 2);
        assert_eq!(
            sleeps.borrow().as_slice(),
            &[COVER_DOWNLOAD_INITIAL_BACKOFF]
        );
    }

    #[test]
    fn does_not_retry_permanent_failures() {
        let attempts = RefCell::new(0u32);
        let sleeps = RefCell::new(0u32);

        let result = download_unvalidated_cover_with(
            || {
                *attempts.borrow_mut() += 1;
                Err(permanent_cover_not_found())
            },
            |_| *sleeps.borrow_mut() += 1,
        );

        assert!(matches!(result, Err(ServiceError::CoverNotFound)));
        assert_eq!(*attempts.borrow(), 1, "404/Gone must never be retried");
        assert_eq!(*sleeps.borrow(), 0);
    }
}
