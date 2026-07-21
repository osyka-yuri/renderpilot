//! Validated catalog views shared by package, artifact, and API projections.

use renderpilot_domain::ArtifactId;

use crate::ServiceError;

use super::types::{LibraryArtifactRecord, LibraryCatalog, LibraryPackage, LibraryVendorCatalog};
use super::{library_error, validate};

/// A catalog whose structural validation and reference resolution ran once.
#[derive(Debug, Clone)]
pub(super) struct ValidatedCatalog {
    catalog: LibraryCatalog,
    index: validate::CatalogIndex,
}

impl ValidatedCatalog {
    pub(super) fn new(catalog: LibraryCatalog) -> Result<Self, ServiceError> {
        let index = validate::validate_catalog(&catalog)?;
        Ok(Self { catalog, index })
    }

    pub(super) fn as_catalog(&self) -> &LibraryCatalog {
        &self.catalog
    }

    pub(super) fn packages(&self) -> impl ExactSizeIterator<Item = ResolvedPackage<'_>> + '_ {
        self.index
            .packages
            .iter()
            .map(|index| ResolvedPackage::new(self, index))
    }

    pub(super) fn package(&self, package_id: &str) -> Result<ResolvedPackage<'_>, ServiceError> {
        self.index
            .package_ids
            .get(package_id)
            .map(|index| ResolvedPackage::new(self, &self.index.packages[*index]))
            .ok_or_else(|| library_error(format!("unknown library package id: `{package_id}`")))
    }

    pub(super) fn package_by_artifact_id(
        &self,
        artifact_id: &ArtifactId,
    ) -> Result<ResolvedPackage<'_>, ServiceError> {
        self.index
            .artifact_ids
            .get(artifact_id)
            .map(|index| ResolvedPackage::new(self, &self.index.packages[*index]))
            .ok_or_else(|| library_error(format!("unknown library artifact id: `{artifact_id}`")))
    }
}

/// One package with all of its physical artifact references resolved.
#[derive(Debug, Clone, Copy)]
pub(super) struct ResolvedPackage<'a> {
    catalog: &'a ValidatedCatalog,
    index: &'a validate::PackageIndex,
}

impl<'a> ResolvedPackage<'a> {
    fn new(catalog: &'a ValidatedCatalog, index: &'a validate::PackageIndex) -> Self {
        Self { catalog, index }
    }

    pub(super) fn vendor(&self) -> &'a LibraryVendorCatalog {
        &self.catalog.catalog.vendors[self.index.vendor]
    }

    pub(super) fn package(&self) -> &'a LibraryPackage {
        &self.vendor().packages[self.index.package]
    }

    pub(super) fn members(
        &self,
    ) -> impl ExactSizeIterator<Item = &'a LibraryArtifactRecord> + Clone + '_ {
        let artifacts = &self.vendor().artifacts;
        self.index.members.iter().map(|member| &artifacts[*member])
    }

    pub(super) fn artifact_id(&self) -> &'a ArtifactId {
        &self.index.artifact_id
    }
}
