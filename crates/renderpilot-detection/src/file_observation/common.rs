//! Platform-neutral byte reading, hashing, and fail-closed error mapping.

use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};

use renderpilot_domain::Sha256Hash;
use sha2::{Digest, Sha256};

use crate::error::detection_context_error;

use super::{FileIdentityProbeResult, FileObservationResult};

const HASH_BUFFER_SIZE: usize = 256 * 1024;

pub(super) fn read_and_hash(
    file: &mut File,
    path: &Path,
) -> Result<(Vec<u8>, Sha256Hash), io::Error> {
    let mut bytes = Vec::new();
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; HASH_BUFFER_SIZE];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                hasher.update(&buffer[..read]);
                bytes.extend_from_slice(&buffer[..read]);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    let sha256 = Sha256Hash::new(hex::encode(hasher.finalize())).map_err(|message| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid SHA-256: {message}"),
        )
    })?;
    let _ = path;
    Ok((bytes, sha256))
}

pub(super) fn unavailable_or_error(path: &Path, error: io::Error) -> FileObservationResult {
    let _ = detection_context_error(format_args!("could not observe {}", path.display()), error);
    FileObservationResult::Unavailable
}

pub(super) fn unavailable_probe(path: &Path, error: io::Error) -> FileIdentityProbeResult {
    let _ = detection_context_error(
        format_args!("could not acquire observation lease for {}", path.display()),
        error,
    );
    FileIdentityProbeResult::Unavailable
}
