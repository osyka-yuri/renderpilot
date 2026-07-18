use std::io::Cursor;
use std::path::Path;

use renderpilot_domain::{
    AddonKind, GameId, InstalledAddon, PathRef, TrackedSource, TrackedSourceRole, Version,
};

use super::*;
use crate::addons::luma::test_support::zip_with_entries;
use crate::addons::luma::types::{
    ExternalConfigEntry, ExternalConfigSection, LumaExternalRequirement, ManagedArchiveSource,
    ManagedInstallMapEntry,
};
use crate::addons::reshade::fetch::sha256_hex;

fn requirement_for_archive(archive: &[u8], dll: &[u8]) -> LumaExternalRequirement {
    LumaExternalRequirement::Dgvoodoo2 {
        version: "2.87.3".to_owned(),
        accepted_detected_apis: vec![renderpilot_domain::GraphicsApi::D3D9],
        reshade_proxy_dll: "dxgi.dll".to_owned(),
        source: ManagedArchiveSource {
            url: "https://example.test/dgVoodoo2.zip".to_owned(),
            sha256: sha256_hex(archive),
            size: archive.len() as u64,
        },
        install_map: vec![ManagedInstallMapEntry {
            source: "MS/x86/D3D9.dll".to_owned(),
            dest: "D3D9.dll".to_owned(),
            sha256: sha256_hex(dll),
            size: dll.len() as u64,
        }],
        config_file: "dgVoodoo.conf".to_owned(),
        config: vec![ExternalConfigSection {
            section: "General".to_owned(),
            entries: vec![ExternalConfigEntry {
                key: "OutputAPI".to_owned(),
                value: "d3d11_fl11_0".to_owned(),
            }],
        }],
    }
}

#[test]
fn extracts_declared_files_and_builds_manifest_owned_config() {
    let dll = b"dgvoodoo-d3d9";
    let archive = zip_with_entries(&[("MS/x86/D3D9.dll", dll.as_slice())]);
    let requirement = requirement_for_archive(&archive, dll);

    let LumaExternalRequirement::Dgvoodoo2 {
        source,
        install_map,
        config,
        ..
    } = &requirement;
    verify_archive_identity(source, &archive).expect("archive identity");
    let mut zip = zip::ZipArchive::new(Cursor::new(archive.as_slice())).expect("zip");
    let file = read_mapped_file(&mut zip, &install_map[0]).expect("file");
    let config_default = managed_config_default(config);

    assert_eq!(file.dest, "D3D9.dll");
    assert_eq!(file.bytes, dll);
    assert_eq!(config_default, "[General]\r\nOutputAPI = d3d11_fl11_0\r\n");
    assert_eq!(config_sections(config)[0].keys[0].0, "OutputAPI");
}

#[test]
fn rejects_entry_hash_mismatch() {
    let archive = zip_with_entries(&[
        ("MS/x86/D3D9.dll", b"actual".as_slice()),
        ("dgVoodoo.conf", b"[General]\r\n"),
    ]);
    let requirement = requirement_for_archive(&archive, b"expected");
    let LumaExternalRequirement::Dgvoodoo2 { install_map, .. } = &requirement;
    let mut zip = zip::ZipArchive::new(Cursor::new(archive.as_slice())).expect("zip");

    assert!(read_mapped_file(&mut zip, &install_map[0]).is_err());
}

#[test]
fn normalizes_dgvoodoo_release_version_to_pe_product_version() {
    assert_eq!(
        normalized_requirement_version("2.87.3")
            .expect("version")
            .as_str(),
        "2.8.7.3"
    );
}

#[test]
fn requires_dgvoodoo_product_name_and_product_version() {
    let required = Version::parse("2.8.7.3").expect("version");
    let inspection = renderpilot_detection::PeInspection {
        identity: renderpilot_detection::VersionIdentityStrings {
            product_name: Some("dgVoodoo".to_owned()),
            product_version: Some("2.8.7.3".to_owned()),
            ..Default::default()
        },
        ..Default::default()
    };

    assert!(is_compatible_inspection(&inspection, &required));
    assert!(!is_compatible_inspection(
        &renderpilot_detection::PeInspection::default(),
        &required
    ));
}

