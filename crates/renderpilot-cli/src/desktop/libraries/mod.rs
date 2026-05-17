//! Desktop UI facade for downloading and managing graphics DLL libraries.

mod group;
mod http;
mod storage;
mod types;
mod validate;

#[cfg(test)]
mod tests;

use crate::desktop::utils::{to_json, JsonResult};
use crate::CliError;

pub use self::types::{LibraryManifest, LibraryManifestEntry, LibraryState};

const DEFAULT_MANIFEST_URL: &str =
    "https://osyka-yuri.github.io/renderpilot-libraries/manifest.json";
const MAX_MANIFEST_SIZE_BYTES: u64 = 2 * 1024 * 1024;

fn command_failed(message: impl Into<String>) -> CliError {
    CliError::CommandFailed(message.into())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Fetches the remote manifest from the configured URL and stores it locally.
pub async fn fetch_libraries_manifest() -> JsonResult {
    let manifest = download_manifest(DEFAULT_MANIFEST_URL).await?;
    save_local_manifest(&manifest)?;
    to_json(manifest)
}

/// Returns the local manifest if available, otherwise fetches it from remote.
pub async fn get_libraries_manifest() -> JsonResult {
    match load_local_manifest()? {
        Some(manifest) => to_json(manifest),
        None => fetch_libraries_manifest().await,
    }
}

/// Downloads a library entry by its ID from the local manifest.
pub async fn download_library(entry_id: String) -> JsonResult {
    let manifest = require_local_manifest()?;
    let entry = require_entry(&manifest, &entry_id)?;
    let archive_path = local_archive_path(entry)?;

    if archive_is_valid(&archive_path, entry)? {
        return to_json(library_state(entry, true, Some(&archive_path)));
    }

    storage::remove_file_if_exists(&archive_path)?;

    let payload = download_archive(entry).await?;
    validate::validate_archive_payload(entry, &payload)?;
    storage::write_file_atomically(&archive_path, &payload)?;

    to_json(library_state(entry, true, Some(&archive_path)))
}

/// Deletes a locally downloaded library by its ID.
///
/// Async for API uniformity with other desktop library commands.
pub async fn delete_library(entry_id: String) -> JsonResult {
    let manifest = require_local_manifest()?;
    let entry = require_entry(&manifest, &entry_id)?;
    let archive_path = local_archive_path(entry)?;

    storage::remove_file_if_exists(&archive_path)?;

    to_json(library_state(entry, false, None))
}

/// Returns the download state for all entries in the local manifest.
///
/// Async for API uniformity with other desktop library commands.
pub async fn get_library_states() -> JsonResult {
    let manifest = require_local_manifest()?;
    let mut states = Vec::with_capacity(manifest.entries.len());

    for entry in &manifest.entries {
        let archive_path = local_archive_path(entry)?;
        let is_downloaded = archive_path.exists();
        let local_path = is_downloaded.then_some(archive_path.as_path());

        states.push(library_state(entry, is_downloaded, local_path));
    }

    to_json(states)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

async fn download_manifest(url: &str) -> Result<LibraryManifest, CliError> {
    let client = http::http_client();
    let bytes =
        http::download_limited_bytes(client, url, MAX_MANIFEST_SIZE_BYTES, "manifest fetch")
            .await?;

    let manifest = serde_json::from_slice::<LibraryManifest>(&bytes)
        .map_err(|error| command_failed(format!("failed to parse manifest: {error}")))?;

    validate::validate_manifest(&manifest)?;
    Ok(manifest)
}

async fn download_archive(entry: &LibraryManifestEntry) -> Result<Vec<u8>, CliError> {
    let client = http::http_client();
    http::download_exact_bytes(
        client,
        &entry.files.zip.download_url,
        entry.files.zip.size_bytes,
        "library download",
    )
    .await
}

fn save_local_manifest(manifest: &LibraryManifest) -> Result<(), CliError> {
    validate::validate_manifest(manifest)?;

    let path = storage::local_manifest_path()?;
    let json = serde_json::to_vec_pretty(manifest)
        .map_err(|error| command_failed(format!("failed to serialize manifest: {error}")))?;

    storage::write_file_atomically(&path, &json)
}

fn load_local_manifest() -> Result<Option<LibraryManifest>, CliError> {
    let path = storage::local_manifest_path()?;

    if !path.exists() {
        return Ok(None);
    }

    let json = std::fs::read(&path)
        .map_err(|error| command_failed(format!("failed to read manifest: {error}")))?;

    let manifest = serde_json::from_slice::<LibraryManifest>(&json)
        .map_err(|error| command_failed(format!("failed to parse local manifest: {error}")))?;

    validate::validate_manifest(&manifest)?;
    Ok(Some(manifest))
}

fn require_local_manifest() -> Result<LibraryManifest, CliError> {
    load_local_manifest()?
        .ok_or_else(|| command_failed("manifest not loaded. please fetch manifest first."))
}

fn require_entry<'a>(
    manifest: &'a LibraryManifest,
    entry_id: &str,
) -> Result<&'a LibraryManifestEntry, CliError> {
    find_entry_by_id(manifest, entry_id).ok_or_else(|| {
        command_failed(format!(
            "library entry with id `{entry_id}` not found in manifest"
        ))
    })
}

fn find_entry_by_id<'a>(
    manifest: &'a LibraryManifest,
    entry_id: &str,
) -> Option<&'a LibraryManifestEntry> {
    manifest
        .entries
        .iter()
        .find(|entry| entry.entry_id == entry_id)
}

fn local_archive_path(entry: &LibraryManifestEntry) -> Result<std::path::PathBuf, CliError> {
    storage::local_archive_path(
        group::library_id_to_group_key(&entry.library.id),
        &entry.archive_file_name(),
    )
}

fn archive_is_valid(
    path: &std::path::Path,
    entry: &LibraryManifestEntry,
) -> Result<bool, CliError> {
    if !path.exists() {
        return Ok(false);
    }

    let payload = storage::read_file(path)?;
    Ok(validate::validate_archive_payload(entry, &payload).is_ok())
}

fn library_state(
    entry: &LibraryManifestEntry,
    is_downloaded: bool,
    local_path: Option<&std::path::Path>,
) -> LibraryState {
    LibraryState {
        id: entry.entry_id.clone(),
        version: entry.version.value.clone(),
        is_downloaded,
        local_path: local_path.map(|path| path.to_string_lossy().into_owned()),
    }
}
