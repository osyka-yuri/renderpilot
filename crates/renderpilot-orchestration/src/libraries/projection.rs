//! UI-facing projections derived from reconciled immutable catalog receipts.

use super::inventory::InventoryEntry;
use super::types::{LibraryLegalDocumentLink, LibraryPackageSummary, LibraryTarget, SignatureInfo};

pub(super) fn package_summary(entry: &InventoryEntry) -> Option<LibraryPackageSummary> {
    let artifact = entry.presentation_artifact()?;
    let receipt = artifact.metadata().catalog_package_receipt()?;
    let primary_file_name = receipt.primary_file_name()?;
    let primary_sha256 = receipt.primary_sha256()?;
    let primary_signature = receipt.primary_signature()?;
    let target = receipt.target();
    Some(LibraryPackageSummary {
        package_id: receipt.package_id().to_owned(),
        vendor: receipt.vendor().to_owned(),
        technology: receipt.technology().to_owned(),
        variant: receipt.variant().to_owned(),
        display_name: receipt.display_name().to_owned(),
        release: receipt.release().clone().into(),
        target: LibraryTarget {
            os: target.os.clone(),
            architecture: target.architecture,
            compatibility: target.compatibility.clone(),
        },
        primary_file_name: primary_file_name.to_owned(),
        primary_sha256: primary_sha256.as_str().to_owned(),
        primary_signature: SignatureInfo::from_receipt(primary_signature),
        legal_documents: receipt
            .legal_documents()
            .iter()
            .map(|document| LibraryLegalDocumentLink {
                legal_document_id: document.legal_document_id.clone(),
                kind: document.kind,
                title: document.title.clone(),
                format: document.format,
                file_name: document.file_name.clone(),
                content_url: document.content_url.clone(),
            })
            .collect(),
        size_bytes: receipt.size_bytes(),
        availability: entry.availability(),
        local_state: entry.local_state,
        automatic_selection_allowed: entry.automatic_selection_allowed(),
    })
}
