//! Immutable receipts attached to downloaded catalog artifacts.

use renderpilot_domain::{
    CatalogLegalDocumentReceipt, CatalogPackageMemberReceipt, CatalogPackageProvenanceReceipt,
    CatalogPackageReceipt, CatalogPackageReceiptV1, CatalogPackageReceiptV2,
    CatalogProvenanceReceipt, CatalogReceiptSchemaV1, CatalogReceiptSchemaV2,
    CatalogSourceBuildToolchainReceipt, CatalogSourcePatchReceipt, CatalogSourceReceipt,
    CatalogTargetReceipt, Sha256Hash,
};

use crate::ServiceError;

use super::library_error;
use super::resolved::ResolvedPackage;
use super::types::{LibraryProvenance, LibraryTarget};

pub(super) fn package_receipt(
    resolved: &ResolvedPackage<'_>,
) -> Result<CatalogPackageReceipt, ServiceError> {
    let package = resolved.package();
    let revision_sha256 = receipt_hash(&package.revision_sha256, "package revision")?;
    let size_bytes = package_size_bytes(resolved)?;
    let legal_documents = legal_document_receipts(resolved);
    if !package.release.components.is_empty() {
        return composite_package_receipt(resolved, revision_sha256, legal_documents, size_bytes)
            .map(CatalogPackageReceipt::V2);
    }

    let primary = resolved
        .members()
        .next()
        .ok_or_else(|| library_error(format!("package `{}` has no members", package.package_id)))?;
    let primary_sha256 = receipt_hash(&primary.dll.sha256, "primary digest")?;

    Ok(CatalogPackageReceipt::V1(CatalogPackageReceiptV1 {
        schema_version: CatalogReceiptSchemaV1,
        package_id: package.package_id.clone(),
        vendor: resolved.vendor().vendor.id.clone(),
        technology: package.technology.clone(),
        variant: package.variant.clone(),
        display_name: package.display_name.clone(),
        release: package.release.to_package_release(),
        target: target_receipt(&package.target),
        provenance: v1_provenance_receipt(package.provenance.as_ref()),
        revision_sha256,
        primary_file_name: primary.file_name.clone(),
        primary_sha256,
        primary_signature: primary.signature.to_receipt(),
        legal_documents,
        size_bytes,
    }))
}

fn receipt_hash(value: &str, label: &str) -> Result<Sha256Hash, ServiceError> {
    Sha256Hash::new(value).map_err(|error| library_error(format!("invalid {label}: {error}")))
}

fn target_receipt(target: &LibraryTarget) -> CatalogTargetReceipt {
    CatalogTargetReceipt {
        os: target.os.clone(),
        architecture: target.architecture,
        compatibility: target.compatibility.clone(),
    }
}

fn package_size_bytes(resolved: &ResolvedPackage<'_>) -> Result<u64, ServiceError> {
    resolved.members().try_fold(0_u64, |total, member| {
        total.checked_add(member.dll.size_bytes).ok_or_else(|| {
            library_error(format!(
                "package `{}` member size overflows",
                resolved.package().package_id
            ))
        })
    })
}

fn legal_document_receipts(resolved: &ResolvedPackage<'_>) -> Vec<CatalogLegalDocumentReceipt> {
    resolved
        .legal_documents()
        .map(|document| CatalogLegalDocumentReceipt {
            legal_document_id: document.legal_document_id.clone(),
            kind: document.kind,
            title: document.title.clone(),
            format: document.format,
            file_name: document.file_name.clone(),
            content_url: crate::cdn::cdn_url(&document.object_key),
        })
        .collect()
}

fn v1_provenance_receipt(
    provenance: Option<&LibraryProvenance>,
) -> Option<CatalogProvenanceReceipt> {
    provenance.and_then(|provenance| match provenance {
        LibraryProvenance::Nuget {
            package_id,
            version,
            package_sha512,
        } => Some(CatalogProvenanceReceipt::Nuget {
            package_id: package_id.clone(),
            version: version.clone(),
            package_sha512: package_sha512.clone(),
        }),
        LibraryProvenance::GithubRelease {
            repository,
            tag,
            commit_sha,
        } => Some(CatalogProvenanceReceipt::GithubRelease {
            repository: repository.clone(),
            tag: tag.clone(),
            commit_sha: commit_sha.clone(),
        }),
        LibraryProvenance::SourceBuild { .. } => None,
    })
}

