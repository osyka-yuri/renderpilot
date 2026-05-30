use crate::{
    desktop::libraries::types::{LibraryManifest, LibraryManifestEntry},
    desktop::utils::{to_json, JsonResult},
    error::CliError,
};

use super::{http, library_error, storage, validate};

const DEFAULT_MANIFEST_URL: &str =
    "https://osyka-yuri.github.io/renderpilot-libraries/manifest.json";
const PRESET_URLS: &[&str] = &[
    "https://osyka-yuri.github.io/renderpilot-libraries/dlss_presets.json",
    "https://osyka-yuri.github.io/renderpilot-libraries/dlss_g_presets.json",
    "https://osyka-yuri.github.io/renderpilot-libraries/dlss_d_presets.json",
];
const MAX_MANIFEST_SIZE_BYTES: u64 = 2 * 1024 * 1024;

pub(super) async fn fetch_manifest() -> JsonResult {
    let manifest = download_manifest(DEFAULT_MANIFEST_URL).await?;
    save_local_manifest(&manifest)?;

    for url in PRESET_URLS {
        if let Err(error) = download_and_save_preset(url).await {
            log::warn!("failed to download preset manifest {url}: {error}");
        }
    }

    to_json(manifest)
}

async fn download_and_save_preset(url: &str) -> Result<(), CliError> {
    let client = http::http_client();
    let bytes = http::download_limited_bytes(client, url, MAX_MANIFEST_SIZE_BYTES, "preset fetch").await?;
    if let Some(file_name) = url.split('/').next_back() {
        let path = storage::local_preset_manifest_path(file_name)?;
        storage::write_file_atomically(&path, &bytes)?;
    }
    Ok(())
}

pub(super) async fn get_or_fetch_manifest() -> JsonResult {
    match load_local_manifest() {
        Ok(Some(manifest)) => to_json(manifest),
        Ok(None) => fetch_manifest().await,
        Err(_) => {
            if let Ok(path) = storage::local_manifest_path() {
                let _ = std::fs::remove_file(&path);
            }
            fetch_manifest().await
        }
    }
}

async fn download_manifest(url: &str) -> Result<LibraryManifest, CliError> {
    let client = http::http_client();
    let bytes =
        http::download_limited_bytes(client, url, MAX_MANIFEST_SIZE_BYTES, "manifest fetch")
            .await?;

    let manifest = serde_json::from_slice::<LibraryManifest>(&bytes)
        .map_err(|error| library_error(format!("failed to parse manifest: {error}")))?;

    validate::validate_manifest(&manifest)?;
    Ok(manifest)
}

pub(super) fn save_local_manifest(manifest: &LibraryManifest) -> Result<(), CliError> {
    validate::validate_manifest(manifest)?;

    let path = storage::local_manifest_path()?;
    let json = serde_json::to_vec_pretty(manifest)
        .map_err(|error| library_error(format!("failed to serialize manifest: {error}")))?;

    storage::write_file_atomically(&path, &json)
}

pub(super) fn load_local_manifest() -> Result<Option<LibraryManifest>, CliError> {
    let path = storage::local_manifest_path()?;

    if !path.exists() {
        return Ok(None);
    }

    let json = std::fs::read(&path)
        .map_err(|error| library_error(format!("failed to read manifest: {error}")))?;

    let manifest = serde_json::from_slice::<LibraryManifest>(&json)
        .map_err(|error| library_error(format!("failed to parse local manifest: {error}")))?;

    validate::validate_manifest(&manifest)?;
    Ok(Some(manifest))
}

pub(super) fn require_local_manifest() -> Result<LibraryManifest, CliError> {
    load_local_manifest()?
        .ok_or_else(|| library_error("manifest not loaded. please fetch manifest first."))
}

pub(super) fn load_local_manifest_entries() -> Result<Option<Vec<LibraryManifestEntry>>, CliError> {
    Ok(load_local_manifest()?.map(|manifest| manifest.entries))
}

pub(super) fn require_local_manifest_entry(
    entry_id: &str,
) -> Result<LibraryManifestEntry, CliError> {
    let manifest = require_local_manifest()?;

    require_entry(&manifest, entry_id).cloned()
}

pub(super) fn require_entry<'a>(
    manifest: &'a LibraryManifest,
    entry_id: &str,
) -> Result<&'a LibraryManifestEntry, CliError> {
    find_entry_by_id(manifest, entry_id).ok_or_else(|| {
        library_error(format!(
            "library entry with id `{entry_id}` not found in manifest"
        ))
    })
}

pub(super) fn find_entry_by_id<'a>(
    manifest: &'a LibraryManifest,
    entry_id: &str,
) -> Option<&'a LibraryManifestEntry> {
    manifest
        .entries
        .iter()
        .find(|entry| entry.entry_id == entry_id)
}
