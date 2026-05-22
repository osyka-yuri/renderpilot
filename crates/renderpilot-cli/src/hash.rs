//! SHA-256 helpers.

#[cfg(test)]
use sha2::{Digest, Sha256};

#[cfg(test)]
fn finalize_hex(hasher: Sha256) -> String {
    hex::encode(hasher.finalize())
}

#[cfg(test)]
pub(crate) fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    finalize_hex(hasher)
}
