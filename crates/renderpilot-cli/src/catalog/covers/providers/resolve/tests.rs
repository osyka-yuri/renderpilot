use std::cell::RefCell;
use std::collections::VecDeque;

use reqwest::blocking::Client;

use super::super::backend::CoverSourceBackend;
use super::{
    normalized_steamgriddb_api_key, resolve_cover_bytes_with_backend, CoverResolutionRequest,
    EffectiveCoverRemotePolicy, RemoteCoverLauncher,
};
use crate::error::CliError;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Call {
    SteamCdn {
        app_id: String,
    },
    GogCdn {
        product_id: String,
    },
    GridSlug {
        api_key: String,
        slug: String,
    },
    GridSlugThenAutocomplete {
        api_key: String,
        slug: String,
        title: String,
    },
    GridAutocomplete {
        api_key: String,
        title: String,
    },
}

#[derive(Debug)]
enum SourceOutcome {
    Bytes(&'static [u8]),
    Miss,
}

impl SourceOutcome {
    fn into_option(self) -> Option<Vec<u8>> {
        match self {
            Self::Bytes(bytes) => Some(bytes.to_vec()),
            Self::Miss => None,
        }
    }

    fn into_result(self) -> Result<Vec<u8>, CliError> {
        match self {
            Self::Bytes(bytes) => Ok(bytes.to_vec()),
            Self::Miss => Err(CliError::CoverNotFound),
        }
    }
}

#[derive(Debug, Default)]
struct MockCoverSourceBackend {
    calls: RefCell<Vec<Call>>,
    steam_cdn: RefCell<VecDeque<SourceOutcome>>,
    gog_cdn: RefCell<VecDeque<SourceOutcome>>,
    grid_slug: RefCell<VecDeque<SourceOutcome>>,
    grid_slug_then_autocomplete: RefCell<VecDeque<SourceOutcome>>,
    grid_autocomplete: RefCell<VecDeque<SourceOutcome>>,
}

impl MockCoverSourceBackend {
    fn with_steam_cdn(self, outcomes: impl IntoIterator<Item = SourceOutcome>) -> Self {
        self.steam_cdn.borrow_mut().extend(outcomes);
        self
    }

    fn with_gog_cdn(self, outcomes: impl IntoIterator<Item = SourceOutcome>) -> Self {
        self.gog_cdn.borrow_mut().extend(outcomes);
        self
    }

    fn with_grid_slug(self, outcomes: impl IntoIterator<Item = SourceOutcome>) -> Self {
        self.grid_slug.borrow_mut().extend(outcomes);
        self
    }

    fn with_grid_slug_then_autocomplete(
        self,
        outcomes: impl IntoIterator<Item = SourceOutcome>,
    ) -> Self {
        self.grid_slug_then_autocomplete
            .borrow_mut()
            .extend(outcomes);
        self
    }

    fn with_grid_autocomplete(self, outcomes: impl IntoIterator<Item = SourceOutcome>) -> Self {
        self.grid_autocomplete.borrow_mut().extend(outcomes);
        self
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.borrow().clone()
    }

    fn next_option(queue: &RefCell<VecDeque<SourceOutcome>>) -> Option<Vec<u8>> {
        queue
            .borrow_mut()
            .pop_front()
            .unwrap_or(SourceOutcome::Miss)
            .into_option()
    }

    fn next_result(queue: &RefCell<VecDeque<SourceOutcome>>) -> Result<Vec<u8>, CliError> {
        queue
            .borrow_mut()
            .pop_front()
            .unwrap_or(SourceOutcome::Miss)
            .into_result()
    }
}

impl CoverSourceBackend for MockCoverSourceBackend {
    fn try_steam_cdn(&self, _client: &Client, app_id: &str) -> Option<Vec<u8>> {
        self.calls.borrow_mut().push(Call::SteamCdn {
            app_id: app_id.to_owned(),
        });

        Self::next_option(&self.steam_cdn)
    }

    fn try_gog_cdn(&self, _client: &Client, product_id: &str) -> Option<Vec<u8>> {
        self.calls.borrow_mut().push(Call::GogCdn {
            product_id: product_id.to_owned(),
        });

        Self::next_option(&self.gog_cdn)
    }

    fn try_download_grid_for_slug(
        &self,
        _client: &Client,
        api_key: &str,
        slug: &str,
    ) -> Result<Vec<u8>, CliError> {
        self.calls.borrow_mut().push(Call::GridSlug {
            api_key: api_key.to_owned(),
            slug: slug.to_owned(),
        });

        Self::next_result(&self.grid_slug)
    }

