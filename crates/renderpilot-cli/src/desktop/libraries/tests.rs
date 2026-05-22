use crate::desktop::libraries::local_library::library_id_to_group_key;
use crate::desktop::libraries::storage::sanitize_path_component;
use crate::desktop::libraries::types::{
    BuildInfo, DllFileInfo, FilesInfo, HashesInfo, LibraryInfo, LibraryManifest,
    LibraryManifestEntry, SignatureInfo, VersionInfo, ZipFileInfo,
};
use crate::desktop::libraries::validate::validate_manifest;

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
            zip: ZipFileInfo {
                size_bytes: 2048,
                download_url: "https://example.com/file.zip".to_string(),
            },
        },
        signature: SignatureInfo::Unsigned,
    }
}
