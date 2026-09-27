//! Shared Minisign updater signature verification and authoritative contract for RenderPilot.
//!
//! Provides the single source of truth for Minisign updater signature validation
//! used by both the desktop runtime (portable updater) and release verification tooling.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use minisign_verify::{PublicKey, Signature};
use std::path::Path;

include!(concat!(env!("OUT_DIR"), "/updater_public_key.rs"));

/// Verifies that the artifact file matches the given signature file using the authoritative public key.
pub fn verify_artifact_file(artifact_path: &Path, signature_path: &Path) -> Result<(), String> {
    let signature_text = std::fs::read_to_string(signature_path)
        .map_err(|error| format!("read {}: {error}", signature_path.display()))?;
    let artifact_bytes = std::fs::read(artifact_path)
        .map_err(|error| format!("read {}: {error}", artifact_path.display()))?;
    verify_default_signature_bytes(&artifact_bytes, signature_text.trim())
}

/// Verifies artifact bytes against outer-base64 Minisign signature using the compile-time authoritative public key.
pub fn verify_default_signature_bytes(bytes: &[u8], encoded_signature: &str) -> Result<(), String> {
    verify_signature_bytes(bytes, encoded_signature, UPDATER_PUBLIC_KEY)
}

/// Verifies artifact bytes against an outer-base64 Minisign signature using a specified outer-base64 public key.
pub(crate) fn verify_signature_bytes(
    bytes: &[u8],
    encoded_signature: &str,
    encoded_public_key: &str,
) -> Result<(), String> {
    let public_key_str = decode_outer_base64(encoded_public_key, "public key")?;
    let signature_str = decode_outer_base64(encoded_signature, "signature")?;
    let public_key = PublicKey::decode(&public_key_str)
        .map_err(|error| format!("decode public key: {error}"))?;
    let signature =
        Signature::decode(&signature_str).map_err(|error| format!("decode signature: {error}"))?;
    public_key
        .verify(bytes, &signature, true)
        .map_err(|error| format!("verify artifact: {error}"))
}

/// Decodes an outer base64-encoded string into UTF-8 text.
fn decode_outer_base64(value: &str, label: &str) -> Result<String, String> {
    let decoded = STANDARD
        .decode(value.trim())
        .map_err(|error| format!("decode {label} base64: {error}"))?;
    String::from_utf8(decoded).map_err(|error| format!("decode {label} UTF-8: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_authoritative_public_key() {
        let decoded = decode_outer_base64(UPDATER_PUBLIC_KEY, "public key").unwrap();
        assert!(decoded.starts_with("untrusted comment: minisign public key:"));
        let pk = PublicKey::decode(&decoded);
        assert!(
            pk.is_ok(),
            "authoritative public key must decode as valid Minisign public key"
        );
    }

    #[test]
    fn test_updater_public_key_policy_invariants() {
        const PLATFORM_CONFIGS: &[&str] = &[
            "tauri.windows.conf.json",
            "tauri.linux.conf.json",
            "tauri.macos.conf.json",
            "tauri.android.conf.json",
            "tauri.ios.conf.json",
        ];

        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let tauri_src_dir = manifest_dir
            .join("../../apps/desktop/src-tauri")
            .canonicalize()
            .expect("apps/desktop/src-tauri must exist");
        let tauri_conf_path = tauri_src_dir.join("tauri.conf.json");
        let content = std::fs::read_to_string(&tauri_conf_path).expect("read tauri.conf.json");
        let json: serde_json::Value =
            serde_json::from_str(&content).expect("parse tauri.conf.json");
        let base_key = json
            .pointer("/plugins/updater/pubkey")
            .and_then(serde_json::Value::as_str)
            .expect("plugins.updater.pubkey must be set in tauri.conf.json");
        assert_eq!(
            base_key, UPDATER_PUBLIC_KEY,
            "UPDATER_PUBLIC_KEY must match tauri.conf.json exactly"
        );

        for config_name in PLATFORM_CONFIGS {
            let path = tauri_src_dir.join(config_name);
            if path.exists() {
                let c = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
                let p_json: serde_json::Value = serde_json::from_str(&c)
                    .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));
                if let Some(val) = p_json.pointer("/plugins/updater/pubkey") {
                    assert_eq!(
                        val.as_str(),
                        Some(base_key),
                        "Platform config {} must not override plugins.updater.pubkey",
                        path.display()
                    );
                }
            }
        }
    }

    #[test]
    fn test_crypto_verification_positive_and_negative() {
        // Genuine test fixture generated using standard Ed25519 / Minisign framing.
        let test_pub_b64 = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDAxMDIwMzA0MDUwNjA3MDgKUldRQkFnTUVCUVlIQ0xkTTZwMTVleGQ4QTJFdTNtSTJtdXlGTzlDNzB0WWpMQzR1akcxNjVFOEEK";
        let test_sig_b64 = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIG1pbmlzaWduIHNlY3JldCBrZXkKUldRQkFnTUVCUVlIQ0ZZbWlHNFRoNTlDMVgwTUFJTlpXQjhiWFZPVDFaZk5qZ2JFcFFPcHM1S2lXOWY2a1VoTkI1SVMzeGZYT1JKTzJKcUl0aElOdlBnTldzMUh6ZEluWEE4PQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxMjM0NTY3ODkwCWZpbGU6dGVzdAowN29BdUlVaW05TFNnNzd0K1hqUUExelREQlQyZUQ3VnlDR2lmNjMwbmZqeEpKeWpQTFhkeWU5MEZZYTN0UXdYTVBNY1Q0d0orR25XSmdId3Y0dm1DQT09Cg==";
        let test_data = b"hello world";

        // 1. Positive verification: valid artifact + valid signature + matching key => Ok
        let positive_res = verify_signature_bytes(test_data, test_sig_b64, test_pub_b64);
        assert!(
            positive_res.is_ok(),
            "valid artifact and signature must verify successfully"
        );

        // 2. Modified artifact => Err
        let tampered_data = b"hello world!";
        let tampered_data_res = verify_signature_bytes(tampered_data, test_sig_b64, test_pub_b64);
        assert!(
            tampered_data_res.is_err(),
            "modified artifact bytes must fail verification"
        );

        // 3. Modified signature => Err
        let corrupted_sig_b64 = "dW50cnVzdGVkIGNvbW1lbnQ6IGJvZ3VzCg==";
        let corrupted_sig_res = verify_signature_bytes(test_data, corrupted_sig_b64, test_pub_b64);
        assert!(
            corrupted_sig_res.is_err(),
            "corrupted signature must fail verification"
        );

        // 4. Wrong public key => Err (use production key against test fixture signature)
        let wrong_key_res = verify_signature_bytes(test_data, test_sig_b64, UPDATER_PUBLIC_KEY);
        assert!(
            wrong_key_res.is_err(),
            "verification with wrong public key must fail"
        );
    }
}