    fn try_download_grid_slug_then_autocomplete(
        &self,
        _client: &Client,
        api_key: &str,
        slug: &str,
        title: &str,
    ) -> Result<Vec<u8>, CliError> {
        self.calls
            .borrow_mut()
            .push(Call::GridSlugThenAutocomplete {
                api_key: api_key.to_owned(),
                slug: slug.to_owned(),
                title: title.to_owned(),
            });

        Self::next_result(&self.grid_slug_then_autocomplete)
    }

    fn try_download_grid_autocomplete_only(
        &self,
        _client: &Client,
        api_key: &str,
        title: &str,
    ) -> Result<Vec<u8>, CliError> {
        self.calls.borrow_mut().push(Call::GridAutocomplete {
            api_key: api_key.to_owned(),
            title: title.to_owned(),
        });

        Self::next_result(&self.grid_autocomplete)
    }
}

fn policy(steam_cdn: bool, gog_cdn: bool, steamgriddb: bool) -> EffectiveCoverRemotePolicy {
    EffectiveCoverRemotePolicy {
        steam_cdn,
        gog_cdn,
        steamgriddb,
    }
}

fn request<'a>(
    launcher: RemoteCoverLauncher,
    external_id: Option<&'a str>,
    title: &'a str,
) -> CoverResolutionRequest<'a> {
    CoverResolutionRequest {
        launcher,
        external_id,
        title,
    }
}

fn resolve_with_backend(
    backend: &MockCoverSourceBackend,
    api_key: Option<&str>,
    policy: EffectiveCoverRemotePolicy,
    request: CoverResolutionRequest<'_>,
) -> Result<Vec<u8>, CliError> {
    let client = Client::new();

    resolve_cover_bytes_with_backend(&client, api_key, policy, request, backend)
}

fn assert_cover_not_found(result: Result<Vec<u8>, CliError>) {
    assert!(
        matches!(result, Err(CliError::CoverNotFound)),
        "expected CoverNotFound"
    );
}

#[test]
fn normalizes_missing_blank_and_non_blank_steamgriddb_api_keys() {
    assert_eq!(normalized_steamgriddb_api_key(None), None);
    assert_eq!(normalized_steamgriddb_api_key(Some("")), None);
    assert_eq!(normalized_steamgriddb_api_key(Some("   ")), None);
    assert_eq!(normalized_steamgriddb_api_key(Some("\n\t")), None);
    assert_eq!(
        normalized_steamgriddb_api_key(Some("  token-123  ")),
        Some("token-123")
    );
}

#[test]
fn steam_without_external_id_returns_cover_not_found_before_any_remote_call() {
    let backend = MockCoverSourceBackend::default();

    let result = resolve_with_backend(
        &backend,
        None,
        policy(true, true, true),
        request(RemoteCoverLauncher::Steam, None, "Portal"),
    );

    assert_cover_not_found(result);
    assert_eq!(backend.calls(), vec![]);
}

#[test]
fn steam_uses_steam_cdn_first_and_does_not_require_grid_key_when_cdn_succeeds() {
    let backend = MockCoverSourceBackend::default()
        .with_steam_cdn([SourceOutcome::Bytes(b"steam-cdn-cover")]);

    let result = resolve_with_backend(
        &backend,
        None,
        policy(true, true, true),
        request(RemoteCoverLauncher::Steam, Some("480"), "Portal"),
    );

    assert_eq!(result.unwrap(), b"steam-cdn-cover".to_vec());
    assert_eq!(
        backend.calls(),
        vec![Call::SteamCdn {
            app_id: "480".to_owned(),
        }]
    );
}

#[test]
fn steam_skips_steam_cdn_when_disabled_and_uses_grid_slug() {
    let backend =
        MockCoverSourceBackend::default().with_grid_slug([SourceOutcome::Bytes(b"grid-cover")]);

    let result = resolve_with_backend(
        &backend,
        Some("grid-key"),
        policy(false, true, true),
        request(RemoteCoverLauncher::Steam, Some("480"), "Portal"),
    );

    assert_eq!(result.unwrap(), b"grid-cover".to_vec());
    assert_eq!(
        backend.calls(),
        vec![Call::GridSlug {
            api_key: "grid-key".to_owned(),
            slug: "steam-480".to_owned(),
        }]
    );
}

#[test]
fn steam_returns_cover_not_found_after_cdn_miss_when_grid_disabled() {
    let backend = MockCoverSourceBackend::default().with_steam_cdn([SourceOutcome::Miss]);

    let result = resolve_with_backend(
        &backend,
        None,
        policy(true, true, false),
        request(RemoteCoverLauncher::Steam, Some("480"), "Portal"),
    );

    assert_cover_not_found(result);
    assert_eq!(
        backend.calls(),
        vec![Call::SteamCdn {
            app_id: "480".to_owned(),
        }]
    );
}

