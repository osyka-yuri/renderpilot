use std::collections::{HashMap, HashSet};

use renderpilot_domain::{PackageVersion, ReleaseChannel, RuntimeCompatibility, openvr};

use crate::ServiceError;

use super::super::library_error;
use super::super::resolved::PackageReferences;
use super::super::revision::package_revision_sha256;
use super::super::types::{LibraryArtifactRecord, LibraryPackage, LibraryProvenance};
use super::fields::{ensure_dll_name, ensure_id, ensure_not_blank, ensure_sha256};
use super::legal::LegalDocumentLookup;

pub(super) type ArtifactLookup<'a> = HashMap<&'a str, (usize, &'a LibraryArtifactRecord)>;

pub(super) fn validate_package(
    package: &LibraryPackage,
    artifacts: &ArtifactLookup<'_>,
    legal_documents: &LegalDocumentLookup<'_>,
) -> Result<PackageReferences, ServiceError> {
    validate_identity_and_release(package)?;
    validate_target(package)?;
    validate_provenance(package)?;
    validate_member_shape(package)?;

    Ok(PackageReferences {
        members: resolve_members(package, artifacts)?,
        legal_documents: resolve_legal_documents(package, legal_documents)?,
    })
}

fn validate_identity_and_release(package: &LibraryPackage) -> Result<(), ServiceError> {
    ensure_id("package id", &package.package_id)?;
    ensure_sha256("package revision", &package.revision_sha256)?;
    let actual_revision = package_revision_sha256(package)?;
    if package.revision_sha256 != actual_revision {
        return Err(library_error(format!(
            "package revision mismatch for `{}`: expected {}, got {actual_revision}",
            package.package_id, package.revision_sha256
        )));
    }

    ensure_id("package technology", &package.technology)?;
    ensure_id("package variant", &package.variant)?;
    ensure_not_blank("package display name", &package.display_name)?;
    if let Some(label) = &package.release.label {
        ensure_not_blank("package release label", label)?;
    }

    Ok(())
}

fn validate_target(package: &LibraryPackage) -> Result<(), ServiceError> {
    if package.target.os != "windows" {
        return Err(library_error(format!(
            "unsupported target OS for package `{}`: {}",
            package.package_id, package.target.os
        )));
    }

    match (&package.technology[..], &package.target.compatibility) {
        ("d3d12_agility", Some(RuntimeCompatibility::D3d12Sdk { version }))
            if *version > 0
                && package
                    .release
                    .version
                    .numeric_core()
                    .segments()
                    .get(1)
                    .copied()
                    == Some(u64::from(*version)) => {}
        ("d3d12_agility", _) => {
            return Err(library_error(format!(
                "package `{}` D3D12 compatibility does not match its release SDK line",
                package.package_id
            )));
        }
        (_, Some(_)) => {
            return Err(library_error(format!(
                "package `{}` declares compatibility for a non-D3D12 technology",
                package.package_id
            )));
        }
        (_, None) => {}
    }

    Ok(())
}

fn validate_provenance(package: &LibraryPackage) -> Result<(), ServiceError> {
    match &package.provenance {
        Some(LibraryProvenance::Nuget {
            package_id,
            version,
            package_sha512,
        }) => validate_nuget_provenance(package, package_id, version, package_sha512)?,
        Some(LibraryProvenance::GithubRelease {
            repository,
            tag,
            commit_sha,
        }) => validate_github_release_provenance(&package.package_id, repository, tag, commit_sha)?,
        None => {}
    }

    if let Some(expected_package_id) = expected_microsoft_package_id(&package.technology) {
        let valid = matches!(
            &package.provenance,
            Some(LibraryProvenance::Nuget { package_id, .. })
                if package_id.eq_ignore_ascii_case(expected_package_id)
        );
        if !valid {
            return Err(library_error(format!(
                "package `{}` Microsoft runtime provenance is missing or inconsistent",
                package.package_id
            )));
        }
    }

    if package.technology == "openvr" {
        let valid_provenance = matches!(
            &package.provenance,
            Some(LibraryProvenance::GithubRelease { repository, .. })
                if repository == openvr::UPSTREAM_REPOSITORY
        );
        if !valid_provenance || package.members.len() != 1 {
            return Err(library_error(format!(
                "package `{}` is not a canonical OpenVR SDK package",
                package.package_id
            )));
        }
    }

    Ok(())
}

fn validate_nuget_provenance(
    package: &LibraryPackage,
    package_id: &str,
    version: &PackageVersion,
    package_sha512: &str,
) -> Result<(), ServiceError> {
    ensure_not_blank("NuGet package id", package_id)?;
    if version != &package.release.version {
        return Err(library_error(format!(
            "package `{}` NuGet version does not match its release",
            package.package_id
        )));
    }
    if expected_microsoft_package_id(&package.technology).is_some() {
        let expected_channel = if version.is_prerelease() {
            ReleaseChannel::Preview
        } else {
            ReleaseChannel::Stable
        };
        if package.release.channel != expected_channel {
            return Err(library_error(format!(
                "package `{}` Microsoft NuGet channel does not match its version",
                package.package_id
            )));
        }
    }
    if package_sha512.len() != 88
        || !package_sha512.ends_with("==")
        || !package_sha512
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
    {
        return Err(library_error(format!(
            "package `{}` has an invalid NuGet SHA-512",
            package.package_id
        )));
    }
    Ok(())
}

fn validate_github_release_provenance(
    package_id: &str,
    repository: &str,
    tag: &str,
    commit_sha: &str,
) -> Result<(), ServiceError> {
    let mut repository_parts = repository.split('/');
    let owner = repository_parts.next().unwrap_or_default();
    let name = repository_parts.next().unwrap_or_default();
    let valid_repository_part = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    };
    if !valid_repository_part(owner)
        || !valid_repository_part(name)
        || repository_parts.next().is_some()
        || tag.trim().is_empty()
        || commit_sha.len() != 40
        || !commit_sha
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(library_error(format!(
            "package `{package_id}` has invalid GitHub release provenance"
        )));
    }
    Ok(())
}

