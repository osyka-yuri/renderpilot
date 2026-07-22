use sha2::{Digest, Sha256};

use crate::ServiceError;

use super::super::library_error;
use super::super::types::LibraryArtifactRecord;
use super::fields::{ensure_dll_name, ensure_id, ensure_numeric_version, ensure_sha256};

pub(super) fn validate_artifact(artifact: &LibraryArtifactRecord) -> Result<(), ServiceError> {
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
    super::super::compression::validate_size_constraints(
        &artifact.artifact_id,
        artifact.dll.size_bytes,
    )?;
    ensure_sha256("artifact transport sha256", &artifact.transport.sha256)?;
    if artifact.transport.compression != "zstd" {
        return Err(library_error(format!(
            "unsupported compression for `{}`: {}",
            artifact.artifact_id, artifact.transport.compression
        )));
    }
    if artifact.transport.size_bytes == 0
        || artifact.transport.size_bytes > super::super::compression::MAX_ARCHIVE_SIZE
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

pub(in crate::libraries) fn validate_transport(
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

pub(in crate::libraries) fn validate_dll_hash(
    artifact: &LibraryArtifactRecord,
    dll_bytes: &[u8],
) -> Result<(), ServiceError> {
    validate_hash(
        &format!("DLL `{}`", artifact.artifact_id),
        &artifact.dll.sha256,
        dll_bytes,
    )
}

pub(in crate::libraries) fn validate_exact_document(
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

fn validate_hash(label: &str, expected: &str, bytes: &[u8]) -> Result<(), ServiceError> {
    let actual = hex::encode(Sha256::digest(bytes));
    if actual != expected {
        return Err(library_error(format!(
            "{label} hash mismatch: expected {expected}, got {actual}"
        )));
    }
    Ok(())
}