#[test]
fn steam_returns_cover_not_found_after_cdn_miss_when_grid_enabled_but_key_absent() {
    let backend = MockCoverSourceBackend::default().with_steam_cdn([SourceOutcome::Miss]);

    let result = resolve_with_backend(
        &backend,
        None,
        policy(true, true, true),
        request(RemoteCoverLauncher::Steam, Some("480"), "Portal"),
    );

    // Missing key === grid effectively disabled: no `Call::GridSlug`,
    // no `SteamGridDbApiKeyMissing` warning surfaced to the UI.
    assert_cover_not_found(result);
    assert_eq!(
        backend.calls(),
        vec![Call::SteamCdn {
            app_id: "480".to_owned(),
        }]
    );
}

#[test]
fn steam_treats_blank_grid_key_as_cover_not_found_after_cdn_miss() {
    let backend = MockCoverSourceBackend::default().with_steam_cdn([SourceOutcome::Miss]);

    let result = resolve_with_backend(
        &backend,
        Some("   "),
        policy(true, true, true),
        request(RemoteCoverLauncher::Steam, Some("480"), "Portal"),
    );

    assert_cover_not_found(result);
    assert_eq!(
        backend.calls(),
        vec![Call::SteamCdn {
            app_id: "480".to_owned(),
        }]
    );
}

#[test]
fn steam_falls_back_to_grid_slug_after_cdn_miss() {
    let backend = MockCoverSourceBackend::default()
        .with_steam_cdn([SourceOutcome::Miss])
        .with_grid_slug([SourceOutcome::Bytes(b"steam-grid-cover")]);

    let result = resolve_with_backend(
        &backend,
        Some("grid-key"),
        policy(true, true, true),
        request(RemoteCoverLauncher::Steam, Some("480"), "Portal"),
    );

    assert_eq!(result.unwrap(), b"steam-grid-cover".to_vec());
    assert_eq!(
        backend.calls(),
        vec![
            Call::SteamCdn {
                app_id: "480".to_owned(),
            },
            Call::GridSlug {
                api_key: "grid-key".to_owned(),
                slug: "steam-480".to_owned(),
            },
        ]
    );
}

#[test]
fn steam_propagates_grid_cover_not_found_after_cdn_miss() {
    let backend = MockCoverSourceBackend::default()
        .with_steam_cdn([SourceOutcome::Miss])
        .with_grid_slug([SourceOutcome::Miss]);

    let result = resolve_with_backend(
        &backend,
        Some("grid-key"),
        policy(true, true, true),
        request(RemoteCoverLauncher::Steam, Some("480"), "Portal"),
    );

    assert_cover_not_found(result);
    assert_eq!(
        backend.calls(),
        vec![
            Call::SteamCdn {
                app_id: "480".to_owned(),
            },
            Call::GridSlug {
                api_key: "grid-key".to_owned(),
                slug: "steam-480".to_owned(),
            },
        ]
    );
}

#[test]
fn gog_without_external_id_returns_cover_not_found_before_any_remote_call() {
    let backend = MockCoverSourceBackend::default();

    let result = resolve_with_backend(
        &backend,
        None,
        policy(true, true, true),
        request(RemoteCoverLauncher::Gog, None, "Cyberpunk 2077"),
    );

    assert_cover_not_found(result);
    assert_eq!(backend.calls(), vec![]);
}

#[test]
fn gog_uses_gog_cdn_first_and_does_not_require_grid_key_when_cdn_succeeds() {
    let backend =
        MockCoverSourceBackend::default().with_gog_cdn([SourceOutcome::Bytes(b"gog-cdn-cover")]);

    let result = resolve_with_backend(
        &backend,
        None,
        policy(true, true, true),
        request(
            RemoteCoverLauncher::Gog,
            Some("1423049311"),
            "Cyberpunk 2077",
        ),
    );

    assert_eq!(result.unwrap(), b"gog-cdn-cover".to_vec());
    assert_eq!(
        backend.calls(),
        vec![Call::GogCdn {
            product_id: "1423049311".to_owned(),
        }]
    );
}

