//! Focused tests for the shared HTTP transport and download behavior.

use std::time::Duration;

use reqwest::Url;

use super::*;
use super::{
    body::{ensure_exact_content_length, validate_exact_content_range},
    download::{
        RESUME_RETRY_INITIAL_DELAY_MS, RESUME_RETRY_MAX_DELAY, restart_anonymously, resume_retry,
    },
    transport::{
        HttpFetchFailure, HttpResponseStatus, resolve_redirect_location, retry_once_anonymously,
        send_with_anonymous_retry,
    },
};

#[test]
fn resolve_redirect_location_joins_relative_path() {
    let current = Url::parse("https://example.com/a/b").expect("current");
    let next = resolve_redirect_location(&current, "/c/d").expect("relative");
    assert_eq!(next.as_str(), "https://example.com/c/d");
}

#[test]
fn resolve_redirect_location_accepts_absolute_https() {
    let current = Url::parse("https://example.com/start").expect("current");
    let next =
        resolve_redirect_location(&current, "https://cdn.example.com/file.zip").expect("absolute");
    assert_eq!(next.as_str(), "https://cdn.example.com/file.zip");
}

#[test]
fn resolve_redirect_location_rejects_non_https() {
    let current = Url::parse("https://example.com/start").expect("current");
    let error = resolve_redirect_location(&current, "http://insecure.example/x").expect_err("http");
    assert!(error.contains("non-HTTPS"), "{error}");
}

#[test]
fn resolve_redirect_location_rejects_unparseable_location() {
    let current = Url::parse("https://example.com/start").expect("current");
    // Broken absolute form that neither joins as a path nor parses as a URL.
    assert!(
        resolve_redirect_location(&current, "http://[").is_err(),
        "broken absolute Location must fail"
    );
}

#[test]
fn exact_download_allows_an_omitted_content_length() {
    assert!(ensure_exact_content_length("archive", None, 42).is_ok());
}

#[test]
fn exact_download_rejects_a_conflicting_content_length() {
    let error = ensure_exact_content_length("archive", Some(41), 42)
        .expect_err("conflicting header must fail before reading the body");
    assert!(error.to_string().contains("size mismatch"));
}

#[test]
fn resumed_download_requires_the_exact_remaining_content_range() {
    validate_exact_content_range("bytes 100-999/1000", 100, 1000).expect("exact remaining range");
    assert!(validate_exact_content_range("bytes 99-999/1000", 100, 1000).is_err());
    assert!(validate_exact_content_range("bytes 100-998/1000", 100, 1000).is_err());
    assert!(validate_exact_content_range("bytes 100-999/1001", 100, 1000).is_err());
}

#[test]
fn partial_download_retries_only_resumeable_transport_failures_without_a_cap() {
    let request = HttpFetchFailure::Request("connection reset".to_owned());
    let body = HttpFetchFailure::Body("truncated gzip body".to_owned());
    let permanent = HttpFetchFailure::Permanent(crate::failed("bad status"));

    assert_eq!(
        resume_retry(&request, 1, 0).map(|(_, delay)| delay),
        Some(Duration::from_millis(RESUME_RETRY_INITIAL_DELAY_MS))
    );
    assert_eq!(
        resume_retry(&body, 100, 10).map(|(_, delay)| delay),
        Some(RESUME_RETRY_MAX_DELAY)
    );
    assert!(resume_retry(&request, 0, 0).is_none());
    assert!(resume_retry(&permanent, 100, 0).is_none());
}

#[derive(Clone, Copy, Debug)]
struct StubResponse(bool);

impl HttpResponseStatus for StubResponse {
    fn is_success(&self) -> bool {
        self.0
    }
}

#[tokio::test]
async fn authenticated_http_failure_retries_exactly_once_without_auth() {
    let attempts = std::sync::Mutex::new(Vec::new());
    let response = send_with_anonymous_retry(true, |authenticated| {
        attempts.lock().expect("attempts lock").push(authenticated);
        async move { Ok::<_, &'static str>(StubResponse(!authenticated)) }
    })
    .await
    .expect("anonymous retry should succeed");

    assert!(response.is_success());
    assert_eq!(*attempts.lock().expect("attempts lock"), [true, false]);
}

#[tokio::test]
async fn manual_redirect_status_retry_restarts_from_origin_without_auth() {
    let origin = "https://github.com/repo/latest.json";
    let requests = std::sync::Mutex::new(Vec::new());
    let result = retry_once_anonymously(
        true,
        |use_token| {
            requests
                .lock()
                .expect("requests lock")
                .push((use_token, origin));
            async move {
                if use_token {
                    Err(HttpFetchFailure::AuthenticatedStatus(
                        reqwest::StatusCode::FORBIDDEN,
                    ))
                } else {
                    Ok(StubResponse(true))
                }
            }
        },
        |result| matches!(result, Err(HttpFetchFailure::AuthenticatedStatus(_))),
    )
    .await
    .ok()
    .expect("anonymous chain retry should succeed");

    assert!(result.is_success());
    assert_eq!(
        *requests.lock().expect("requests lock"),
        [(true, origin), (false, origin)]
    );
}

#[tokio::test]
async fn transport_failure_does_not_retry_anonymously() {
    let attempts = std::sync::Mutex::new(Vec::new());
    let result = send_with_anonymous_retry(true, |authenticated| {
        attempts.lock().expect("attempts lock").push(authenticated);
        async { Err::<StubResponse, _>("transport failure") }
    })
    .await;

    assert_eq!(
        result.expect_err("transport failure").to_string(),
        "transport failure"
    );
    assert_eq!(*attempts.lock().expect("attempts lock"), [true]);
}

#[tokio::test]
async fn unauthenticated_off_host_failure_does_not_retry() {
    let attempts = std::sync::Mutex::new(Vec::new());
    let response = send_with_anonymous_retry(false, |authenticated| {
        attempts.lock().expect("attempts lock").push(authenticated);
        async { Ok::<_, &'static str>(StubResponse(false)) }
    })
    .await
    .expect("HTTP responses remain available for normal status handling");

    assert!(!response.is_success());
    assert_eq!(*attempts.lock().expect("attempts lock"), [false]);
}

#[test]
fn anonymous_resume_restart_discards_authenticated_prefix_and_chain() {
    let mut token = Some("test-token");
    let mut bytes = vec![1, 2, 3];
    let mut validators = Some(HttpValidators {
        etag: Some("etag".to_owned()),
        last_modified: None,
    });
    let mut url_chain = vec![Url::parse("https://github.com/repo/file").expect("URL")];

    restart_anonymously(&mut token, &mut bytes, &mut validators, &mut url_chain);

    assert!(token.is_none());
    assert!(bytes.is_empty());
    assert!(validators.is_none());
    assert!(url_chain.is_empty());
}
