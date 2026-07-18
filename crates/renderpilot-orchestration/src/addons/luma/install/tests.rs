use std::path::Path;

use super::*;
use crate::ServiceError;
use crate::addons::engine::{self, IniSection};
use crate::addons::luma::dgvoodoo::{
    AdoptedDgVoodoo, DgVoodooInstall, PreparedDgVoodoo, PreparedDgVoodooFile, ReusedDgVoodoo,
};
use crate::addons::luma::fetch::types::LumaPayloadFile;
use crate::addons::luma::test_support::{
    MACHINE_AMD64, PE32_PLUS_MAGIC, build_nvidia_dlss_pe, build_pe_with_exports,
};
use renderpilot_application::InstalledAddonRepository;
use renderpilot_domain::{
    AddonKind, GameId, InstalledAddon, InstalledAddonHostKind, TrackedSourceRole, Version,
};
use std::fs;
use tempfile::tempdir;

fn min_version() -> Version {
    Version::parse("6.7.0").expect("version")
}

fn install_test(
    game_dir: &Path,
    prepared: PreparedInstall,
    min_host_version: &Version,
) -> Result<(InstalledAddon, engine::PendingInstallCommit), ServiceError> {
    let db = tempdir().expect("db");
    let context = crate::Context::open_at(db.path().join("catalog.sqlite")).expect("context");
    super::install(&context, game_dir, prepared, min_host_version)
}

fn reshade_host_bytes(addon_support: bool) -> Vec<u8> {
    let mut exports = vec!["ReShadeVersion"];
    if addon_support {
        exports.extend([
            "ReShadeRegisterAddon",
            "ReShadeUnregisterAddon",
            "ReShadeRegisterEvent",
            "ReShadeUnregisterEvent",
        ]);
    }
    build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &exports)
}

fn prepared() -> PreparedInstall {
    PreparedInstall {
        game_id: GameId::new("steam:403640").expect("id"),
        proxy_dll_name: "dxgi.dll".to_owned(),
        payload: vec![
            LumaPayloadFile {
                relative_path: "Luma-Dishonored_2.addon".to_owned(),
                bytes: b"addon-bytes".to_vec(),
            },
            LumaPayloadFile {
                relative_path: "Luma/Global/Copy_PS.hlsl".to_owned(),
                bytes: b"technique {}".to_vec(),
            },
            LumaPayloadFile {
                relative_path: "Luma/Includes/Common.hlsl".to_owned(),
                bytes: b"// common".to_vec(),
            },
        ],
        main_addon_rel: "Luma-Dishonored_2.addon".to_owned(),
        asset_source_url: "https://github.com/Filoppi/Luma-Framework/releases/latest/download/Luma-Dishonored_2.zip".to_owned(),
        zip_digest: "zip-digest".to_owned(),
        source_etag: Some("\"etag-1\"".to_owned()),
        source_last_modified: Some("Wed, 18 Jun 2026 12:00:00 GMT".to_owned()),
        build_label: Some("Build 515".to_owned()),
        reshade_dll_bytes: reshade_host_bytes(true),
        reshade_source_url: "https://nightly.link/crosire/reshade/x64.zip".to_owned(),
        reshade_source_etag: Some("\"rs-etag-1\"".to_owned()),
        reshade_last_modified: Some("Tue, 17 Jun 2026 09:00:00 GMT".to_owned()),
        reshade_digest: "reshade-digest".to_owned(),
        dgvoodoo: None,
    }
}

