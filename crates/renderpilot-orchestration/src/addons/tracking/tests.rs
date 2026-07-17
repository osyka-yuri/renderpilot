use renderpilot_domain::{
    AddonKind, GameId, InstalledAddonHostKind, ManagedAddonFile, ManagedFileBaseline, PathRef,
    Sha256Hash, TrackedSource, TrackedSourceRole,
};

use super::*;

fn game_id() -> GameId {
    GameId::new("steam:42").expect("id")
}

fn path(value: &str) -> PathRef {
    PathRef::new(value).expect("path")
}

fn base_renodx() -> InstalledAddon {
    InstalledAddon::from_parts(
        game_id(),
        AddonKind::RenoDx,
        path(r"C:\Games\Test\renodx-test.addon64"),
        Some("1.0".to_owned()),
        vec![
            path(r"C:\Games\Test\renodx-test.addon64"),
            path(r"C:\Games\Test\dxgi.dll"),
        ],
        Vec::new(),
        vec![TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            "https://example/a",
            None,
            "d",
        )],
    )
    .expect("record")
    .with_host_kind(InstalledAddonHostKind::Proxy)
    .with_reshade_channel("stable".to_owned())
    .with_registered_exe_path(path(r"C:\Games\Test\game.exe"))
    .with_timestamps(Some(10), Some(20))
}

#[test]
fn effective_addon_dated_prefers_tracked_when_no_file_or_unreasonable_mtime() {
    let rec = InstalledAddon::new(
        game_id(),
        AddonKind::RenoDx,
        path(r"C:\nonexistent\renodx-test.addon64"),
    )
    .with_tracked_source(
        TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            "https://example/a",
            None,
            "d",
        )
        .with_last_modified(Some("Wed, 18 Jun 2026 12:00:00 GMT".to_owned())),
    );

    // No on-disk file ->' falls back to tracked
    assert_eq!(
        effective_addon_dated(&rec),
        Some("Wed, 18 Jun 2026 12:00:00 GMT".to_owned())
    );
}

#[test]
fn host_proxy_path_finds_in_created_or_backed_up() {
    let rec = InstalledAddon::from_parts(
        game_id(),
        AddonKind::Luma,
        path(r"C:\Games\Test\Luma-Test.addon"),
        None,
        vec![
            path(r"C:\Games\Test\Luma-Test.addon"),
            path(r"C:\Games\Test\dxgi.dll"),
        ],
        Vec::new(),
        Vec::new(),
    )
    .expect("record");

    assert_eq!(
        host_proxy_path(&rec).as_deref(),
        Some(Path::new("C:/Games/Test/dxgi.dll"))
    );
}

#[test]
fn rebuild_renodx_preserves_metadata_and_version() {
    let source = base_renodx();
    let rebuilt = rebuild_install_record(
        &source,
        RebuildParts {
            addon_file: source.addon_file().clone(),
            addon_version: AddonVersionUpdate::Keep,
            managed_files: ManagedFilesUpdate::Keep,
            created_files: source.created_files().to_vec(),
            backed_up_files: source.backed_up_files().to_vec(),
            tracked_sources: source.tracked_sources().to_vec(),
            label: "renodx test".to_owned(),
        },
        PreserveMetadata::renodx(),
    )
    .expect("rebuild");

    assert_eq!(rebuilt.addon_version(), Some("1.0"));
    assert_eq!(rebuilt.host_kind(), Some(InstalledAddonHostKind::Proxy));
    assert_eq!(rebuilt.reshade_channel(), Some("stable"));
    assert_eq!(
        rebuilt.registered_exe_path().map(PathRef::as_str),
        Some("C:/Games/Test/game.exe")
    );
    assert_eq!(rebuilt.installed_at(), Some(10));
    assert_eq!(rebuilt.updated_at(), Some(20));
}

#[test]
fn rebuild_luma_forces_proxy_and_sets_version() {
    let source = InstalledAddon::from_parts(
        game_id(),
        AddonKind::Luma,
        path(r"C:\Games\Test\Luma-Old.addon"),
        Some("Build 1".to_owned()),
        vec![path(r"C:\Games\Test\Luma-Old.addon")],
        Vec::new(),
        Vec::new(),
    )
    .expect("record")
    .with_reshade_channel("nightly".to_owned())
    .with_timestamps(Some(1), Some(2));

    let new_addon = path(r"C:\Games\Test\Luma-New.addon");
    let rebuilt = rebuild_install_record(
        &source,
        RebuildParts {
            addon_file: new_addon.clone(),
            addon_version: AddonVersionUpdate::Set(Some("Build 99".to_owned())),
            managed_files: ManagedFilesUpdate::Keep,
            created_files: vec![new_addon],
            backed_up_files: Vec::new(),
            tracked_sources: Vec::new(),
            label: "luma test".to_owned(),
        },
        PreserveMetadata::luma(),
    )
    .expect("rebuild");

    assert_eq!(rebuilt.addon_version(), Some("Build 99"));
    assert_eq!(rebuilt.host_kind(), Some(InstalledAddonHostKind::Proxy));
    assert_eq!(rebuilt.reshade_channel(), Some("nightly"));
    assert!(rebuilt.registered_exe_path().is_none());
    assert_eq!(rebuilt.installed_at(), Some(1));
    assert_eq!(rebuilt.updated_at(), Some(2));
}

