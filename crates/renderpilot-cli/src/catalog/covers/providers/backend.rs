use reqwest::blocking::Client;

use crate::error::CliError;

use super::{gog, steam_cdn, steamgriddb};

/// Abstraction over remote cover sources.
///
/// The production implementation delegates to real network-backed modules.
/// Tests use a mock implementation to verify branching, fallback, and errors
/// without touching the network.
pub(super) trait CoverSourceBackend {
    fn try_steam_cdn(&self, client: &Client, app_id: &str) -> Option<Vec<u8>>;

    fn try_gog_cdn(&self, client: &Client, product_id: &str) -> Option<Vec<u8>>;

    fn try_download_grid_for_slug(
        &self,
        client: &Client,
        api_key: &str,
        slug: &str,
    ) -> Result<Vec<u8>, CliError>;

    fn try_download_grid_slug_then_autocomplete(
        &self,
        client: &Client,
        api_key: &str,
        slug: &str,
        title: &str,
    ) -> Result<Vec<u8>, CliError>;

    fn try_download_grid_autocomplete_only(
        &self,
        client: &Client,
        api_key: &str,
        title: &str,
    ) -> Result<Vec<u8>, CliError>;
}

#[derive(Debug, Default)]
pub(super) struct RealCoverSourceBackend;

impl CoverSourceBackend for RealCoverSourceBackend {
    fn try_steam_cdn(&self, client: &Client, app_id: &str) -> Option<Vec<u8>> {
        steam_cdn::try_steam_cdn(client, app_id).ok()
    }

    fn try_gog_cdn(&self, client: &Client, product_id: &str) -> Option<Vec<u8>> {
        gog::try_gog_cdn(client, product_id).ok()
    }

    fn try_download_grid_for_slug(
        &self,
        client: &Client,
        api_key: &str,
        slug: &str,
    ) -> Result<Vec<u8>, CliError> {
        steamgriddb::try_download_grid_for_slug(client, api_key, slug)
    }

    fn try_download_grid_slug_then_autocomplete(
        &self,
        client: &Client,
        api_key: &str,
        slug: &str,
        title: &str,
    ) -> Result<Vec<u8>, CliError> {
        steamgriddb::try_download_grid_slug_then_autocomplete(client, api_key, slug, title)
    }

    fn try_download_grid_autocomplete_only(
        &self,
        client: &Client,
        api_key: &str,
        title: &str,
    ) -> Result<Vec<u8>, CliError> {
        steamgriddb::try_download_grid_autocomplete_only(client, api_key, title)
    }
}
