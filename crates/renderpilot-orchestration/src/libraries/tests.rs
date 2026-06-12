use sha2::{Digest, Sha256};

use super::local_library::library_id_to_group_key;
use super::storage::sanitize_path_component;
use super::types::{
    BuildInfo, DllFileInfo, FilesInfo, HashesInfo, LibraryInfo, LibraryManifest,
    LibraryManifestEntry, SignatureInfo, VersionInfo, ZstFileInfo,
};
use super::validate::validate_manifest;

// ---------------------------------------------------------------------------
// compression::decompress_library tests
// ---------------------------------------------------------------------------

#[test]
fn test_decompress_library_valid() {
    let original = b"hello world, this is a test DLL payload";
    let compressed = compress(original);

    let mut entry = sample_entry("test", "1.0", "1.0", "stable");
    entry.files.dll.size_bytes = original.len() as u64;

    let result = super::compression::decompress_library(&entry, &compressed);
    assert!(
        result.is_ok(),
        "decompress_library failed: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap(), original);
}

#[test]
fn test_decompress_library_size_mismatch() {
    let original = b"payload with unexpected size after decompress";
    let compressed = compress(original);

    let mut entry = sample_entry("test", "1.0", "1.0", "stable");
    entry.files.dll.size_bytes = 9999; // wrong expected size

    let result = super::compression::decompress_library(&entry, &compressed);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("size mismatch"));
}

#[test]
fn test_decompress_library_size_zero() {
    let mut entry = sample_entry("test", "1.0", "1.0", "stable");
    entry.files.dll.size_bytes = 0;

    let result = super::compression::decompress_library(&entry, &[]);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("must be greater than zero"));
}

#[test]
fn test_decompress_library_too_large() {
    let mut entry = sample_entry("test", "1.0", "1.0", "stable");
    entry.files.dll.size_bytes = super::compression::MAX_DLL_SIZE + 1;

    let result = super::compression::decompress_library(&entry, &[]);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("exceeds maximum allowed"));
}

#[test]
fn test_decompress_library_corrupted() {
    let entry = sample_entry("zstd_bomb", "1.0", "1.0", "stable");

    let result = super::compression::decompress_library(&entry, b"not-zstd-data");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// local_library::DecompressedArtifact tests
// ---------------------------------------------------------------------------

#[test]
fn test_decompress_and_verify_returns_bytes_and_manifest_hash() {
    let original = b"decompressed DLL content";
    let compressed = compress(original);

    let mut entry = sample_entry("test", "1.0", "1.0", "stable");
    entry.files.dll.size_bytes = original.len() as u64;
    entry.files.dll.hashes.sha256 = hex::encode(Sha256::digest(original));

    let result =
        super::local_library::DecompressedArtifact::decompress_and_verify(&entry, &compressed);
    assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());

    let artifact = result.unwrap();
    assert_eq!(
        artifact.bytes, original,
        "returned bytes must match the original"
    );
    assert_eq!(
        artifact.sha256, entry.files.dll.hashes.sha256,
        "artifact hash must be the verified manifest hash"
    );
}

#[test]
fn test_decompress_and_verify_rejects_hash_mismatch() {
    let original = b"DLL content that does not match the manifest hash";
    let compressed = compress(original);

    let mut entry = sample_entry("test", "1.0", "1.0", "stable");
    entry.files.dll.size_bytes = original.len() as u64;
    // sample_entry's all-zero hash never matches real content.

    let result =
        super::local_library::DecompressedArtifact::decompress_and_verify(&entry, &compressed);
    let error = result.err().expect("hash mismatch must be rejected");
    assert!(error.to_string().contains("DLL hash mismatch"));
}

// ---------------------------------------------------------------------------
// validate::validate_compressed_size tests
// ---------------------------------------------------------------------------

#[test]
fn test_validate_compressed_size_mismatch() {
    let mut entry = sample_entry("test", "1.0", "1.0", "stable");
    entry.files.zst.size_bytes = 4;

    let result = super::validate::validate_compressed_size(&entry, b"too long");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("downloaded archive size mismatch"));
}

// ---------------------------------------------------------------------------
// validate::validate_dll_hash tests
// ---------------------------------------------------------------------------

