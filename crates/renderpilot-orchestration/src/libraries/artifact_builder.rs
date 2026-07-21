//! Direct adapter from explicit catalog packages to domain artifacts.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use renderpilot_application::validate_runtime_artifact;
use renderpilot_domain::{
    ArtifactId, ArtifactMetadata, ArtifactTrustLevel, ComponentFile, GraphicsTechnology,
    LibraryArtifact, PathRef, PeCompatibilityProfile, RuntimeTarget, Sha256Hash, UpstreamPackage,
    UpstreamPackageProvider, Version,
};

use crate::ServiceError;

use super::resolved::{ResolvedPackage, ValidatedCatalog};
use super::types::{
    LibraryArtifactRecord, LibraryPackage, LibraryPackageSummary, LibraryProvenance,
};
use super::{CATALOG_SOURCE, catalog, library_error};

/// Domain artifacts plus their catalog package ids and debug-package ids.
pub(crate) struct CatalogArtifactSet {
    pub(super) artifacts: Vec<LibraryArtifact>,
    pub(super) package_ids: HashMap<ArtifactId, String>,
    pub(super) debug_package_ids: HashSet<String>,
}

impl CatalogArtifactSet {
    pub(crate) fn into_parts(
        self,
    ) -> (
        Vec<LibraryArtifact>,
        HashMap<ArtifactId, String>,
        HashSet<String>,
    ) {
        (self.artifacts, self.package_ids, self.debug_package_ids)
    }
}

/// Converts every supported explicit package in the active catalog.
pub(crate) fn catalog_packages_as_artifacts() -> Result<CatalogArtifactSet, ServiceError> {
    let Some(catalog) = catalog::load_local_catalog()? else {
        return Ok(CatalogArtifactSet {
            artifacts: Vec::new(),
            package_ids: HashMap::new(),
            debug_package_ids: HashSet::new(),
        });
    };
    catalog_as_artifacts(&catalog)
}

fn catalog_as_artifacts(catalog: &ValidatedCatalog) -> Result<CatalogArtifactSet, ServiceError> {
    let package_count = catalog.packages().len();
    let mut artifacts = Vec::with_capacity(package_count);
    let mut package_ids = HashMap::with_capacity(package_count);
    let mut debug_package_ids = HashSet::new();

    for resolved in catalog.packages() {
        let package = resolved.package();
        let artifact = match build_catalog_artifact(&resolved, None) {
            Ok(Some(artifact)) => artifact,
            Ok(None) => {
                log::warn!(
                    "catalog package `{}` uses unknown technology `{}`; skipping it",
                    package.package_id,
                    package.technology
                );
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
        if package.release.channel == super::types::LibraryReleaseChannel::Debug {
            debug_package_ids.insert(package.package_id.clone());
        }
        package_ids.insert(artifact.id().clone(), package.package_id.clone());
        artifacts.push(artifact);
    }

    Ok(CatalogArtifactSet {
        artifacts,
        package_ids,
        debug_package_ids,
    })
}

/// Builds a virtual catalog artifact or its materialized local counterpart.
/// `local_paths`, when present, must follow package-member order.
pub(super) fn build_catalog_artifact(
    resolved: &ResolvedPackage<'_>,
    local_paths: Option<&[std::path::PathBuf]>,
) -> Result<Option<LibraryArtifact>, ServiceError> {
    let vendor = resolved.vendor();
    let package = resolved.package();
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
    .with_metadata(package_metadata(package)?)
    .with_source(CATALOG_SOURCE)
    .map_err(|error| library_error(format!("failed to attach catalog source: {error}")))?;
    validate_runtime_artifact(&artifact)
        .map_err(|error| library_error(format!("invalid runtime package contract: {error}")))?;
    Ok(Some(artifact))
}

/// Builds the compact package projection consumed by desktop clients.
pub(super) fn package_summary(
    resolved: &ResolvedPackage<'_>,
    artifact: &LibraryArtifact,
    is_downloaded: bool,
) -> Result<LibraryPackageSummary, ServiceError> {
    let package = resolved.package();
    let primary = resolved
        .members()
        .next()
        .ok_or_else(|| library_error(format!("package `{}` has no members", package.package_id)))?;
    let primary_sha256 = primary.dll.sha256.clone();
    let size_bytes = resolved.members().try_fold(0_u64, |total, member| {
        total.checked_add(member.dll.size_bytes).ok_or_else(|| {
            library_error(format!(
                "package `{}` member size overflows",
                package.package_id
            ))
        })
    })?;

    Ok(LibraryPackageSummary {
        package_id: package.package_id.clone(),
        artifact_id: artifact.id().as_str().to_owned(),
        vendor: resolved.vendor().vendor.id.clone(),
        technology: package.technology.clone(),
        variant: package.variant.clone(),
        display_name: package.display_name.clone(),
        release: package.release.clone(),
        target: package.target.clone(),
        revision_sha256: package.revision_sha256.clone(),
        primary_file_name: primary.file_name.clone(),
        primary_sha256,
        primary_signature: primary.signature.clone(),
        size_bytes,
        is_downloaded,
    })
}

pub(super) fn package_is_supported(package: &LibraryPackage) -> bool {
    package_technology(package).is_some()
}

fn package_technology(package: &LibraryPackage) -> Option<GraphicsTechnology> {
    GraphicsTechnology::from_slug(&package.technology)
        .filter(|technology| *technology != GraphicsTechnology::Unknown)
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

fn package_metadata(package: &LibraryPackage) -> Result<ArtifactMetadata, ServiceError> {
    let release = Version::parse(&package.release.version)
        .map_err(|error| library_error(format!("invalid package release version: {error}")))?;
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
                UpstreamPackage::new(UpstreamPackageProvider::NuGet, package_id, version)
                    .map_err(|error| library_error(format!("invalid NuGet provenance: {error}")))?,
            );
        }
        Some(LibraryProvenance::GithubRelease { repository, .. }) => {
            metadata = metadata.with_upstream_package(
                UpstreamPackage::new(
                    UpstreamPackageProvider::GitHub,
                    repository,
                    &package.release.version,
                )
                .map_err(|error| library_error(format!("invalid GitHub provenance: {error}")))?,
            );
        }
        None => {}
    }
    Ok(metadata)
}

fn path_ref(path: &Path) -> Result<PathRef, ServiceError> {
    PathRef::new(path.to_string_lossy().as_ref())
        .map_err(|error| library_error(format!("invalid local artifact path: {error}")))
}
