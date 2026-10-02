//! Shared producer-backed receipts for local cleanup regressions.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use renderpilot_application::{GameRepository, InstalledAddonRepository};
use renderpilot_domain::{
    AddonKind, ComponentFile, ComponentId, ComponentRollbackBaseline, GameId, GameIdentity,
    GameInstallation, GameRuntime, Launcher, PathRef, Platform, RootAuthority, Sha256Hash,
    TrackedSourceRole,
};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use crate::{
    Context,
    addons::{
        renodx::{install, test_support, types::renodx_ini_defaults},
        reshade::{proxy::HostKind, types::ReshadeChannel},
    },
};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

pub(crate) struct NormalRenoDxFixture {
    _temp: TempDir,
    pub(crate) context: Context,
    pub(crate) game: GameInstallation,
    pub(crate) root: PathBuf,
    /// The eight supported native files, in receipt categories rather than scan order.
    pub(crate) files: Vec<PathBuf>,
}

pub(crate) fn normal_renodx_fixture() -> NormalRenoDxFixture {
    let temp = tempfile::tempdir().expect("fixture directory");
    let root = temp.path().join("Game");
    fs::create_dir_all(&root).expect("game root");
    let game_id = GameId::new(format!(
        "manual:claim-provenance-{}-{}",
        std::process::id(),
        NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
    ))
    .expect("game id");
    let game = GameInstallation::new(
        GameIdentity::new(game_id.clone(), "Claim provenance test", Launcher::Manual)
            .expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        path_ref(&root),
    )
    .with_root_authority(RootAuthority::UserConfirmed)
    .with_confirmed_executable(path_ref(&root.join("ExampleGame.exe")));
    let context = Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    context
        .storage()
        .upsert_game(&game)
        .expect("persist game registration");

    let addon_bytes = test_support::build_pe_with_exports(
        test_support::MACHINE_AMD64,
        test_support::PE32_PLUS_MAGIC,
        &[],
    );
    let host_bytes = test_support::build_pe_with_exports(
        test_support::MACHINE_AMD64,
        test_support::PE32_PLUS_MAGIC,
        &[
            "ReShadeVersion",
            "ReShadeRegisterAddon",
            "ReShadeUnregisterAddon",
            "ReShadeRegisterEvent",
            "ReShadeUnregisterEvent",
        ],
    );
    let addon_digest = sha256(&addon_bytes);
    let host_digest = sha256(&host_bytes);
    let prepared = install::PreparedInstall {
        game_id: game_id.clone(),
        host_kind: HostKind::Proxy,
        proxy_dll_name: "dxgi.dll".to_owned(),
        addon_file_name: "renodx-claim-provenance.addon64".to_owned(),
        addon_source_url: "https://example.invalid/renodx.addon64".to_owned(),
        source_digest: addon_digest.as_str().to_owned(),
        source_etag: None,
        source_last_modified: None,
        addon_bytes,
        reshade_dll_bytes: host_bytes,
        reshade_source_url: "https://example.invalid/reshade.exe".to_owned(),
        reshade_source_etag: None,
        reshade_last_modified: None,
        reshade_digest: host_digest.as_str().to_owned(),
        reshade_channel: Some(ReshadeChannel::Stable),
        processing_path: crate::addons::renodx::types::RenoDxProcessingPath::Unmanaged,
        renodx_config: None,
        ini_tweaks: renodx_ini_defaults(),
    };
    let (record, commit) = install::install(&root, &prepared).expect("ordinary RenoDX install");
    assert_eq!(record.kind(), AddonKind::RenoDx);
    assert_eq!(record.created_files().len(), 3);
    assert!(record.managed_files().is_empty());
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("persist ordinary install receipt");
    commit.finish_committed();

    // The component swap archives three exact vendor originals while placing
    // two renamed active files; the same-name third replacement is now absent.
    let originals = [
        ("ogg_vs2010_x64_rwdi.dll", b"vendor ogg original".as_slice()),
        (
            "vorbis_vs2010_x64_rwdi.dll",
            b"vendor vorbis original".as_slice(),
        ),
        (
            "vorbisfile_vs2010_x64_rwdi.dll",
            b"vendor vorbisfile original".as_slice(),
        ),
    ];
    let expected = [
        ("ogg.dll", b"replacement ogg".as_slice()),
        ("vorbis.dll", b"replacement vorbis".as_slice()),
        (
            "vorbisfile_vs2010_x64_rwdi.dll",
            b"replacement vorbisfile".as_slice(),
        ),
    ];
    let mut files = record
        .created_files()
        .iter()
        .map(|path| PathBuf::from(path.as_str()))
        .collect::<Vec<_>>();
    let mut baseline_files = Vec::new();
    for (name, bytes) in originals {
        let original = root.join(name);
        let sidecar = crate::fs::backup_path(&original).expect("sidecar path");
        fs::write(&sidecar, bytes).expect("vendor original sidecar");
        files.push(sidecar);
        baseline_files.push(ComponentFile::new(path_ref(&original)).with_sha256(sha256(bytes)));
    }
    let mut expected_files = Vec::new();
    for (index, (name, bytes)) in expected.into_iter().enumerate() {
        let path = root.join(name);
        expected_files.push(ComponentFile::new(path_ref(&path)).with_sha256(sha256(bytes)));
        if index < 2 {
            fs::write(&path, bytes).expect("active component replacement");
            files.push(path);
        }
    }
    let component_id = ComponentId::new(format!(
        "component:{}:xiph_vorbis:{}",
        game_id.as_str(),
        root.to_string_lossy().replace('\\', "/")
    ))
    .expect("component id");
    let baseline =
        ComponentRollbackBaseline::new(baseline_files).with_expected_active_files(expected_files);
    context
        .storage()
        .recover_component_rollback_baseline(game.id(), &component_id, &baseline)
        .expect("persist completed component baseline");

    assert_eq!(files.len(), 8);
    let reloaded = context
        .storage()
        .get_installed_addon(game.id())
        .expect("reload")
        .expect("canonical RenoDX receipt");
    assert_eq!(reloaded.created_files().len(), 3);
    assert!(reloaded.managed_files().is_empty());
    assert!(reloaded.tracked_sources().iter().any(|source| {
        source.role() == TrackedSourceRole::AddonPayload
            && !source.is_advisory()
            && source.digest() == addon_digest.as_str()
    }));
    assert!(reloaded.tracked_sources().iter().any(|source| {
        source.role() == TrackedSourceRole::HostBinary
            && !source.is_advisory()
            && source.digest() == host_digest.as_str()
    }));
    NormalRenoDxFixture {
        _temp: temp,
        context,
        game,
        root,
        files,
    }
}

pub(crate) fn path_ref(path: &Path) -> PathRef {
    PathRef::new(path.to_string_lossy().replace('\\', "/")).expect("path")
}

pub(crate) fn sha256(bytes: &[u8]) -> Sha256Hash {
    Sha256Hash::new(hex::encode(Sha256::digest(bytes))).expect("sha256")
}

pub(crate) fn mark_absent(context: &Context, game: &GameInstallation) {
    let readiness = context
        .storage()
        .catalog_readiness(game.id())
        .expect("catalog readiness");
    context
        .storage()
        .mark_installation_absent(
            game,
            renderpilot_storage_sqlite::AuthorityCas::new(readiness.authority_epoch()),
        )
        .expect("mark absent");
}

pub(crate) fn file_snapshot(paths: &[PathBuf]) -> Vec<Vec<u8>> {
    paths
        .iter()
        .map(|path| fs::read(path).expect("snapshot fixture file"))
        .collect()
}
