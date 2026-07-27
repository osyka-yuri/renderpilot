//! Desktop UI facade for downloading and managing graphics DLL libraries.
//!
//! All heavy computation (network, filesystem, artifact registry) lives in
//! `renderpilot-orchestration::libraries`. This module wraps those typed results
//! in `serde_json::Value` for the GUI command layer.

use crate::utils::{JsonResult, to_json};

pub use renderpilot_orchestration::libraries::{
    LibraryCatalogStatus, LibraryLegalDocumentFormat, LibraryLegalDocumentKind,
    LibraryLegalDocumentLink, LibraryLocalState, LibraryPackageMutation, LibraryPackageState,
    LibraryPackageSummary, LibraryPackagesOutput,
};
pub use renderpilot_orchestration::net::{DownloadProgress, ProgressObserver};

/// Fetches the replacement catalog and reports whether its authoritative file
/// changed, without forcing the coordinator to inspect serialized JSON.
pub async fn fetch_libraries_catalog_output() -> Result<bool, crate::ApiError> {
    renderpilot_orchestration::libraries::fetch_catalog()
        .await
        .map_err(Into::into)
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

/// Deletes the local registration for one logical package.
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