#[test]
fn rebuild_luma_drops_legacy_shared_host_executable_metadata() {
    let source = InstalledAddon::from_parts(
        game_id(),
        AddonKind::Luma,
        path(r"C:\Games\Test\Luma-Old.addon"),
        Some("Build 1".to_owned()),
        vec![path(r"C:\Games\Test\Luma-Old.addon")],
        Vec::new(),
        Vec::new(),
    )
    .expect("record")
    .with_registered_exe_path(path(r"C:\Games\Test\Game.exe"));

    let new_addon = path(r"C:\Games\Test\Luma-New.addon");
    let rebuilt = rebuild_install_record(
        &source,
        RebuildParts {
            addon_file: new_addon.clone(),
            addon_version: AddonVersionUpdate::Keep,
            managed_files: ManagedFilesUpdate::Keep,
            created_files: vec![new_addon],
            backed_up_files: Vec::new(),
            tracked_sources: Vec::new(),
            label: "luma metadata cleanup".to_owned(),
        },
        PreserveMetadata::luma(),
    )
    .expect("rebuild");

    assert!(rebuilt.registered_exe_path().is_none());
}

#[test]
fn rebuild_keeps_managed_files_by_default() {
    let binding = ManagedAddonFile::owned(
        path(r"C:\Games\Test\nvngx_dlss.dll"),
        ManagedFileBaseline::Absent,
        Sha256Hash::new("a".repeat(64)).expect("hash"),
    );
    let source = InstalledAddon::from_parts(
        game_id(),
        AddonKind::Luma,
        path(r"C:\Games\Test\Luma-Test.addon"),
        None,
        vec![path(r"C:\Games\Test\Luma-Test.addon")],
        Vec::new(),
        Vec::new(),
    )
    .expect("record")
    .try_with_managed_files(vec![binding.clone()])
    .expect("managed");

    let rebuilt = rebuild_install_record(
        &source,
        RebuildParts {
            addon_file: source.addon_file().clone(),
            addon_version: AddonVersionUpdate::Keep,
            managed_files: ManagedFilesUpdate::Keep,
            created_files: source.created_files().to_vec(),
            backed_up_files: Vec::new(),
            tracked_sources: Vec::new(),
            label: "keep managed".to_owned(),
        },
        PreserveMetadata::luma(),
    )
    .expect("rebuild");

    assert_eq!(rebuilt.managed_files(), std::slice::from_ref(&binding));
}

#[test]
fn rebuild_can_replace_managed_files() {
    let old = ManagedAddonFile::owned(
        path(r"C:\Games\Test\old_dlss.dll"),
        ManagedFileBaseline::Absent,
        Sha256Hash::new("a".repeat(64)).expect("hash"),
    );
    let next = ManagedAddonFile::owned(
        path(r"C:\Games\Test\new_dlss.dll"),
        ManagedFileBaseline::Absent,
        Sha256Hash::new("b".repeat(64)).expect("hash"),
    );
    let source = InstalledAddon::from_parts(
        game_id(),
        AddonKind::Luma,
        path(r"C:\Games\Test\Luma-Test.addon"),
        None,
        vec![path(r"C:\Games\Test\Luma-Test.addon")],
        Vec::new(),
        Vec::new(),
    )
    .expect("record")
    .try_with_managed_files(vec![old])
    .expect("managed");

    let rebuilt = rebuild_install_record(
        &source,
        RebuildParts {
            addon_file: source.addon_file().clone(),
            addon_version: AddonVersionUpdate::Keep,
            managed_files: ManagedFilesUpdate::Replace(vec![next.clone()]),
            created_files: source.created_files().to_vec(),
            backed_up_files: Vec::new(),
            tracked_sources: Vec::new(),
            label: "replace managed".to_owned(),
        },
        PreserveMetadata::luma(),
    )
    .expect("rebuild");

    assert_eq!(rebuilt.managed_files(), std::slice::from_ref(&next));
}

#[test]
fn rebuild_rejects_missing_addon_file_in_created_list() {
    let source = base_renodx();
    let err = rebuild_install_record(
        &source,
        RebuildParts {
            addon_file: path(r"C:\Games\Test\missing.addon64"),
            addon_version: AddonVersionUpdate::Keep,
            managed_files: ManagedFilesUpdate::Keep,
            created_files: vec![path(r"C:\Games\Test\dxgi.dll")],
            backed_up_files: Vec::new(),
            tracked_sources: Vec::new(),
            label: "invariant test".to_owned(),
        },
        PreserveMetadata::renodx(),
    )
    .expect_err("must fail when addon_file not in created_files");

    let msg = err.to_string();
    assert!(
        msg.contains("invariant test"),
        "error should include label, got: {msg}"
    );
}
