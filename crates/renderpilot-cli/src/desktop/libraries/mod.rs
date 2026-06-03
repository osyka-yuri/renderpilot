//! Desktop UI facade for downloading and managing graphics DLL libraries.

mod artifact_builder;
mod fsr_packages;
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
pub(crate) use self::storage::local_preset_manifest_path;

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

/// Materializes a swap artifact by its **artifact id**, downloading whatever it
/// needs: a single manifest DLL, or every member of a composed FSR release
/// package. Returns a [`LibraryState`] whose `artifact_id` is the downloaded
/// artifact ready to apply.
pub async fn download_artifact(artifact_id: String) -> JsonResult {
    let target_id = renderpilot_domain::ArtifactId::new(artifact_id.clone())
        .map_err(|error| library_error(format!("invalid artifact id: {error}")))?;
    let manifest = manifest::require_local_manifest()?;

    // Single-file manifest artifact.
    let (_, entry_ids) = manifest_entries_as_artifacts()?;
    if let Some(entry_id) = entry_ids.get(&target_id) {
        let entry = manifest::require_entry(&manifest, entry_id)?.clone();
        let downloaded = local_library::ensure_downloaded_and_registered(&entry).await?;
        return to_json(library_state(
            &entry,
            true,
            Some(&downloaded.archive_path),
            Some(downloaded.artifact_id),
        ));
    }

    // Composed FSR release package (download every member, register one artifact).
    if let Some(package) = fsr_packages::compose_fsr_packages(&manifest.entries)
        .into_iter()
        .find(|package| *package.artifact.id() == target_id)
    {
        let registered = local_library::ensure_package_downloaded(&manifest, &package).await?;
        return to_json(fsr_package_state(&package, registered));
    }

    Err(library_error(format!("unknown artifact id: {artifact_id}")))
}

fn fsr_package_state(package: &fsr_packages::FsrPackage, artifact_id: String) -> LibraryState {
    LibraryState {
        id: artifact_id.clone(),
        version: package
            .artifact
            .version()
            .map(|version| version.as_str().to_owned())
            .unwrap_or_default(),
        is_downloaded: true,
        local_path: None,
        artifact_id: Some(artifact_id),
    }
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