fn composite_package_receipt(
    resolved: &ResolvedPackage<'_>,
    revision_sha256: Sha256Hash,
    legal_documents: Vec<CatalogLegalDocumentReceipt>,
    size_bytes: u64,
) -> Result<CatalogPackageReceiptV2, ServiceError> {
    let package = resolved.package();
    let provenance = composite_provenance_receipt(package.provenance.as_ref())?;
    let members = package
        .members
        .iter()
        .zip(resolved.members())
        .map(|(member, artifact)| {
            let named_exports = artifact.pe_named_exports.clone().ok_or_else(|| {
                library_error(format!(
                    "composite package `{}` member `{}` has no verified exports",
                    package.package_id, member.install_as
                ))
            })?;
            let imports = artifact.pe_imports.clone().ok_or_else(|| {
                library_error(format!(
                    "composite package `{}` member `{}` has no verified imports",
                    package.package_id, member.install_as
                ))
            })?;
            Ok(CatalogPackageMemberReceipt {
                component: member.component.clone(),
                role: member.role.clone(),
                install_as: member.install_as.clone(),
                sha256: receipt_hash(&artifact.dll.sha256, "member digest")?,
                architecture: artifact.architecture,
                named_exports,
                imports,
                signature: artifact.signature.to_receipt(),
            })
        })
        .collect::<Result<Vec<_>, ServiceError>>()?;
    if members.is_empty() {
        return Err(library_error(format!(
            "composite package `{}` has no members",
            package.package_id
        )));
    }

    Ok(CatalogPackageReceiptV2 {
        schema_version: CatalogReceiptSchemaV2,
        package_id: package.package_id.clone(),
        vendor: resolved.vendor().vendor.id.clone(),
        technology: package.technology.clone(),
        variant: package.variant.clone(),
        display_name: package.display_name.clone(),
        release: package.release.to_package_release(),
        target: target_receipt(&package.target),
        provenance,
        revision_sha256,
        members,
        legal_documents,
        size_bytes,
    })
}

fn composite_provenance_receipt(
    provenance: Option<&LibraryProvenance>,
) -> Result<CatalogPackageProvenanceReceipt, ServiceError> {
    let Some(provenance) = provenance else {
        return Err(library_error("composite package requires provenance"));
    };
    match provenance {
        LibraryProvenance::Nuget {
            package_id,
            version,
            package_sha512,
        } => Ok(CatalogPackageProvenanceReceipt::Nuget {
            package_id: package_id.clone(),
            version: version.clone(),
            package_sha512: package_sha512.clone(),
        }),
        LibraryProvenance::GithubRelease {
            repository,
            tag,
            commit_sha,
        } => Ok(CatalogPackageProvenanceReceipt::GithubRelease {
            repository: repository.clone(),
            tag: tag.clone(),
            commit_sha: commit_sha.clone(),
        }),
        LibraryProvenance::SourceBuild {
            sources,
            build_revision,
            recipe_sha256,
            verification_policy_sha256,
            patches,
            toolchain,
        } => {
            let sources = sources
                .iter()
                .map(|(name, source)| {
                    Ok((
                        name.clone(),
                        CatalogSourceReceipt {
                            repository: source.repository.clone(),
                            version: source.version.clone(),
                            tag: source.tag.clone(),
                            tag_object_sha: source.tag_object_sha.clone(),
                            commit_sha: source.commit_sha.clone(),
                            archive_url: source.archive_url.clone(),
                            archive_sha256: receipt_hash(
                                &source.archive_sha256,
                                "source archive digest",
                            )?,
                        },
                    ))
                })
                .collect::<Result<_, ServiceError>>()?;
            let patches = patches
                .iter()
                .map(|(patch_id, patch)| {
                    Ok((
                        patch_id.clone(),
                        CatalogSourcePatchReceipt {
                            source: patch.source.clone(),
                            target: patch.target.clone(),
                            descriptor_sha256: receipt_hash(
                                &patch.descriptor_sha256,
                                "patch descriptor digest",
                            )?,
                            original_sha256: receipt_hash(
                                &patch.original_sha256,
                                "original patch digest",
                            )?,
                            patched_sha256: receipt_hash(
                                &patch.patched_sha256,
                                "patched source digest",
                            )?,
                        },
                    ))
                })
                .collect::<Result<_, ServiceError>>()?;
            Ok(CatalogPackageProvenanceReceipt::SourceBuild {
                sources,
                build_revision: *build_revision,
                recipe_sha256: receipt_hash(recipe_sha256, "recipe digest")?,
                verification_policy_sha256: receipt_hash(
                    verification_policy_sha256,
                    "verification policy digest",
                )?,
                patches,
                toolchain: CatalogSourceBuildToolchainReceipt {
                    runner_image: toolchain.runner_image.clone(),
                    compiler: toolchain.compiler.clone(),
                    linker: toolchain.linker.clone(),
                    windows_sdk: toolchain.windows_sdk.clone(),
                    cmake: toolchain.cmake.clone(),
                },
            })
        }
    }
}
