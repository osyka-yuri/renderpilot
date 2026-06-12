use crate::ServiceError;

use super::{
    http, library_error, storage,
    types::{LibraryManifest, LibraryManifestEntry},
    validate,
};

/// Single source of truth for the public library CDN host: the host constant
/// and every URL built by `lib_url!` are derived from this one literal, so
/// changing the host cannot desync URL construction from host pinning in
/// `validate::validate_entry`.
macro_rules! libs_host {
    () => {
        "pub-48612a35034d40f88f42b4181547925a.r2.dev"
    };
}

/// Host that all manifest, preset, and archive downloads are pinned to.
pub(crate) const LIBS_HOST: &str = libs_host!();

macro_rules! lib_url {
    ($path:literal) => {
        concat!("https://", libs_host!(), "/", $path)
    };
}

const DEFAULT_MANIFEST_URL: &str = lib_url!("manifest.json");
const PRESET_URLS: &[&str] = &[
    lib_url!("dlss_presets.json"),
    lib_url!("dlss_g_presets.json"),
    lib_url!("dlss_d_presets.json"),
];
const MAX_MANIFEST_SIZE_BYTES: u64 = 2 * 1024 * 1024;

/// Fetches the remote manifest, saves it locally, and returns the manifest.
pub(super) async fn fetch_manifest() -> Result<LibraryManifest, ServiceError> {
    let manifest = download_manifest(DEFAULT_MANIFEST_URL).await?;
    save_local_manifest(&manifest)?;

    for url in PRESET_URLS {
        if let Err(error) = download_and_save_preset(url).await {
            log::warn!("failed to download preset manifest {url}: {error}");
        }
    }

    Ok(manifest)
}

async fn download_and_save_preset(url: &str) -> Result<(), ServiceError> {
    let client = http::http_client();
    let bytes =
        http::download_limited_bytes(client, url, MAX_MANIFEST_SIZE_BYTES, "preset fetch").await?;
    if let Some(file_name) = url.split('/').next_back() {
        let path = storage::local_preset_manifest_path(file_name)?;
        storage::write_file_atomically(&path, strip_utf8_bom(&bytes))?;
    }
    Ok(())
}

/// Returns `bytes` without a leading UTF-8 byte-order mark.
///
/// The published JSON documents are produced by PowerShell tooling, which
/// historically prepends a BOM that `serde_json` rejects. Stripping it at the
/// download/read boundary keeps parsing independent of how the publisher
/// encoded the file.
pub(super) fn strip_utf8_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

/// Returns the local manifest if available, otherwise fetches and saves it.
pub(super) async fn get_or_fetch_manifest() -> Result<LibraryManifest, ServiceError> {
    match load_local_manifest() {
        Ok(Some(manifest)) => Ok(manifest),
        Ok(None) => fetch_manifest().await,
        Err(_) => {
            if let Ok(path) = storage::local_manifest_path() {
                let _ = std::fs::remove_file(&path);
            }
            fetch_manifest().await
        }
    }
}

async fn download_manifest(url: &str) -> Result<LibraryManifest, ServiceError> {
    let client = http::http_client();
    let bytes =
        http::download_limited_bytes(client, url, MAX_MANIFEST_SIZE_BYTES, "manifest fetch")
            .await?;

    let manifest = serde_json::from_slice::<LibraryManifest>(strip_utf8_bom(&bytes))
        .map_err(|error| library_error(format!("failed to parse manifest: {error}")))?;

    validate::validate_manifest(&manifest)?;
    Ok(manifest)
}

pub(super) fn save_local_manifest(manifest: &LibraryManifest) -> Result<(), ServiceError> {
    validate::validate_manifest(manifest)?;

    let path = storage::local_manifest_path()?;
    let json = serde_json::to_vec_pretty(manifest)
        .map_err(|error| library_error(format!("failed to serialize manifest: {error}")))?;

    storage::write_file_atomically(&path, &json)
}

pub(super) fn load_local_manifest() -> Result<Option<LibraryManifest>, ServiceError> {
    let path = storage::local_manifest_path()?;

    if !path.exists() {
        return Ok(None);
    }

    let json = std::fs::read(&path)
        .map_err(|error| library_error(format!("failed to read manifest: {error}")))?;

    let manifest = serde_json::from_slice::<LibraryManifest>(strip_utf8_bom(&json))
        .map_err(|error| library_error(format!("failed to parse local manifest: {error}")))?;

    validate::validate_manifest(&manifest)?;
    Ok(Some(manifest))
}

pub(super) fn require_local_manifest() -> Result<LibraryManifest, ServiceError> {
    load_local_manifest()?
        .ok_or_else(|| library_error("manifest not loaded. please fetch manifest first."))
}

pub(super) fn load_local_manifest_entries(
) -> Result<Option<Vec<LibraryManifestEntry>>, ServiceError> {
    Ok(load_local_manifest()?.map(|manifest| manifest.entries))
}

pub(super) fn require_local_manifest_entry(
    entry_id: &str,
) -> Result<LibraryManifestEntry, ServiceError> {
    let manifest = require_local_manifest()?;

    require_entry(&manifest, entry_id).cloned()
}

pub(super) fn require_entry<'a>(
    manifest: &'a LibraryManifest,
    entry_id: &str,
) -> Result<&'a LibraryManifestEntry, ServiceError> {
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
