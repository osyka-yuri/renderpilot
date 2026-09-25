//! Shared HTTP clients, request status handling, authentication, and redirects.

use std::sync::OnceLock;
use std::time::Duration;

use reqwest::header::{LOCATION, RANGE};
use reqwest::redirect::Policy;
use reqwest::{Client, Method, Response, Url};

use crate::{
    ServiceError,
    github_auth::{GitHubToken, is_github_auth_target},
};

use super::HttpValidators;

const HTTP_TIMEOUT: Duration = Duration::from_mins(1);
const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
/// Manual redirect hops (GitHub latest → tag → CDN is typically 2).
const MAX_REDIRECTS: usize = 10;

static HTTP_CLIENT: OnceLock<Result<Client, String>> = OnceLock::new();
/// No auto-follow: used only by hop-chain helpers.
static HTTP_CLIENT_NO_REDIRECT: OnceLock<Result<Client, String>> = OnceLock::new();
/// Successful response after manually following redirects, plus hop URLs.
pub(super) struct FollowedResponse {
    pub(super) response: Response,
    pub(super) url_chain: Vec<Url>,
}

impl FollowedResponse {
    pub(super) fn validators(&self) -> HttpValidators {
        super::body::validators_of(&self.response)
    }
}

/// A failure while fetching an HTTP payload. Only request/body
/// transport failures are retryable; all protocol and integrity failures stay
/// terminal and retain their existing error contracts.
pub(super) enum HttpFetchFailure {
    Request(String),
    Body(String),
    AuthenticatedStatus(reqwest::StatusCode),
    Permanent(ServiceError),
}

impl HttpFetchFailure {
    pub(super) fn into_service_error(self, operation: &str) -> ServiceError {
        match self {
            Self::Request(error) => crate::failed(format!("{operation} failed: {error}")),
            Self::Body(error) => {
                crate::failed(format!("failed to read {operation} chunk: {error}"))
            }
            Self::AuthenticatedStatus(status) => {
                crate::failed(format!("{operation} failed with status {status}"))
            }
            Self::Permanent(error) => error,
        }
    }
}

pub(super) trait HttpResponseStatus {
    fn is_success(&self) -> bool;
}

impl HttpResponseStatus for Response {
    fn is_success(&self) -> bool {
        Response::status(self).is_success()
    }
}

pub(super) async fn send_with_anonymous_retry<R, E, F, Fut>(
    authenticated: bool,
    send: F,
) -> Result<R, E>
where
    R: HttpResponseStatus,
    F: FnMut(bool) -> Fut,
    Fut: std::future::Future<Output = Result<R, E>>,
{
    retry_once_anonymously(authenticated, send, |result| {
        result.as_ref().is_ok_and(|response| !response.is_success())
    })
    .await
}

pub(super) async fn retry_once_anonymously<R, E, F, Fut, P>(
    authenticated: bool,
    mut send: F,
    should_retry: P,
) -> Result<R, E>
where
    F: FnMut(bool) -> Fut,
    Fut: std::future::Future<Output = Result<R, E>>,
    P: Fn(&Result<R, E>) -> bool,
{
    let result = send(authenticated).await;
    if authenticated && should_retry(&result) {
        send(false).await
    } else {
        result
    }
}

/// The process-wide HTTPS client (lazily built, then reused). A failure to build
/// the client (e.g. a malformed TLS backend) surfaces as a `ServiceError` at the
/// first download rather than a process panic.
pub(crate) fn http_client() -> Result<&'static Client, ServiceError> {
    resolve_client(&HTTP_CLIENT, || build_client(Policy::default()))
}

fn http_client_no_redirect() -> Result<&'static Client, ServiceError> {
    resolve_client(&HTTP_CLIENT_NO_REDIRECT, || build_client(Policy::none()))
}

fn resolve_client(
    slot: &'static OnceLock<Result<Client, String>>,
    build: impl FnOnce() -> Result<Client, String>,
) -> Result<&'static Client, ServiceError> {
    slot.get_or_init(build)
        .as_ref()
        .map_err(|e| crate::failed(e.clone()))
}

fn build_client(redirect: Policy) -> Result<Client, String> {
    Client::builder()
        .timeout(HTTP_TIMEOUT)
        .user_agent(USER_AGENT)
        .redirect(redirect)
        .build()
        .map_err(|e| format!("failed to create global HTTP client: {e}"))
}

// ---------------------------------------------------------------------------
pub(super) async fn get_successful_response(
    url: &str,
    operation: &str,
) -> Result<Response, ServiceError> {
    request_successful(Method::GET, url, operation).await
}

pub(super) async fn head_successful_response(
    url: &str,
    operation: &str,
) -> Result<Response, ServiceError> {
    request_successful(Method::HEAD, url, operation).await
}

