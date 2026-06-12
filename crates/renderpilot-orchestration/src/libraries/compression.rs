use std::io::{Cursor, Read};

use crate::ServiceError;

use super::library_error;
use super::types::LibraryManifestEntry;

/// Hard upper bound for any single decompressed DLL (500 MiB).
/// Prevents runaway allocation from a crafted or corrupted manifest.
pub(crate) const MAX_DLL_SIZE: u64 = 500 * 1024 * 1024;

/// Hard upper bound for a compressed archive (.zst).
/// A zstd archive cannot be meaningfully larger than the decompressed output it
/// contains, so we reuse the same 500 MiB ceiling for `files.zst.size_bytes`.
pub(crate) const MAX_ARCHIVE_SIZE: u64 = MAX_DLL_SIZE;

pub(super) fn decompress_library(
    entry: &LibraryManifestEntry,
    payload: &[u8],
) -> Result<Vec<u8>, ServiceError> {
    let entry_id = &entry.entry_id;
    let expected_size = entry.files.dll.size_bytes;

    validate_size_constraints(entry_id, expected_size)?;

    // MAX_DLL_SIZE (500 MiB) fits in usize on any supported platform
    // (32-bit usize max ≈ 4 GiB).
    let capacity = expected_size as usize;

    let decoder =
        zstd::stream::Decoder::new(Cursor::new(payload)).map_err(|e| decode_error(entry_id, e))?;

    let mut output = Vec::with_capacity(capacity);

    // Bomb protection: decompressor must never output more than expected + 1.
    // expected_size <= MAX_DLL_SIZE so this never wraps.
    decoder
        .take(expected_size + 1)
        .read_to_end(&mut output)
        .map_err(|e| decode_error(entry_id, e))?;

    ensure_exact_size(entry_id, expected_size, output.len())?;

    Ok(output)
}

/// Validates that a declared DLL size is non-zero and within [`MAX_DLL_SIZE`].
///
/// Shared by manifest validation and decompression so both report identical
/// errors for out-of-range sizes.
pub(super) fn validate_size_constraints(entry_id: &str, size: u64) -> Result<(), ServiceError> {
    if size == 0 {
        return Err(library_error(format!(
            "DLL size for `{entry_id}` must be greater than zero"
        )));
    }

    if size > MAX_DLL_SIZE {
        return Err(library_error(format!(
            "DLL size for `{entry_id}` exceeds maximum allowed ({MAX_DLL_SIZE})"
        )));
    }

    Ok(())
}

fn ensure_exact_size(entry_id: &str, expected: u64, actual: usize) -> Result<(), ServiceError> {
    if expected != actual as u64 {
        return Err(library_error(format!(
            "decompressed size mismatch for `{entry_id}`: expected {expected} bytes, got {actual} bytes"
        )));
    }

    Ok(())
}

fn decode_error(entry_id: &str, err: impl std::fmt::Display) -> ServiceError {
    library_error(format!("zstd decode failed for `{entry_id}`: {err}"))
}
