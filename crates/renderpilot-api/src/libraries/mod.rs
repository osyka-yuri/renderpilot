//! Desktop UI facade for downloading and managing graphics DLL libraries.
//!
//! All heavy computation (network, filesystem, artifact registry) lives in
//! `renderpilot-orchestration::libraries`. This module wraps those typed results
//! in `serde_json::Value` for the GUI command layer.

use crate::utils::{to_json, JsonResult};

pub use renderpilot_orchestration::libraries::{
    LibraryManifest, LibraryManifestEntry, LibraryState,
};

// ---------------------------------------------------------------------------
// Public JSON facade
// ---------------------------------------------------------------------------

/// Fetches the remote manifest from the configured URL and stores it locally.
pub async fn fetch_libraries_manifest() -> JsonResult {
    to_json(renderpilot_orchestration::libraries::fetch_manifest().await?)
}

/// Returns the local manifest if available, otherwise fetches it from remote.
pub async fn get_libraries_manifest() -> JsonResult {
    to_json(renderpilot_orchestration::libraries::get_or_fetch_manifest().await?)
}

/// Downloads a library entry by its ID from the local manifest.
pub async fn download_library(
    context: &renderpilot_orchestration::Context,
    entry_id: String,
) -> JsonResult {
    to_json(renderpilot_orchestration::libraries::download_library(context, entry_id).await?)
}

/// Materializes a swap artifact by its **artifact id**.
pub async fn download_artifact(
    context: &renderpilot_orchestration::Context,
    artifact_id: String,
) -> JsonResult {
    to_json(renderpilot_orchestration::libraries::download_artifact(context, artifact_id).await?)
}

/// Deletes a locally downloaded library by its ID.
pub async fn delete_library(
    context: &renderpilot_orchestration::Context,
    entry_id: String,
) -> JsonResult {
    to_json(renderpilot_orchestration::libraries::delete_library(context, entry_id).await?)
}

/// Returns the download state for all entries in the local manifest.
pub async fn get_library_states() -> JsonResult {
    to_json(renderpilot_orchestration::libraries::get_library_states()?)
}
