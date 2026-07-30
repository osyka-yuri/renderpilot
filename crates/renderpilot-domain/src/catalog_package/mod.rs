//! Stable catalog-package identities and download receipts.

mod provenance;
mod receipt;
mod release;

pub use provenance::{
    CatalogLegalDocumentFormat, CatalogLegalDocumentKind, CatalogLegalDocumentReceipt,
    CatalogPackageProvenanceReceipt, CatalogProvenanceReceipt, CatalogSignatureReceipt,
    CatalogSourceBuildToolchainReceipt, CatalogSourcePatchReceipt, CatalogSourceReceipt,
    CatalogTargetReceipt,
};
pub use receipt::{
    CatalogPackageMemberReceipt, CatalogPackageReceipt, CatalogPackageReceiptV1,
    CatalogPackageReceiptV2, CatalogReceiptSchemaV1, CatalogReceiptSchemaV2,
};
pub use release::{CatalogPackageAvailability, PackageRelease, ReleaseChannel};
