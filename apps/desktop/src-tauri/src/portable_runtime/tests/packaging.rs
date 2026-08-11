use super::{error_code, temp_root};
use crate::portable_runtime::{rpu::embedded_rpu, signature::sha256_hex};

fn digest_bytes(bytes: &[u8]) -> [u8; 32] {
    let text = sha256_hex(bytes);
    std::array::from_fn(|index| {
        u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).expect("hex digest byte")
    })
}

fn overlay(rpu: &[u8], signature: &[u8]) -> Vec<u8> {
    let supervisor = b"MZ stable supervisor";
    let rpu_offset = supervisor.len() as u64;
    let signature_offset = rpu_offset + rpu.len() as u64;
    let mut footer = [0_u8; 102];
    footer[..5].copy_from_slice(b"RPSX1");
    footer[5] = 1;
    footer[6..14].copy_from_slice(&rpu_offset.to_le_bytes());
    footer[14..22].copy_from_slice(&(rpu.len() as u64).to_le_bytes());
    footer[22..30].copy_from_slice(&signature_offset.to_le_bytes());
    footer[30..38].copy_from_slice(&(signature.len() as u64).to_le_bytes());
    footer[38..70].copy_from_slice(&digest_bytes(rpu));
    footer[70..102].copy_from_slice(&digest_bytes(signature));
    [supervisor.as_slice(), rpu, signature, footer.as_slice()].concat()
}

#[test]
fn rpsx1_overlay_binds_exact_public_rpu_and_signature_bytes() {
    let rpu = b"exact public rpu";
    let signature = b"untrusted comment: test\ntrusted comment: test\n";
    let raw = overlay(rpu, signature);
    let embedded = embedded_rpu(&raw).expect("parse exact RPSX1 overlay");
    assert_eq!(embedded.rpu, rpu);
    assert_eq!(
        embedded.signature,
        std::str::from_utf8(signature).expect("UTF-8 signature")
    );
}

#[test]
fn rpsx1_rejects_out_of_bounds_or_wrong_digest_overlays() {
    let rpu = b"exact public rpu";
    let signature = b"untrusted comment: test\ntrusted comment: test\n";
    let mut bounds = overlay(rpu, signature);
    let footer = bounds.len() - 102;
    bounds[footer + 6..footer + 14].copy_from_slice(&u64::MAX.to_le_bytes());
    assert_eq!(error_code(embedded_rpu(&bounds)), "portable_sfx_footer");

    let mut wrong_digest = overlay(rpu, signature);
    wrong_digest[footer + 38] ^= 0xff;
    assert_eq!(
        error_code(embedded_rpu(&wrong_digest)),
        "portable_sfx_footer"
    );
}

#[test]
fn packaging_contract_keeps_rpu_grammar_and_effective_trust_inputs_closed() {
    let rpu = include_str!("../rpu.rs");
    for required in [
        "archive.len() != 2",
        "rpu-manifest.json",
        "app/renderpilot-app.exe",
        "name.contains(\"..\")",
        "entry.compression() != zip::CompressionMethod::Stored",
        "manifest.platform != \"windows-x86_64-portable\"",
        "manifest.portable_role != \"app\"",
        "manifest.minimum_schema != MINIMUM_SCHEMA",
        "manifest.maximum_schema != MAXIMUM_SCHEMA",
        "manifest.app_sha256 != sha256_hex(&app_bytes)",
        "canonical_version(&manifest.version)?",
        "pub fn verify_rpu_expected",
        "signed RPU version did not match its expected release context",
    ] {
        assert!(rpu.contains(required), "missing RPU rejection: {required}");
    }
    let build_contract = include_str!("../../../build-support/updater_contract.rs");
    assert!(build_contract.contains("/plugins/updater/pubkey"));
    let config = include_str!("../../../build-support/tauri_config.rs");
    assert!(config.contains("json_patch::merge"));

    let root = temp_root("packaging-fixture");
    let raw = root.path().join("raw.exe");
    std::fs::write(&raw, b"raw identity fixture").expect("write exact raw fixture");
    assert_eq!(
        sha256_hex(&std::fs::read(&raw).expect("read raw fixture")),
        sha256_hex(b"raw identity fixture")
    );
}

#[test]
fn release_supervisor_uses_the_silent_windows_subsystem() {
    let source = include_str!("../../bin/portable_supervisor.rs");
    assert!(source.contains(
        "#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = \"windows\")]"
    ));
}

#[test]
fn rust_version_parser_rejects_empty_malformed_and_noncanonical_values() {
    use crate::portable_runtime::rpu::canonical_version;

    for version in ["", "1.2", "v1.2.3", "01.2.3", "1.02.3", "1.2.03"] {
        assert_eq!(
            error_code(canonical_version(version)),
            "portable_rpu_version",
            "{version:?} must not become a portable generation version"
        );
    }
    assert_eq!(
        canonical_version("1.2.3-rc.1+build.7")
            .expect("canonical SemVer")
            .to_string(),
        "1.2.3-rc.1+build.7"
    );
}