fn dgvoodoo_dependency() -> PreparedDgVoodoo {
    PreparedDgVoodoo {
        version: "2.87.3".to_owned(),
        files: vec![PreparedDgVoodooFile {
            dest: "D3D9.dll".to_owned(),
            bytes: b"dgvoodoo-d3d9".to_vec(),
        }],
        config_file: "dgVoodoo.conf".to_owned(),
        config_default: "[General]\r\nOutputAPI = d3d12_fl11_0\r\n\r\n[DirectX]\r\nVideoCard = svga\r\ndgVoodooWatermark = true\r\nVRAM = 256\r\n".to_owned(),
        config_sections: vec![
            IniSection {
                name: "General".to_owned(),
                keys: vec![("OutputAPI".to_owned(), "d3d11_fl11_0".to_owned())],
            },
            IniSection {
                name: "DirectX".to_owned(),
                keys: vec![
                    ("VideoCard".to_owned(), "geforce_9800_gt".to_owned()),
                    ("dgVoodooWatermark".to_owned(), "false".to_owned()),
                    ("VRAM".to_owned(), "1024".to_owned()),
                ],
            },
        ],
        source_url: "https://github.com/dege-diosg/dgVoodoo2/releases/download/v2.87.3/dgVoodoo2_87_3.zip".to_owned(),
        source_etag: Some("\"dg-etag-1\"".to_owned()),
        source_last_modified: Some("Mon, 06 Jul 2026 10:00:00 GMT".to_owned()),
        archive_digest: "dgvoodoo-archive-digest".to_owned(),
    }
}

fn reused_dgvoodoo() -> ReusedDgVoodoo {
    let managed = dgvoodoo_dependency();
    ReusedDgVoodoo {
        config_file: managed.config_file,
        config_default: managed.config_default,
        config_sections: managed.config_sections,
    }
}

fn read(path: &Path) -> Vec<u8> {
    fs::read(path).expect("file should exist")
}

#[test]
fn fresh_install_lays_down_host_addon_and_tree() {
    let dir = tempdir().expect("tempdir");
    let (record, commit) = install_test(dir.path(), prepared(), &min_version()).expect("install");
    commit.finish_committed();

    assert_eq!(
        read(&dir.path().join("Luma-Dishonored_2.addon")),
        b"addon-bytes"
    );
    assert_eq!(read(&dir.path().join("dxgi.dll")), reshade_host_bytes(true));
    assert_eq!(
        read(&dir.path().join("Luma").join("Global").join("Copy_PS.hlsl")),
        b"technique {}"
    );
    assert_eq!(record.addon_version(), Some("Build 515"));
    assert!(record.has_host_binary_provenance());
    assert_eq!(record.host_kind(), Some(InstalledAddonHostKind::Proxy));
    assert_eq!(record.reshade_channel(), Some("nightly"));
    assert!(
        record
            .created_files()
            .iter()
            .any(|path| path.as_str().ends_with("dxgi.dll")),
        "host proxy must be in created_files so uninstall removes it"
    );
    assert!(record.registered_exe_path().is_none());

    let addon = record
        .tracked_sources()
        .iter()
        .find(|s| s.role() == TrackedSourceRole::AddonPayload)
        .expect("addon source recorded");
    assert_eq!(addon.digest(), "zip-digest");

    // addon + 2 shader files + proxy DLL.
    assert_eq!(record.created_files().len(), 4);
    assert!(record.backed_up_files().is_empty());
}

#[test]
fn uninstall_removes_reshade_host_so_renodx_is_not_blocked_by_inactive_slot() {
    // Real-world failure: Luma left dxgi.dll behind; RenoDX on a D3D9 title
    // then saw InactiveSlot conflict (ReShade in dxgi, active proxy d3d9).
    let dir = tempdir().expect("tempdir");
    let (record, commit) = install_test(dir.path(), prepared(), &min_version()).expect("install");
    commit.finish_committed();
    assert!(dir.path().join("dxgi.dll").is_file());

    uninstall_engine_files(&record).expect("uninstall");

    assert!(
        !dir.path().join("dxgi.dll").exists(),
        "host proxy must be gone"
    );
    assert!(!dir.path().join("Luma-Dishonored_2.addon").exists());
    assert!(!dir.path().join("Luma").exists());
}

