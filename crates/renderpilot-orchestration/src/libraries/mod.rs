//! Download and management of graphics DLL libraries.

#[cfg(test)]
mod tests;

mod artifact_builder;
mod compression;
mod fsr_packages;
mod http;
mod local_library;
mod manifest;
mod storage;
mod types;
mod validate;

use crate::ServiceError;

pub use self::types::{LibraryManifest, LibraryManifestEntry, LibraryState};

pub use self::artifact_builder::manifest_entries_as_artifacts;
pub use self::storage::local_preset_manifest_path;

// ---------------------------------------------------------------------------
// Download progress contract
// ---------------------------------------------------------------------------

/// Cumulative progress of a library download, in bytes.
#[derive(Clone, Copy, Debug)]
pub struct DownloadProgress {
    /// Number of bytes received so far.
    pub downloaded_bytes: u64,
    /// Total expected size in bytes.
    pub total_bytes: u64,
}

/// Observer invoked as bytes arrive; must be cheap and non-blocking.
///
/// The lifetime parameter keeps the alias usable for non-`'static` observers,
/// e.g. per-member wrappers that borrow an outer observer.
pub type ProgressObserver<'a> = dyn Fn(DownloadProgress) + Send + Sync + 'a;

fn library_error(message: impl Into<String>) -> ServiceError {
    ServiceError::CommandFailed(message.into())
}

// ---------------------------------------------------------------------------
// Public orchestration entry points — CLI/API facade wraps these with to_json
// ---------------------------------------------------------------------------

/// Fetches the remote manifest and saves it locally. Returns the manifest.
pub async fn fetch_manifest() -> Result<LibraryManifest, ServiceError> {
    manifest::fetch_manifest().await
}

/// Returns the local manifest if available, otherwise fetches and saves it.
pub async fn get_or_fetch_manifest() -> Result<LibraryManifest, ServiceError> {
    manifest::get_or_fetch_manifest().await
}

/// Downloads a library entry by its manifest entry ID.
pub async fn download_library(
    context: &crate::Context,
    entry_id: String,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<LibraryState, ServiceError> {
    let entry = manifest::require_local_manifest_entry(&entry_id)?;
    let downloaded =
        local_library::ensure_downloaded_and_registered(context, &entry, progress).await?;

    Ok(local_library::library_state(
        &entry,
        true,
        Some(&downloaded.dll_path),
        Some(downloaded.artifact_id),
    ))
}

/// Materializes a swap artifact by its artifact id.
///
/// Handles both single manifest DLL entries and composed FSR release packages.
/// Returns a [`LibraryState`] whose `artifact_id` is the downloaded artifact.
pub async fn download_artifact(
    context: &crate::Context,
    artifact_id: String,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<LibraryState, ServiceError> {
    let target_id = renderpilot_domain::ArtifactId::new(artifact_id.clone())
        .map_err(|error| library_error(format!("invalid artifact id: {error}")))?;
    let manifest = manifest::require_local_manifest()?;

    // Single-file manifest artifact.
    let (_, entry_ids, _) = artifact_builder::manifest_entries_as_artifacts()?;
    if let Some(entry_id) = entry_ids.get(&target_id) {
        let entry = manifest::require_entry(&manifest, entry_id)?.clone();
        let downloaded =
            local_library::ensure_downloaded_and_registered(context, &entry, progress).await?;
        return Ok(local_library::library_state(
            &entry,
            true,
            Some(&downloaded.dll_path),
            Some(downloaded.artifact_id),
        ));
    }

    // Composed FSR release package (download every member, register one artifact).
    if let Some(package) = fsr_packages::compose_fsr_packages(&manifest.entries)
        .into_iter()
        .find(|package| *package.artifact.id() == target_id)
    {
        let registered =
            local_library::ensure_package_downloaded(context, &manifest, &package, progress)
                .await?;
        return Ok(fsr_package_state(&package, registered));
    }

    Err(library_error(format!("unknown artifact id: {artifact_id}")))
}

/// Deletes a locally downloaded library by its manifest entry ID.
pub async fn delete_library(
    context: &crate::Context,
    entry_id: String,
) -> Result<LibraryState, ServiceError> {
    let entry = manifest::require_local_manifest_entry(&entry_id)?;

    if let Err(error) = local_library::delete_local_library(context, &entry) {
        log::error!(
            "Failed to delete local library for entry {}: {}",
            entry_id,
            error
        );
    }

    Ok(local_library::library_state(&entry, false, None, None))
}

/// Returns the download state for all entries in the local manifest.
pub fn get_library_states() -> Result<Vec<LibraryState>, ServiceError> {
    let entries = match manifest::load_local_manifest_entries() {
        Ok(Some(entries)) => entries,
        Ok(None) | Err(_) => return Ok(Vec::new()),
    };

    local_library::library_states(&entries)
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