#[test]
fn owned_status_accepts_equal_or_newer_versions_and_flags_old_ones() {
    let required = Version::parse("2.8.7.3").expect("required");
    let inspection = |version: &str| renderpilot_detection::PeInspection {
        identity: renderpilot_detection::VersionIdentityStrings {
            product_name: Some("dgVoodoo".to_owned()),
            product_version: Some(version.to_owned()),
            ..Default::default()
        },
        ..Default::default()
    };

    assert_eq!(
        owned_status_from_inspection(&inspection("2.8.7.3"), &required),
        OwnedDgVoodooStatus::Current
    );
    assert_eq!(
        owned_status_from_inspection(&inspection("2.8.8.0"), &required),
        OwnedDgVoodooStatus::Current,
        "a newer user-installed dgVoodoo must never be downgraded"
    );
    assert_eq!(
        owned_status_from_inspection(&inspection("2.8.7.2"), &required),
        OwnedDgVoodooStatus::Outdated
    );
    assert_eq!(
        owned_status_from_inspection(&renderpilot_detection::PeInspection::default(), &required),
        OwnedDgVoodooStatus::Unknown
    );
}

#[test]
fn owned_status_reports_incomplete_when_mapped_dest_is_missing() {
    let dll = b"dgvoodoo-d3d9";
    let archive = zip_with_entries(&[("MS/x86/D3D9.dll", dll.as_slice())]);
    let requirement = requirement_for_archive(&archive, dll);
    let dir = tempfile::tempdir().expect("tempdir");
    // No D3D9.dll on disk → Incomplete, not Unknown.
    assert_eq!(
        owned_status(dir.path(), &requirement),
        OwnedDgVoodooStatus::Incomplete
    );
}

#[test]
fn runtime_update_ownership_requires_the_exact_mapped_dll_paths() {
    let dll = b"dgvoodoo-d3d9";
    let archive = zip_with_entries(&[("MS/x86/D3D9.dll", dll.as_slice())]);
    let requirement = requirement_for_archive(&archive, dll);
    let dir = tempfile::tempdir().expect("tempdir");
    let addon = dir.path().join("Luma-Game.addon");
    let d3d9 = dir.path().join("D3D9.dll");
    let path_ref =
        |path: &Path| PathRef::new(path.to_string_lossy().into_owned()).expect("valid owned path");
    let forward_slash_path_ref = |path: &Path| {
        PathRef::new(path.to_string_lossy().replace('\\', "/"))
            .expect("valid normalized owned path")
    };

    let owned = InstalledAddon::from_parts(
        GameId::new("steam:49520").expect("game id"),
        AddonKind::Luma,
        path_ref(&addon),
        None,
        vec![
            forward_slash_path_ref(&addon),
            forward_slash_path_ref(&d3d9),
        ],
        Vec::new(),
        Vec::new(),
    )
    .expect("record");
    assert!(record_owns_runtime(&owned, dir.path(), &requirement));

    let unowned = InstalledAddon::new(
        GameId::new("steam:49521").expect("game id"),
        AddonKind::Luma,
        path_ref(&addon),
    );
    assert!(
        !record_owns_runtime(&unowned, dir.path(), &requirement),
        "an add-on record alone must not authorize a dgVoodoo update"
    );
    assert!(
        !record_can_manage_runtime(&owned, dir.path(), &requirement),
        "full ownership without a wrapper source is still not managed"
    );
}

fn requirement_with_map(
    archive: &[u8],
    entries: &[(&str, &str, &[u8])],
) -> LumaExternalRequirement {
    LumaExternalRequirement::Dgvoodoo2 {
        version: "2.87.3".to_owned(),
        accepted_detected_apis: vec![renderpilot_domain::GraphicsApi::D3D9],
        reshade_proxy_dll: "dxgi.dll".to_owned(),
        source: ManagedArchiveSource {
            url: "https://example.test/dgVoodoo2.zip".to_owned(),
            sha256: sha256_hex(archive),
            size: archive.len() as u64,
        },
        install_map: entries
            .iter()
            .map(|(source, dest, bytes)| ManagedInstallMapEntry {
                source: (*source).to_owned(),
                dest: (*dest).to_owned(),
                sha256: sha256_hex(bytes),
                size: bytes.len() as u64,
            })
            .collect(),
        config_file: "dgVoodoo.conf".to_owned(),
        config: vec![ExternalConfigSection {
            section: "General".to_owned(),
            entries: vec![ExternalConfigEntry {
                key: "OutputAPI".to_owned(),
                value: "d3d11_fl11_0".to_owned(),
            }],
        }],
    }
}

