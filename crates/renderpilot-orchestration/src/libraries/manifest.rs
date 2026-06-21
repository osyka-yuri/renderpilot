use crate::cdn::{self, CdnManifestSpec};
use crate::ServiceError;

use super::{
    library_error, storage,
    types::{LibraryManifest, LibraryManifestEntry},
    validate,
};

const MANIFEST_FILE_NAME: &str = "libraries_manifest.json";
const MAX_MANIFEST_SIZE_BYTES: u64 = 2 * 1024 * 1024;

/// The CDN cache spec for the library manifest. `ttl: None` keeps the established
/// behavior — a present cache is used as-is; it refreshes only on an explicit
/// [`fetch_manifest`].
fn manifest_spec() -> CdnManifestSpec {
    CdnManifestSpec {
        file_name: MANIFEST_FILE_NAME,
        url: cdn::cdn_url("manifest.json"),
        max_size_bytes: MAX_MANIFEST_SIZE_BYTES,
        ttl: None,
    }
}

fn preset_urls() -> [String; 3] {
    [
        cdn::cdn_url("dlss_presets.json"),
        cdn::cdn_url("dlss_g_presets.json"),
        cdn::cdn_url("dlss_d_presets.json"),
    ]
}

fn parse_manifest(bytes: &[u8]) -> Result<LibraryManifest, ServiceError> {
    let manifest = serde_json::from_slice::<LibraryManifest>(bytes)
        .map_err(|error| library_error(format!("failed to parse manifest: {error}")))?;
    validate::validate_manifest(&manifest)?;
    Ok(manifest)
}

/// Fetches the remote manifest, caches it, downloads the preset manifests
/// (best-effort), and returns the manifest.
pub(super) async fn fetch_manifest() -> Result<LibraryManifest, ServiceError> {
    let manifest = cdn::fetch(&manifest_spec(), parse_manifest).await?;

    for url in preset_urls() {
        if let Err(error) = download_and_save_preset(&url).await {
            log::warn!("failed to download preset manifest {url}: {error}");
        }
    }

    Ok(manifest)
}

async fn download_and_save_preset(url: &str) -> Result<(), ServiceError> {
    let bytes =
        crate::net::download_limited_bytes(url, MAX_MANIFEST_SIZE_BYTES, "preset fetch").await?;
    if let Some(file_name) = url.split('/').next_back() {
        let path = storage::local_preset_manifest_path(file_name)?;
        crate::fs::write_file_atomically(&path, crate::fs::strip_utf8_bom(&bytes))?;
    }
    Ok(())
}

/// Returns the cached manifest if present, otherwise fetches it (with presets).
pub(super) async fn get_or_fetch_manifest() -> Result<LibraryManifest, ServiceError> {
    if let Some(manifest) = cdn::cached(&manifest_spec(), parse_manifest) {
        return Ok(manifest);
    }
    fetch_manifest().await
}

pub(super) fn require_local_manifest() -> Result<LibraryManifest, ServiceError> {
    load_local_manifest()?
        .ok_or_else(|| library_error("manifest not loaded. please fetch manifest first."))
}

/// Returns the cached manifest if one is present (ignoring TTL — the library
/// manifest's spec has `ttl: None`), otherwise `None`.
pub(super) fn load_local_manifest() -> Result<Option<LibraryManifest>, ServiceError> {
    Ok(cdn::cached(&manifest_spec(), parse_manifest))
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
