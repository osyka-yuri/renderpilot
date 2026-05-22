//! Desktop UI facade for downloading and managing graphics DLL libraries.

mod artifact_builder;
mod http;
mod local_library;
mod manifest;
mod storage;
mod types;
mod validate;

#[cfg(test)]
mod tests;

use crate::desktop::utils::{to_json, JsonResult};
use crate::CliError;

pub use self::types::{LibraryManifest, LibraryManifestEntry, LibraryState};

pub(crate) use self::artifact_builder::manifest_entries_as_artifacts;
use self::local_library::{library_state, library_states, local_archive_path};

pub(super) fn library_error(message: impl Into<String>) -> CliError {
    CliError::CommandFailed(message.into())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Fetches the remote manifest from the configured URL and stores it locally.
pub async fn fetch_libraries_manifest() -> JsonResult {
    manifest::fetch_manifest().await
}

/// Returns the local manifest if available, otherwise fetches it from remote.
pub async fn get_libraries_manifest() -> JsonResult {
    manifest::get_or_fetch_manifest().await
}

/// Downloads a library entry by its ID from the local manifest.
pub async fn download_library(entry_id: String) -> JsonResult {
    let entry = manifest::require_local_manifest_entry(&entry_id)?;
    let downloaded_library = local_library::ensure_downloaded_and_registered(&entry).await?;

    to_json(library_state(
        &entry,
        true,
        Some(&downloaded_library.archive_path),
        Some(downloaded_library.artifact_id),
    ))
}

/// Deletes a locally downloaded library by its ID.
///
/// Async for API uniformity with other desktop library commands.
pub async fn delete_library(entry_id: String) -> JsonResult {
    let entry = manifest::require_local_manifest_entry(&entry_id)?;
    let archive_path = local_archive_path(&entry)?;

    storage::remove_file_if_exists(&archive_path)?;

    to_json(library_state(&entry, false, None, None))
}

/// Returns the download state for all entries in the local manifest.
///
/// Async for API uniformity with other desktop library commands.
pub async fn get_library_states() -> JsonResult {
    let entries = match manifest::load_local_manifest_entries() {
        Ok(Some(entries)) => entries,
        Ok(None) | Err(_) => return to_json(Vec::<LibraryState>::new()),
    };

    to_json(library_states(&entries)?)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------
