use std::fs;
use std::path::Path;

use renderpilot_domain::{
    AddonKind, GameId, InstalledAddon, InstalledAddonHostKind, PathRef, TrackedSource,
    TrackedSourceRole,
};
use tempfile::tempdir;

use super::{PreparedInstall, install, uninstall};
use crate::ServiceError;
use crate::addons::renodx::test_support::{MACHINE_AMD64, PE32_PLUS_MAGIC, build_pe_with_exports};
use crate::addons::renodx::types::renodx_ini_defaults;
use crate::addons::reshade::proxy::HostKind;
use crate::addons::reshade::types::ReshadeChannel;

fn prepared() -> PreparedInstall {
    PreparedInstall {
        game_id: GameId::new("steam:1091500").expect("id"),
        host_kind: HostKind::Proxy,
        proxy_dll_name: "dxgi.dll".to_owned(),
        addon_file_name: "renodx-cp2077.addon64".to_owned(),
        addon_source_url: "https://clshortfuse.github.io/renodx/renodx-cp2077.addon64".to_owned(),
        source_digest: "abc123".to_owned(),
        source_etag: Some("\"etag-1\"".to_owned()),
        source_last_modified: Some("Wed, 18 Jun 2026 12:00:00 GMT".to_owned()),
        addon_bytes: b"addon-bytes".to_vec(),
        reshade_dll_bytes: reshade_host_bytes(true),
        reshade_source_url: "https://nightly.link/crosire/reshade/x64.zip".to_owned(),
        reshade_source_etag: Some("\"rs-etag-1\"".to_owned()),
        reshade_last_modified: Some("Tue, 17 Jun 2026 09:00:00 GMT".to_owned()),
        reshade_digest: "reshade-digest".to_owned(),
        reshade_channel: Some(ReshadeChannel::Nightly),
        ini_tweaks: renodx_ini_defaults(),
    }
}
fn read(path: &Path) -> Vec<u8> {
    fs::read(path).expect("file should exist")
}
fn path_ref(path: &Path) -> PathRef {
    PathRef::new(path.to_string_lossy().into_owned()).expect("valid path")
}
fn write_effect_asset(game_dir: &Path) {
    let shaders = game_dir.join("reshade-shaders").join("Shaders");
    fs::create_dir_all(&shaders).expect("create shaders dir");
    fs::write(shaders.join("UserEffect.fx"), b"technique User {}").expect("write effect");
}
fn source(record: &InstalledAddon, role: TrackedSourceRole) -> TrackedSource {
    record
        .tracked_sources()
        .iter()
        .find(|s| s.role() == role)
        .cloned()
        .unwrap_or_else(|| panic!("expected a tracked source for {role:?}"))
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
#[test]
fn fresh_install_lays_down_host_addon_and_ini_without_marker() {
    let dir = tempdir().expect("tempdir");
    let record = install(dir.path(), &prepared()).expect("install");
    assert_eq!(
        read(&dir.path().join("renodx-cp2077.addon64")),
        b"addon-bytes"
    );
    assert_eq!(read(&dir.path().join("dxgi.dll")), reshade_host_bytes(true));
    assert!(dir.path().join("ReShade.ini").is_file());
    let ini = String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap();
    assert!(ini.contains("DisabledAddons=Generic Depth,Effect Runtime Sync"));
    assert!(record.has_host_binary_provenance());
    // The add-on source is tracked for updates.
    let addon = source(&record, TrackedSourceRole::AddonPayload);
    assert_eq!(
        addon.url(),
        "https://clshortfuse.github.io/renodx/renodx-cp2077.addon64"
    );
    assert_eq!(addon.digest(), "abc123");
    assert_eq!(addon.etag(), Some("\"etag-1\""));
    // A replaced or created host records its upstream entry for host update tracking.
    let host = source(&record, TrackedSourceRole::HostBinary);
    assert_eq!(host.url(), "https://nightly.link/crosire/reshade/x64.zip");
    assert_eq!(host.digest(), "reshade-digest");
    assert_eq!(host.etag(), Some("\"rs-etag-1\""));
    // addon + proxy + ini.
    assert_eq!(record.created_files().len(), 3);
    assert!(record.backed_up_files().is_empty());
}
#[test]
fn fresh_install_round_trips_to_clean_folder() {
    let dir = tempdir().expect("tempdir");
    // A pre-existing unrelated file must survive the round trip.
    fs::write(dir.path().join("game.exe"), b"game").expect("write");
    let record = install(dir.path(), &prepared()).expect("install");
    uninstall(&record, None).expect("uninstall");
    assert!(!dir.path().join("renodx-cp2077.addon64").exists());
    assert!(!dir.path().join("dxgi.dll").exists());
    assert!(!dir.path().join("ReShade.ini").exists());
    assert_eq!(read(&dir.path().join("game.exe")), b"game");
}
#[test]
fn compatible_detected_reshade_is_reused_untouched() {
    let dir = tempdir().expect("tempdir");
    // Simulate an existing compatible ReShade install with a hand-tuned config.
    let original_ini = "[GENERAL]\r\nPreset=mine.ini\r\n";
    fs::write(dir.path().join("dxgi.dll"), reshade_host_bytes(true)).expect("write");
    fs::write(dir.path().join("ReShade.ini"), original_ini).expect("write");
    write_effect_asset(dir.path());
    let record = install(dir.path(), &prepared()).expect("install");
    assert!(!record.has_host_binary_provenance());
    // No Host source is tracked for a reused detected host.
    assert!(
        record
            .tracked_sources()
            .iter()
            .all(|s| s.role() != TrackedSourceRole::HostBinary)
    );
    // Existing DLL untouched (we did not rewrite it or back it up).
    assert_eq!(read(&dir.path().join("dxgi.dll")), reshade_host_bytes(true));
    // The add-on is present; the existing ini is left byte-for-byte untouched.
    assert!(dir.path().join("renodx-cp2077.addon64").is_file());
    assert_eq!(
        String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap(),
        original_ini
    );
    // Nothing was backed up: we touched only the add-on file.
    assert!(record.backed_up_files().is_empty());
}
#[test]
fn detected_host_without_effects_gets_default_disabled_addons() {
    let dir = tempdir().expect("tempdir");
    let original_ini = "[GENERAL]\r\nNoPreset=1\r\n";
    fs::write(dir.path().join("dxgi.dll"), reshade_host_bytes(true)).expect("write");
    fs::write(dir.path().join("ReShade.ini"), original_ini).expect("write");
    let record = install(dir.path(), &prepared()).expect("install");
    assert!(!record.has_host_binary_provenance());
    assert_eq!(
        String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap(),
        "[GENERAL]\r\nNoPreset=1\r\n\r\n[ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\n"
    );
    // No `.bak` — the merge into a pre-existing ini uses `UpdateText`, not
    // `MergeText`, so it is never backed up.
    assert!(record.backed_up_files().is_empty());
    assert!(!dir.path().join("ReShade.ini.bak").exists());
    uninstall(&record, None).expect("uninstall");
    // The ini pre-dates this install (never in `created_files`), so uninstall
    // never deletes or snapshot-restores it — only RenoDX's own key is
    // stripped, leaving the user's `[GENERAL]` content exactly as it was.
    let ini = String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap();
    assert!(ini.contains("NoPreset=1"));
    assert!(!ini.contains("DisabledAddons"));
    assert_eq!(read(&dir.path().join("dxgi.dll")), reshade_host_bytes(true));
    assert!(!dir.path().join("renodx-cp2077.addon64").exists());
}
#[test]
fn reused_host_with_no_pre_existing_ini_gets_a_fresh_one_stripped_not_deleted() {
    let dir = tempdir().expect("tempdir");
    // A compatible, already-active ReShade host — reused, never rewritten —
    // but no `ReShade.ini` exists yet (e.g. installed but never launched).
    fs::write(dir.path().join("dxgi.dll"), reshade_host_bytes(true)).expect("write");
    let record = install(dir.path(), &prepared()).expect("install");
    assert!(!record.has_host_binary_provenance());
    let ini = String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap();
    assert!(ini.contains("DisabledAddons=Generic Depth,Effect Runtime Sync"));
    uninstall(&record, None).expect("uninstall");
    // The ini RenoDX created from nothing survives uninstall — stripped, not
    // deleted — because the host beside it was merely reused, never written
    // by this install; RenoDX doesn't own the whole stack here, only the
    // add-on and the keys it added to the config.
    let ini = String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap();
    assert!(!ini.contains("DisabledAddons"));
    assert_eq!(read(&dir.path().join("dxgi.dll")), reshade_host_bytes(true));
}
#[test]
fn legacy_backed_up_ini_is_stripped_not_restored_from_its_stale_snapshot() {
    let dir = tempdir().expect("tempdir");
    let addon_path = dir.path().join("renodx-cp2077.addon64");
    fs::write(&addon_path, b"addon").expect("write addon");
    // The current ini: what an old `MergeText`-based install produced, plus a
    // setting the user added by hand afterward.
    fs::write(
        dir.path().join("ReShade.ini"),
        "[GENERAL]\r\nPreset=mine.ini\r\n\r\n\
         [ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\n",
    )
    .expect("write ini");
    // The stale pre-install snapshot that install's `MergeText` backed up —
    // this must never come back, even though it's still sitting right there.
    fs::write(
        dir.path().join("ReShade.ini.bak"),
        "[GENERAL]\r\nPreset=old.ini\r\n",
    )
    .expect("write bak");
    let addon_ref = path_ref(&addon_path);
    let ini_ref = path_ref(&dir.path().join("ReShade.ini"));
    let record = InstalledAddon::from_parts(
        GameId::new("steam:1091500").expect("id"),
        AddonKind::RenoDx,
        addon_ref.clone(),
        None,
        vec![addon_ref, ini_ref.clone()],
        vec![ini_ref],
        Vec::new(),
    )
    .expect("record");
    uninstall(&record, None).expect("uninstall");
    let ini = String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap();
    assert!(ini.contains("Preset=mine.ini"));
    assert!(!ini.contains("DisabledAddons"));
    assert!(!ini.contains("Preset=old.ini"));
    // The legacy `.bak` is left exactly as it was — orphaned, never restored
    // from and never deleted either.
    assert_eq!(
        String::from_utf8(read(&dir.path().join("ReShade.ini.bak"))).unwrap(),
        "[GENERAL]\r\nPreset=old.ini\r\n"
    );
}
#[test]
fn strip_locates_an_untracked_ini_via_the_host_directory_not_the_addon_directory() {
    let dir = tempdir().expect("tempdir");
    // The add-on lives in a subfolder (a custom `[ADDON] AddonPath`); the host
    // and the pre-existing `ReShade.ini` live in the game directory itself.
    let addons_subdir = dir.path().join("addons");
    fs::create_dir_all(&addons_subdir).expect("mkdir");
    let addon_path = addons_subdir.join("renodx-cp2077.addon64");
    fs::write(&addon_path, b"addon").expect("write addon");
    let host_path = dir.path().join("dxgi.dll");
    fs::write(&host_path, reshade_host_bytes(true)).expect("write host");
    fs::write(
        dir.path().join("ReShade.ini"),
        "[GENERAL]\r\nPreset=mine.ini\r\n\r\n\
         [ADDON]\r\nAddonPath=addons\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\n",
    )
    .expect("write ini");
    let record = InstalledAddon::from_parts(
        GameId::new("steam:1091500").expect("id"),
        AddonKind::RenoDx,
        path_ref(&addon_path),
        None,
        vec![path_ref(&addon_path), path_ref(&host_path)],
        Vec::new(),
        Vec::new(),
    )
    .expect("record");
    uninstall(&record, None).expect("uninstall");
    // The ini is found via the host's own directory (not the add-on's
    // subfolder) and stripped, not left with RenoDX's key still in it.
    let ini = String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap();
    assert!(ini.contains("Preset=mine.ini"));
    assert!(!ini.contains("DisabledAddons"));
}
#[test]
fn inactive_reshade_engine_dll_refuses_second_host() {
    let dir = tempdir().expect("tempdir");
    // ReShade exists, but not in the slot this game will load.
    fs::write(dir.path().join("ReShade64.dll"), reshade_host_bytes(true)).expect("write");
    let error = install(dir.path(), &prepared()).expect_err("should refuse inactive host");
    assert!(matches!(error, ServiceError::InvalidInput(_)));
    assert!(!dir.path().join("ReShade.ini").exists());
    assert!(!dir.path().join("renodx-cp2077.addon64").exists());
}
#[test]
fn detected_host_install_leaves_original_ini_intact() {
    let dir = tempdir().expect("tempdir");
    let original_ini = "[GENERAL]\r\nPreset=mine.ini\r\n";
    fs::write(dir.path().join("dxgi.dll"), reshade_host_bytes(true)).expect("write");
    fs::write(dir.path().join("ReShade.ini"), original_ini).expect("write");
    write_effect_asset(dir.path());
    let record = install(dir.path(), &prepared()).expect("install");
    // The existing ini is never backed up or rewritten.
    assert!(record.backed_up_files().is_empty());
    assert_eq!(
        String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap(),
        original_ini
    );
    uninstall(&record, None).expect("uninstall");
    // Add-on removed, existing DLL and original ini intact.
    assert!(!dir.path().join("renodx-cp2077.addon64").exists());
    assert_eq!(read(&dir.path().join("dxgi.dll")), reshade_host_bytes(true));
    assert_eq!(
        String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap(),
        original_ini
    );
}
#[test]
fn repeated_install_is_idempotent() {
    let dir = tempdir().expect("tempdir");
    install(dir.path(), &prepared()).expect("first install");
    let record = install(dir.path(), &prepared()).expect("second install");
    assert!(
        record
            .created_files()
            .iter()
            .any(|path| { path.as_str().ends_with("renodx-cp2077.addon64") })
    );
}
#[test]
fn active_host_without_addon_support_is_replaced_with_no_backup() {
    let dir = tempdir().expect("tempdir");
    // A ReShade host occupies the active slot, but it is the build WITHOUT
    // add-on support — RenoDX's add-on cannot load there, so install must
    // replace it with the bundled add-on-capable build. Its identity was
    // already confirmed by `host_policy::assess` before this runs, so RenoDX
    // treats it as an unambiguous official ReShade build: no backup is kept.
    fs::write(dir.path().join("dxgi.dll"), reshade_host_bytes(false)).expect("write");
    let record = install(dir.path(), &prepared()).expect("install");
    // Our add-on-capable host now occupies the slot; the original is gone,
    // not backed up.
    assert_eq!(read(&dir.path().join("dxgi.dll")), reshade_host_bytes(true));
    assert!(record.has_host_binary_provenance());
    assert!(record.backed_up_files().is_empty());
    assert!(!dir.path().join("dxgi.dll.bak").exists());
    uninstall(&record, None).expect("uninstall");
    // Uninstall deletes the host we installed outright — there is no `.bak`
    // to restore the original add-on-less build from.
    assert!(!dir.path().join("dxgi.dll").exists());
    assert!(!dir.path().join("renodx-cp2077.addon64").exists());
}
#[test]
fn host_repair_requires_reshade_bytes() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("dxgi.dll"), reshade_host_bytes(false)).expect("write");
    let mut prepared = prepared();
    prepared.reshade_dll_bytes.clear();
    let error = install(dir.path(), &prepared).expect_err("repair needs bytes");
    assert!(matches!(error, ServiceError::InvalidInput(_)));
    assert_eq!(
        read(&dir.path().join("dxgi.dll")),
        reshade_host_bytes(false)
    );
    assert!(!dir.path().join("renodx-cp2077.addon64").exists());
}
#[test]
fn multiple_reshade_hosts_refuse_install_before_writes() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("dxgi.dll"), reshade_host_bytes(true)).expect("write");
    fs::write(dir.path().join("ReShade64.dll"), reshade_host_bytes(true)).expect("write");
    let error = install(dir.path(), &prepared()).expect_err("multiple hosts conflict");
    assert!(matches!(error, ServiceError::InvalidInput(_)));
    assert_eq!(read(&dir.path().join("dxgi.dll")), reshade_host_bytes(true));
    assert_eq!(
        read(&dir.path().join("ReShade64.dll")),
        reshade_host_bytes(true)
    );
    assert!(!dir.path().join("renodx-cp2077.addon64").exists());
}
#[test]
fn refuses_install_when_proxy_slot_is_occupied_by_an_unknown_file() {
    let dir = tempdir().expect("tempdir");
    // A file already occupies the proxy-DLL slot — another graphics overlay or a
    // game-shipped dxgi.dll. With no ReShade host detected, the install must
    // refuse rather than silently displace it.
    fs::write(dir.path().join("dxgi.dll"), b"another-overlay").expect("write");
    let error = install(dir.path(), &prepared()).expect_err("should refuse");
    assert!(matches!(error, ServiceError::InvalidInput(_)));
    // The occupying file is left untouched, and nothing else was laid down.
    assert_eq!(read(&dir.path().join("dxgi.dll")), b"another-overlay");
    assert!(!dir.path().join("renodx-cp2077.addon64").exists());
}
/// A Vulkan-host prepared install: no proxy DLL, no ReShade bytes (the host is
/// the shared layer handled separately by the service).
fn vulkan_prepared() -> PreparedInstall {
    PreparedInstall {
        host_kind: HostKind::Vulkan,
        proxy_dll_name: String::new(),
        reshade_dll_bytes: Vec::new(),
        ..prepared()
    }
}
#[test]
fn vulkan_install_lays_down_addon_and_ini_without_a_proxy() {
    let dir = tempdir().expect("tempdir");
    let record = install(dir.path(), &vulkan_prepared()).expect("vulkan install");
    assert_eq!(
        read(&dir.path().join("renodx-cp2077.addon64")),
        b"addon-bytes"
    );
    assert!(dir.path().join("ReShade.ini").is_file());
    // No proxy DLL is written for a Vulkan install (the host is the shared layer).
    assert!(!dir.path().join("dxgi.dll").exists());
    let ini = String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap();
    assert!(ini.contains("[ADDON]"));
    // A file install (no upstream URL is set on the addon source here) still
    // tracks the add-on when one is recorded; the host is never tracked.
    assert!(
        record
            .tracked_sources()
            .iter()
            .all(|s| s.role() != TrackedSourceRole::HostBinary)
    );
    // addon + ini.
    assert_eq!(record.created_files().len(), 2);
}
#[test]
fn vulkan_install_round_trips_to_clean_folder() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("game.exe"), b"game").expect("write");
    // In production, `use_cases::commands::install::annotate_install_record`
    // stamps `host_kind` onto the record straight after this call — `uninstall`
    // relies on it to know a Vulkan install owns its per-game ini outright.
    let record = install(dir.path(), &vulkan_prepared())
        .expect("install")
        .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer);
    uninstall(&record, None).expect("uninstall");
    assert!(!dir.path().join("renodx-cp2077.addon64").exists());
    assert!(!dir.path().join("ReShade.ini").exists());
    assert_eq!(read(&dir.path().join("game.exe")), b"game");
}
#[test]
fn vulkan_install_refuses_when_already_installed() {
    let dir = tempdir().expect("tempdir");
    install(dir.path(), &vulkan_prepared()).expect("first install");
    let error = install(dir.path(), &vulkan_prepared()).expect_err("should refuse");
    assert!(matches!(error, ServiceError::InvalidInput(_)));
}