#[test]
fn fresh_install_lays_down_dgvoodoo_and_tracks_its_source() {
    let dir = tempdir().expect("tempdir");
    let mut prepared = prepared();
    prepared.dgvoodoo = Some(DgVoodooInstall::Managed(dgvoodoo_dependency()));

    let (record, commit) = install_test(dir.path(), prepared, &min_version()).expect("install");
    commit.finish_committed();

    assert_eq!(read(&dir.path().join("D3D9.dll")), b"dgvoodoo-d3d9");
    let config = std::fs::read_to_string(dir.path().join("dgVoodoo.conf")).expect("config");
    assert!(config.contains("[General]"));
    assert!(config.contains("OutputAPI = d3d11_fl11_0"));
    assert!(config.contains("[DirectX]"));
    assert!(config.contains("VideoCard = geforce_9800_gt"));

    let source = record
        .tracked_sources()
        .iter()
        .find(|source| source.role() == TrackedSourceRole::DgVoodooWrapper)
        .expect("dgVoodoo source recorded");
    assert_eq!(source.digest(), "dgvoodoo-archive-digest");
    assert_eq!(source.channel(), Some("dgvoodoo2@2.87.3"));
    assert!(
        record
            .created_files()
            .iter()
            .any(|path| path.as_str().ends_with("D3D9.dll"))
    );
    assert!(
        record
            .created_files()
            .iter()
            .any(|path| path.as_str().ends_with("dgVoodoo.conf"))
    );
}

#[test]
fn fresh_borderlands_style_install_with_dgvoodoo_round_trips_to_clean_folder() {
    let dir = tempdir().expect("tempdir");
    let mut prepared = prepared();
    prepared.game_id =
        GameId::new("manual:D:/SteamLibrary/steamapps/common/Borderlands 2").expect("id");
    prepared.main_addon_rel = "Luma-Borderlands 2 and The Pre-Sequel.addon".to_owned();
    prepared.payload = vec![
        LumaPayloadFile {
            relative_path: prepared.main_addon_rel.clone(),
            bytes: b"borderlands-addon".to_vec(),
        },
        LumaPayloadFile {
            relative_path: "Luma/Global/Luma_Copy_PS.hlsl".to_owned(),
            bytes: b"copy".to_vec(),
        },
        LumaPayloadFile {
            relative_path: "Luma/Borderlands 2 and The Pre-Sequel/Includes/Common.hlsl".to_owned(),
            bytes: b"common".to_vec(),
        },
    ];
    prepared.dgvoodoo = Some(DgVoodooInstall::Managed(dgvoodoo_dependency()));
    let main_addon_rel = prepared.main_addon_rel.clone();

    let (record, commit) = install_test(dir.path(), prepared, &min_version()).expect("install");
    commit.finish_committed();
    assert!(dir.path().join(&main_addon_rel).is_file());
    assert!(dir.path().join("dxgi.dll").is_file());
    assert!(dir.path().join("D3D9.dll").is_file());
    assert!(dir.path().join("dgVoodoo.conf").is_file());
    assert!(record.backed_up_files().is_empty());

    uninstall_engine_files(&record).expect("uninstall");

    assert!(!dir.path().join(&main_addon_rel).exists());
    assert!(!dir.path().join("dxgi.dll").exists());
    assert!(!dir.path().join("D3D9.dll").exists());
    assert!(!dir.path().join("dgVoodoo.conf").exists());
    assert!(!dir.path().join("Luma").exists());
}

#[test]
fn reused_dgvoodoo_merges_only_manifest_keys_and_survives_uninstall() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("D3D9.dll"), b"user-dgvoodoo").expect("write dll");
    fs::write(
        dir.path().join("dgVoodoo.conf"),
        "[General]\r\nOutputAPI = old\r\nUserOption = keep\r\n\r\n[DirectX]\r\nVRAM = 512\r\n",
    )
    .expect("write config");
    let mut prepared = prepared();
    prepared.dgvoodoo = Some(DgVoodooInstall::Reused(reused_dgvoodoo()));

    let (record, commit) = install_test(dir.path(), prepared, &min_version()).expect("install");
    commit.finish_committed();

    assert_eq!(read(&dir.path().join("D3D9.dll")), b"user-dgvoodoo");
    let config = std::fs::read_to_string(dir.path().join("dgVoodoo.conf")).expect("config");
    assert!(config.contains("OutputAPI = d3d11_fl11_0"));
    assert!(config.contains("UserOption = keep"));
    assert!(config.contains("VideoCard=geforce_9800_gt"));
    assert!(!dir.path().join("dgVoodoo.conf.bak").exists());
    assert!(
        !record
            .created_files()
            .iter()
            .any(|path| path.as_str().ends_with("dgVoodoo.conf"))
    );
    assert!(
        !record
            .tracked_sources()
            .iter()
            .any(|source| source.role() == TrackedSourceRole::DgVoodooWrapper)
    );

    uninstall_engine_files(&record).expect("uninstall");

    assert_eq!(read(&dir.path().join("D3D9.dll")), b"user-dgvoodoo");
    assert!(dir.path().join("dgVoodoo.conf").is_file());
    assert!(
        std::fs::read_to_string(dir.path().join("dgVoodoo.conf"))
            .expect("config")
            .contains("OutputAPI = d3d11_fl11_0")
    );
}