async fn request_successful(
    method: Method,
    url: &str,
    operation: &str,
) -> Result<Response, ServiceError> {
    let url = parse_https_url(url, operation)?;
    let client = http_client()?;
    let token = if is_github_auth_target(&url) {
        GitHubToken::from_local_machine_async().await
    } else {
        None
    };
    let mut authorization = token
        .as_ref()
        .and_then(|token| token.authorization_header_for_url(&url));
    let response = send_with_anonymous_retry(authorization.is_some(), |use_authorization| {
        let mut request = client.request(method.clone(), url.clone());
        if use_authorization && let Some(header) = authorization.take() {
            request = request.header(crate::github_auth::authorization_header_name(), header);
        }
        request.send()
    })
    .await
    .map_err(|error| crate::failed(format!("{operation} failed: {error}")))?;
    ensure_success(operation, response.status())?;
    Ok(response)
}

fn ensure_success(operation: &str, status: reqwest::StatusCode) -> Result<(), ServiceError> {
    if status.is_success() {
        Ok(())
    } else {
        Err(crate::failed(format!(
            "{operation} failed with status {status}"
        )))
    }
}

/// Follows redirects manually so the hop chain is available. Validates HTTPS on
/// every target. `method` is re-issued on each hop (standard for 302/303 asset
/// downloads and HEAD pre-checks).
pub(super) async fn follow_redirects(
    method: Method,
    url: &str,
    operation: &str,
) -> Result<FollowedResponse, ServiceError> {
    let parsed_url = parse_https_url(url, operation)?;
    let token = if is_github_auth_target(&parsed_url) {
        GitHubToken::from_local_machine_async().await
    } else {
        None
    };
    retry_once_anonymously(
        token.is_some(),
        |use_token| {
            follow_redirects_classified(
                method.clone(),
                &parsed_url,
                operation,
                None,
                if use_token { token.as_ref() } else { None },
            )
        },
        |result| matches!(result, Err(HttpFetchFailure::AuthenticatedStatus(_))),
    )
    .await
    .map_err(|failure| failure.into_service_error(operation))
}

pub(super) async fn follow_redirects_classified(
    method: Method,
    start: &Url,
    operation: &str,
    range_start: Option<u64>,
    token: Option<&GitHubToken>,
) -> Result<FollowedResponse, HttpFetchFailure> {
    let client = http_client_no_redirect().map_err(HttpFetchFailure::Permanent)?;
    let mut url_chain = vec![start.clone()];
    let mut current = start.clone();

    for _ in 0..=MAX_REDIRECTS {
        let mut request = client.request(method.clone(), current.clone());
        let authorization = token.and_then(|token| token.authorization_header_for_url(&current));
        let authenticated = authorization.is_some();
        if let Some(header) = authorization {
            request = request.header(crate::github_auth::authorization_header_name(), header);
        }
        if let Some(start) = range_start {
            request = request.header(RANGE, format!("bytes={start}-"));
        }
        let response = request
            .send()
            .await
            .map_err(|error| HttpFetchFailure::Request(error.to_string()))?;
        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| {
                    HttpFetchFailure::Permanent(crate::failed(format!(
                        "{operation} redirect missing Location (status {status})"
                    )))
                })?;
            let next = resolve_redirect_location(&current, location).map_err(|error| {
                HttpFetchFailure::Permanent(crate::failed(format!(
                    "{operation} redirect has invalid Location: {error}"
                )))
            })?;
            url_chain.push(next.clone());
            current = next;
            continue;
        }
        if !status.is_success() {
            if authenticated {
                return Err(HttpFetchFailure::AuthenticatedStatus(status));
            }
            ensure_success(operation, status).map_err(HttpFetchFailure::Permanent)?;
        }
        return Ok(FollowedResponse {
            response,
            url_chain,
        });
    }

    Err(HttpFetchFailure::Permanent(crate::failed(format!(
        "{operation} exceeded redirect limit ({MAX_REDIRECTS})"
    ))))
}

/// Resolves a redirect `Location` against the current request URL. Requires HTTPS.
pub(super) fn resolve_redirect_location(current: &Url, location: &str) -> Result<Url, String> {
    let next = current
        .join(location)
        .or_else(|_| Url::parse(location))
        .map_err(|error| format!("`{location}`: {error}"))?;
    if next.scheme() != "https" {
        return Err(format!("non-HTTPS URL is not allowed (`{next}`)"));
    }
    Ok(next)
}

/// Parses `url` and requires it to be HTTPS.
pub(crate) fn parse_https_url(url: &str, operation: &str) -> Result<Url, ServiceError> {
    let url = Url::parse(url)
        .map_err(|error| crate::failed(format!("invalid URL for {operation}: {error}")))?;

    if url.scheme() != "https" {
        return Err(crate::failed(format!(
            "invalid URL for {operation}: only HTTPS URLs are allowed"
        )));
    }

    Ok(url)
}
