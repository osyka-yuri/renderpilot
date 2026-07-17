use super::*;
use crate::addons::renodx::test_support::{self, MACHINE_AMD64, PE32_PLUS_MAGIC};
use renderpilot_domain::InstalledAddonHostKind;
use tempfile::tempdir;

fn context() -> (tempfile::TempDir, Context) {
    let dir = tempdir().expect("tempdir");
    let context = Context::open_at(dir.path().join("catalog.sqlite")).expect("context");
    (dir, context)
}

/// Test helper: acquire the game lock then run the production locked path.
fn adopt_orphaned(
    context: &Context,
    candidate: &OrphanedInstall,
) -> Result<Option<InstalledAddon>, ServiceError> {
    let _guard =
        crate::game_mutation_lock::try_lock(&candidate.game_id).expect("test game lock available");
    reconcile_orphaned_install_locked(context, candidate)
}

fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent");
    }
    std::fs::write(path, bytes).expect("write file");
}

fn full_reshade_host() -> Vec<u8> {
    test_support::build_pe_with_exports(
        MACHINE_AMD64,
        PE32_PLUS_MAGIC,
        &[
            "ReShadeVersion",
            "ReShadeRegisterAddon",
            "ReShadeUnregisterAddon",
            "ReShadeRegisterEvent",
            "ReShadeUnregisterEvent",
        ],
    )
}

/// A distinct game ID per test -- the per-game `game_mutation_lock` is a global
/// `static`, so tests sharing one ID would contend for the same lock when
/// `cargo test` runs them in parallel.
fn game_id(appid: &str) -> GameId {
    GameId::new(format!("steam:{appid}")).expect("game id")
}

fn proxy_candidate(game_dir: &Path, appid: &str) -> OrphanedInstall {
    OrphanedInstall {
        game_id: game_id(appid),
        game_dir: game_dir.to_path_buf(),
        addon_file: game_dir.join("renodx-cp2077.addon64"),
        host_file: Some(game_dir.join("dxgi.dll")),
        host_kind: InstalledAddonHostKind::Proxy,
        registered_exe_path: None,
        reshade_config: test_support::reshade_sources(),
        game_arch: None,
        addon_url: None,
    }
}

fn created_names(record: &InstalledAddon) -> Vec<String> {
    record
        .created_files()
        .iter()
        .filter_map(PathRef::file_name)
        .map(str::to_owned)
        .collect()
}

fn backed_names(record: &InstalledAddon) -> Vec<String> {
    record
        .backed_up_files()
        .iter()
        .filter_map(PathRef::file_name)
        .map(str::to_owned)
        .collect()
}

#[test]
fn proxy_adoption_rereads_timestamps_and_claims_minimal_renderpilot_files() {
    let (_db_dir, context) = context();
    let game_dir = tempdir().expect("game dir");
    write_file(&game_dir.path().join("renodx-cp2077.addon64"), b"addon");
    write_file(&game_dir.path().join("dxgi.dll"), &full_reshade_host());
    write_file(&game_dir.path().join("dxgi.dll.bak"), b"original");
    write_file(
        &game_dir.path().join("ReShade.ini"),
        b"[ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\n",
    );
    write_file(&game_dir.path().join("ReShade.ini.bak"), b"original ini");

    let record = adopt_orphaned(&context, &proxy_candidate(game_dir.path(), "1938090"))
        .expect("adopt")
        .expect("adopted record");

    assert_eq!(record.host_kind(), Some(InstalledAddonHostKind::Proxy));
    assert_eq!(record.addon_version(), None);
    assert!(record.installed_at().is_some());
    assert!(record.updated_at().is_some());
    assert_eq!(
        created_names(&record),
        vec!["renodx-cp2077.addon64", "dxgi.dll", "ReShade.ini"]
    );
    assert!(backed_names(&record).is_empty());
}

#[tokio::test]
async fn orphan_adoption_does_not_block_the_async_runtime() {
    // Regression: adoption used to take a blocking game lock that panics
    // from a tokio context. `load_availability` is async, so adoption must
    // stay non-blocking when exercised from an async test.
    let (_db_dir, context) = context();
    let game_dir = tempdir().expect("game dir");
    write_file(&game_dir.path().join("renodx-cp2077.addon64"), b"addon");
    write_file(&game_dir.path().join("dxgi.dll"), b"reshade");

    let record = adopt_orphaned(&context, &proxy_candidate(game_dir.path(), "2050650"))
        .expect("adopting from an async context must not panic")
        .expect("adopted record");

    assert_eq!(record.host_kind(), Some(InstalledAddonHostKind::Proxy));
}

