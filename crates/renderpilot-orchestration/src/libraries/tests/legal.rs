use super::super::artifact_builder::build_catalog_artifact;
use super::super::projection::package_summary;
use super::super::resolved::ValidatedCatalog;
use super::super::types::{
    LibraryCatalog, LibraryLegalDocumentFormat, LibraryLegalDocumentKind, LibraryVendorCatalog,
};
use super::openvr_catalog;

fn valve(catalog: &LibraryCatalog) -> &LibraryVendorCatalog {
    catalog
        .vendors
        .iter()
        .find(|vendor| vendor.vendor.id == "valve")
        .expect("Valve")
}

fn valve_mut(catalog: &mut LibraryCatalog) -> &mut LibraryVendorCatalog {
    catalog
        .vendors
        .iter_mut()
        .find(|vendor| vendor.vendor.id == "valve")
        .expect("Valve")
}

fn openvr_fixture() -> LibraryCatalog {
    openvr_catalog(renderpilot_domain::openvr::UPSTREAM_REPOSITORY)
}

fn assert_invalid(catalog: LibraryCatalog, expected: &str) {
    let error = ValidatedCatalog::new(catalog).expect_err("catalog must be rejected");
    assert!(
        error.to_string().contains(expected),
        "expected `{expected}` in `{error}`"
    );
}

#[test]
fn package_summary_resolves_legal_documents_to_validated_content_links() {
    let catalog = ValidatedCatalog::new(openvr_fixture()).expect("validated OpenVR catalog");
    let resolved = catalog
        .packages()
        .find(|resolved| resolved.package().technology == "openvr")
        .expect("OpenVR package");
    let artifact = build_catalog_artifact(&resolved, None)
        .expect("adapter")
        .expect("known technology");

    let entry = super::super::inventory::InventoryEntry {
        package_id: resolved.package().package_id.clone(),
        active: Some(artifact),
        local: None,
        local_state: super::super::types::LibraryLocalState::Absent,
    };
    let summary = package_summary(&entry).expect("package summary");
    let document = summary.legal_documents.first().expect("license");
    assert_eq!(document.kind, LibraryLegalDocumentKind::License);
    assert_eq!(document.title, "OpenVR SDK License");
    assert_eq!(document.file_name, "LICENSE.txt");
    assert!(
        document
            .content_url
            .ends_with(&format!("/libraries/legal/sha256/{}.txt", "f".repeat(64)))
    );

    let wire = serde_json::to_value(document).expect("legal document link");
    assert!(wire.get("content_url").is_some());
    assert!(wire.get("url").is_none());
}

#[test]
fn legal_document_references_fail_closed_without_changing_package_revision() {
    let base = openvr_fixture();
    let package_revision = valve(&base).packages[0].revision_sha256.clone();

    let mut missing = base.clone();
    let package = &mut valve_mut(&mut missing).packages[0];
    package.legal_document_ids = vec!["license.missing".to_owned()];
    assert_eq!(package.revision_sha256, package_revision);
    assert_invalid(missing, "missing legal document");

    let mut duplicate = base.clone();
    let document_id = valve(&duplicate).packages[0].legal_document_ids[0].clone();
    valve_mut(&mut duplicate).packages[0].legal_document_ids =
        vec![document_id.clone(), document_id];
    assert_invalid(duplicate, "sorted and unique");

    let mut orphaned = base;
    valve_mut(&mut orphaned).packages[0]
        .legal_document_ids
        .clear();
    assert_invalid(orphaned, "unreferenced legal document");
}

#[test]
fn legal_document_metadata_is_bounded_and_content_addressed() {
    let base = openvr_fixture();

    let mut wrong_identity = base.clone();
    let valve = valve_mut(&mut wrong_identity);
    let wrong_id = format!("license.{}", "d".repeat(64));
    valve.legal_documents[0].legal_document_id = wrong_id.clone();
    valve.packages[0].legal_document_ids = vec![wrong_id];
    assert_invalid(wrong_identity, "not content-addressed");

    let mut mismatched_format = base.clone();
    valve_mut(&mut mismatched_format).legal_documents[0].format = LibraryLegalDocumentFormat::Pdf;
    assert_invalid(mismatched_format, "does not match its format");

    let mut oversized = base.clone();
    valve_mut(&mut oversized).legal_documents[0]
        .content
        .size_bytes = 16 * 1024 * 1024 + 1;
    assert_invalid(oversized, "size is outside");

    let mut unsafe_title = base;
    valve_mut(&mut unsafe_title).legal_documents[0].title = "OpenVR\nSDK License".to_owned();
    assert_invalid(unsafe_title, "concise, printable, and trimmed");
}