#[test]
fn managed_runtime_allows_map_expansion_when_new_dest_is_missing() {
    let d3d9 = b"dgvoodoo-d3d9";
    let d3d8 = b"dgvoodoo-d3d8";
    let archive = zip_with_entries(&[
        ("MS/x86/D3D9.dll", d3d9.as_slice()),
        ("MS/x86/D3D8.dll", d3d8.as_slice()),
    ]);
    let expanded = requirement_with_map(
        &archive,
        &[
            ("MS/x86/D3D9.dll", "D3D9.dll", d3d9),
            ("MS/x86/D3D8.dll", "D3D8.dll", d3d8),
        ],
    );
    let dir = tempfile::tempdir().expect("tempdir");
    let addon = dir.path().join("Luma-Game.addon");
    let d3d9_path = dir.path().join("D3D9.dll");
    let path_ref =
        |path: &Path| PathRef::new(path.to_string_lossy().into_owned()).expect("valid owned path");
    let wrapper = TrackedSource::new(
        TrackedSourceRole::DgVoodooWrapper,
        "https://example.test/dgVoodoo2.zip",
        None,
        "archive-digest",
    );
    let record = InstalledAddon::from_parts(
        GameId::new("steam:49520").expect("game id"),
        AddonKind::Luma,
        path_ref(&addon),
        None,
        vec![path_ref(&addon), path_ref(&d3d9_path)],
        Vec::new(),
        vec![wrapper],
    )
    .expect("record");

    assert!(record_can_manage_runtime(&record, dir.path(), &expanded));
    assert!(map_needs_ownership_sync(&record, dir.path(), &expanded));
    assert!(!record_owns_runtime(&record, dir.path(), &expanded));
}

#[test]
fn managed_runtime_blocks_map_expansion_onto_unowned_existing_file() {
    let d3d9 = b"dgvoodoo-d3d9";
    let d3d8 = b"dgvoodoo-d3d8";
    let archive = zip_with_entries(&[
        ("MS/x86/D3D9.dll", d3d9.as_slice()),
        ("MS/x86/D3D8.dll", d3d8.as_slice()),
    ]);
    let expanded = requirement_with_map(
        &archive,
        &[
            ("MS/x86/D3D9.dll", "D3D9.dll", d3d9),
            ("MS/x86/D3D8.dll", "D3D8.dll", d3d8),
        ],
    );
    let dir = tempfile::tempdir().expect("tempdir");
    let addon = dir.path().join("Luma-Game.addon");
    let d3d9_path = dir.path().join("D3D9.dll");
    let d3d8_path = dir.path().join("D3D8.dll");
    std::fs::write(&d3d8_path, b"foreign-d3d8").expect("write foreign dll");
    let path_ref =
        |path: &Path| PathRef::new(path.to_string_lossy().into_owned()).expect("valid owned path");
    let wrapper = TrackedSource::new(
        TrackedSourceRole::DgVoodooWrapper,
        "https://example.test/dgVoodoo2.zip",
        None,
        "archive-digest",
    );
    let record = InstalledAddon::from_parts(
        GameId::new("steam:49520").expect("game id"),
        AddonKind::Luma,
        path_ref(&addon),
        None,
        vec![path_ref(&addon), path_ref(&d3d9_path)],
        Vec::new(),
        vec![wrapper],
    )
    .expect("record");

    assert!(!record_can_manage_runtime(&record, dir.path(), &expanded));
    assert!(record_owns_any_map_dest(&record, dir.path(), &expanded));
}

#[test]
fn advisory_wrapper_source_is_enough_for_manage_gate_with_owned_map() {
    let dll = b"dgvoodoo-d3d9";
    let archive = zip_with_entries(&[("MS/x86/D3D9.dll", dll.as_slice())]);
    let requirement = requirement_for_archive(&archive, dll);
    let dir = tempfile::tempdir().expect("tempdir");
    let addon = dir.path().join("Luma-Game.addon");
    let d3d9_path = dir.path().join("D3D9.dll");
    std::fs::write(&d3d9_path, dll).expect("write dll");
    let path_ref =
        |path: &Path| PathRef::new(path.to_string_lossy().into_owned()).expect("valid owned path");
    let wrapper = advisory_wrapper_source(&requirement);
    assert!(wrapper.is_advisory());
    assert_eq!(wrapper.role(), TrackedSourceRole::DgVoodooWrapper);
    assert!(
        wrapper
            .channel()
            .is_some_and(|c| c.starts_with("dgvoodoo2@"))
    );

    let record = InstalledAddon::from_parts(
        GameId::new("steam:49520").expect("game id"),
        AddonKind::Luma,
        path_ref(&addon),
        None,
        vec![path_ref(&addon), path_ref(&d3d9_path)],
        Vec::new(),
        vec![wrapper],
    )
    .expect("record");

    assert!(
        record_can_manage_runtime(&record, dir.path(), &requirement),
        "DB-loss advisory wrapper + owned map must authorize manage/freshness"
    );
}