#[test]
fn proxy_adoption_keeps_user_effect_hosts_read_only() {
    let (_db_dir, context) = context();
    let game_dir = tempdir().expect("game dir");
    write_file(&game_dir.path().join("renodx-cp2077.addon64"), b"addon");
    write_file(&game_dir.path().join("dxgi.dll"), b"reshade");
    write_file(
        &game_dir.path().join("ReShade.ini"),
        b"[ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\n",
    );
    write_file(
        &game_dir
            .path()
            .join("reshade-shaders")
            .join("Shaders")
            .join("User.fx"),
        b"technique User {}",
    );

    let record = adopt_orphaned(&context, &proxy_candidate(game_dir.path(), "1145360"))
        .expect("adopt")
        .expect("adopted record");

    assert_eq!(record.host_kind(), Some(InstalledAddonHostKind::Proxy));
    assert_eq!(created_names(&record), vec!["renodx-cp2077.addon64"]);
    assert!(record.backed_up_files().is_empty());
}

#[test]
fn proxy_adoption_keeps_foreign_addon_hosts_read_only() {
    let (_db_dir, context) = context();
    let game_dir = tempdir().expect("game dir");
    write_file(&game_dir.path().join("renodx-cp2077.addon64"), b"addon");
    write_file(&game_dir.path().join("dxgi.dll"), &full_reshade_host());
    write_file(&game_dir.path().join("foreign.addon64"), b"foreign");

    let record = adopt_orphaned(&context, &proxy_candidate(game_dir.path(), "1145361"))
        .expect("adopt")
        .expect("adopted record");

    assert_eq!(created_names(&record), vec!["renodx-cp2077.addon64"]);
    assert!(host_source(&record).is_none());
}

#[test]
fn proxy_adoption_without_a_detected_host_keeps_only_the_addon_payload() {
    let (_db_dir, context) = context();
    let game_dir = tempdir().expect("game dir");
    let addon = game_dir.path().join("renodx-cp2077.addon64");
    write_file(&addon, b"addon");

    let record = adopt_orphaned(
        &context,
        &OrphanedInstall {
            game_id: game_id("1145362"),
            game_dir: game_dir.path().to_path_buf(),
            addon_file: addon,
            host_file: None,
            host_kind: InstalledAddonHostKind::Proxy,
            registered_exe_path: None,
            reshade_config: test_support::reshade_sources(),
            game_arch: None,
            addon_url: None,
        },
    )
    .expect("adopt")
    .expect("adopted record");

    assert_eq!(created_names(&record), vec!["renodx-cp2077.addon64"]);
    assert!(record.tracked_sources().is_empty());
}

#[test]
fn vulkan_adoption_records_registered_exe_without_claiming_shared_layer() {
    let (_db_dir, context) = context();
    let game_dir = tempdir().expect("game dir");
    let layer_dir = tempdir().expect("layer dir");
    let exe = game_dir.path().join("Game.exe");
    write_file(&game_dir.path().join("renodx-cp2077.addon64"), b"addon");
    write_file(&exe, b"exe");
    write_file(&layer_dir.path().join("ReShade64.dll"), b"reshade");

    let record = adopt_orphaned(
        &context,
        &OrphanedInstall {
            game_id: game_id("1817070"),
            game_dir: game_dir.path().to_path_buf(),
            addon_file: game_dir.path().join("renodx-cp2077.addon64"),
            host_file: Some(layer_dir.path().join("ReShade64.dll")),
            host_kind: InstalledAddonHostKind::SharedVulkanLayer,
            registered_exe_path: Some(exe.clone()),
            reshade_config: test_support::reshade_sources(),
            game_arch: None,
            addon_url: None,
        },
    )
    .expect("adopt")
    .expect("adopted record");

    assert_eq!(
        record.host_kind(),
        Some(InstalledAddonHostKind::SharedVulkanLayer)
    );
    assert_eq!(
        record
            .registered_exe_path()
            .map(PathRef::as_str)
            .map(str::to_owned),
        Some(exe.to_string_lossy().replace('\\', "/"))
    );
    assert_eq!(created_names(&record), vec!["renodx-cp2077.addon64"]);
    assert!(record.backed_up_files().is_empty());
}

fn base_record(host_kind: InstalledAddonHostKind) -> InstalledAddon {
    InstalledAddon::new(
        game_id("1091500"),
        AddonKind::RenoDx,
        PathRef::new("C:/Games/Test/renodx-test.addon64").expect("addon path"),
    )
    .with_host_kind(host_kind)
}

fn host_source(record: &InstalledAddon) -> Option<&TrackedSource> {
    record
        .tracked_sources()
        .iter()
        .find(|s| s.role() == TrackedSourceRole::HostBinary)
}