#[test]
fn adopted_dgvoodoo_is_removed_without_gaining_update_provenance() {
    let dir = tempdir().expect("tempdir");
    let d3d9 = dir.path().join("D3D9.dll");
    let config = dir.path().join("dgVoodoo.conf");
    fs::write(&d3d9, b"old-luma-dgvoodoo").expect("write dll");
    fs::write(
        &config,
        "[General]\r\nOutputAPI=d3d11_fl11_0\r\n\r\n[DirectX]\r\nVideoCard=geforce_9800_gt\r\ndgVoodooWatermark=false\r\nVRAM=1024\r\n",
    )
    .expect("write config");
    let mut prepared = prepared();
    prepared.dgvoodoo = Some(DgVoodooInstall::Adopted(AdoptedDgVoodoo {
        config: reused_dgvoodoo(),
        existing_paths: vec![d3d9.clone(), config.clone()],
    }));

    let (record, commit) = install_test(dir.path(), prepared, &min_version()).expect("install");
    commit.finish_committed();
    assert!(
        record
            .created_files()
            .iter()
            .any(|path| path.as_str().ends_with("D3D9.dll"))
    );
    assert!(
        record
            .created_files()
            .iter()
            .any(|path| path.as_str().ends_with("dgVoodoo.conf"))
    );
    assert!(
        !record
            .tracked_sources()
            .iter()
            .any(|source| source.role() == TrackedSourceRole::DgVoodooWrapper)
    );

    uninstall_engine_files(&record).expect("uninstall");
    assert!(!d3d9.exists());
    assert!(!config.exists());
}

#[test]
fn uninstall_restores_shadowed_dgvoodoo_files() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("D3D9.dll"), b"game-d3d9").expect("write d3d9");
    fs::write(
        dir.path().join("dgVoodoo.conf"),
        "[General]\r\nOutputAPI=old\r\n",
    )
    .expect("write config");
    let mut prepared = prepared();
    prepared.dgvoodoo = Some(DgVoodooInstall::Managed(dgvoodoo_dependency()));

    let (record, commit) = install_test(dir.path(), prepared, &min_version()).expect("install");
    commit.finish_committed();
    assert_eq!(read(&dir.path().join("D3D9.dll")), b"dgvoodoo-d3d9");
    assert_eq!(record.backed_up_files().len(), 2);

    uninstall_engine_files(&record).expect("uninstall");

    assert_eq!(read(&dir.path().join("D3D9.dll")), b"game-d3d9");
    let config = std::fs::read_to_string(dir.path().join("dgVoodoo.conf")).expect("config");
    assert!(config.contains("OutputAPI=old"));
    assert!(!dir.path().join("D3D9.dll.bak").exists());
    assert!(!dir.path().join("dgVoodoo.conf.bak").exists());
}

#[test]
fn fresh_install_round_trips_to_clean_folder() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("game.exe"), b"game").expect("write");

    let (record, commit) = install_test(dir.path(), prepared(), &min_version()).expect("install");
    commit.finish_committed();
    uninstall_engine_files(&record).expect("uninstall");

    assert!(!dir.path().join("Luma-Dishonored_2.addon").exists());
    assert!(!dir.path().join("dxgi.dll").exists());
    assert!(!dir.path().join("Luma").exists());
    assert_eq!(read(&dir.path().join("game.exe")), b"game");
}

