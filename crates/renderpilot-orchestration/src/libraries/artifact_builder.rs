//! Direct adapter from explicit catalog packages to domain artifacts.

use std::path::Path;

use renderpilot_application::validate_runtime_artifact;
use renderpilot_domain::{
    ArtifactMetadata, ArtifactTrustLevel, ComponentFile, LibraryArtifact, LibraryTechnology,
    PathRef, PeCompatibilityProfile, RuntimeTarget, Sha256Hash, UpstreamPackage,
    UpstreamPackageProvider, Version,
};

use crate::ServiceError;

use super::resolved::{ResolvedPackage, ValidatedCatalog};
use super::types::{LibraryArtifactRecord, LibraryPackage, LibraryProvenance};
use super::{CATALOG_SOURCE, catalog, library_error};

const DXC_PACKAGE_FILE_NAMES: [&str; 2] = ["dxcompiler.dll", "dxil.dll"];

/// Converts every supported explicit package in the active catalog.
pub(crate) fn catalog_packages_as_artifacts() -> Result<Vec<LibraryArtifact>, ServiceError> {
    let Some(catalog) = catalog::load_local_catalog()? else {
        return Ok(Vec::new());
    };
    catalog_as_artifacts(&catalog)
}

pub(super) fn catalog_as_artifacts(
    catalog: &ValidatedCatalog,
) -> Result<Vec<LibraryArtifact>, ServiceError> {
    let package_count = catalog.packages().len();
    let mut artifacts = Vec::with_capacity(package_count);

    for resolved in catalog.packages() {
        let package = resolved.package();
        if !package_is_supported(package) {
            continue;
        }
        let artifact = match build_catalog_artifact(&resolved, None) {
            Ok(Some(artifact)) => artifact,
            Ok(None) => {
                continue;
            }
            Err(error) => {
                log::warn!(
                    "catalog package `{}` cannot be represented by this client: {error}; skipping it",
                    package.package_id
                );
                continue;
            }
        };
        artifacts.push(artifact);
    }

    Ok(artifacts)
}

/// Builds a virtual catalog artifact or its materialized local counterpart.
/// `local_paths`, when present, must follow package-member order.
pub(super) fn build_catalog_artifact(
    resolved: &ResolvedPackage<'_>,
    local_paths: Option<&[std::path::PathBuf]>,
) -> Result<Option<LibraryArtifact>, ServiceError> {
    let vendor = resolved.vendor();
    let package = resolved.package();
    if !package_is_supported(package) {
        return Ok(None);
    }
    let Some(technology) = package_technology(package) else {
        return Ok(None);
    };
    if local_paths.is_some_and(|paths| paths.len() != package.members.len()) {
        return Err(library_error(format!(
            "local member paths are out of sync for package `{}`",
            package.package_id
        )));
    }

    let mut files = Vec::with_capacity(package.members.len());
    for (index, (member, artifact)) in package.members.iter().zip(resolved.members()).enumerate() {
        let path = match local_paths {
            Some(paths) => path_ref(&paths[index])?,
            None => PathRef::new(format!(
                "catalog://{}/{}/{}",
                vendor.vendor.id, package.package_id, member.install_as
            ))
            .map_err(|error| library_error(format!("invalid virtual catalog path: {error}")))?,
        };
        files.push(build_component_file(path, artifact, &member.install_as)?);
    }

    let primary_name = package
        .members
        .first()
        .ok_or_else(|| library_error(format!("package `{}` has no members", package.package_id)))?
        .install_as
        .as_str();
    let artifact = LibraryArtifact::new(
        resolved.artifact_id().clone(),
        technology,
        primary_name,
        files,
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .map_err(|error| library_error(format!("failed to build catalog artifact: {error}")))?
    .with_metadata(package_metadata(resolved)?)
    .with_source(CATALOG_SOURCE)
    .map_err(|error| library_error(format!("failed to attach catalog source: {error}")))?;
    validate_runtime_artifact(&artifact)
        .map_err(|error| library_error(format!("invalid runtime package contract: {error}")))?;
    Ok(Some(artifact))
}

pub(super) fn package_is_supported(package: &LibraryPackage) -> bool {
    match package_technology(package) {
        Some(LibraryTechnology::MicrosoftDxc) => has_complete_dxc_pair(package),
        Some(_) => true,
        None => false,
    }
}

fn package_technology(package: &LibraryPackage) -> Option<LibraryTechnology> {
    LibraryTechnology::from_slug(&package.technology)
        .filter(|technology| *technology != LibraryTechnology::Unknown)
}

fn has_complete_dxc_pair(package: &LibraryPackage) -> bool {
    package.members.len() == DXC_PACKAGE_FILE_NAMES.len()
        && DXC_PACKAGE_FILE_NAMES.iter().all(|expected| {
            package
                .members
                .iter()
                .any(|member| member.install_as.eq_ignore_ascii_case(expected))
        })
}

fn build_component_file(
    path: PathRef,
    artifact: &LibraryArtifactRecord,
    install_as: &str,
) -> Result<ComponentFile, ServiceError> {
    let sha256 = Sha256Hash::new(&artifact.dll.sha256)
        .map_err(|error| library_error(format!("invalid artifact digest: {error}")))?;
    let mut file = ComponentFile::new(path)
        .with_sha256(sha256)
        .with_install_as(install_as);
    if let Some(version) = &artifact.file_version {
        file =
            file.with_version(Version::parse(version).map_err(|error| {
                library_error(format!("invalid artifact file version: {error}"))
            })?);
    }
    if let Some(exports) = &artifact.pe_named_exports {
        file = file.with_pe_compatibility(PeCompatibilityProfile::new(
            artifact.architecture,
            exports.clone(),
        ));
    }
    Ok(file)
}

fn package_metadata(resolved: &ResolvedPackage<'_>) -> Result<ArtifactMetadata, ServiceError> {
    let package = resolved.package();
    let release = package.release.version.numeric_core().clone();
    let mut metadata = ArtifactMetadata::default()
        .with_release(release, package.release.label.clone())
        .map_err(|error| library_error(format!("invalid release metadata: {error}")))?
        .with_runtime_target(match &package.target.compatibility {
            Some(compatibility) => RuntimeTarget::new(package.target.architecture)
                .with_compatibility(compatibility.clone()),
            None => RuntimeTarget::new(package.target.architecture),
        });
    match &package.provenance {
        Some(LibraryProvenance::Nuget {
            package_id,
            version,
            ..
        }) => {
            metadata = metadata.with_upstream_package(
                UpstreamPackage::new(UpstreamPackageProvider::NuGet, package_id, version.as_str())
                    .map_err(|error| library_error(format!("invalid NuGet provenance: {error}")))?,
            );
        }
        Some(LibraryProvenance::GithubRelease { repository, .. }) => {
            metadata = metadata.with_upstream_package(
                UpstreamPackage::new(
                    UpstreamPackageProvider::GitHub,
                    repository,
                    package.release.version.as_str(),
                )
                .map_err(|error| library_error(format!("invalid GitHub provenance: {error}")))?,
            );
        }
        None => {}
    }
    Ok(metadata.with_catalog_package_receipt(super::receipt::package_receipt(resolved)?))
}

fn path_ref(path: &Path) -> Result<PathRef, ServiceError> {
    PathRef::new(path.to_string_lossy().as_ref())
        .map_err(|error| library_error(format!("invalid local artifact path: {error}")))
}