#[test]
fn test_validate_dll_hash_match() {
    let data = b"some DLL content";
    let hash = hex::encode(sha2::Sha256::digest(data));
    assert!(super::validate::validate_dll_hash("test", &hash, data).is_ok());
}

#[test]
fn test_validate_dll_hash_mismatch() {
    let data = b"some DLL content";
    let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";
    let result = super::validate::validate_dll_hash("test", wrong_hash, data);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("DLL hash mismatch"));
}

#[test]
fn test_validate_dll_hash_uppercase_rejected() {
    let data = b"strict == comparison relies on parse-time lowercasing";
    let hash_lower = hex::encode(sha2::Sha256::digest(data));
    let hash_upper = hash_lower.to_uppercase();

    let result = super::validate::validate_dll_hash("test", &hash_upper, data);
    assert!(
        result.is_err(),
        "uppercase hash should be rejected with == comparison"
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("DLL hash mismatch"));
}

#[test]
fn test_strip_utf8_bom_removes_leading_bom_only() {
    use super::manifest::strip_utf8_bom;

    assert_eq!(strip_utf8_bom(b"\xEF\xBB\xBF{}"), b"{}");
    assert_eq!(strip_utf8_bom(b"{}"), b"{}");
    // Partial BOM prefixes and interior BOM bytes must stay untouched.
    assert_eq!(strip_utf8_bom(b"\xEF\xBB{}"), b"\xEF\xBB{}");
    assert_eq!(strip_utf8_bom(b"{\xEF\xBB\xBF}"), b"{\xEF\xBB\xBF}");
    assert_eq!(strip_utf8_bom(b""), b"");
}

#[test]
fn test_bom_prefixed_manifest_json_parses_after_strip() {
    let json = br#"{"sha256":"ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789"}"#;
    let mut bom_prefixed = b"\xEF\xBB\xBF".to_vec();
    bom_prefixed.extend_from_slice(json);

    serde_json::from_slice::<HashesInfo>(&bom_prefixed)
        .expect_err("serde_json rejects a BOM, so the strip at the boundary is load-bearing");

    let parsed: HashesInfo = serde_json::from_slice(super::manifest::strip_utf8_bom(&bom_prefixed))
        .expect("BOM-prefixed JSON should parse once the BOM is stripped");
    assert_eq!(
        parsed.sha256,
        "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
    );
}

#[test]
fn test_manifest_hash_lowercased_on_parse() {
    let parsed: HashesInfo = serde_json::from_str(
        r#"{"sha256":"ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789"}"#,
    )
    .expect("HashesInfo should parse");

    assert_eq!(
        parsed.sha256, "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
        "sha256 must be normalized to lowercase during deserialization"
    );
}

// ---------------------------------------------------------------------------
// validate::validate_entry host pinning tests
// ---------------------------------------------------------------------------

#[test]
fn test_validate_entry_allowed_host_passes() {
    let entry = sample_entry("test", "1.0", "1.0", "stable");
    assert!(super::validate::validate_entry(&entry).is_ok());
}

#[test]
fn test_validate_entry_external_host_rejected() {
    let mut entry = sample_entry("test", "1.0", "1.0", "stable");
    entry.files.zst.download_url = "https://example.com/file.dll.zst".to_string();
    let result = super::validate::validate_entry(&entry);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("host must be"));
}

#[test]
fn test_validate_entry_http_rejected() {
    let mut entry = sample_entry("test", "1.0", "1.0", "stable");
    entry.files.zst.download_url = format!("http://{}/file.dll.zst", super::manifest::LIBS_HOST);
    let result = super::validate::validate_entry(&entry);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("only HTTPS URLs are allowed"));
}

// ---------------------------------------------------------------------------
// Existing tests (unchanged)
// ---------------------------------------------------------------------------

#[test]
fn test_library_id_to_group_key() {
    assert_eq!(library_id_to_group_key("nvngx_dlss"), "dlss");
    assert_eq!(library_id_to_group_key("nvngx_dlssg"), "dlss_g");
    assert_eq!(
        library_id_to_group_key("amd_fidelityfx_dx12"),
        "fsr_31_dx12"
    );
    assert_eq!(library_id_to_group_key("libxess"), "xess");
    assert_eq!(library_id_to_group_key("unknown"), "other");
}