#[test]
fn a_game_owned_nvngx_dlss_is_backed_up_on_install_and_restored_on_uninstall() {
    let db = tempdir().expect("db");
    let dir = tempdir().expect("tempdir");
    let context = crate::Context::open_at(db.path().join("catalog.sqlite")).expect("context");
    let game_owned_dlss = dir.path().join("nvngx_dlss.dll");
    let original = build_nvidia_dlss_pe([2, 5, 0, 0]);
    let bundled = build_nvidia_dlss_pe([3, 7, 0, 0]);
    assert_eq!(
        renderpilot_detection::DlssBinaryInfo::from_bytes(&original)
            .expect("original info")
            .version()
            .as_str(),
        "2.5.0.0"
    );
    assert_eq!(
        renderpilot_detection::DlssBinaryInfo::from_bytes(&bundled)
            .expect("bundled info")
            .version()
            .as_str(),
        "3.7.0.0"
    );
    fs::write(&game_owned_dlss, &original).expect("write game dlss");

    let mut prepared = prepared();
    prepared.payload.push(LumaPayloadFile {
        relative_path: "nvngx_dlss.dll".to_owned(),
        bytes: bundled.clone(),
    });

    let (record, commit) =
        super::install(&context, dir.path(), prepared, &min_version()).expect("install");
    commit.finish_committed();
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("persist record");

    assert_eq!(read(&game_owned_dlss), bundled);
    assert_eq!(read(&dir.path().join("nvngx_dlss.dll.bak")), original);
    assert!(
        record
            .managed_files()
            .iter()
            .any(|binding| binding.path().as_str().ends_with("nvngx_dlss.dll")),
        "the install record must track the coordinated path"
    );
    assert!(record.backed_up_files().is_empty());

    crate::addons::luma::use_cases::commands::uninstall::uninstall(&context, record.game_id())
        .expect("uninstall");

    assert_eq!(
        read(&game_owned_dlss),
        original,
        "uninstall must restore the game's own nvngx_dlss.dll"
    );
    assert!(!dir.path().join("nvngx_dlss.dll.bak").exists());
}

#[test]
fn build_record_leaves_the_channel_unrecorded_when_the_host_is_reused_untouched() {
    // B.6: only a host this install itself wrote is known to be nightly for
    // certain; a reused foreign host's real channel is never guessed. Tested
    // directly against `build_record` (bypassing `assess_for_tool`'s PE scan
    // entirely) since the synthetic PE fixture can't stamp a version
    // resource, so every *present* host it produces ends up needing a
    // repair/write once Luma's `min_host_version` gate is in play -- see
    // the `a_present_host_with_an_unreadable_version_is_repaired...` test
    // below for that path, and `reshade::host_policy`'s own suite for the
    // genuinely-reused (`writes_host() == false`) case.
    let dir = tempdir().expect("tempdir");
    let receipt = engine::install(
        dir.path(),
        &engine::InstallPlan {
            kind: AddonKind::Luma,
            ops: vec![engine::FileOp::CreateNested {
                relative_path: "Luma-Dishonored_2.addon".to_owned(),
                bytes: b"addon-bytes".to_vec(),
            }],
        },
    )
    .expect("install addon only");

    let record = build_record(
        &prepared(),
        dir.path(),
        dir.path(),
        record::RecordInstallResult {
            tracks_host: false,
            adopted_host_path: None,
            adopted_existing: &[],
            receipt: &receipt,
            managed_file: None,
        },
    )
    .expect("build record");

    assert!(!record.has_host_binary_provenance());
    assert_eq!(record.reshade_channel(), None);
    assert!(record.registered_exe_path().is_none());
}

#[test]
fn uninstall_leaves_a_reused_foreign_reshade_host_untouched() {
    // ReuseUser: host was never owned (no created_files entry, no channel /
    // host provenance). Uninstall must not delete the user's ReShade.
    let dir = tempdir().expect("tempdir");
    let host = dir.path().join("dxgi.dll");
    fs::write(&host, reshade_host_bytes(true)).expect("write foreign host");
    let receipt = engine::install(
        dir.path(),
        &engine::InstallPlan {
            kind: AddonKind::Luma,
            ops: vec![engine::FileOp::CreateNested {
                relative_path: "Luma-Dishonored_2.addon".to_owned(),
                bytes: b"addon-bytes".to_vec(),
            }],
        },
    )
    .expect("install addon only");

    let record = build_record(
        &prepared(),
        dir.path(),
        dir.path(),
        record::RecordInstallResult {
            tracks_host: false,
            adopted_host_path: None,
            adopted_existing: &[],
            receipt: &receipt,
            managed_file: None,
        },
    )
    .expect("build record");

    assert!(!record.has_host_binary_provenance());
    assert_eq!(record.reshade_channel(), None);
    assert!(
        !record
            .created_files()
            .iter()
            .any(|path| path.as_str().ends_with("dxgi.dll")),
        "reused host must not be listed as created"
    );

    uninstall_engine_files(&record).expect("uninstall");

    assert!(
        host.is_file(),
        "foreign reused ReShade host must survive Luma uninstall"
    );
    assert!(!dir.path().join("Luma-Dishonored_2.addon").exists());
}

