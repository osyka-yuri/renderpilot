//! Shared HTTPS transport and download helpers for provider-specific callers.
//!
//! The transport can discover a local GitHub token, but sends it only to exact
//! HTTPS github.com targets on effective port 443. An authenticated HTTP status
//! failure gets one anonymous retry. Streaming, size checks, range validation,
//! and resume logic live in focused internal modules below.

use reqwest::Url;

mod body;
mod download;
#[cfg(test)]
mod tests;
mod transport;

pub(crate) use download::{
    download_exact_bytes, download_exact_with_url_chain, download_limited_bytes,
    download_with_referer, download_with_url_chain, download_with_validators,
    download_with_validators_and_final_url, head_validators, head_with_url_chain,
};
pub(crate) use transport::{http_client, parse_https_url};
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

/// Body bytes plus cache validators and every redirect hop (start → … → final).
///
/// Produced only by hop-chain downloads.
#[derive(Debug)]
pub(crate) struct ValidatedDownload {
    pub bytes: Vec<u8>,
    pub validators: HttpValidators,
    pub url_chain: Vec<Url>,
}
