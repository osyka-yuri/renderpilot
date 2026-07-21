use super::probe::{
    addon_payload_verdict, check_addon, check_dgvoodoo, check_host, elevate_addon_if_torn,
};
use super::*;
use crate::addons::engine;
use crate::addons::luma::fetch;
use crate::addons::luma::fetch::types::{LumaPayload, LumaPayloadFile};
use crate::addons::luma::test_support::manifest;
use crate::addons::luma::tracking;
use crate::addons::luma::types::{
    ExternalConfigEntry, ExternalConfigSection, LumaExternalRequirement, ManagedArchiveSource,
    ManagedInstallMapEntry,
};
use crate::addons::luma::use_cases::update_target::{
    ResolvedUpdateTarget, host_status_from_digests,
};
use crate::addons::reshade::fetch::sha256_hex;
use renderpilot_application::InstalledAddonRepository;
use renderpilot_domain::{Architecture, PathRef, TrackedSource};
use tempfile::tempdir;

#[tokio::test]
async fn check_updates_skips_a_record_belonging_to_a_different_addon_kind() {
    let db_dir = tempdir().expect("db dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let renodx_record = InstalledAddon::new(
        GameId::new("steam:1091500").expect("id"),
        AddonKind::RenoDx,
        PathRef::new(r"C:\Games\Test\renodx-test.addon64").expect("path"),
    );
    context
        .storage()
        .upsert_installed_addon(&renodx_record)
        .expect("seed renodx record");

    let results = check_updates(
        &context,
        &manifest(Vec::new()),
        &crate::addons::luma::test_support::reshade_sources(),
    )
    .await
    .expect("check_updates");

    assert!(
        results.is_empty(),
        "a RenoDX record must never be reported as a Luma update result"
    );
}

#[test]
fn unavailable_manifest_reports_unknown_for_each_installed_luma_record() {
    let db_dir = tempdir().expect("db dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    for (id, kind, file) in [
        ("steam:1", AddonKind::Luma, "Luma-One.addon"),
        ("steam:2", AddonKind::Luma, "Luma-Two.addon"),
        ("steam:3", AddonKind::RenoDx, "renodx-three.addon64"),
    ] {
        let record = InstalledAddon::new(
            GameId::new(id).expect("id"),
            kind,
            PathRef::new(format!(r"C:\Games\Test\{file}")).expect("path"),
        );
        context
            .storage()
            .upsert_installed_addon(&record)
            .expect("seed record");
    }

    let results = unknown_updates_for_installed(&context).expect("unknown updates");

    assert_eq!(results.len(), 2);
    for id in ["steam:1", "steam:2"] {
        assert!(results.contains(&(GameId::new(id).expect("id"), UpdateStatus::Unknown,)));
    }
}

#[tokio::test]
async fn check_addon_reports_available_when_tracked_payload_tree_file_is_missing() {
    let dir = tempdir().expect("tempdir");
    let addon = dir.path().join("Luma-Game.addon");
    let shader = dir.path().join("Luma/Global/Copy_PS.hlsl");
    std::fs::create_dir_all(shader.parent().expect("parent")).expect("mkdir");
    std::fs::write(&addon, b"addon").expect("write addon");
    // Shader is tracked but intentionally absent on disk.
    let record = InstalledAddon::from_parts(
        GameId::new("steam:403640").expect("id"),
        AddonKind::Luma,
        PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
        None,
        vec![
            PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
            PathRef::new(shader.to_string_lossy().into_owned()).expect("path"),
        ],
        Vec::new(),
        vec![TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            "https://example/Luma-Game.zip",
            Some("etag-match".to_owned()),
            "digest",
        )],
    )
    .expect("record");

    let db_dir = tempdir().expect("db dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let status = check_addon(&context, &record, None, false).await;
    assert_eq!(status, Some(UpdateStatus::Available));
}

#[test]
fn promote_advisory_payload_source_clears_advisory_for_check_path() {
    let payload = payload();
    let advisory = TrackedSource::new(
        TrackedSourceRole::AddonPayload,
        "https://example.test/Luma-Game.zip",
        None,
        fetch::digest::recovery_payload_digest(&payload),
    )
    .with_advisory();
    let mut sources = vec![advisory.clone()];
    tracking::promote_advisory_payload_source(&mut sources, &advisory, &payload);
    let promoted = sources
        .iter()
        .find(|s| s.role() == TrackedSourceRole::AddonPayload)
        .expect("payload source");
    assert!(!promoted.is_advisory());
    assert_eq!(promoted.digest(), "zip-digest");
}