#[test]
fn adopted_empty_luma_runtime_is_owned_with_advisory_nightly_provenance() {
    let dir = tempdir().expect("tempdir");
    let host = dir.path().join("dxgi.dll");
    let ini = dir.path().join("ReShade.ini");
    fs::write(&host, reshade_host_bytes(true)).expect("host");
    fs::write(&ini, "[GENERAL]\r\nNoPreset=1\r\n").expect("ini");
    fs::write(dir.path().join("ReShade.ini.bak"), b"legacy").expect("bak");
    fs::write(dir.path().join("ReShade.log"), b"log").expect("log");
    let receipt = engine::install(
        dir.path(),
        &engine::InstallPlan {
            kind: AddonKind::Luma,
            ops: vec![engine::FileOp::CreateNested {
                relative_path: "Luma-Dishonored_2.addon".to_owned(),
                bytes: b"addon-bytes".to_vec(),
            }],
        },
    )
    .expect("install addon only");

    let record = build_record(
        &prepared(),
        dir.path(),
        dir.path(),
        record::RecordInstallResult {
            tracks_host: false,
            adopted_host_path: Some(&host),
            adopted_existing: &[host.clone(), ini.clone()],
            receipt: &receipt,
            managed_file: None,
        },
    )
    .expect("build record");

    let source = record
        .tracked_sources()
        .iter()
        .find(|source| source.role() == TrackedSourceRole::HostBinary)
        .expect("advisory host source");
    assert!(source.is_advisory());
    assert_eq!(source.channel(), Some("nightly"));
    assert_eq!(record.reshade_channel(), Some("nightly"));
    assert!(
        record
            .created_files()
            .iter()
            .any(|path| path.as_str().ends_with("dxgi.dll"))
    );
    assert!(
        record
            .created_files()
            .iter()
            .any(|path| path.as_str().ends_with("ReShade.ini"))
    );

    uninstall_engine_files(&record).expect("uninstall");
    assert!(!host.exists());
    assert!(!ini.exists());
    assert!(!dir.path().join("ReShade.log").exists());
    assert!(dir.path().join("ReShade.ini.bak").exists());
}

#[test]
fn an_empty_host_with_an_unreadable_version_is_repaired_on_first_install() {
    // The synthetic PE fixture stamps no version resource. With no user
    // content it is safe to repair it into the Add-on build Luma requires.
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("dxgi.dll"), reshade_host_bytes(true)).expect("write");

    let prepared = prepared();
    let expected_host = prepared.reshade_dll_bytes.clone();
    let (record, commit) = install_test(dir.path(), prepared, &min_version()).expect("must repair");
    commit.finish_committed();

    assert_eq!(read(&dir.path().join("dxgi.dll")), expected_host);
    assert!(record.has_host_binary_provenance());
    uninstall_engine_files(&record).expect("uninstall");
    assert!(!dir.path().join("dxgi.dll").exists());
}

#[test]
fn refuses_install_when_proxy_slot_is_occupied_by_an_unknown_file() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("dxgi.dll"), b"another-overlay").expect("write");

    let error = install_test(dir.path(), prepared(), &min_version()).expect_err("should refuse");
    assert!(matches!(error, ServiceError::InvalidInput(_)));
    assert_eq!(read(&dir.path().join("dxgi.dll")), b"another-overlay");
    assert!(!dir.path().join("Luma-Dishonored_2.addon").exists());
    assert!(!dir.path().join("Luma").exists());
}

