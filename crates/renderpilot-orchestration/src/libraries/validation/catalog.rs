use std::collections::{HashMap, HashSet};

use renderpilot_domain::{ArtifactId, Sha256Hash};

use crate::ServiceError;

use super::super::library_error;
use super::super::resolved::{CatalogIndex, PackageIndex, PackageReferences};
use super::super::types::{
    LibraryArtifactRecord, LibraryCatalog, LibraryIndex, LibraryLegalDocument, LibraryPackage,
    LibraryVendorReference, LibraryVendorSnapshot,
};
use super::artifact::validate_artifact;
use super::fields::{ensure_id, ensure_not_blank, ensure_sha256, validate_schema};
use super::legal::validate_legal_documents;
use super::package::{ArtifactLookup, validate_package};

pub(in crate::libraries) const MAX_INDEX_SIZE: u64 = 256 * 1024;
const MAX_VENDOR_SNAPSHOT_SIZE: u64 = 2 * 1024 * 1024;

struct VendorPolicy {
    id: &'static str,
    required_in_v1_cache: bool,
}

// Valve and Xiph are supported by this client, but remain optional so older
// last-known-good caches can still be used offline.
const VENDOR_POLICIES: &[VendorPolicy] = &[
    VendorPolicy {
        id: "amd",
        required_in_v1_cache: true,
    },
    VendorPolicy {
        id: "intel",
        required_in_v1_cache: true,
    },
    VendorPolicy {
        id: "microsoft",
        required_in_v1_cache: true,
    },
    VendorPolicy {
        id: "nvidia",
        required_in_v1_cache: true,
    },
    VendorPolicy {
        id: "valve",
        required_in_v1_cache: false,
    },
    VendorPolicy {
        id: "xiph",
        required_in_v1_cache: false,
    },
];

pub(in crate::libraries) fn is_supported_vendor(vendor_id: &str) -> bool {
    VENDOR_POLICIES.iter().any(|policy| policy.id == vendor_id)
}

fn required_vendor_ids() -> impl Iterator<Item = &'static str> {
    VENDOR_POLICIES
        .iter()
        .filter(|policy| policy.required_in_v1_cache)
        .map(|policy| policy.id)
}

pub(in crate::libraries) fn validate_index(index: &LibraryIndex) -> Result<(), ServiceError> {
    validate_schema(index.schema_version, "library index")?;
    ensure_not_blank("index generated_at", &index.generated_at)?;

    let mut ids = HashSet::with_capacity(index.vendors.len());
    for vendor in &index.vendors {
        validate_vendor_reference(vendor)?;
        if !ids.insert(vendor.vendor_id.as_str()) {
            return Err(library_error(format!(
                "duplicate vendor `{}` in library index",
                vendor.vendor_id
            )));
        }
    }

    for required in required_vendor_ids() {
        if !ids.contains(required) {
            return Err(library_error(format!(
                "library index is missing supported vendor `{required}`"
            )));
        }
    }

    Ok(())
}

fn validate_vendor_reference(reference: &LibraryVendorReference) -> Result<(), ServiceError> {
    ensure_id("vendor id", &reference.vendor_id)?;
    ensure_not_blank("vendor display name", &reference.display_name)?;
    ensure_sha256("vendor snapshot sha256", &reference.snapshot_sha256)?;
    if reference.snapshot_size_bytes == 0
        || reference.snapshot_size_bytes > MAX_VENDOR_SNAPSHOT_SIZE
    {
        return Err(library_error(format!(
            "vendor snapshot size for `{}` is outside 1..={MAX_VENDOR_SNAPSHOT_SIZE}",
            reference.vendor_id
        )));
    }

    let expected = format!(
        "libraries/v1/vendors/{}/{}.json",
        reference.vendor_id, reference.snapshot_sha256
    );
    if reference.snapshot_key != expected {
        return Err(library_error(format!(
            "vendor snapshot key for `{}` is not canonical",
            reference.vendor_id
        )));
    }

    Ok(())
}

pub(in crate::libraries) fn validate_vendor_snapshot_envelope(
    snapshot: &LibraryVendorSnapshot,
    reference: &LibraryVendorReference,
) -> Result<(), ServiceError> {
    validate_schema(snapshot.schema_version, "vendor snapshot")?;
    ensure_not_blank("vendor snapshot generated_at", &snapshot.generated_at)?;
    if snapshot.vendor.id != reference.vendor_id
        || snapshot.vendor.display_name != reference.display_name
    {
        return Err(library_error(format!(
            "vendor snapshot identity does not match index reference `{}`",
            reference.vendor_id
        )));
    }

    Ok(())
}

