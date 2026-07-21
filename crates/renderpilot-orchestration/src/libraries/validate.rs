//! Structural, referential, and content-integrity validation for catalog v1.

use std::collections::{HashMap, HashSet};

use renderpilot_domain::{ArtifactId, RuntimeCompatibility, Sha256Hash, Version, openvr};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::ServiceError;

use super::library_error;
use super::types::{
    LibraryArtifactRecord, LibraryCatalog, LibraryIndex, LibraryPackage, LibraryPackageMember,
    LibraryProvenance, LibraryReleaseChannel, LibraryTarget, LibraryVendorReference,
    LibraryVendorSnapshot,
};

pub(super) const SUPPORTED_SCHEMA_VERSION: u32 = 1;
pub(super) const MAX_INDEX_SIZE: u64 = 256 * 1024;
pub(super) const MAX_VENDOR_SNAPSHOT_SIZE: u64 = 2 * 1024 * 1024;

struct VendorPolicy {
    id: &'static str,
    supported: bool,
    required_in_v1_cache: bool,
}

// Valve is supported by this client, but remains optional so an older
// last-known-good cache can still be used offline.
const VENDOR_POLICIES: &[VendorPolicy] = &[
    VendorPolicy {
        id: "amd",
        supported: true,
        required_in_v1_cache: true,
    },
    VendorPolicy {
        id: "intel",
        supported: true,
        required_in_v1_cache: true,
    },
    VendorPolicy {
        id: "microsoft",
        supported: true,
        required_in_v1_cache: true,
    },
    VendorPolicy {
        id: "nvidia",
        supported: true,
        required_in_v1_cache: true,
    },
    VendorPolicy {
        id: "valve",
        supported: true,
        required_in_v1_cache: false,
    },
];
const PACKAGE_REVISION_SCHEMA_VERSION: u32 = 1;

pub(super) fn is_supported_vendor(vendor_id: &str) -> bool {
    VENDOR_POLICIES
        .iter()
        .any(|policy| policy.id == vendor_id && policy.supported)
}

fn required_vendor_ids() -> impl Iterator<Item = &'static str> {
    VENDOR_POLICIES
        .iter()
        .filter(|policy| policy.supported && policy.required_in_v1_cache)
        .map(|policy| policy.id)
}