#[test]
fn host_repair_requires_reshade_bytes() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("dxgi.dll"), reshade_host_bytes(false)).expect("write");
    let mut prepared = prepared();
    prepared.reshade_dll_bytes.clear();

    let error = install_test(dir.path(), prepared, &min_version()).expect_err("repair needs bytes");

    assert!(matches!(error, ServiceError::InvalidInput(_)));
    assert_eq!(
        read(&dir.path().join("dxgi.dll")),
        reshade_host_bytes(false)
    );
    assert!(!dir.path().join("Luma-Dishonored_2.addon").exists());
}

// -----------------------------------------------------------------
// Torn-install recovery.
// -----------------------------------------------------------------

#[test]
fn recover_torn_install_removes_debris_and_clears_the_sentinel() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("Luma-Dishonored_2.addon"), b"half-written").expect("write");
    fs::create_dir_all(dir.path().join("Luma").join("Global")).expect("mkdir");
    fs::write(
        dir.path().join("Luma").join("Global").join("Copy_PS.hlsl"),
        b"technique {}",
    )
    .expect("write");
    fs::write(dir.path().join("game.exe"), b"game").expect("write unrelated file");
    // Simulate the crash-safety sentinel a crashed `engine::install` left behind.
    fs::write(dir.path().join("renderpilot-luma-install.lock"), b"").expect("write sentinel");
    assert!(engine::is_install_torn(dir.path(), AddonKind::Luma));

    recover_torn_install(&[dir.path()]);

    assert!(!dir.path().join("Luma-Dishonored_2.addon").exists());
    assert!(!dir.path().join("Luma").exists());
    assert!(
        dir.path().join("game.exe").exists(),
        "unrelated files must survive recovery"
    );
    assert!(
        !engine::is_install_torn(dir.path(), AddonKind::Luma),
        "the sentinel must clear once the folder is confirmed clean"
    );
    assert!(!crate::addons::tool::unmanaged_files_present(
        dir.path(),
        AddonKind::Luma
    ));
}

#[test]
fn recover_torn_install_restores_a_game_owned_nvngx_dlss_from_its_backup() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("nvngx_dlss.dll"), b"luma-dlss").expect("write shadowing dll");
    fs::write(dir.path().join("nvngx_dlss.dll.bak"), b"game-own-dlss").expect("write backup");
    fs::write(dir.path().join("renderpilot-luma-install.lock"), b"").expect("write sentinel");

    recover_torn_install(&[dir.path()]);

    assert_eq!(read(&dir.path().join("nvngx_dlss.dll")), b"game-own-dlss");
    assert!(!dir.path().join("nvngx_dlss.dll.bak").exists());
    assert!(!engine::is_install_torn(dir.path(), AddonKind::Luma));
}

#[test]
fn recover_torn_install_restores_nvngx_dlss_on_a_split_addon_root() {
    // Split AddonPath: sentinel + host live in game_dir; payload (including
    // optional root nvngx_dlss.dll) lives in addon_dir. Recovery must restore
    // the shadowed DLSS from the payload root, not only from scan_dirs[0].
    let game = tempdir().expect("game dir");
    let addon = tempdir().expect("addon dir");
    fs::write(game.path().join("renderpilot-luma-install.lock"), b"").expect("write sentinel");
    fs::write(addon.path().join("Luma-Game.addon"), b"half-written").expect("write debris");
    fs::write(addon.path().join("nvngx_dlss.dll"), b"luma-dlss").expect("write shadowing dll");
    fs::write(addon.path().join("nvngx_dlss.dll.bak"), b"game-own-dlss").expect("write backup");
    assert!(engine::is_install_torn(game.path(), AddonKind::Luma));

    recover_torn_install(&[game.path(), addon.path()]);

    assert!(!addon.path().join("Luma-Game.addon").exists());
    assert_eq!(read(&addon.path().join("nvngx_dlss.dll")), b"game-own-dlss");
    assert!(!addon.path().join("nvngx_dlss.dll.bak").exists());
    assert!(
        !engine::is_install_torn(game.path(), AddonKind::Luma),
        "sentinel clears once every scan root is clean"
    );
}

#[test]
fn recover_torn_install_never_deletes_an_nvngx_dlss_with_no_backup() {
    // No `.bak` sibling means no Luma install ever shadowed this file -- it is
    // simply the game's own, and recovery must leave it alone untouched.
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("nvngx_dlss.dll"), b"game-own-dlss").expect("write");
    fs::write(dir.path().join("renderpilot-luma-install.lock"), b"").expect("write sentinel");

    recover_torn_install(&[dir.path()]);

    assert_eq!(read(&dir.path().join("nvngx_dlss.dll")), b"game-own-dlss");
}

