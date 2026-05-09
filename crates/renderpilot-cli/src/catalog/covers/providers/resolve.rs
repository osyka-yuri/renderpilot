use renderpilot_domain::{GameInstallation, Launcher};
use reqwest::blocking::Client;

use super::super::policy::CoverRemotePolicy;
use super::backend::{CoverSourceBackend, RealCoverSourceBackend};
use crate::error::CliError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RemoteCoverLauncher {
    Steam,
    Gog,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EffectiveCoverRemotePolicy {
    pub(super) steam_cdn: bool,
    pub(super) gog_cdn: bool,
    pub(super) steamgriddb: bool,
}

impl From<&CoverRemotePolicy> for EffectiveCoverRemotePolicy {
    fn from(policy: &CoverRemotePolicy) -> Self {
        Self {
            steam_cdn: policy.steam_cdn,
            gog_cdn: policy.gog_cdn,
            steamgriddb: policy.steamgriddb,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CoverResolutionRequest<'a> {
    pub(super) launcher: RemoteCoverLauncher,
    pub(super) external_id: Option<&'a str>,
    pub(super) title: &'a str,
}

impl<'a> From<&'a GameInstallation> for CoverResolutionRequest<'a> {
    fn from(game: &'a GameInstallation) -> Self {
        let identity = game.identity();

        let launcher = match identity.launcher() {
            Launcher::Steam => RemoteCoverLauncher::Steam,
            Launcher::Gog => RemoteCoverLauncher::Gog,
            _ => RemoteCoverLauncher::Other,
        };

        Self {
            launcher,
            external_id: identity.external_id(),
            title: identity.title(),
        }
    }
}

/// Non-empty SteamGridDB bearer token, or `None` if the setting is missing or blank.
pub(super) fn normalized_steamgriddb_api_key(key: Option<&str>) -> Option<&str> {
    key.map(str::trim).filter(|key| !key.is_empty())
}

fn external_id_or_cover_not_found<'a>(
    request: CoverResolutionRequest<'a>,
) -> Result<&'a str, CliError> {
    request.external_id.ok_or(CliError::CoverNotFound)
}

fn require_griddb_key_if_allowed(
    api_key: Option<&str>,
    policy: EffectiveCoverRemotePolicy,
) -> Result<&str, CliError> {
    if !policy.steamgriddb {
        return Err(CliError::CoverNotFound);
    }

    // A missing or blank SteamGridDB API key is treated identically to
    // `policy.steamgriddb == false`: the resolver returns `CoverNotFound`
    // instead of `SteamGridDbApiKeyMissing`. Auto cover sync runs per game,
    // so a key-absent error would surface as a per-game warning even though
    // the underlying issue is a single global setting. The UI already
    // mirrors this in `gameMayReceiveRemoteCoverViaPolicy` (cover-sync.ts),
    // which never flags a game as grid-eligible without a key.
    api_key.ok_or(CliError::CoverNotFound)
}

fn resolve_steam_cover_bytes(
    client: &Client,
    api_key: Option<&str>,
    policy: EffectiveCoverRemotePolicy,
    request: CoverResolutionRequest<'_>,
    backend: &impl CoverSourceBackend,
) -> Result<Vec<u8>, CliError> {
    let app_id = external_id_or_cover_not_found(request)?;

    if policy.steam_cdn {
        if let Some(bytes) = backend.try_steam_cdn(client, app_id) {
            return Ok(bytes);
        }
    }

    let key = require_griddb_key_if_allowed(api_key, policy)?;
    let slug = format!("steam-{app_id}");

    backend.try_download_grid_for_slug(client, key, &slug)
}

fn resolve_gog_cover_bytes(
    client: &Client,
    api_key: Option<&str>,
    policy: EffectiveCoverRemotePolicy,
    request: CoverResolutionRequest<'_>,
    backend: &impl CoverSourceBackend,
) -> Result<Vec<u8>, CliError> {
    let product_id = external_id_or_cover_not_found(request)?;

    if policy.gog_cdn {
        if let Some(bytes) = backend.try_gog_cdn(client, product_id) {
            return Ok(bytes);
        }
    }

    let key = require_griddb_key_if_allowed(api_key, policy)?;
    let slug = format!("gog-{product_id}");

    backend.try_download_grid_slug_then_autocomplete(client, key, &slug, request.title)
}

fn resolve_other_cover_bytes(
    client: &Client,
    api_key: Option<&str>,
    policy: EffectiveCoverRemotePolicy,
    request: CoverResolutionRequest<'_>,
    backend: &impl CoverSourceBackend,
) -> Result<Vec<u8>, CliError> {
    let key = require_griddb_key_if_allowed(api_key, policy)?;

    backend.try_download_grid_autocomplete_only(client, key, request.title)
}

pub(super) fn resolve_cover_bytes_with_backend(
    client: &Client,
    api_key: Option<&str>,
    policy: EffectiveCoverRemotePolicy,
    request: CoverResolutionRequest<'_>,
    backend: &impl CoverSourceBackend,
) -> Result<Vec<u8>, CliError> {
    let api_key = normalized_steamgriddb_api_key(api_key);

    match request.launcher {
        RemoteCoverLauncher::Steam => {
            resolve_steam_cover_bytes(client, api_key, policy, request, backend)
        }
        RemoteCoverLauncher::Gog => {
            resolve_gog_cover_bytes(client, api_key, policy, request, backend)
        }
        RemoteCoverLauncher::Other => {
            resolve_other_cover_bytes(client, api_key, policy, request, backend)
        }
    }
}

pub(crate) fn resolve_cover_bytes(
    client: &Client,
    api_key: Option<&str>,
    policy: &CoverRemotePolicy,
    game: &GameInstallation,
) -> Result<Vec<u8>, CliError> {
    resolve_cover_bytes_with_backend(
        client,
        api_key,
        EffectiveCoverRemotePolicy::from(policy),
        CoverResolutionRequest::from(game),
        &RealCoverSourceBackend,
    )
}

#[cfg(test)]
mod tests;
