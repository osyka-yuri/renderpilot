//! Manifest and archive validation helpers.

use std::{collections::HashSet, io::Read};

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

    if entry.files.zip.size_bytes == 0 {
        return Err(library_error(format!(
            "ZIP size for `{}` must be greater than zero",
            entry.entry_id
        )));
    }

    if entry.files.dll.size_bytes == 0 {
        return Err(library_error(format!(
            "DLL size for `{}` must be greater than zero",
            entry.entry_id
        )));
    }

    if !is_sha256_hex(&entry.files.dll.hashes.sha256) {
        return Err(library_error(format!(
            "invalid DLL SHA-256 checksum for `{}`",
            entry.entry_id
        )));
    }

    super::http::parse_https_url(&entry.files.zip.download_url, "manifest validation")?;

    Ok(())
}

pub(super) fn validate_archive_payload(
    entry: &LibraryManifestEntry,
    payload: &[u8],
) -> Result<(), ServiceError> {
    let expected_size = usize::try_from(entry.files.zip.size_bytes).map_err(|_| {
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

    validate_dll_in_archive(entry, payload)
}

fn validate_dll_in_archive(
    entry: &LibraryManifestEntry,
    payload: &[u8],
) -> Result<(), ServiceError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(payload)).map_err(|error| {
        library_error(format!(
            "invalid ZIP archive for `{}`: {error}",
            entry.entry_id
        ))
    })?;

    let dll_file_name = &entry.library.file_name;
    let mut dll_file = archive.by_name(dll_file_name).map_err(|error| {
        library_error(format!(
            "DLL `{dll_file_name}` not found in archive for `{}`: {error}",
            entry.entry_id
        ))
    })?;

    let expected_dll_size = entry.files.dll.size_bytes;
    if dll_file.size() != expected_dll_size {
        return Err(library_error(format!(
            "DLL size mismatch for `{}`: expected {expected_dll_size} bytes, got {} bytes",
            entry.entry_id,
            dll_file.size()
        )));
    }

    let dll_capacity = usize::try_from(expected_dll_size).map_err(|_| {
        library_error(format!(
            "DLL size for `{}` is too large for this platform",
            entry.entry_id
        ))
    })?;

    let mut dll_bytes = Vec::with_capacity(dll_capacity);
    dll_file.read_to_end(&mut dll_bytes).map_err(|error| {
        library_error(format!(
            "failed to read DLL `{dll_file_name}` from archive for `{}`: {error}",
            entry.entry_id
        ))
    })?;

    let actual_sha256 = hex::encode(Sha256::digest(&dll_bytes));
    let expected_sha256 = &entry.files.dll.hashes.sha256;

    if !actual_sha256.eq_ignore_ascii_case(expected_sha256) {
        return Err(library_error(format!(
            "DLL hash mismatch for `{}`: expected {expected_sha256}, got {actual_sha256}",
            entry.entry_id
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
