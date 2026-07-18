use std::io::{Cursor, Read};

use crate::ServiceError;
use crate::addons::reshade::fetch::sha256_hex;
use crate::net::{ProgressObserver, download_with_validators_and_final_url};

use super::super::errors;
use super::super::types::{LumaExternalRequirement, ManagedArchiveSource, ManagedInstallMapEntry};
use super::model::{PreparedDgVoodoo, PreparedDgVoodooFile};
use super::plan::{config_sections, managed_config_default};

/// dgVoodoo archives are currently below 10 MiB. Allow a little headroom over
/// the manifest's exact-size check without making accidental large downloads
/// cheap to trigger.
const MAX_DGVOODOO_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;

/// Downloads and verifies the managed dgVoodoo archive declared by `requirement`.
pub(crate) async fn fetch(
    requirement: &LumaExternalRequirement,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PreparedDgVoodoo, ServiceError> {
    let LumaExternalRequirement::Dgvoodoo2 {
        version,
        source,
        install_map,
        config_file,
        config,
        ..
    } = requirement;

    let label = format!("dgVoodoo2 {version}");
    let max_bytes = source.size.min(MAX_DGVOODOO_ARCHIVE_BYTES);
    let (archive, validators, _final_url) =
        download_with_validators_and_final_url(&source.url, max_bytes, &label, progress).await?;
    verify_archive_identity(source, &archive)?;

    let mut zip = zip::ZipArchive::new(Cursor::new(archive.as_slice()))
        .map_err(|error| errors::failed(format!("dgVoodoo archive is not a valid zip: {error}")))?;

    let mut files = Vec::with_capacity(install_map.len());
    for entry in install_map {
        files.push(read_mapped_file(&mut zip, entry)?);
    }
    let config_default = managed_config_default(config);

    Ok(PreparedDgVoodoo {
        version: version.clone(),
        files,
        config_file: config_file.clone(),
        config_default,
        config_sections: config_sections(config),
        source_url: source.url.clone(),
        source_etag: validators.cache_validator(),
        source_last_modified: validators.last_modified,
        archive_digest: source.sha256.clone(),
    })
}

pub(super) fn verify_archive_identity(
    source: &ManagedArchiveSource,
    bytes: &[u8],
) -> Result<(), ServiceError> {
    if bytes.len() as u64 != source.size {
        return Err(errors::failed(format!(
            "dgVoodoo archive size mismatch: expected {} bytes, got {} bytes",
            source.size,
            bytes.len()
        )));
    }
    let digest = sha256_hex(bytes);
    if digest != source.sha256 {
        return Err(errors::failed(format!(
            "dgVoodoo archive SHA-256 mismatch: expected {}, got {digest}",
            source.sha256
        )));
    }
    Ok(())
}

pub(super) fn read_mapped_file<R: Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    expected: &ManagedInstallMapEntry,
) -> Result<PreparedDgVoodooFile, ServiceError> {
    let bytes = read_exact_entry(zip, &expected.source, expected.size)?;
    let digest = sha256_hex(&bytes);
    if digest != expected.sha256 {
        return Err(errors::failed(format!(
            "dgVoodoo archive entry `{}` SHA-256 mismatch: expected {}, got {digest}",
            expected.source, expected.sha256
        )));
    }
    Ok(PreparedDgVoodooFile {
        dest: expected.dest.clone(),
        bytes,
    })
}

fn read_exact_entry<R: Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    path: &str,
    expected_size: u64,
) -> Result<Vec<u8>, ServiceError> {
    let mut entry = zip.by_name(path).map_err(|error| {
        errors::failed(format!("dgVoodoo archive is missing `{path}`: {error}"))
    })?;
    if entry.is_dir() || entry.enclosed_name().is_none() {
        return Err(errors::failed(format!(
            "dgVoodoo archive entry `{path}` is not a safe file"
        )));
    }
    let capacity = usize::try_from(expected_size).unwrap_or(0);
    let mut bytes = Vec::with_capacity(capacity);
    entry
        .by_ref()
        .take(expected_size.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| errors::failed(format!("failed to extract `{path}`: {error}")))?;
    if bytes.len() as u64 != expected_size {
        return Err(errors::failed(format!(
            "dgVoodoo archive entry `{path}` size mismatch: expected {expected_size} bytes, got {} bytes",
            bytes.len()
        )));
    }
    Ok(bytes)
}
