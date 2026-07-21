//! Desktop UI facade for downloading and managing graphics DLL libraries.
//!
//! All heavy computation (network, filesystem, artifact registry) lives in
//! `renderpilot-orchestration::libraries`. This module wraps those typed results
//! in `serde_json::Value` for the GUI command layer.

use crate::utils::{JsonResult, to_json};

pub use renderpilot_orchestration::libraries::{LibraryPackageState, LibraryPackageSummary};
pub use renderpilot_orchestration::net::{DownloadProgress, ProgressObserver};

// ---------------------------------------------------------------------------
// Public JSON facade
// ---------------------------------------------------------------------------

/// Fetches and atomically activates the remote catalog snapshot.
pub async fn fetch_libraries_catalog() -> JsonResult {
    to_json(renderpilot_orchestration::libraries::fetch_catalog().await?)
}

/// Downloads an explicit library package by its catalog ID.
pub async fn download_library_package(
    context: &renderpilot_orchestration::Context,
    package_id: String,
    progress: Option<&ProgressObserver<'_>>,
) -> JsonResult {
    to_json(
        renderpilot_orchestration::libraries::download_package(context, package_id, progress)
            .await?,
    )
}

/// Materializes a swap artifact by its **artifact id**.
pub async fn download_artifact(
    context: &renderpilot_orchestration::Context,
    artifact_id: String,
    progress: Option<&ProgressObserver<'_>>,
) -> JsonResult {
    to_json(
        renderpilot_orchestration::libraries::download_artifact(context, artifact_id, progress)
            .await?,
    )
}

/// Deletes a locally downloaded package by its catalog ID.
pub async fn delete_library_package(
    context: &renderpilot_orchestration::Context,
    package_id: String,
) -> JsonResult {
    to_json(renderpilot_orchestration::libraries::delete_package(context, package_id).await?)
}

/// Returns the resolved package projection used by desktop clients.
pub async fn list_library_packages(context: &renderpilot_orchestration::Context) -> JsonResult {
    to_json(renderpilot_orchestration::libraries::list_packages(context).await?)
}