#[test]
fn test_validate_manifest_valid() {
    let manifest = LibraryManifest {
        schema_version: 1,
        generated_at: "2023-01-01T00:00:00Z".to_string(),
        entries: vec![sample_entry("entry1", "1.0.0", "1.0.0", "stable")],
    };
    assert!(validate_manifest(&manifest).is_ok());
}

#[test]
fn test_validate_manifest_invalid_schema() {
    let manifest = LibraryManifest {
        schema_version: 99,
        generated_at: "2023-01-01T00:00:00Z".to_string(),
        entries: vec![],
    };
    let result = validate_manifest(&manifest);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("unsupported manifest schema version"));
}

#[test]
fn test_validate_manifest_duplicate_ids() {
    let entry = sample_entry("entry1", "1.0.0", "1.0.0", "stable");
    let manifest = LibraryManifest {
        schema_version: 1,
        generated_at: "2023-01-01T00:00:00Z".to_string(),
        entries: vec![entry.clone(), entry],
    };
    let result = validate_manifest(&manifest);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("duplicate library entry id"));
}

#[test]
fn test_validate_manifest_invalid_build_type() {
    let manifest = LibraryManifest {
        schema_version: 1,
        generated_at: "2023-01-01T00:00:00Z".to_string(),
        entries: vec![sample_entry("entry1", "1.0.0", "1.0.0", "preview")],
    };
    let result = validate_manifest(&manifest);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("invalid build type"));
}

#[test]
fn test_library_id_to_group_key_coverage() {
    let known_ids = [
        "nvngx_dlss",
        "nvngx_dlssg",
        "nvngx_dlssd",
        "amd_fidelityfx_dx12",
        "amd_fidelityfx_vk",
        "amd_fidelityfx_loader_dx12",
        "amd_fidelityfx_upscaler_dx12",
        "amd_fidelityfx_framegeneration_dx12",
        "amd_fidelityfx_denoiser_dx12",
        "amd_fidelityfx_radiancecache_dx12",
        "libxess",
        "libxess_dx11",
        "libxess_fg",
        "libxell",
    ];

    for id in known_ids {
        let key = library_id_to_group_key(id);
        assert_ne!(
            key, "other",
            "known library id `{id}` should not map to 'other'"
        );
    }
}

#[test]
fn test_sanitize_path_component() {
    assert_eq!(sanitize_path_component("valid-name_1.2"), "valid-name_1.2");
    assert_eq!(sanitize_path_component("invalid/name?"), "invalid_name");
    assert_eq!(sanitize_path_component("CON"), "_CON");
    assert_eq!(sanitize_path_component("  "), "unknown");
    assert_eq!(sanitize_path_component("..."), "unknown");
}

#[test]
fn test_validate_entry_rejects_oversized_dll() {
    let mut entry = sample_entry("big", "1.0", "1.0", "stable");
    entry.files.dll.size_bytes = super::compression::MAX_DLL_SIZE + 1;
    let result = super::validate::validate_entry(&entry);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("exceeds maximum allowed"));
}

#[test]
fn test_validate_entry_rejects_oversized_zst() {
    let mut entry = sample_entry("big_zst", "1.0", "1.0", "stable");
    entry.files.zst.size_bytes = super::compression::MAX_ARCHIVE_SIZE + 1;
    let result = super::validate::validate_entry(&entry);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("exceeds maximum allowed"));
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sample_entry(id: &str, version: &str, sort_key: &str, build_type: &str) -> LibraryManifestEntry {
    LibraryManifestEntry {
        entry_id: id.to_string(),
        library: LibraryInfo {
            id: "lib".to_string(),
            file_name: "lib.dll".to_string(),
        },
        version: VersionInfo {
            value: version.to_string(),
            sort_key: sort_key.to_string(),
        },
        build: BuildInfo {
            build_type: build_type.to_string(),
            label: None,
        },
        files: FilesInfo {
            dll: DllFileInfo {
                size_bytes: 1024,
                hashes: HashesInfo {
                    sha256: "0000000000000000000000000000000000000000000000000000000000000000"
                        .to_string(),
                },
            },
            zst: ZstFileInfo {
                size_bytes: 2048,
                download_url: format!("https://{}/file.dll.zst", super::manifest::LIBS_HOST),
            },
        },
        signature: SignatureInfo::Unsigned,
    }
}

fn compress(data: &[u8]) -> Vec<u8> {
    zstd::encode_all(data, 3).expect("zstd compression failed")
}
