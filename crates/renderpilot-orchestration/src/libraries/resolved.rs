//! Validated catalog views shared by package, artifact, and API projections.

use std::collections::HashMap;

use renderpilot_domain::ArtifactId;

use crate::ServiceError;

use super::library_error;
use super::types::{
    LibraryArtifactRecord, LibraryCatalog, LibraryLegalDocument, LibraryPackage,
    LibraryVendorCatalog,
};

/// A catalog whose structural validation and reference resolution ran once.
#[derive(Debug, Clone)]
pub(super) struct ValidatedCatalog {
    catalog: LibraryCatalog,
    index: CatalogIndex,
}

impl ValidatedCatalog {
    pub(super) fn new(catalog: LibraryCatalog) -> Result<Self, ServiceError> {
        let index = super::validation::validate_catalog(&catalog)?;
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
    index: &'a PackageIndex,
}

impl<'a> ResolvedPackage<'a> {
    fn new(catalog: &'a ValidatedCatalog, index: &'a PackageIndex) -> Self {
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

    pub(super) fn legal_documents(
        &self,
    ) -> impl ExactSizeIterator<Item = &'a LibraryLegalDocument> + Clone + '_ {
        let documents = &self.vendor().legal_documents;
        self.index
            .legal_documents
            .iter()
            .map(|document| &documents[*document])
    }

    pub(super) fn artifact_id(&self) -> &'a ArtifactId {
        &self.index.artifact_id
    }
}

#[derive(Debug, Clone)]
pub(super) struct CatalogIndex {
    pub(super) packages: Vec<PackageIndex>,
    pub(super) package_ids: HashMap<String, usize>,
    pub(super) artifact_ids: HashMap<ArtifactId, usize>,
}

#[derive(Debug, Clone)]
pub(super) struct PackageIndex {
    pub(super) vendor: usize,
    pub(super) package: usize,
    pub(super) members: Vec<usize>,
    pub(super) legal_documents: Vec<usize>,
    pub(super) artifact_id: ArtifactId,
}

pub(super) struct PackageReferences {
    pub(super) members: Vec<usize>,
    pub(super) legal_documents: Vec<usize>,
}
