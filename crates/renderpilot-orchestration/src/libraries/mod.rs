//! Download and lifecycle management for explicit graphics-library packages.

#[cfg(test)]
mod tests;

mod artifact_builder;
mod catalog;
mod compression;
mod locks;
mod packages;
mod resolved;
mod storage;
mod types;
mod validate;

use crate::ServiceError;
use crate::net::ProgressObserver;

pub(crate) use self::artifact_builder::catalog_packages_as_artifacts;
pub use self::storage::local_dlss_document_path;
pub use self::types::{
    LibraryPackageState, LibraryPackageSummary, LibraryRelease, LibraryReleaseChannel,
    LibraryTarget, SignatureInfo,
};

pub(super) const CATALOG_SOURCE: &str = "catalog-v1";

fn library_error(message: impl Into<String>) -> ServiceError {
    ServiceError::command_failed(message)
}

/// Fetches and atomically activates a complete catalog snapshot.
pub async fn fetch_catalog() -> Result<(), ServiceError> {
    catalog::fetch_validated_catalog().await.map(drop)
}

/// Returns the active local snapshot, fetching one when absent.
pub async fn get_or_fetch_catalog() -> Result<(), ServiceError> {
    catalog::get_or_fetch_validated_catalog().await.map(drop)
}

/// Downloads and registers one explicit catalog package.
pub async fn download_package(
    context: &crate::Context,
    package_id: String,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<LibraryPackageState, ServiceError> {
    let catalog = catalog::require_local_catalog()?;
    let package = catalog::require_package(&catalog, &package_id)?;
    let _lock = locks::acquire(format!(
        "package-revision:{}",
        package.package().revision_sha256
    ))
    .await;
    packages::ensure_package_downloaded(context, &package, progress).await
}

/// Downloads a catalog package by its domain artifact id.
pub async fn download_artifact(
    context: &crate::Context,
    artifact_id: String,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<LibraryPackageState, ServiceError> {
    let artifact_id = renderpilot_domain::ArtifactId::new(artifact_id)
        .map_err(|error| library_error(format!("invalid artifact id: {error}")))?;
    let catalog = catalog::require_local_catalog()?;
    let package = catalog::require_package_by_artifact_id(&catalog, &artifact_id)?;
    let _lock = locks::acquire(format!(
        "package-revision:{}",
        package.package().revision_sha256
    ))
    .await;
    packages::ensure_package_downloaded(context, &package, progress).await
}

/// Unregisters a downloaded package while retaining shared content blobs.
pub async fn delete_package(
    context: &crate::Context,
    package_id: String,
) -> Result<LibraryPackageState, ServiceError> {
    let catalog = catalog::require_local_catalog()?;
    let package = catalog::require_package(&catalog, &package_id)?;
    let _lock = locks::acquire(format!(
        "package-revision:{}",
        package.package().revision_sha256
    ))
    .await;
    packages::delete_package(context, &package)
}

/// Returns the resolved package projection used by desktop clients.
pub async fn list_packages(
    context: &crate::Context,
) -> Result<Vec<LibraryPackageSummary>, ServiceError> {
    let catalog = catalog::get_or_fetch_validated_catalog().await?;
    let states = packages::package_states(context, &catalog)?;
    let states: std::collections::HashMap<_, _> = states
        .into_iter()
        .map(|state| (state.package_id.clone(), state))
        .collect();
    let mut summaries = Vec::new();
    for package in catalog.packages() {
        let Some(artifact) = artifact_builder::build_catalog_artifact(&package, None)? else {
            continue;
        };
        let is_downloaded = states
            .get(&package.package().package_id)
            .is_some_and(|state| state.is_downloaded);
        summaries.push(artifact_builder::package_summary(
            &package,
            &artifact,
            is_downloaded,
        )?);
    }
    Ok(summaries)
}
