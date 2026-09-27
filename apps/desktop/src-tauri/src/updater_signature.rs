//! Shared verification of Tauri's outer-base64 Minisign updater signatures.
//!
//! Delegates to the single authoritative `renderpilot-updater-signature` crate.

pub(crate) fn verify(bytes: &[u8], encoded_signature: &str) -> Result<(), String> {
    renderpilot_updater_signature::verify_default_signature_bytes(bytes, encoded_signature)
}
