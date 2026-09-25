//! Optional, process-local authentication for requests to GitHub.
//!
//! Credentials are discovered from the process environment or the local `gh`
//! executable. They are never persisted, and authorization headers are only
//! produced for HTTPS URLs whose exact host is `github.com` on effective port
//! 443.

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    env, fmt,
    io::Read,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use http::{HeaderValue, header::AUTHORIZATION};
use reqwest::Url;

/// A token held only for the lifetime of the caller's operation.
pub struct GitHubToken(String);

impl fmt::Debug for GitHubToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GitHubToken([REDACTED])")
    }
}

impl GitHubToken {
    /// Resolves an optional local token using GH_TOKEN, GITHUB_TOKEN, then the
    /// authenticated GitHub CLI. No interactive authentication is attempted.
    #[must_use]
    pub fn from_local_machine() -> Option<Self> {
        Self::from_sources(
            env::var("GH_TOKEN").ok(),
            env::var("GITHUB_TOKEN").ok(),
            token_from_github_cli,
        )
    }

    /// Keeps local credential-store lookup off the async request executor.
    pub async fn from_local_machine_async() -> Option<Self> {
        tokio::task::spawn_blocking(Self::from_local_machine)
            .await
            .ok()
            .flatten()
    }

    /// Creates a sensitive Authorization value only for an eligible URL.
    #[must_use]
    pub fn authorization_header_for_url(&self, url: &Url) -> Option<HeaderValue> {
        if !is_github_auth_target(url) {
            return None;
        }

        let mut header = HeaderValue::from_str(&format!("Bearer {}", self.0)).ok()?;
        header.set_sensitive(true);
        Some(header)
    }

    /// Parses a string URL and creates an eligible sensitive Authorization value.
    #[must_use]
    pub fn authorization_header_for_url_str(&self, url: &str) -> Option<HeaderValue> {
        Url::parse(url)
            .ok()
            .and_then(|url| self.authorization_header_for_url(&url))
    }

    /// Creates a sensitive Authorization value only when every URL is an
    /// eligible GitHub URL. An empty or malformed list fails closed.
    #[must_use]
    pub fn authorization_header_for_urls<'a>(
        &self,
        urls: impl IntoIterator<Item = &'a str>,
    ) -> Option<HeaderValue> {
        let mut urls = urls.into_iter();
        let first = Url::parse(urls.next()?).ok()?;
        if !is_github_auth_target(&first) || urls.any(|url| !is_github_auth_target_url(url)) {
            return None;
        }
        self.authorization_header_for_url(&first)
    }

    fn from_sources(
        gh_token: Option<String>,
        github_token: Option<String>,
        gh_cli: impl FnOnce() -> Option<String>,
    ) -> Option<Self> {
        fn non_empty(value: Option<String>) -> Option<String> {
            value
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        }

        non_empty(gh_token)
            .or_else(|| non_empty(github_token))
            .or_else(|| non_empty(gh_cli()))
            .map(Self)
    }
}

