use sha2::{Digest, Sha256};

use super::error::{PortableRuntimeError, Result};

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    super::random::hex(&hasher.finalize())
}

pub fn sha256_file(path: &std::path::Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(PortableRuntimeError::from)?;
    Ok(sha256_hex(&bytes))
}

/// All portable payload signatures use the effective build-time updater key.
pub fn verify(bytes: &[u8], signature: &str) -> Result<()> {
    crate::updater_signature::verify(bytes, signature.trim())
        .map_err(|detail| PortableRuntimeError::new("portable_signature_invalid", detail))
}