#[tokio::test]
async fn check_host_passive_matches_recorded_digest_without_deep_download() {
    use crate::addons::luma::test_support::{
        MACHINE_AMD64, PE32_PLUS_MAGIC, build_pe_with_exports,
    };

    let dir = tempdir().expect("tempdir");
    let addon = dir.path().join("Luma-Game.addon");
    let host = dir.path().join("dxgi.dll");
    let host_bytes = build_pe_with_exports(
        MACHINE_AMD64,
        PE32_PLUS_MAGIC,
        &[
            "ReShadeVersion",
            "ReShadeRegisterAddon",
            "ReShadeUnregisterAddon",
            "ReShadeRegisterEvent",
            "ReShadeUnregisterEvent",
        ],
    );
    std::fs::write(&addon, b"addon").expect("write addon");
    std::fs::write(&host, &host_bytes).expect("write host");
    let digest = sha256_hex(&host_bytes);
    let record = InstalledAddon::from_parts(
        GameId::new("steam:403640").expect("id"),
        AddonKind::Luma,
        PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
        None,
        vec![
            PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
            PathRef::new(host.to_string_lossy().into_owned()).expect("path"),
        ],
        Vec::new(),
        vec![
            TrackedSource::new(
                TrackedSourceRole::AddonPayload,
                "https://example/Luma-Game.zip",
                None,
                "payload-digest",
            ),
            TrackedSource::new(
                TrackedSourceRole::HostBinary,
                "https://example/reshade-nightly.zip",
                None,
                digest,
            )
            .with_channel("nightly"),
        ],
    )
    .expect("record")
    .with_reshade_channel("nightly");
    let target = ResolvedUpdateTarget {
        game_dir: dir.path().to_path_buf(),
        asset: "Luma-Game.zip".to_owned(),
        addon_file: "Luma-Game.addon".to_owned(),
        arch: Architecture::X64,
        proxy_dll_name: "dxgi.dll".to_owned(),
        external_requirement: None,
    };

    let status = check_host(
        &record,
        &manifest(Vec::new()),
        &crate::addons::luma::test_support::reshade_sources(),
        Some(&target),
        /* deep */ false,
    )
    .await;

    // Fixture PE has no version resource -> under-min RepairEmpty -> Available
    // via host_status_when_validators_match (same as ETag-match path). The
    // important guarantee is we no longer return Unknown without a download.
    assert!(
        matches!(
            status,
            Some(UpdateStatus::Current) | Some(UpdateStatus::Available)
        ),
        "passive host check must use install digest without downloading nightly, got {status:?}"
    );
    assert_ne!(
        status,
        Some(UpdateStatus::Unknown),
        "matching recorded digest must not leave host as Unknown on passive probe"
    );
}

#[tokio::test]
async fn check_host_reports_available_when_owned_host_file_is_missing() {
    let dir = tempdir().expect("tempdir");
    let addon = dir.path().join("Luma-Game.addon");
    let host = dir.path().join("dxgi.dll");
    std::fs::write(&addon, b"addon").expect("write addon");
    // Host path is owned by the record but absent on disk.
    let record = InstalledAddon::from_parts(
        GameId::new("steam:403640").expect("id"),
        AddonKind::Luma,
        PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
        None,
        vec![
            PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
            PathRef::new(host.to_string_lossy().into_owned()).expect("path"),
        ],
        Vec::new(),
        vec![TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            "https://example/Luma-Game.zip",
            None,
            "digest",
        )],
    )
    .expect("record");
    let target = ResolvedUpdateTarget {
        game_dir: dir.path().to_path_buf(),
        asset: "Luma-Game.zip".to_owned(),
        addon_file: "Luma-Game.addon".to_owned(),
        arch: Architecture::X64,
        proxy_dll_name: "dxgi.dll".to_owned(),
        external_requirement: None,
    };

    let status = check_host(
        &record,
        &manifest(Vec::new()),
        &crate::addons::luma::test_support::reshade_sources(),
        Some(&target),
        false,
    )
    .await;

    assert_eq!(status, Some(UpdateStatus::Available));
}

#[tokio::test]
async fn check_update_reports_none_for_no_record() {
    let db_dir = tempdir().expect("db dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:1091500").expect("id");

    let report = check_update(
        &context,
        &manifest(Vec::new()),
        &crate::addons::luma::test_support::reshade_sources(),
        &game_id,
        false,
    )
    .await
    .expect("check_update");

    assert!(report.addon.is_none());
    assert!(report.host.is_none());
    assert!(report.dgvoodoo.is_none());
    assert_eq!(report.overall, UpdateStatus::Unknown);
}