#[test]
fn wrapper_source_alone_does_not_authorize_managed_update() {
    let dll = b"dgvoodoo-d3d9";
    let archive = zip_with_entries(&[("MS/x86/D3D9.dll", dll.as_slice())]);
    let requirement = requirement_for_archive(&archive, dll);
    let dir = tempfile::tempdir().expect("tempdir");
    let addon = dir.path().join("Luma-Game.addon");
    let path_ref =
        |path: &Path| PathRef::new(path.to_string_lossy().into_owned()).expect("valid owned path");
    let wrapper = TrackedSource::new(
        TrackedSourceRole::DgVoodooWrapper,
        "https://example.test/dgVoodoo2.zip",
        None,
        "archive-digest",
    );
    let record = InstalledAddon::from_parts(
        GameId::new("steam:49520").expect("game id"),
        AddonKind::Luma,
        path_ref(&addon),
        None,
        vec![path_ref(&addon)],
        Vec::new(),
        vec![wrapper],
    )
    .expect("record");

    assert!(!record_can_manage_runtime(
        &record,
        dir.path(),
        &requirement
    ));
}

#[test]
fn partial_existing_dgvoodoo_is_a_conflict_but_no_files_is_installable() {
    let archive = zip_with_entries(&[("MS/x86/D3D9.dll", b"dll".as_slice())]);
    let requirement = requirement_for_archive(&archive, b"dll");
    let dir = tempfile::tempdir().expect("tempdir");

    assert_eq!(
        assess_existing(dir.path(), &requirement),
        ExistingDgVoodoo::Absent
    );

    // A directory at an expected file path counts as an incomplete
    // footprint, not an absent runtime that the installer may overwrite.
    std::fs::create_dir(dir.path().join("D3D9.dll")).expect("create directory");
    assert!(matches!(
        assess_existing(dir.path(), &requirement),
        ExistingDgVoodoo::Conflict(_)
    ));
}

#[test]
fn config_classifier_accepts_only_exact_manifest_assignments() {
    let archive = zip_with_entries(&[("MS/x86/D3D9.dll", b"dll".as_slice())]);
    let requirement = requirement_for_archive(&archive, b"dll");
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("dgVoodoo.conf");
    let LumaExternalRequirement::Dgvoodoo2 {
        config: expected, ..
    } = &requirement;

    assert!(
        config_is_adoptable(&config, expected),
        "missing config is empty"
    );

    std::fs::write(
        &config,
        "; old RenderPilot stack\r\n[general]\r\nOutputAPI = d3d11_fl11_0\r\n",
    )
    .expect("config");
    assert!(config_is_adoptable(&config, expected));

    std::fs::write(&config, "[General]\r\nOutputAPI=d3d12_fl11_0\r\n").expect("different value");
    assert!(!config_is_adoptable(&config, expected));

    std::fs::write(&config, "[General]\r\nUserOption=true\r\n").expect("unknown key");
    assert!(!config_is_adoptable(&config, expected));

    std::fs::write(
        &config,
        "[General]\r\nOutputAPI=d3d11_fl11_0\r\n[General]\r\nOutputAPI=d3d11_fl11_0\r\n",
    )
    .expect("duplicate section");
    assert!(!config_is_adoptable(&config, expected));

    std::fs::write(
        &config,
        "[General]\r\nOutputAPI=d3d11_fl11_0\r\nOutputAPI=d3d11_fl11_0\r\n",
    )
    .expect("duplicate key");
    assert!(!config_is_adoptable(&config, expected));

    std::fs::write(&config, "not-an-assignment\r\n").expect("malformed line");
    assert!(!config_is_adoptable(&config, expected));
}

#[test]
fn config_classifier_refuses_an_unreadable_or_non_file_config() {
    let archive = zip_with_entries(&[("MS/x86/D3D9.dll", b"dll".as_slice())]);
    let requirement = requirement_for_archive(&archive, b"dll");
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("dgVoodoo.conf");
    let LumaExternalRequirement::Dgvoodoo2 {
        config: expected, ..
    } = &requirement;

    std::fs::create_dir(&config).expect("config directory");
    assert!(!config_is_adoptable(&config, expected));
}