pub(in crate::libraries) fn validate_catalog(
    catalog: &LibraryCatalog,
) -> Result<CatalogIndex, ServiceError> {
    validate_schema(catalog.schema_version, "library catalog cache")?;
    ensure_not_blank("catalog generated_at", &catalog.generated_at)?;

    let package_count = catalog
        .vendors
        .iter()
        .map(|vendor| vendor.packages.len())
        .sum();
    let mut vendors = HashSet::new();
    let mut packages = Vec::with_capacity(package_count);
    let mut package_ids = HashMap::with_capacity(package_count);
    let mut artifact_ids = HashMap::with_capacity(package_count);

    for (vendor_index, vendor) in catalog.vendors.iter().enumerate() {
        if !is_supported_vendor(&vendor.vendor.id) {
            return Err(library_error(format!(
                "cached catalog contains unsupported vendor `{}`",
                vendor.vendor.id
            )));
        }
        if !vendors.insert(vendor.vendor.id.as_str()) {
            return Err(library_error(format!(
                "cached catalog contains duplicate vendor `{}`",
                vendor.vendor.id
            )));
        }

        let references = validate_vendor_contents(
            &vendor.vendor.id,
            &vendor.vendor.display_name,
            &vendor.generated_at,
            &vendor.legal_documents,
            &vendor.artifacts,
            &vendor.packages,
        )?;
        for (package_index, (package, references)) in
            vendor.packages.iter().zip(references).enumerate()
        {
            let resolved_index = packages.len();
            if package_ids
                .insert(package.package_id.clone(), resolved_index)
                .is_some()
            {
                return Err(library_error(format!(
                    "cached catalog contains duplicate package `{}`",
                    package.package_id
                )));
            }

            let revision = Sha256Hash::new(&package.revision_sha256)
                .map_err(|error| library_error(format!("invalid package revision: {error}")))?;
            let artifact_id = ArtifactId::for_package_revision(&revision);
            if artifact_ids
                .insert(artifact_id.clone(), resolved_index)
                .is_some()
            {
                return Err(library_error(format!(
                    "cached catalog contains duplicate package revision `{}`",
                    package.revision_sha256
                )));
            }
            packages.push(PackageIndex {
                vendor: vendor_index,
                package: package_index,
                members: references.members,
                legal_documents: references.legal_documents,
                artifact_id,
            });
        }
    }

    for required in required_vendor_ids() {
        if !vendors.contains(required) {
            return Err(library_error(format!(
                "cached catalog is missing supported vendor `{required}`"
            )));
        }
    }

    Ok(CatalogIndex {
        packages,
        package_ids,
        artifact_ids,
    })
}

fn validate_vendor_contents(
    vendor_id: &str,
    display_name: &str,
    generated_at: &str,
    legal_documents: &[LibraryLegalDocument],
    artifacts: &[LibraryArtifactRecord],
    packages: &[LibraryPackage],
) -> Result<Vec<PackageReferences>, ServiceError> {
    ensure_not_blank("vendor display name", display_name)?;
    ensure_not_blank("vendor generated_at", generated_at)?;

    let artifact_lookup = build_artifact_lookup(vendor_id, artifacts)?;
    let legal_document_lookup = validate_legal_documents(vendor_id, legal_documents)?;
    let mut package_ids = HashSet::with_capacity(packages.len());
    let mut referenced_artifacts = HashSet::new();
    let mut referenced_legal_documents = HashSet::new();
    let mut resolved_packages = Vec::with_capacity(packages.len());

    for package in packages {
        let references = validate_package(package, &artifact_lookup, &legal_document_lookup)?;
        if !package_ids.insert(package.package_id.as_str()) {
            return Err(library_error(format!(
                "duplicate package `{}` in vendor `{vendor_id}`",
                package.package_id
            )));
        }
        referenced_artifacts.extend(references.members.iter().copied());
        referenced_legal_documents.extend(references.legal_documents.iter().copied());
        resolved_packages.push(references);
    }

    reject_orphaned_artifacts(vendor_id, artifacts, &referenced_artifacts)?;
    reject_orphaned_legal_documents(vendor_id, legal_documents, &referenced_legal_documents)?;

    Ok(resolved_packages)
}

fn build_artifact_lookup<'a>(
    vendor_id: &str,
    artifacts: &'a [LibraryArtifactRecord],
) -> Result<ArtifactLookup<'a>, ServiceError> {
    let mut artifacts_by_id = HashMap::with_capacity(artifacts.len());
    for (index, artifact) in artifacts.iter().enumerate() {
        validate_artifact(artifact)?;
        if artifacts_by_id
            .insert(artifact.artifact_id.as_str(), (index, artifact))
            .is_some()
        {
            return Err(library_error(format!(
                "duplicate artifact `{}` in vendor `{vendor_id}`",
                artifact.artifact_id
            )));
        }
    }
    Ok(artifacts_by_id)
}

fn reject_orphaned_artifacts(
    vendor_id: &str,
    artifacts: &[LibraryArtifactRecord],
    referenced: &HashSet<usize>,
) -> Result<(), ServiceError> {
    if referenced.len() == artifacts.len() {
        return Ok(());
    }
    if let Some((_, artifact)) = artifacts
        .iter()
        .enumerate()
        .find(|(index, _)| !referenced.contains(index))
    {
        return Err(library_error(format!(
            "vendor `{vendor_id}` contains unreferenced artifact `{}`",
            artifact.artifact_id
        )));
    }
    Ok(())
}

fn reject_orphaned_legal_documents(
    vendor_id: &str,
    documents: &[LibraryLegalDocument],
    referenced: &HashSet<usize>,
) -> Result<(), ServiceError> {
    if referenced.len() == documents.len() {
        return Ok(());
    }
    if let Some((_, document)) = documents
        .iter()
        .enumerate()
        .find(|(index, _)| !referenced.contains(index))
    {
        return Err(library_error(format!(
            "vendor `{vendor_id}` contains unreferenced legal document `{}`",
            document.legal_document_id
        )));
    }
    Ok(())
}