pub(super) fn validate_index(index: &LibraryIndex) -> Result<(), ServiceError> {
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

pub(super) fn validate_vendor_snapshot_envelope(
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
    pub(super) artifact_id: ArtifactId,
}

pub(super) fn validate_catalog(catalog: &LibraryCatalog) -> Result<CatalogIndex, ServiceError> {
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
        let package_members = validate_vendor_contents(
            &vendor.vendor.id,
            &vendor.vendor.display_name,
            &vendor.generated_at,
            &vendor.artifacts,
            &vendor.packages,
        )?;
        for (package_index, (package, members)) in
            vendor.packages.iter().zip(package_members).enumerate()
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
            artifact_ids.insert(artifact_id.clone(), resolved_index);
            packages.push(PackageIndex {
                vendor: vendor_index,
                package: package_index,
                members,
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
    artifacts: &[LibraryArtifactRecord],
    packages: &[LibraryPackage],
) -> Result<Vec<Vec<usize>>, ServiceError> {
    ensure_not_blank("vendor display name", display_name)?;
    ensure_not_blank("vendor generated_at", generated_at)?;

    let mut artifact_map = HashMap::with_capacity(artifacts.len());
    for (artifact_index, artifact) in artifacts.iter().enumerate() {
        validate_artifact(artifact)?;
        if artifact_map
            .insert(artifact.artifact_id.as_str(), (artifact_index, artifact))
            .is_some()
        {
            return Err(library_error(format!(
                "duplicate artifact `{}` in vendor `{vendor_id}`",
                artifact.artifact_id
            )));
        }
    }

    let mut package_ids = HashSet::with_capacity(packages.len());
    let mut referenced_artifacts = HashSet::with_capacity(artifacts.len());
    let mut package_members = Vec::with_capacity(packages.len());
    for package in packages {
        let members = validate_package(package, &artifact_map)?;
        if !package_ids.insert(package.package_id.as_str()) {
            return Err(library_error(format!(
                "duplicate package `{}` in vendor `{vendor_id}`",
                package.package_id
            )));
        }
        referenced_artifacts.extend(members.iter().copied());
        package_members.push(members);
    }
    if let Some((_, orphan)) = artifacts
        .iter()
        .enumerate()
        .find(|(index, _)| !referenced_artifacts.contains(index))
    {
        return Err(library_error(format!(
            "vendor `{vendor_id}` contains unreferenced artifact `{}`",
            orphan.artifact_id
        )));
    }
    Ok(package_members)
}

fn validate_artifact(artifact: &LibraryArtifactRecord) -> Result<(), ServiceError> {
    ensure_id("library id", &artifact.library_id)?;
    ensure_dll_name("artifact file name", &artifact.file_name)?;
    if let Some(version) = &artifact.file_version {
        ensure_numeric_version("artifact file version", version)?;
    }
    ensure_sha256("artifact DLL sha256", &artifact.dll.sha256)?;
    if artifact.artifact_id != format!("sha256:{}", artifact.dll.sha256) {
        return Err(library_error(format!(
            "artifact id does not match DLL digest for `{}`",
            artifact.artifact_id
        )));
    }
    super::compression::validate_size_constraints(&artifact.artifact_id, artifact.dll.size_bytes)?;
    ensure_sha256("artifact transport sha256", &artifact.transport.sha256)?;
    if artifact.transport.compression != "zstd" {
        return Err(library_error(format!(
            "unsupported compression for `{}`: {}",
            artifact.artifact_id, artifact.transport.compression
        )));
    }
    if artifact.transport.size_bytes == 0
        || artifact.transport.size_bytes > super::compression::MAX_ARCHIVE_SIZE
    {
        return Err(library_error(format!(
            "archive size for `{}` is outside the allowed range",
            artifact.artifact_id
        )));
    }
    let expected_key = format!(
        "libraries/blobs/sha256/{}.dll.zst",
        artifact.transport.sha256
    );
    if artifact.transport.object_key != expected_key {
        return Err(library_error(format!(
            "transport key is not canonical for `{}`",
            artifact.artifact_id
        )));
    }
    Ok(())
}

fn validate_package(
    package: &LibraryPackage,
    artifacts: &HashMap<&str, (usize, &LibraryArtifactRecord)>,
) -> Result<Vec<usize>, ServiceError> {
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
    ensure_numeric_version("package release version", &package.release.version)?;
    if let Some(label) = &package.release.label {
        ensure_not_blank("package release label", label)?;
    }
    if package.target.os != "windows" {
        return Err(library_error(format!(
            "unsupported target OS for package `{}`: {}",
            package.package_id, package.target.os
        )));
    }
    match (&package.technology[..], &package.target.compatibility) {
        ("d3d12_agility", Some(RuntimeCompatibility::D3d12Sdk { version }))
            if *version > 0
                && Version::parse(&package.release.version)
                    .ok()
                    .and_then(|release| release.segments().get(1).copied())
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
    if let Some(LibraryProvenance::Nuget {
        package_id,
        version,
        package_sha512,
    }) = &package.provenance
    {
        ensure_not_blank("NuGet package id", package_id)?;
        ensure_numeric_version("NuGet package version", version)?;
        if version != &package.release.version {
            return Err(library_error(format!(
                "package `{}` NuGet version does not match its release",
                package.package_id
            )));
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
    }
    if let Some(LibraryProvenance::GithubRelease {
        repository,
        tag,
        commit_sha,
    }) = &package.provenance
    {
        validate_github_release_provenance(&package.package_id, repository, tag, commit_sha)?;
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
    if package.members.is_empty() {
        return Err(library_error(format!(
            "package `{}` has no members",
            package.package_id
        )));
    }
    if package.members[0].role != "primary"
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

    let mut member_ids = HashSet::new();
    let mut install_targets = HashSet::new();
    let mut resolved_members = Vec::with_capacity(package.members.len());
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
        resolved_members.push(artifact_index);
    }
    Ok(resolved_members)
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

#[derive(Serialize)]
struct RevisionRelease<'a> {
    version: &'a str,
    channel: &'a LibraryReleaseChannel,
}

#[derive(Serialize)]
struct RevisionInput<'a> {
    schema_version: u32,
    package_id: &'a str,
    technology: &'a str,
    variant: &'a str,
    release: RevisionRelease<'a>,
    target: &'a LibraryTarget,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: &'a Option<LibraryProvenance>,
    members: &'a [LibraryPackageMember],
}

pub(super) fn package_revision_sha256(package: &LibraryPackage) -> Result<String, ServiceError> {
    let input = RevisionInput {
        schema_version: PACKAGE_REVISION_SCHEMA_VERSION,
        package_id: &package.package_id,
        technology: &package.technology,
        variant: &package.variant,
        release: RevisionRelease {
            version: &package.release.version,
            channel: &package.release.channel,
        },
        target: &package.target,
        provenance: &package.provenance,
        members: &package.members,
    };
    let value = serde_json::to_value(input)
        .map_err(|error| library_error(format!("failed to encode package revision: {error}")))?;
    let canonical = canonical_json(&value)?;
    Ok(hex::encode(Sha256::digest(canonical.as_bytes())))
}

fn canonical_json(value: &serde_json::Value) -> Result<String, ServiceError> {
    match value {
        serde_json::Value::Array(items) => Ok(format!(
            "[{}]",
            items
                .iter()
                .map(canonical_json)
                .collect::<Result<Vec<_>, _>>()?
                .join(",")
        )),
        serde_json::Value::Object(object) => {
            let mut keys: Vec<_> = object.keys().collect();
            keys.sort_unstable();
            let fields = keys
                .into_iter()
                .map(|key| {
                    let encoded_key = serde_json::to_string(key).map_err(|error| {
                        library_error(format!("failed to encode revision key: {error}"))
                    })?;
                    Ok(format!("{encoded_key}:{}", canonical_json(&object[key])?))
                })
                .collect::<Result<Vec<_>, ServiceError>>()?;
            Ok(format!("{{{}}}", fields.join(",")))
        }
        scalar => serde_json::to_string(scalar)
            .map_err(|error| library_error(format!("failed to encode package revision: {error}"))),
    }
}

pub(super) fn validate_exact_document(
    label: &str,
    expected_size: u64,
    expected_sha256: &str,
    bytes: &[u8],
) -> Result<(), ServiceError> {
    if bytes.len() as u64 != expected_size {
        return Err(library_error(format!(
            "{label} size mismatch: expected {expected_size} bytes, got {} bytes",
            bytes.len()
        )));
    }
    validate_hash(label, expected_sha256, bytes)
}

pub(super) fn validate_transport(
    artifact: &LibraryArtifactRecord,
    bytes: &[u8],
) -> Result<(), ServiceError> {
    validate_exact_document(
        &format!("archive `{}`", artifact.artifact_id),
        artifact.transport.size_bytes,
        &artifact.transport.sha256,
        bytes,
    )
}

pub(crate) fn validate_dll_hash(
    label: &str,
    expected_sha256: &str,
    dll_bytes: &[u8],
) -> Result<(), ServiceError> {
    validate_hash(&format!("DLL `{label}`"), expected_sha256, dll_bytes)
}

fn validate_hash(label: &str, expected: &str, bytes: &[u8]) -> Result<(), ServiceError> {
    let actual = hex::encode(Sha256::digest(bytes));
    if actual != expected {
        return Err(library_error(format!(
            "{label} hash mismatch: expected {expected}, got {actual}"
        )));
    }
    Ok(())
}

fn validate_schema(actual: u32, label: &str) -> Result<(), ServiceError> {
    if actual != SUPPORTED_SCHEMA_VERSION {
        return Err(library_error(format!(
            "unsupported {label} schema version: expected {SUPPORTED_SCHEMA_VERSION}, got {actual}"
        )));
    }
    Ok(())
}

fn ensure_not_blank(field: &str, value: &str) -> Result<(), ServiceError> {
    if value.trim().is_empty() {
        return Err(library_error(format!(
            "catalog field `{field}` must not be empty"
        )));
    }
    Ok(())
}

fn ensure_sha256(field: &str, value: &str) -> Result<(), ServiceError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(library_error(format!(
            "catalog field `{field}` must be a lowercase SHA-256 digest"
        )));
    }
    Ok(())
}

fn ensure_numeric_version(field: &str, value: &str) -> Result<(), ServiceError> {
    let version = Version::parse(value).map_err(|error| {
        library_error(format!(
            "catalog field `{field}` is not a dotted numeric version: {error}"
        ))
    })?;
    if version.as_str() != value {
        return Err(library_error(format!(
            "catalog field `{field}` is not in canonical form: `{value}`"
        )));
    }
    Ok(())
}

fn ensure_id(field: &str, value: &str) -> Result<(), ServiceError> {
    let mut bytes = value.bytes();
    if !bytes.next().is_some_and(|byte| byte.is_ascii_lowercase())
        || !bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
    {
        return Err(library_error(format!(
            "catalog field `{field}` is not a valid identifier: `{value}`"
        )));
    }
    Ok(())
}

fn ensure_dll_name(field: &str, value: &str) -> Result<(), ServiceError> {
    if !value.to_ascii_lowercase().ends_with(".dll")
        || value.is_empty()
        || value
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')))
    {
        return Err(library_error(format!(
            "catalog field `{field}` is not a safe DLL basename: `{value}`"
        )));
    }
    Ok(())
}
