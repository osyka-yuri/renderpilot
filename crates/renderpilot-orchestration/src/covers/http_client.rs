//! Shared HTTP client configuration for cover downloads.

use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::redirect::Policy;

use crate::ServiceError;

pub(crate) const REDIRECT_LIMIT: usize = 8;

const REQUEST_TIMEOUT_SECS: u64 = 45;
const USER_AGENT: &str = concat!(
    "RenderPilot/",
    env!("CARGO_PKG_VERSION"),
    " (+https://renderpilot.local)"
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HttpClientConfig {
    pub(crate) redirect_limit: usize,
    pub(crate) timeout: Duration,
    pub(crate) user_agent: &'static str,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            redirect_limit: REDIRECT_LIMIT,
            timeout: Duration::from_secs(REQUEST_TIMEOUT_SECS),
            user_agent: USER_AGENT,
        }
    }
}

pub(crate) fn http_client() -> Result<Client, ServiceError> {
    http_client_with_config(HttpClientConfig::default())
}

pub(crate) fn http_client_with_config(config: HttpClientConfig) -> Result<Client, ServiceError> {
    Client::builder()
        .redirect(Policy::limited(config.redirect_limit))
        .timeout(config.timeout)
        .user_agent(config.user_agent)
        .build()
        .map_err(client_build_error)
}

fn client_build_error(error: reqwest::Error) -> ServiceError {
    ServiceError::CoverDownloadFailed(format!("failed to build HTTP client: {error}"))
}