fn token_from_github_cli() -> Option<String> {
    const CLI_TIMEOUT: Duration = Duration::from_secs(2);
    const MAX_TOKEN_OUTPUT_BYTES: u64 = 4096;

    let mut command = Command::new("gh");
    command
        .args(["auth", "token", "--hostname", "github.com"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    let mut child = command.spawn().ok()?;
    let deadline = Instant::now() + CLI_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    let mut stdout = child.stdout.take()?;
    let mut bytes = Vec::new();
    stdout
        .by_ref()
        .take(MAX_TOKEN_OUTPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_TOKEN_OUTPUT_BYTES {
        return None;
    }
    cli_token_from_output(true, bytes)
}

fn cli_token_from_output(success: bool, stdout: Vec<u8>) -> Option<String> {
    if !success {
        return None;
    }
    String::from_utf8(stdout).ok()
}

/// Whether a parsed URL is allowed to receive a local GitHub token.
#[must_use]
pub fn is_github_auth_target(url: &Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.port_or_known_default() == Some(443)
}

/// Parses a URL and applies [`is_github_auth_target`].
#[must_use]
pub fn is_github_auth_target_url(url: &str) -> bool {
    Url::parse(url).is_ok_and(|url| is_github_auth_target(&url))
}

/// Returns the Authorization header name used by the policy.
#[must_use]
pub const fn authorization_header_name() -> http::HeaderName {
    AUTHORIZATION
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(url: &str) -> Url {
        Url::parse(url).expect("valid test URL")
    }

    #[test]
    fn token_sources_follow_environment_then_authenticated_cli_precedence() {
        let gh = GitHubToken::from_sources(
            Some(" primary-token ".to_owned()),
            Some("secondary-token".to_owned()),
            || panic!("CLI must not run when GH_TOKEN is set"),
        )
        .expect("GH_TOKEN should resolve");
        assert_eq!(gh.0, "primary-token");

        let github = GitHubToken::from_sources(
            Some("  ".to_owned()),
            Some(" secondary-token ".to_owned()),
            || panic!("CLI must not run when GITHUB_TOKEN is set"),
        )
        .expect("GITHUB_TOKEN should resolve");
        assert_eq!(github.0, "secondary-token");

        let cli = GitHubToken::from_sources(None, None, || Some(" cli-token\n".to_owned()))
            .expect("CLI token should resolve");
        assert_eq!(cli.0, "cli-token");
        assert_eq!(
            cli_token_from_output(true, b"cli-token\n".to_vec()).as_deref(),
            Some("cli-token\n")
        );
        assert!(cli_token_from_output(false, b"expired-token".to_vec()).is_none());
        assert!(cli_token_from_output(true, vec![0xff]).is_none());
        assert!(GitHubToken::from_sources(None, None, || None).is_none());
    }

    #[test]
    fn only_exact_https_github_host_on_effective_port_443_is_allowed() {
        for url in [
            "https://github.com/owner/repo",
            "https://github.com:443/owner/repo",
        ] {
            assert!(is_github_auth_target(&parsed(url)), "{url}");
        }
        for url in [
            "http://github.com/owner/repo",
            "https://github.com:444/owner/repo",
            "https://api.github.com/owner/repo",
            "https://github.com.evil.example/owner/repo",
            "https://evilgithub.com/owner/repo",
            "https://github.com./owner/repo",
            "https://github.com@evil.example/owner/repo",
        ] {
            assert!(!is_github_auth_target(&parsed(url)), "{url}");
        }
    }

    #[test]
    fn authorization_header_is_sensitive_and_only_generated_for_eligible_hops() {
        let token = GitHubToken("test-secret-token".to_owned());
        let allowed = token
            .authorization_header_for_url(&parsed("https://github.com/repo"))
            .expect("GitHub header");
        assert!(allowed.is_sensitive());
        assert!(!format!("{allowed:?}").contains("test-secret-token"));
        assert_eq!(
            allowed.to_str().expect("ASCII header"),
            "Bearer test-secret-token"
        );

        for url in [
            "https://github.com.evil.example/file",
            "https://objects.githubusercontent.com/file",
            "https://github.com:8443/file",
        ] {
            assert!(token.authorization_header_for_url(&parsed(url)).is_none());
        }
    }

    #[test]
    fn authorization_for_url_set_requires_every_url_to_be_github() {
        let token = GitHubToken("test-token".to_owned());
        assert!(
            token
                .authorization_header_for_urls([
                    "https://github.com/repo/latest.json",
                    "https://github.com/repo/file.zip",
                ])
                .is_some()
        );
        for urls in [
            &[
                "https://github.com/repo/latest.json",
                "https://cdn.example/file.zip",
            ][..],
            &["https://github.com.evil.example/repo"][..],
            &["not a URL"][..],
            &[][..],
        ] {
            assert!(
                token
                    .authorization_header_for_urls(urls.iter().copied())
                    .is_none()
            );
        }
    }

    #[test]
    fn manually_followed_redirect_hops_are_independently_gated() {
        let token = GitHubToken("test-token".to_owned());
        for (url, expected) in [
            ("https://github.com/repo/latest.json", true),
            ("https://objects.githubusercontent.com/file", false),
            ("https://github.com/repo/file.zip", true),
        ] {
            assert_eq!(
                token.authorization_header_for_url_str(url).is_some(),
                expected,
                "hop {url}"
            );
        }
        assert!(
            token
                .authorization_header_for_url_str("not a URL")
                .is_none()
        );
    }

    #[test]
    fn malformed_local_token_is_ignored_without_exposing_it() {
        let token = GitHubToken("invalid\nsecret".to_owned());
        assert!(
            token
                .authorization_header_for_url(&parsed("https://github.com/repo"))
                .is_none()
        );
        assert_eq!(format!("{token:?}"), "GitHubToken([REDACTED])");
    }
}