#[test]
fn gog_skips_gog_cdn_when_disabled_and_uses_grid_slug_then_autocomplete() {
    let backend = MockCoverSourceBackend::default()
        .with_grid_slug_then_autocomplete([SourceOutcome::Bytes(b"gog-grid-cover")]);

    let result = resolve_with_backend(
        &backend,
        Some("grid-key"),
        policy(true, false, true),
        request(
            RemoteCoverLauncher::Gog,
            Some("1423049311"),
            "Cyberpunk 2077",
        ),
    );

    assert_eq!(result.unwrap(), b"gog-grid-cover".to_vec());
    assert_eq!(
        backend.calls(),
        vec![Call::GridSlugThenAutocomplete {
            api_key: "grid-key".to_owned(),
            slug: "gog-1423049311".to_owned(),
            title: "Cyberpunk 2077".to_owned(),
        }]
    );
}

#[test]
fn gog_returns_cover_not_found_after_cdn_miss_when_grid_disabled() {
    let backend = MockCoverSourceBackend::default().with_gog_cdn([SourceOutcome::Miss]);

    let result = resolve_with_backend(
        &backend,
        None,
        policy(true, true, false),
        request(
            RemoteCoverLauncher::Gog,
            Some("1423049311"),
            "Cyberpunk 2077",
        ),
    );

    assert_cover_not_found(result);
    assert_eq!(
        backend.calls(),
        vec![Call::GogCdn {
            product_id: "1423049311".to_owned(),
        }]
    );
}

#[test]
fn gog_returns_cover_not_found_after_cdn_miss_when_grid_enabled_but_key_absent() {
    let backend = MockCoverSourceBackend::default().with_gog_cdn([SourceOutcome::Miss]);

    let result = resolve_with_backend(
        &backend,
        None,
        policy(true, true, true),
        request(
            RemoteCoverLauncher::Gog,
            Some("1423049311"),
            "Cyberpunk 2077",
        ),
    );

    // Missing key === grid effectively disabled. See the matching Steam
    // test above for rationale.
    assert_cover_not_found(result);
    assert_eq!(
        backend.calls(),
        vec![Call::GogCdn {
            product_id: "1423049311".to_owned(),
        }]
    );
}

#[test]
fn gog_falls_back_to_grid_slug_then_autocomplete_after_cdn_miss() {
    let backend = MockCoverSourceBackend::default()
        .with_gog_cdn([SourceOutcome::Miss])
        .with_grid_slug_then_autocomplete([SourceOutcome::Bytes(b"gog-grid-cover")]);

    let result = resolve_with_backend(
        &backend,
        Some("grid-key"),
        policy(true, true, true),
        request(
            RemoteCoverLauncher::Gog,
            Some("1423049311"),
            "Cyberpunk 2077",
        ),
    );

    assert_eq!(result.unwrap(), b"gog-grid-cover".to_vec());
    assert_eq!(
        backend.calls(),
        vec![
            Call::GogCdn {
                product_id: "1423049311".to_owned(),
            },
            Call::GridSlugThenAutocomplete {
                api_key: "grid-key".to_owned(),
                slug: "gog-1423049311".to_owned(),
                title: "Cyberpunk 2077".to_owned(),
            },
        ]
    );
}

#[test]
fn gog_propagates_grid_cover_not_found_after_cdn_miss() {
    let backend = MockCoverSourceBackend::default()
        .with_gog_cdn([SourceOutcome::Miss])
        .with_grid_slug_then_autocomplete([SourceOutcome::Miss]);

    let result = resolve_with_backend(
        &backend,
        Some("grid-key"),
        policy(true, true, true),
        request(
            RemoteCoverLauncher::Gog,
            Some("1423049311"),
            "Cyberpunk 2077",
        ),
    );

    assert_cover_not_found(result);
    assert_eq!(
        backend.calls(),
        vec![
            Call::GogCdn {
                product_id: "1423049311".to_owned(),
            },
            Call::GridSlugThenAutocomplete {
                api_key: "grid-key".to_owned(),
                slug: "gog-1423049311".to_owned(),
                title: "Cyberpunk 2077".to_owned(),
            },
        ]
    );
}

#[test]
fn other_returns_cover_not_found_when_grid_disabled_and_does_not_require_key() {
    let backend = MockCoverSourceBackend::default();

    let result = resolve_with_backend(
        &backend,
        None,
        policy(true, true, false),
        request(RemoteCoverLauncher::Other, None, "Unknown Game"),
    );

    assert_cover_not_found(result);
    assert_eq!(backend.calls(), vec![]);
}

#[test]
fn other_returns_cover_not_found_when_grid_enabled_but_key_absent() {
    let backend = MockCoverSourceBackend::default();

    let result = resolve_with_backend(
        &backend,
        None,
        policy(true, true, true),
        request(RemoteCoverLauncher::Other, None, "Unknown Game"),
    );

    // No CDN fallback exists for `Other` launchers, so a missing grid key
    // resolves to `CoverNotFound` immediately, with zero backend calls.
    assert_cover_not_found(result);
    assert_eq!(backend.calls(), vec![]);
}