fn expected_microsoft_package_id(technology: &str) -> Option<&'static str> {
    match technology {
        "d3d12_agility" => Some("Microsoft.Direct3D.D3D12"),
        "direct_storage" => Some("Microsoft.Direct3D.DirectStorage"),
        "microsoft_dxc" => Some("Microsoft.Direct3D.DXC"),
        _ => None,
    }
}

fn validate_member_shape(package: &LibraryPackage) -> Result<(), ServiceError> {
    if package.members.is_empty() {
        return Err(library_error(format!(
            "package `{}` has no members",
            package.package_id
        )));
    }

    if package
        .members
        .first()
        .is_none_or(|member| member.role != "primary")
        || package
            .members
            .iter()
            .filter(|member| member.role == "primary")
            .count()
            != 1
    {
        return Err(library_error(format!(
            "package `{}` must have exactly one primary member, listed first",
            package.package_id
        )));
    }

    Ok(())
}

fn resolve_legal_documents(
    package: &LibraryPackage,
    legal_documents: &LegalDocumentLookup<'_>,
) -> Result<Vec<usize>, ServiceError> {
    let mut resolved = Vec::with_capacity(package.legal_document_ids.len());
    let mut previous_id: Option<&str> = None;

    for legal_document_id in &package.legal_document_ids {
        ensure_id("package legal document id", legal_document_id)?;
        if previous_id.is_some_and(|previous| previous >= legal_document_id.as_str()) {
            return Err(library_error(format!(
                "package `{}` legal document ids must be sorted and unique",
                package.package_id
            )));
        }
        previous_id = Some(legal_document_id);

        let document_index = legal_documents
            .get(legal_document_id.as_str())
            .map(|(index, _)| *index)
            .ok_or_else(|| {
                library_error(format!(
                    "package `{}` references missing legal document `{legal_document_id}`",
                    package.package_id
                ))
            })?;
        resolved.push(document_index);
    }

    Ok(resolved)
}

fn resolve_members(
    package: &LibraryPackage,
    artifacts: &ArtifactLookup<'_>,
) -> Result<Vec<usize>, ServiceError> {
    let mut member_ids = HashSet::new();
    let mut install_targets = HashSet::new();
    let mut resolved = Vec::with_capacity(package.members.len());

    for member in &package.members {
        ensure_dll_name("package install target", &member.install_as)?;
        ensure_id("package member role", &member.role)?;
        if !member_ids.insert(member.artifact_id.as_str()) {
            return Err(library_error(format!(
                "package `{}` references artifact `{}` more than once",
                package.package_id, member.artifact_id
            )));
        }
        if !install_targets.insert(member.install_as.to_ascii_lowercase()) {
            return Err(library_error(format!(
                "package `{}` has duplicate install target `{}`",
                package.package_id, member.install_as
            )));
        }

        let (artifact_index, artifact) = artifacts
            .get(member.artifact_id.as_str())
            .copied()
            .ok_or_else(|| {
                library_error(format!(
                    "package `{}` references missing artifact `{}`",
                    package.package_id, member.artifact_id
                ))
            })?;
        if artifact.architecture != package.target.architecture {
            return Err(library_error(format!(
                "package `{}` mixes target architectures",
                package.package_id
            )));
        }
        if package.technology == "openvr"
            && (member.install_as != openvr::DLL_NAME
                || artifact.file_name != openvr::DLL_NAME
                || artifact.pe_named_exports.is_none())
        {
            return Err(library_error(format!(
                "package `{}` has an invalid OpenVR member contract",
                package.package_id
            )));
        }

        resolved.push(artifact_index);
    }

    Ok(resolved)
}
