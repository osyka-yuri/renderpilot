//! Immutable receipts attached to downloaded catalog artifacts.

use renderpilot_domain::{
    CatalogLegalDocumentReceipt, CatalogPackageReceiptV1, CatalogProvenanceReceipt,
    CatalogReceiptSchemaV1, CatalogTargetReceipt, Sha256Hash,
};

use crate::ServiceError;

use super::library_error;
use super::resolved::ResolvedPackage;
use super::types::LibraryProvenance;

pub(super) fn package_receipt(
    resolved: &ResolvedPackage<'_>,
) -> Result<CatalogPackageReceiptV1, ServiceError> {
    let package = resolved.package();
    let primary = resolved
        .members()
        .next()
        .ok_or_else(|| library_error(format!("package `{}` has no members", package.package_id)))?;
    let primary_sha256 = Sha256Hash::new(&primary.dll.sha256)
        .map_err(|error| library_error(format!("invalid primary digest: {error}")))?;
    let revision_sha256 = Sha256Hash::new(&package.revision_sha256)
        .map_err(|error| library_error(format!("invalid package revision: {error}")))?;
    let size_bytes = resolved.members().try_fold(0_u64, |total, member| {
        total.checked_add(member.dll.size_bytes).ok_or_else(|| {
            library_error(format!(
                "package `{}` member size overflows",
                package.package_id
            ))
        })
    })?;
    let legal_documents = resolved
        .legal_documents()
        .map(|document| CatalogLegalDocumentReceipt {
            legal_document_id: document.legal_document_id.clone(),
            kind: document.kind,
            title: document.title.clone(),
            format: document.format,
            file_name: document.file_name.clone(),
            content_url: crate::cdn::cdn_url(&document.object_key),
        })
        .collect();
    let provenance = package
        .provenance
        .as_ref()
        .map(|provenance| match provenance {
            LibraryProvenance::Nuget {
                package_id,
                version,
                package_sha512,
            } => CatalogProvenanceReceipt::Nuget {
                package_id: package_id.clone(),
                version: version.clone(),
                package_sha512: package_sha512.clone(),
            },
            LibraryProvenance::GithubRelease {
                repository,
                tag,
                commit_sha,
            } => CatalogProvenanceReceipt::GithubRelease {
                repository: repository.clone(),
                tag: tag.clone(),
                commit_sha: commit_sha.clone(),
            },
        });
    Ok(CatalogPackageReceiptV1 {
        schema_version: CatalogReceiptSchemaV1,
        package_id: package.package_id.clone(),
        vendor: resolved.vendor().vendor.id.clone(),
        technology: package.technology.clone(),
        variant: package.variant.clone(),
        display_name: package.display_name.clone(),
        release: package.release.to_package_release(),
        target: CatalogTargetReceipt {
            os: package.target.os.clone(),
            architecture: package.target.architecture,
            compatibility: package.target.compatibility.clone(),
        },
        provenance,
        revision_sha256,
        primary_file_name: primary.file_name.clone(),
        primary_sha256,
        primary_signature: primary.signature.to_receipt(),
        legal_documents,
        size_bytes,
    })
}
