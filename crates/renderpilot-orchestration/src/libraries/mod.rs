//! Download and lifecycle management for explicit graphics-library packages.

#[cfg(test)]
mod tests;

mod artifact_builder;
mod catalog;
mod compression;
mod inventory;
mod local_verifier;
mod locks;
mod packages;
mod projection;
mod receipt;
mod resolved;
mod revision;
mod storage;
mod types;
mod validation;

use crate::ServiceError;
use crate::net::ProgressObserver;
use renderpilot_application::{ActiveCatalogPackage, ArtifactRepository};
use renderpilot_domain::{ArtifactId, LibraryArtifact};

pub(crate) use self::artifact_builder::catalog_packages_as_artifacts;
pub use self::storage::local_dlss_document_path;
pub use self::types::{
    LibraryCatalogStatus, LibraryLegalDocumentFormat, LibraryLegalDocumentKind,
    LibraryLegalDocumentLink, LibraryLocalState, LibraryPackageAvailability,
    LibraryPackageMutation, LibraryPackageState, LibraryPackageSummary, LibraryPackagesOutput,
    LibraryRelease, LibraryReleaseChannel, LibraryTarget, SignatureInfo,
};

pub(super) const CATALOG_SOURCE: &str = "catalog-v1";

type ReplacementArtifactProjection = (
    Vec<LibraryArtifact>,
    std::collections::HashSet<ArtifactId>,
    std::collections::HashMap<ArtifactId, ActiveCatalogPackage>,
);

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
) -> Result<LibraryPackageMutation, ServiceError> {
    let catalog = catalog::require_local_catalog()?;
    let package = catalog::require_package(&catalog, &package_id)?;
    let _lock = locks::acquire(format!("package-id:{package_id}")).await;
    packages::ensure_package_downloaded(context, &package, progress).await?;
    let inventory =
        inventory::Inventory::load(context, Some(&catalog), LibraryCatalogStatus::Active)?;
    Ok(LibraryPackageMutation {
        package_id: package_id.clone(),
        package: inventory.package(&package_id),
    })
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
    let _lock = locks::acquire(format!("package-id:{}", package.package().package_id)).await;
    packages::ensure_package_downloaded(context, &package, progress).await
}

/// Unregisters one logical package while retaining content-addressed blobs.
pub async fn delete_package(
    context: &crate::Context,
    package_id: String,
) -> Result<LibraryPackageMutation, ServiceError> {
    let _lock = locks::acquire(format!("package-id:{package_id}")).await;
    let catalog = match catalog::load_local_catalog() {
        Ok(catalog) => catalog,
        Err(error) => {
            log::warn!("could not load catalog while deleting package registration: {error}");
            None
        }
    };
    let status = if catalog.is_some() {
        LibraryCatalogStatus::Active
    } else {
        LibraryCatalogStatus::LocalFallback
    };
    context
        .storage()
        .delete_catalog_package_artifacts(&package_id)
        .map_err(ServiceError::from)?;
    let refreshed = inventory::Inventory::load(context, catalog.as_ref(), status)?;
    Ok(LibraryPackageMutation {
        package_id: package_id.clone(),
        package: refreshed.package(&package_id),
    })
}

/// Returns the resolved package projection used by desktop clients.
pub async fn list_packages(
    context: &crate::Context,
) -> Result<LibraryPackagesOutput, ServiceError> {
    let (catalog, status) = match catalog::get_or_fetch_validated_catalog().await {
        Ok(catalog) => (Some(catalog), LibraryCatalogStatus::Active),
        Err(error) => {
            log::warn!(
                "library catalog unavailable; returning receipt-only local fallback: {error}"
            );
            (None, LibraryCatalogStatus::LocalFallback)
        }
    };
    Ok(inventory::Inventory::load(context, catalog.as_ref(), status)?.package_output())
}

/// Builds the candidate universe from the same reconciliation used by Libraries.
pub(crate) fn replacement_artifacts(
    context: &crate::Context,
) -> Result<ReplacementArtifactProjection, ServiceError> {
    let catalog = match catalog::load_local_catalog() {
        Ok(catalog) => catalog,
        Err(error) => {
            log::warn!("could not load catalog replacement artifacts: {error}");
            None
        }
    };
    let status = if catalog.is_some() {
        LibraryCatalogStatus::Active
    } else {
        LibraryCatalogStatus::LocalFallback
    };
    Ok(inventory::Inventory::load(context, catalog.as_ref(), status)?.replacement_projection())
}
