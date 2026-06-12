//! Manifest and archive validation helpers.

use std::collections::HashSet;

use sha2::{Digest, Sha256};

use crate::ServiceError;

use super::library_error;
use super::types::LibraryManifestEntry;

pub(super) fn validate_manifest(
    manifest: &super::types::LibraryManifest,
) -> Result<(), ServiceError> {
    const SUPPORTED_SCHEMA_VERSION: u32 = 1;

    if manifest.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(library_error(format!(
            "unsupported manifest schema version: expected {SUPPORTED_SCHEMA_VERSION}, got {}",
            manifest.schema_version
        )));
    }

    let mut entry_ids = HashSet::with_capacity(manifest.entries.len());

    for entry in &manifest.entries {
        validate_entry(entry)?;

        if !entry_ids.insert(entry.entry_id.as_str()) {
            return Err(library_error(format!(
                "duplicate library entry id `{}` in manifest",
                entry.entry_id
            )));
        }
    }

    Ok(())
}

pub(super) fn validate_entry(entry: &LibraryManifestEntry) -> Result<(), ServiceError> {
    ensure_not_blank("entry id", &entry.entry_id)?;
    ensure_not_blank("library id", &entry.library.id)?;
    ensure_not_blank("library file name", &entry.library.file_name)?;
    ensure_not_blank("version", &entry.version.value)?;
    ensure_not_blank("version sort key", &entry.version.sort_key)?;
    ensure_not_blank("build type", &entry.build.build_type)?;
    ensure_valid_build_type(&entry.build.build_type)?;

    if entry.files.zst.size_bytes == 0 {
        return Err(library_error(format!(
            "ZST size for `{}` must be greater than zero",
            entry.entry_id
        )));
    }

    if entry.files.zst.size_bytes > super::compression::MAX_ARCHIVE_SIZE {
        return Err(library_error(format!(
            "ZST size for `{}` exceeds maximum allowed ({})",
            entry.entry_id,
            super::compression::MAX_ARCHIVE_SIZE
        )));
    }

    super::compression::validate_size_constraints(&entry.entry_id, entry.files.dll.size_bytes)?;

    if !is_sha256_hex(&entry.files.dll.hashes.sha256) {
        return Err(library_error(format!(
            "invalid DLL SHA-256 checksum for `{}`",
            entry.entry_id
        )));
    }

    let parsed_url =
        super::http::parse_https_url(&entry.files.zst.download_url, "manifest validation")?;
    if parsed_url.host_str() != Some(super::manifest::LIBS_HOST) {
        return Err(library_error(format!(
            "invalid download URL for `{}`: host must be {}",
            entry.entry_id,
            super::manifest::LIBS_HOST
        )));
    }

    Ok(())
}

/// Validates that the downloaded archive payload has the exact size declared
/// in the manifest.
///
/// `download_exact_bytes` already enforces this for fresh downloads; keeping
/// the check here as well guards any future caller that obtains the payload
/// some other way.
pub(super) fn validate_compressed_size(
    entry: &LibraryManifestEntry,
    payload: &[u8],
) -> Result<(), ServiceError> {
    let expected_size = usize::try_from(entry.files.zst.size_bytes).map_err(|_| {
        library_error(format!(
            "archive size for `{}` is too large for this platform",
            entry.entry_id
        ))
    })?;

    if payload.len() != expected_size {
        return Err(library_error(format!(
            "downloaded archive size mismatch for `{}`: expected {expected_size} bytes, got {} bytes",
            entry.entry_id,
            payload.len()
        )));
    }

    Ok(())
}

pub(super) fn validate_dll_hash(
    entry_id: &str,
    expected_sha256: &str,
    dll_bytes: &[u8],
) -> Result<(), ServiceError> {
    let actual_sha256 = hex::encode(Sha256::digest(dll_bytes));

    // Both sides are lowercase: the manifest hash is lowercased during serde
    // deserialization and hex::encode produces lowercase output.
    if actual_sha256 != expected_sha256 {
        return Err(library_error(format!(
            "DLL hash mismatch for `{entry_id}`: expected {expected_sha256}, got {actual_sha256}"
        )));
    }

    Ok(())
}

fn ensure_not_blank(field_name: &str, value: &str) -> Result<(), ServiceError> {
    if value.trim().is_empty() {
        return Err(library_error(format!(
            "manifest field `{field_name}` must not be empty"
        )));
    }

    Ok(())
}

fn ensure_valid_build_type(build_type: &str) -> Result<(), ServiceError> {
    const VALID_BUILD_TYPES: &[&str] = &["stable", "beta", "debug"];

    if VALID_BUILD_TYPES.contains(&build_type) {
        return Ok(());
    }

    Err(library_error(format!(
        "invalid build type `{build_type}`: expected one of stable, beta, debug"
    )))
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|character| character.is_ascii_hexdigit())
}