#[tokio::test]
async fn check_update_reports_unknown_for_a_luma_record_without_payload_provenance() {
    let db_dir = tempdir().expect("db dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:1091500").expect("id");
    let malformed = InstalledAddon::new(
        game_id.clone(),
        AddonKind::Luma,
        PathRef::new(r"C:\Games\Test\Luma-Test.addon").expect("path"),
    );
    context
        .storage()
        .upsert_installed_addon(&malformed)
        .expect("seed malformed Luma record");

    let report = check_update(
        &context,
        &manifest(Vec::new()),
        &crate::addons::luma::test_support::reshade_sources(),
        &game_id,
        false,
    )
    .await
    .expect("check_update");

    assert_eq!(report.addon, Some(UpdateStatus::Unknown));
    assert!(report.host.is_none());
    assert!(report.dgvoodoo.is_none());
    assert_eq!(report.overall, UpdateStatus::Unknown);
}

fn dgvoodoo_requirement() -> LumaExternalRequirement {
    LumaExternalRequirement::Dgvoodoo2 {
        version: "2.87.3".to_owned(),
        accepted_detected_apis: vec![renderpilot_domain::GraphicsApi::D3D9],
        reshade_proxy_dll: "dxgi.dll".to_owned(),
        source: ManagedArchiveSource {
            url: "https://example.test/dgVoodoo2.zip".to_owned(),
            sha256: "archive-digest".to_owned(),
            size: 10,
        },
        install_map: vec![ManagedInstallMapEntry {
            source: "MS/x86/D3D9.dll".to_owned(),
            dest: "D3D9.dll".to_owned(),
            sha256: "dll-digest".to_owned(),
            size: 3,
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

fn target(requirement: Option<LumaExternalRequirement>) -> ResolvedUpdateTarget {
    ResolvedUpdateTarget {
        game_dir: std::path::PathBuf::from(r"C:\Games\Test"),
        asset: "Luma-Game.zip".to_owned(),
        addon_file: "Luma-Game.addon".to_owned(),
        arch: Architecture::X86,
        proxy_dll_name: "dxgi.dll".to_owned(),
        external_requirement: requirement,
    }
}

fn luma_record_with_sources(sources: Vec<TrackedSource>) -> InstalledAddon {
    InstalledAddon::from_parts(
        GameId::new("steam:49520").expect("id"),
        AddonKind::Luma,
        PathRef::new(r"C:\Games\Test\Luma-Test.addon").expect("path"),
        None,
        vec![PathRef::new(r"C:\Games\Test\Luma-Test.addon").expect("path")],
        Vec::new(),
        sources,
    )
    .expect("record")
}

fn payload() -> LumaPayload {
    LumaPayload {
        files: vec![
            LumaPayloadFile {
                relative_path: "Luma-Game.addon".to_owned(),
                bytes: b"addon".to_vec(),
            },
            LumaPayloadFile {
                relative_path: "Luma/Global/Copy_PS.hlsl".to_owned(),
                bytes: b"shader".to_vec(),
            },
        ],
        main_addon_rel: "Luma-Game.addon".to_owned(),
        zip_digest: "zip-digest".to_owned(),
        etag: None,
        last_modified: None,
        build_number: None,
    }
}

#[test]
fn advisory_payload_source_compares_extracted_payload_identity_not_zip_digest() {
    let payload = payload();
    let source = TrackedSource::new(
        TrackedSourceRole::AddonPayload,
        "https://example.test/Luma-Game.zip",
        None,
        fetch::digest::recovery_payload_digest(&payload),
    )
    .with_advisory();

    assert_eq!(
        addon_payload_verdict(&source, &payload),
        UpdateStatus::Current
    );

    let mut changed = payload;
    changed.files[1].bytes = b"changed shader".to_vec();
    assert_eq!(
        addon_payload_verdict(&source, &changed),
        UpdateStatus::Available
    );
}

#[test]
fn normal_payload_source_still_compares_zip_digest() {
    let payload = payload();
    let source = TrackedSource::new(
        TrackedSourceRole::AddonPayload,
        "https://example.test/Luma-Game.zip",
        None,
        "zip-digest",
    );

    assert_eq!(
        addon_payload_verdict(&source, &payload),
        UpdateStatus::Current
    );
}

#[test]
fn host_digest_status_matches_shared_policy() {
    use crate::addons::reshade::host_policy::HostLifecycle;

    assert_eq!(
        host_status_from_digests(HostLifecycle::AdoptEmpty, "same-digest", "same-digest"),
        UpdateStatus::Current
    );
    assert_eq!(
        host_status_from_digests(HostLifecycle::AdoptEmpty, "old-nightly", "current-nightly"),
        UpdateStatus::Available
    );
    assert_eq!(
        host_status_from_digests(HostLifecycle::RepairEmpty, "same-digest", "same-digest"),
        UpdateStatus::Available,
        "a host below Luma's minimum must be replaced even if an archive digest matched"
    );
}

#[test]
fn elevate_addon_if_torn_forces_available_when_sentinel_present() {
    let dir = tempdir().expect("tempdir");
    engine::write_sentinel(&engine::sentinel_path(dir.path(), AddonKind::Luma))
        .expect("seed sentinel");
    let resolved = ResolvedUpdateTarget {
        game_dir: dir.path().to_path_buf(),
        asset: "Luma-Game.zip".to_owned(),
        addon_file: "Luma-Game.addon".to_owned(),
        arch: Architecture::X64,
        proxy_dll_name: "dxgi.dll".to_owned(),
        external_requirement: None,
    };

    assert_eq!(
        elevate_addon_if_torn(Some(UpdateStatus::Current), Some(&resolved)),
        Some(UpdateStatus::Available),
        "a torn marker must make the install actionable even when digests match"
    );
    assert_eq!(
        elevate_addon_if_torn(None, Some(&resolved)),
        Some(UpdateStatus::Available),
    );
}

#[test]
fn elevate_addon_if_torn_leaves_verdict_when_no_sentinel() {
    let dir = tempdir().expect("tempdir");
    let resolved = ResolvedUpdateTarget {
        game_dir: dir.path().to_path_buf(),
        asset: "Luma-Game.zip".to_owned(),
        addon_file: "Luma-Game.addon".to_owned(),
        arch: Architecture::X64,
        proxy_dll_name: "dxgi.dll".to_owned(),
        external_requirement: None,
    };

    assert_eq!(
        elevate_addon_if_torn(Some(UpdateStatus::Current), Some(&resolved)),
        Some(UpdateStatus::Current)
    );
    assert_eq!(
        elevate_addon_if_torn(Some(UpdateStatus::Current), None),
        Some(UpdateStatus::Current),
        "unresolved target must not invent Available"
    );
}

#[test]
fn dgvoodoo_check_is_none_when_profile_has_no_requirement() {
    let record = luma_record_with_sources(Vec::new());

    assert_eq!(check_dgvoodoo(&record, Some(&target(None))), None);
}

#[test]
fn dgvoodoo_check_ignores_a_reused_runtime_without_a_source() {
    let record = luma_record_with_sources(Vec::new());

    assert_eq!(
        check_dgvoodoo(&record, Some(&target(Some(dgvoodoo_requirement())))),
        None
    );
}

#[test]
fn dgvoodoo_check_never_treats_a_source_as_runtime_ownership() {
    let record = luma_record_with_sources(vec![TrackedSource::new(
        TrackedSourceRole::DgVoodooWrapper,
        "https://example.test/dgVoodoo2.zip",
        None,
        "archive-digest",
    )]);

    assert_eq!(
        check_dgvoodoo(&record, Some(&target(Some(dgvoodoo_requirement())))),
        None,
        "a stale provenance row must not make a reused DLL updateable"
    );
}

#[test]
fn dgvoodoo_check_surfaces_verdict_for_db_loss_advisory_with_owned_map() {
    // After wipe recovery attaches advisory DgVoodooWrapper + map ownership.
    // Without the source, check returned None and the UI never showed freshness.
    let dir = tempdir().expect("tempdir");
    let d3d9 = dir.path().join("D3D9.dll");
    std::fs::write(&d3d9, b"not-a-real-pe").expect("write dll");
    let addon = dir.path().join("Luma-Test.addon");
    std::fs::write(&addon, b"addon").expect("write addon");
    let record = InstalledAddon::from_parts(
        GameId::new("steam:49520").expect("id"),
        AddonKind::Luma,
        PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
        None,
        vec![
            PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
            PathRef::new(d3d9.to_string_lossy().into_owned()).expect("path"),
        ],
        Vec::new(),
        vec![crate::addons::luma::dgvoodoo::advisory_wrapper_source(
            &dgvoodoo_requirement(),
        )],
    )
    .expect("record");
    let mut resolved = target(Some(dgvoodoo_requirement()));
    resolved.game_dir = dir.path().to_path_buf();

    let status = check_dgvoodoo(&record, Some(&resolved));
    assert!(
        status.is_some(),
        "recovery advisory + owned map dest must be manageable, got {status:?}"
    );
}
