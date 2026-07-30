//! Versioned contracts for the graphics-library catalog.

mod catalog;
mod legal;
mod package;

pub(crate) use catalog::{LibraryCatalog, LibraryVendorCatalog};
pub(crate) use catalog::{LibraryIndex, LibraryVendorReference, LibraryVendorSnapshot};
pub(crate) use legal::LibraryLegalDocument;
pub use legal::{LibraryLegalDocumentFormat, LibraryLegalDocumentKind, LibraryLegalDocumentLink};
pub(crate) use package::{
    LibraryArtifactRecord, LibraryContent, LibraryPackage, LibraryPackageMember, LibraryProvenance,
};
pub use package::{
    LibraryCatalogStatus, LibraryLocalState, LibraryPackageAvailability, LibraryPackageMutation,
    LibraryPackageState, LibraryPackageSummary, LibraryPackagesOutput, LibraryRelease,
    LibraryReleaseChannel, LibraryTarget, SignatureInfo,
};

#[cfg(test)]
pub(crate) use catalog::LibraryVendor;
#[cfg(test)]
pub(crate) use package::{
    LibrarySourceBuildToolchain, LibrarySourceInput, LibrarySourcePatch, LibraryTransport,
};