#[test]
fn attach_advisory_provenance_records_channel_and_host_source_for_proxy_pe_host() {
    let dir = tempdir().expect("tempdir");
    let host_file = dir.path().join("dxgi.dll");
    write_file(
        &host_file,
        &test_support::build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    );
    let candidate = OrphanedInstall {
        game_id: game_id("1091500"),
        game_dir: dir.path().to_path_buf(),
        addon_file: dir.path().join("renodx-test.addon64"),
        host_file: Some(host_file),
        host_kind: InstalledAddonHostKind::Proxy,
        registered_exe_path: None,
        reshade_config: test_support::reshade_sources(),
        game_arch: None,
        addon_url: None,
    };

    let record =
        attach_advisory_provenance(base_record(InstalledAddonHostKind::Proxy), &candidate, true);

    assert_eq!(record.reshade_channel(), Some("stable"));
    let host = host_source(&record).expect("advisory host source recorded");
    assert!(host.is_advisory());
    assert_eq!(host.channel(), Some("stable"));
}

#[test]
fn attach_advisory_provenance_skips_channel_and_host_source_for_a_recognized_custom_build() {
    let dir = tempdir().expect("tempdir");
    let host_file = dir.path().join("dxgi.dll");
    write_file(
        &host_file,
        &test_support::build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    );
    // GShade's real runtime sitting next to the proxy stub is the reliable
    // signal -- adoption must never guess a channel or track this as ours.
    write_file(&dir.path().join("GShade64.dll"), b"gshade-runtime");
    let candidate = OrphanedInstall {
        game_id: game_id("1091500"),
        game_dir: dir.path().to_path_buf(),
        addon_file: dir.path().join("renodx-test.addon64"),
        host_file: Some(host_file),
        host_kind: InstalledAddonHostKind::Proxy,
        registered_exe_path: None,
        reshade_config: test_support::reshade_sources(),
        game_arch: None,
        addon_url: None,
    };

    let record =
        attach_advisory_provenance(base_record(InstalledAddonHostKind::Proxy), &candidate, true);

    assert_eq!(record.reshade_channel(), None);
    assert!(host_source(&record).is_none());
}

#[test]
fn attach_advisory_provenance_skips_provenance_when_pe_inspection_fails() {
    let dir = tempdir().expect("tempdir");
    // Never written to disk, so `inspect_pe` cannot read it.
    let host_file = dir.path().join("dxgi.dll");
    let candidate = OrphanedInstall {
        game_id: game_id("1091500"),
        game_dir: dir.path().to_path_buf(),
        addon_file: dir.path().join("renodx-test.addon64"),
        host_file: Some(host_file),
        host_kind: InstalledAddonHostKind::Proxy,
        registered_exe_path: None,
        reshade_config: test_support::reshade_sources(),
        game_arch: None,
        addon_url: None,
    };

    let record =
        attach_advisory_provenance(base_record(InstalledAddonHostKind::Proxy), &candidate, true);

    assert_eq!(record.reshade_channel(), None);
    assert!(host_source(&record).is_none());
}

#[test]
fn attach_advisory_provenance_for_vulkan_host_records_channel_but_no_host_source() {
    let dir = tempdir().expect("tempdir");
    let host_file = dir.path().join("ReShade64.dll");
    write_file(
        &host_file,
        &test_support::build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    );
    let candidate = OrphanedInstall {
        game_id: game_id("1091500"),
        game_dir: dir.path().to_path_buf(),
        addon_file: dir.path().join("renodx-test.addon64"),
        host_file: Some(host_file),
        host_kind: InstalledAddonHostKind::SharedVulkanLayer,
        registered_exe_path: None,
        reshade_config: test_support::reshade_sources(),
        game_arch: None,
        addon_url: None,
    };

    let record = attach_advisory_provenance(
        base_record(InstalledAddonHostKind::SharedVulkanLayer),
        &candidate,
        false,
    );

    assert_eq!(record.reshade_channel(), Some("stable"));
    assert!(host_source(&record).is_none());
}

#[test]
fn attach_advisory_provenance_records_advisory_addon_source_when_addon_url_present() {
    let dir = tempdir().expect("tempdir");
    let host_file = dir.path().join("dxgi.dll");
    let addon_file = dir.path().join("renodx-test.addon64");
    write_file(&host_file, b"reshade");
    write_file(&addon_file, b"addon-bytes");
    let candidate = OrphanedInstall {
        game_id: game_id("1091500"),
        game_dir: dir.path().to_path_buf(),
        addon_file,
        host_file: Some(host_file),
        host_kind: InstalledAddonHostKind::Proxy,
        registered_exe_path: None,
        reshade_config: test_support::reshade_sources(),
        game_arch: None,
        addon_url: Some("https://example.com/renodx-test.addon64".to_owned()),
    };

    let record =
        attach_advisory_provenance(base_record(InstalledAddonHostKind::Proxy), &candidate, true);

    let addon = record
        .tracked_sources()
        .iter()
        .find(|s| s.role() == TrackedSourceRole::AddonPayload)
        .expect("advisory addon source recorded");
    assert!(addon.is_advisory());
    assert_eq!(addon.url(), "https://example.com/renodx-test.addon64");
}