#[test]
fn recover_torn_install_restores_a_game_owned_d3d9_from_its_backup() {
    // Managed dgVoodoo uses BackupAndReplace for D3D9.dll; a mid-install crash
    // leaves Luma bytes + game original bak. Recovery must restore the original.
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("D3D9.dll"), b"luma-dgvoodoo").expect("write shadowing dll");
    fs::write(dir.path().join("D3D9.dll.bak"), b"game-own-d3d9").expect("write backup");
    fs::write(dir.path().join("renderpilot-luma-install.lock"), b"").expect("write sentinel");

    recover_torn_install(&[dir.path()]);

    assert_eq!(read(&dir.path().join("D3D9.dll")), b"game-own-d3d9");
    assert!(!dir.path().join("D3D9.dll.bak").exists());
    assert!(!engine::is_install_torn(dir.path(), AddonKind::Luma));
}

#[test]
fn recover_torn_install_restores_d3d9_on_game_dir_with_split_payload_debris() {
    let game = tempdir().expect("game dir");
    let addon = tempdir().expect("addon dir");
    fs::write(game.path().join("renderpilot-luma-install.lock"), b"").expect("write sentinel");
    fs::write(game.path().join("D3D9.dll"), b"luma-dgvoodoo").expect("write shadowing dll");
    fs::write(game.path().join("D3D9.dll.bak"), b"game-own-d3d9").expect("write backup");
    fs::write(addon.path().join("Luma-Game.addon"), b"half-written").expect("write debris");

    recover_torn_install(&[game.path(), addon.path()]);

    assert_eq!(read(&game.path().join("D3D9.dll")), b"game-own-d3d9");
    assert!(!game.path().join("D3D9.dll.bak").exists());
    assert!(!addon.path().join("Luma-Game.addon").exists());
    assert!(!engine::is_install_torn(game.path(), AddonKind::Luma));
}

#[test]
fn recover_torn_install_never_restores_unknown_bak_siblings() {
    // Allowlist-only: an unrelated foo.dll.bak must not be promoted over foo.dll.
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("foo.dll"), b"current").expect("write");
    fs::write(dir.path().join("foo.dll.bak"), b"other").expect("write bak");
    fs::write(dir.path().join("renderpilot-luma-install.lock"), b"").expect("write sentinel");

    recover_torn_install(&[dir.path()]);

    assert_eq!(read(&dir.path().join("foo.dll")), b"current");
    assert_eq!(read(&dir.path().join("foo.dll.bak")), b"other");
}

#[test]
fn recover_torn_install_leaves_non_reshade_proxy_slots_alone() {
    // A plain stub dxgi.dll without ReShade PE identity must survive recovery.
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("dxgi.dll"), b"not-a-reshade-pe").expect("write");
    fs::write(dir.path().join("renderpilot-luma-install.lock"), b"").expect("write sentinel");

    recover_torn_install(&[dir.path()]);

    assert_eq!(read(&dir.path().join("dxgi.dll")), b"not-a-reshade-pe");
}

#[test]
fn recover_torn_install_removes_a_lone_addon_bak_and_clears_the_sentinel() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("Luma-Game.addon.bak"), b"half-written").expect("write");
    fs::write(dir.path().join("renderpilot-luma-install.lock"), b"").expect("write sentinel");

    recover_torn_install(&[dir.path()]);

    assert!(!dir.path().join("Luma-Game.addon.bak").exists());
    assert!(!engine::is_install_torn(dir.path(), AddonKind::Luma));
    assert!(!crate::addons::tool::unmanaged_files_present(
        dir.path(),
        AddonKind::Luma
    ));
}

#[test]
fn recover_torn_install_is_a_no_op_on_an_already_clean_folder() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("game.exe"), b"game").expect("write");

    recover_torn_install(&[dir.path()]);

    assert!(dir.path().join("game.exe").exists());
    assert!(!engine::is_install_torn(dir.path(), AddonKind::Luma));
}
