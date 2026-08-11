//! Shared verification of Tauri's outer-base64 Minisign updater signatures.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use minisign_verify::{PublicKey, Signature};

pub(crate) fn verify(bytes: &[u8], encoded_signature: &str) -> Result<(), String> {
    let public_key =
        decode_outer_base64(crate::updater_contract::UPDATER_PUBLIC_KEY, "public key")?;
    let signature = decode_outer_base64(encoded_signature, "signature")?;
    let public_key =
        PublicKey::decode(&public_key).map_err(|error| format!("decode public key: {error}"))?;
    let signature =
        Signature::decode(&signature).map_err(|error| format!("decode signature: {error}"))?;
    public_key
        .verify(bytes, &signature, true)
        .map_err(|error| format!("verify artifact: {error}"))
}

fn decode_outer_base64(value: &str, label: &str) -> Result<String, String> {
    let decoded = STANDARD
        .decode(value)
        .map_err(|error| format!("decode {label} base64: {error}"))?;
    String::from_utf8(decoded).map_err(|error| format!("decode {label} UTF-8: {error}"))
}

#[cfg(feature = "updater-artifact-verify")]
pub(crate) fn verify_files(
    artifact: &std::path::Path,
    signature: &std::path::Path,
) -> Result<(), String> {
    let artifact_bytes =
        std::fs::read(artifact).map_err(|error| format!("read {}: {error}", artifact.display()))?;
    let signature = std::fs::read_to_string(signature)
        .map_err(|error| format!("read {}: {error}", signature.display()))?;
    verify(&artifact_bytes, signature.trim())
}