fn dlss_fix_source(record: &InstalledAddon) -> Option<&TrackedSource> {
    record
        .tracked_sources()
        .iter()
        .find(|s| s.role() == TrackedSourceRole::DlssFix)
}

#[test]
fn attach_advisory_provenance_records_dlss_fix_source_when_companion_present_proxy() {
    let dir = tempdir().expect("tempdir");
    let host_file = dir.path().join("dxgi.dll");
    let addon_file = dir.path().join("renodx-test.addon64");
    write_file(&host_file, b"reshade");
    write_file(&addon_file, b"addon-bytes");
    write_file(&dir.path().join("renodx-dlssfix.addon64"), b"dlssfix-bytes");
    let candidate = OrphanedInstall {
        game_id: game_id("1091500"),
        game_dir: dir.path().to_path_buf(),
        addon_file,
        host_file: Some(host_file),
        host_kind: InstalledAddonHostKind::Proxy,
        registered_exe_path: None,
        reshade_config: test_support::reshade_sources(),
        game_arch: Some(Architecture::X64),
        addon_url: Some("https://example.com/renodx-test.addon64".to_owned()),
    };

    let record =
        attach_advisory_provenance(base_record(InstalledAddonHostKind::Proxy), &candidate, true);

    let dlss_fix = dlss_fix_source(&record).expect("advisory dlss-fix source recorded");
    assert!(dlss_fix.is_advisory());
    assert_eq!(
        dlss_fix.url(),
        "https://clshortfuse.github.io/renodx/renodx-dlssfix.addon64"
    );
    // Digest must come from the DLSS-Fix file, not the main addon.
    assert_ne!(
        dlss_fix.digest(),
        record
            .tracked_sources()
            .iter()
            .find(|s| s.role() == TrackedSourceRole::AddonPayload)
            .expect("addon source recorded")
            .digest()
    );
}

#[test]
fn attach_advisory_provenance_records_dlss_fix_source_when_companion_present_vulkan() {
    let dir = tempdir().expect("tempdir");
    let host_file = dir.path().join("ReShade64.dll");
    let addon_file = dir.path().join("renodx-test.addon64");
    write_file(
        &host_file,
        &test_support::build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    );
    write_file(&addon_file, b"addon-bytes");
    write_file(&dir.path().join("renodx-dlssfix.addon64"), b"dlssfix-bytes");
    let candidate = OrphanedInstall {
        game_id: game_id("1091500"),
        game_dir: dir.path().to_path_buf(),
        addon_file,
        host_file: Some(host_file),
        host_kind: InstalledAddonHostKind::SharedVulkanLayer,
        registered_exe_path: None,
        reshade_config: test_support::reshade_sources(),
        game_arch: Some(Architecture::X64),
        addon_url: Some("https://example.com/renodx-test.addon64".to_owned()),
    };

    let record = attach_advisory_provenance(
        base_record(InstalledAddonHostKind::SharedVulkanLayer),
        &candidate,
        false,
    );

    // Proves DLSS-Fix attribution is NOT gated by host kind, unlike HostBinary (Proxy-only).
    let dlss_fix = dlss_fix_source(&record).expect("advisory dlss-fix source recorded");
    assert!(dlss_fix.is_advisory());
}

#[test]
fn attach_advisory_provenance_skips_dlss_fix_source_when_companion_absent() {
    let dir = tempdir().expect("tempdir");
    let host_file = dir.path().join("dxgi.dll");
    let addon_file = dir.path().join("renodx-test.addon64");
    write_file(&host_file, b"reshade");
    write_file(&addon_file, b"addon-bytes");
    // No renodx-dlssfix.addon64 written.
    let candidate = OrphanedInstall {
        game_id: game_id("1091500"),
        game_dir: dir.path().to_path_buf(),
        addon_file,
        host_file: Some(host_file),
        host_kind: InstalledAddonHostKind::Proxy,
        registered_exe_path: None,
        reshade_config: test_support::reshade_sources(),
        game_arch: Some(Architecture::X64),
        addon_url: Some("https://example.com/renodx-test.addon64".to_owned()),
    };

    let record =
        attach_advisory_provenance(base_record(InstalledAddonHostKind::Proxy), &candidate, true);

    assert!(dlss_fix_source(&record).is_none());
}
