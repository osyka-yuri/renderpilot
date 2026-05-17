//! Manifest and archive validation helpers.

use std::{collections::HashSet, io::Read};

use crate::CliError;

use super::command_failed;
use super::types::LibraryManifestEntry;

pub(super) fn validate_manifest(manifest: &super::types::LibraryManifest) -> Result<(), CliError> {
    const SUPPORTED_SCHEMA_VERSION: u32 = 1;

    if manifest.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(command_failed(format!(
            "unsupported manifest schema version: expected {SUPPORTED_SCHEMA_VERSION}, got {}",
            manifest.schema_version
        )));
    }

    let mut entry_ids = HashSet::with_capacity(manifest.entries.len());

    for entry in &manifest.entries {
        validate_entry(entry)?;

        if !entry_ids.insert(entry.entry_id.as_str()) {
            return Err(command_failed(format!(
                "duplicate library entry id `{}` in manifest",
                entry.entry_id
            )));
        }
    }

    Ok(())
}

pub(super) fn validate_entry(entry: &LibraryManifestEntry) -> Result<(), CliError> {
    ensure_not_blank("entry id", &entry.entry_id)?;
    ensure_not_blank("library id", &entry.library.id)?;
    ensure_not_blank("library file name", &entry.library.file_name)?;
    ensure_not_blank("version", &entry.version.value)?;
    ensure_not_blank("version sort key", &entry.version.sort_key)?;
    ensure_not_blank("build type", &entry.build.build_type)?;
    ensure_valid_build_type(&entry.build.build_type)?;

    if entry.files.zip.size_bytes == 0 {
        return Err(command_failed(format!(
            "ZIP size for `{}` must be greater than zero",
            entry.entry_id
        )));
    }

    if entry.files.dll.size_bytes == 0 {
        return Err(command_failed(format!(
            "DLL size for `{}` must be greater than zero",
            entry.entry_id
        )));
    }

    if !is_md5_hex(&entry.files.dll.hashes.md5) {
        return Err(command_failed(format!(
            "invalid DLL MD5 checksum for `{}`",
            entry.entry_id
        )));
    }

    super::http::parse_https_url(&entry.files.zip.download_url, "manifest validation")?;

    Ok(())
}

pub(super) fn validate_archive_payload(
    entry: &LibraryManifestEntry,
    payload: &[u8],
) -> Result<(), CliError> {
    let expected_size = usize::try_from(entry.files.zip.size_bytes).map_err(|_| {
        command_failed(format!(
            "archive size for `{}` is too large for this platform",
            entry.entry_id
        ))
    })?;

    if payload.len() != expected_size {
        return Err(command_failed(format!(
            "downloaded archive size mismatch for `{}`: expected {expected_size} bytes, got {} bytes",
            entry.entry_id,
            payload.len()
        )));
    }

    validate_dll_in_archive(entry, payload)
}

fn validate_dll_in_archive(entry: &LibraryManifestEntry, payload: &[u8]) -> Result<(), CliError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(payload)).map_err(|error| {
        command_failed(format!(
            "invalid ZIP archive for `{}`: {error}",
            entry.entry_id
        ))
    })?;

    let dll_file_name = &entry.library.file_name;
    let mut dll_file = archive.by_name(dll_file_name).map_err(|error| {
        command_failed(format!(
            "DLL `{dll_file_name}` not found in archive for `{}`: {error}",
            entry.entry_id
        ))
    })?;

    let expected_dll_size = entry.files.dll.size_bytes;
    if dll_file.size() != expected_dll_size {
        return Err(command_failed(format!(
            "DLL size mismatch for `{}`: expected {expected_dll_size} bytes, got {} bytes",
            entry.entry_id,
            dll_file.size()
        )));
    }

    let dll_capacity = usize::try_from(expected_dll_size).map_err(|_| {
        command_failed(format!(
            "DLL size for `{}` is too large for this platform",
            entry.entry_id
        ))
    })?;

    let mut dll_bytes = Vec::with_capacity(dll_capacity);
    dll_file.read_to_end(&mut dll_bytes).map_err(|error| {
        command_failed(format!(
            "failed to read DLL `{dll_file_name}` from archive for `{}`: {error}",
            entry.entry_id
        ))
    })?;

    let actual_md5 = format!("{:x}", md5::compute(&dll_bytes));
    let expected_md5 = &entry.files.dll.hashes.md5;

    if !actual_md5.eq_ignore_ascii_case(expected_md5) {
        return Err(command_failed(format!(
            "DLL hash mismatch for `{}`: expected {expected_md5}, got {actual_md5}",
            entry.entry_id
        )));
    }

    Ok(())
}

fn ensure_not_blank(field_name: &str, value: &str) -> Result<(), CliError> {
    if value.trim().is_empty() {
        return Err(command_failed(format!(
            "manifest field `{field_name}` must not be empty"
        )));
    }

    Ok(())
}

fn ensure_valid_build_type(build_type: &str) -> Result<(), CliError> {
    const VALID_BUILD_TYPES: &[&str] = &["stable", "beta", "debug"];

    if VALID_BUILD_TYPES.contains(&build_type) {
        return Ok(());
    }

    Err(command_failed(format!(
        "invalid build type `{build_type}`: expected one of stable, beta, debug"
    )))
}

fn is_md5_hex(value: &str) -> bool {
    value.len() == 32 && value.chars().all(|character| character.is_ascii_hexdigit())
}