#[test]
fn other_uses_grid_autocomplete_only_when_grid_enabled_and_key_present() {
    let backend = MockCoverSourceBackend::default()
        .with_grid_autocomplete([SourceOutcome::Bytes(b"autocomplete-cover")]);

    let result = resolve_with_backend(
        &backend,
        Some("grid-key"),
        policy(true, true, true),
        request(RemoteCoverLauncher::Other, None, "Unknown Game"),
    );

    assert_eq!(result.unwrap(), b"autocomplete-cover".to_vec());
    assert_eq!(
        backend.calls(),
        vec![Call::GridAutocomplete {
            api_key: "grid-key".to_owned(),
            title: "Unknown Game".to_owned(),
        }]
    );
}

#[test]
fn other_trims_grid_key_before_calling_backend() {
    let backend = MockCoverSourceBackend::default()
        .with_grid_autocomplete([SourceOutcome::Bytes(b"autocomplete-cover")]);

    let result = resolve_with_backend(
        &backend,
        Some("  trimmed-key  "),
        policy(true, true, true),
        request(RemoteCoverLauncher::Other, None, "Unknown Game"),
    );

    assert_eq!(result.unwrap(), b"autocomplete-cover".to_vec());
    assert_eq!(
        backend.calls(),
        vec![Call::GridAutocomplete {
            api_key: "trimmed-key".to_owned(),
            title: "Unknown Game".to_owned(),
        }]
    );
}

#[test]
fn other_propagates_grid_cover_not_found() {
    let backend = MockCoverSourceBackend::default().with_grid_autocomplete([SourceOutcome::Miss]);

    let result = resolve_with_backend(
        &backend,
        Some("grid-key"),
        policy(true, true, true),
        request(RemoteCoverLauncher::Other, None, "Unknown Game"),
    );

    assert_cover_not_found(result);
    assert_eq!(
        backend.calls(),
        vec![Call::GridAutocomplete {
            api_key: "grid-key".to_owned(),
            title: "Unknown Game".to_owned(),
        }]
    );
}

#[test]
fn steam_cdn_disabled_means_no_steam_cdn_call_even_if_outcome_is_configured() {
    let backend = MockCoverSourceBackend::default()
        .with_steam_cdn([SourceOutcome::Bytes(b"should-not-be-used")])
        .with_grid_slug([SourceOutcome::Bytes(b"grid-cover")]);

    let result = resolve_with_backend(
        &backend,
        Some("grid-key"),
        policy(false, true, true),
        request(RemoteCoverLauncher::Steam, Some("480"), "Portal"),
    );

    assert_eq!(result.unwrap(), b"grid-cover".to_vec());
    assert_eq!(
        backend.calls(),
        vec![Call::GridSlug {
            api_key: "grid-key".to_owned(),
            slug: "steam-480".to_owned(),
        }]
    );
}

#[test]
fn gog_cdn_disabled_means_no_gog_cdn_call_even_if_outcome_is_configured() {
    let backend = MockCoverSourceBackend::default()
        .with_gog_cdn([SourceOutcome::Bytes(b"should-not-be-used")])
        .with_grid_slug_then_autocomplete([SourceOutcome::Bytes(b"grid-cover")]);

    let result = resolve_with_backend(
        &backend,
        Some("grid-key"),
        policy(true, false, true),
        request(
            RemoteCoverLauncher::Gog,
            Some("1423049311"),
            "Cyberpunk 2077",
        ),
    );

    assert_eq!(result.unwrap(), b"grid-cover".to_vec());
    assert_eq!(
        backend.calls(),
        vec![Call::GridSlugThenAutocomplete {
            api_key: "grid-key".to_owned(),
            slug: "gog-1423049311".to_owned(),
            title: "Cyberpunk 2077".to_owned(),
        }]
    );
}

#[test]
fn grid_disabled_means_no_grid_call_even_when_key_is_present() {
    let backend = MockCoverSourceBackend::default().with_steam_cdn([SourceOutcome::Miss]);

    let result = resolve_with_backend(
        &backend,
        Some("grid-key"),
        policy(true, true, false),
        request(RemoteCoverLauncher::Steam, Some("480"), "Portal"),
    );

    assert_cover_not_found(result);
    assert_eq!(
        backend.calls(),
        vec![Call::SteamCdn {
            app_id: "480".to_owned(),
        }]
    );
}
