use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use renderpilot_application::{GameRepository, InstalledAddonRepository};
use renderpilot_detection::InstallTreeCompleteness;
use renderpilot_domain::{
    AddonKind, FileReceipt, GameId, GameIdentity, GameInstallation, GameRuntime, InstalledAddon,
    Launcher, ManagedAddonFile, ManagedFileBaseline, OptiScalerFileBaseline,
    OptiScalerModuleRuntimeBinding, PathRef, Platform, RootAuthority, Sha256Hash, Version,
};

use crate::{
    Context,
    addons::luma::install::{self, PreparedInstall},
    catalog::{
        installation_lifecycle::{coordinator, residue::test_support},
        leftovers::list_retired_game_leftovers,
    },
};

static NEXT_GAME: AtomicU64 = AtomicU64::new(0);

fn game(root: &Path) -> GameInstallation {
    let id = GameId::new(format!(
        "manual:payload-presence-{}-{}",
        std::process::id(),
        NEXT_GAME.fetch_add(1, Ordering::Relaxed)
    ))
    .expect("game id");
    GameInstallation::new(
        GameIdentity::new(id, "Payload presence test", Launcher::Manual).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        path_ref(root),
    )
    .with_root_authority(RootAuthority::UserConfirmed)
    .with_executable_candidate(PathRef::new("ExampleGame.exe").expect("candidate"))
}

fn path_ref(path: &Path) -> PathRef {
    PathRef::new(path.to_string_lossy().replace('\\', "/")).expect("path")
}

#[test]
fn edited_reshade_ini_and_log_hide_the_deleted_game_without_changing_files_or_offering_cleanup() {
    let fixture = test_support::normal_renodx_fixture();
    let ini = fixture.root.join("ReShade.ini");
    let log = fixture.root.join("ReShade.log");
    fs::write(&ini, b"[GENERAL]\r\nUserSetting=keep\r\n").expect("edited INI");
    fs::write(&log, b"user log bytes").expect("log");
    let mut watched = fixture.files.clone();
    watched.extend([ini, log]);
    let before = test_support::file_snapshot(&watched);

    let sweep = coordinator::reconcile_registered_roots(&fixture.context).expect("reconcile");

    assert_eq!(sweep.removed_game_ids, vec![fixture.game.id().clone()]);
    assert!(sweep.errors.is_empty());
    assert!(
        fixture
            .context
            .storage()
            .find_active_game(fixture.game.id())
            .expect("active game")
            .is_none()
    );
    assert!(
        fixture
            .context
            .storage()
            .get_installed_addon(fixture.game.id())
            .expect("retained add-on owner")
            .is_some()
    );
    assert_eq!(test_support::file_snapshot(&watched), before);
    assert!(
        list_retired_game_leftovers(&fixture.context)
            .expect("public leftovers refresh")
            .proposals
            .is_empty(),
        "presence-only evidence must not become cleanup authority"
    );
    assert_eq!(test_support::file_snapshot(&watched), before);
}

#[test]
fn source_less_renodx_created_paths_are_presence_only_for_a_deleted_game() {
    let temp = tempfile::tempdir().expect("fixture directory");
    let root = temp.path().join("Game");
    fs::create_dir_all(&root).expect("game root");
    let context = Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&root);
    context
        .storage()
        .upsert_game(&game)
        .expect("game registration");

    let payload = root.join("legacy-renodx.addon64");
    let host = root.join("dxgi.dll");
    let log = root.join("ReShade.log");
    fs::write(&payload, b"legacy add-on bytes").expect("payload");
    fs::write(&host, b"legacy host bytes").expect("host");
    fs::write(&log, b"legacy log bytes").expect("log");
    let addon = InstalledAddon::new(game.id().clone(), AddonKind::RenoDx, path_ref(&payload))
        .with_created_file(path_ref(&host))
        .with_created_file(path_ref(&log));
    assert!(addon.tracked_sources().is_empty());
    context
        .storage()
        .upsert_installed_addon(&addon)
        .expect("source-less RenoDX record");
    let before =
        [payload.clone(), host.clone(), log.clone()].map(|path| fs::read(path).expect("snapshot"));

    let sweep = coordinator::reconcile_registered_roots(&context).expect("reconcile");

    assert_eq!(sweep.removed_game_ids, vec![game.id().clone()]);
    assert!(sweep.errors.is_empty());
    assert!(
        context
            .storage()
            .is_installation_absent(game.id())
            .expect("absence")
    );
    assert_eq!(fs::read(&payload).expect("payload retained"), before[0]);
    assert_eq!(fs::read(&host).expect("host retained"), before[1]);
    assert_eq!(fs::read(&log).expect("log retained"), before[2]);
    assert!(
        context
            .storage()
            .get_installed_addon(game.id())
            .expect("retained legacy owner")
            .is_some()
    );
}

#[test]
fn a_missing_nested_executable_parent_falls_through_to_the_complete_payload_report() {
    let temp = tempfile::tempdir().expect("fixture directory");
    let root = temp.path().join("Game");
    fs::create_dir_all(&root).expect("game root");
    let context = Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let id = GameId::new("manual:nested-payload-candidate").expect("id");
    let candidate = PathRef::new("Project/Binaries/Win64/Game-Win64-Shipping.exe")
        .expect("nested executable candidate");
    let game = GameInstallation::new(
        GameIdentity::new(id, "Nested candidate game", Launcher::Manual).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        path_ref(&root),
    )
    .with_root_authority(RootAuthority::UserConfirmed)
    .with_executable_candidate(candidate);
    context
        .storage()
        .upsert_game(&game)
        .expect("game registration");

    let host = root.join("dxgi.dll");
    let ini = root.join("ReShade.ini");
    let log = root.join("ReShade.log");
    fs::write(&host, b"path-only RenoDX host").expect("host");
    fs::write(&ini, b"user settings").expect("INI");
    fs::write(&log, b"runtime log").expect("log");
    let addon = InstalledAddon::new(
        game.id().clone(),
        AddonKind::RenoDx,
        path_ref(&root.join("legacy.addon64")),
    )
    .with_created_file(path_ref(&host));
    context
        .storage()
        .upsert_installed_addon(&addon)
        .expect("legacy owner");

    let sweep = coordinator::reconcile_registered_roots(&context).expect("reconcile");

    assert_eq!(sweep.removed_game_ids, vec![game.id().clone()]);
    assert!(sweep.errors.is_empty());
    assert!(
        context
            .storage()
            .is_installation_absent(game.id())
            .expect("absence")
    );
    assert_eq!(
        fs::read(&host).expect("host retained"),
        b"path-only RenoDX host"
    );
    assert_eq!(fs::read(&ini).expect("INI retained"), b"user settings");
    assert_eq!(fs::read(&log).expect("log retained"), b"runtime log");
}

#[test]
fn an_unexpected_nested_executable_parent_keeps_presence_indeterminate() {
    let temp = tempfile::tempdir().expect("fixture directory");
    let root = temp.path().join("Game");
    fs::create_dir_all(&root).expect("game root");
    let context = Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let id = GameId::new("manual:blocked-nested-payload-candidate").expect("id");
    let game = GameInstallation::new(
        GameIdentity::new(id, "Blocked nested candidate game", Launcher::Manual).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        path_ref(&root),
    )
    .with_root_authority(RootAuthority::UserConfirmed)
    .with_executable_candidate(
        PathRef::new("Project/Binaries/Win64/Game-Win64-Shipping.exe")
            .expect("nested executable candidate"),
    );
    context
        .storage()
        .upsert_game(&game)
        .expect("game registration");
    fs::write(root.join("Project"), b"unexpected non-directory parent")
        .expect("blocked candidate parent");

    let sweep = coordinator::reconcile_registered_roots(&context).expect("reconcile");

    assert!(sweep.removed_game_ids.is_empty());
    assert_eq!(sweep.errors.len(), 1);
    assert!(
        !context
            .storage()
            .is_installation_absent(game.id())
            .expect("availability")
    );
    assert!(
        context
            .storage()
            .find_active_game(game.id())
            .expect("active lookup")
            .is_some()
    );
}

#[test]
fn actual_luma_install_record_does_not_turn_its_remaining_host_into_a_game() {
    let temp = tempfile::tempdir().expect("fixture directory");
    let root = temp.path().join("Game");
    fs::create_dir_all(&root).expect("game root");
    let context = Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&root);
    context
        .storage()
        .upsert_game(&game)
        .expect("game registration");
    let addon_path = root.join("Luma-Game.addon");
    fs::write(&addon_path, b"Luma archive payload").expect("remaining archive");

    let prepared = PreparedInstall {
        game_id: game.id().clone(),
        proxy_dll_name: "dxgi.dll".to_owned(),
        // The archive file is pre-positioned to focus this fixture on the real
        // install record builder and its path-only host/archive ownership.
        payload: Vec::new(),
        main_addon_rel: "Luma-Game.addon".to_owned(),
        asset_source_url: "https://example.invalid/Luma-Game.zip".to_owned(),
        zip_digest: "a".repeat(64),
        source_etag: None,
        source_last_modified: None,
        build_label: None,
        reshade_dll_bytes: b"Luma host bytes".to_vec(),
        reshade_source_url: "https://example.invalid/reshade.zip".to_owned(),
        reshade_source_etag: None,
        reshade_last_modified: None,
        reshade_digest: "b".repeat(64),
        dgvoodoo: None,
    };
    let min_host_version = Version::parse("6.7.0").expect("minimum ReShade version");
    let (addon, commit) = install::install(&context, &root, prepared, &min_host_version)
        .expect("actual Luma install path");
    commit.finish_committed();
    assert!(addon.managed_files().is_empty());
    assert!(addon.created_files().contains(&path_ref(&addon_path)));
    context
        .storage()
        .upsert_installed_addon(&addon)
        .expect("persist actual Luma record");
    let host = root.join("dxgi.dll");
    assert!(addon.created_files().contains(&path_ref(&host)));
    let before = [
        fs::read(&addon_path).expect("archive"),
        fs::read(&host).expect("host"),
    ];

    let sweep = coordinator::reconcile_registered_roots(&context).expect("reconcile");

    assert_eq!(sweep.removed_game_ids, vec![game.id().clone()]);
    assert!(sweep.errors.is_empty());
    assert!(
        context
            .storage()
            .is_installation_absent(game.id())
            .expect("absence")
    );
    assert_eq!(fs::read(&addon_path).expect("archive retained"), before[0]);
    assert_eq!(fs::read(&host).expect("host retained"), before[1]);
}

#[test]
fn actual_executable_unknown_dll_and_archive_keep_the_catalog_entry_present() {
    for (name, bytes) in [
        ("ExampleGame.exe", b"actual executable".as_slice()),
        ("Setup.exe", b"rejected helper executable".as_slice()),
        ("foreign.dll", b"unknown structural DLL".as_slice()),
        ("chunk.pak", b"unknown game archive".as_slice()),
    ] {
        let temp = tempfile::tempdir().expect("fixture directory");
        let root = temp.path().join("Game");
        fs::create_dir_all(&root).expect("game root");
        let path = root.join(name);
        fs::write(&path, bytes).expect("game payload");
        let context = Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let game = game(&root);
        context
            .storage()
            .upsert_game(&game)
            .expect("game registration");
        let before = fs::read(&path).expect("snapshot");

        let sweep = coordinator::reconcile_registered_roots(&context).expect("reconcile");

        assert!(sweep.removed_game_ids.is_empty(), "{name}");
        assert!(sweep.errors.is_empty(), "{name}");
        assert!(
            context
                .storage()
                .find_active_game(game.id())
                .expect("active lookup")
                .is_some(),
            "{name}"
        );
        assert_eq!(fs::read(&path).expect("payload retained"), before, "{name}");
    }
}

#[test]
fn empty_root_with_a_stored_candidate_and_no_owner_is_confirmed_absent() {
    let temp = tempfile::tempdir().expect("fixture directory");
    let root = temp.path().join("Game");
    fs::create_dir_all(&root).expect("game root");
    let context = Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&root);
    context
        .storage()
        .upsert_game(&game)
        .expect("game registration");

    let sweep = coordinator::reconcile_registered_roots(&context).expect("reconcile");

    assert_eq!(sweep.removed_game_ids, vec![game.id().clone()]);
    assert!(sweep.errors.is_empty());
    assert!(
        context
            .storage()
            .is_installation_absent(game.id())
            .expect("absence")
    );
}

#[cfg(windows)]
#[test]
fn a_later_directory_junction_keeps_the_game_probe_indeterminate() {
    let temp = tempfile::tempdir().expect("fixture directory");
    let root = temp.path().join("Game");
    let target = temp.path().join("junction-target");
    fs::create_dir_all(&root).expect("game root");
    fs::create_dir_all(&target).expect("junction target");
    let edited_ini = root.join("ReShade.ini");
    fs::write(&edited_ini, b"[GENERAL]\r\nUserSetting=keep\r\n").expect("edited INI");
    let target_payload = target.join("chunk.pak");
    fs::write(&target_payload, b"external target bytes").expect("target payload");
    let junction = root.join("ZGame");
    create_directory_junction(&junction, &target);
    let _junction_guard = JunctionGuard(junction.clone());

    let context = Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&root);
    context
        .storage()
        .upsert_game(&game)
        .expect("game registration");
    let addon = InstalledAddon::new(
        game.id().clone(),
        AddonKind::RenoDx,
        path_ref(&root.join("legacy-addon.addon64")),
    );
    context
        .storage()
        .upsert_installed_addon(&addon)
        .expect("retained owner row");
    let ini_before = fs::read(&edited_ini).expect("INI snapshot");
    let target_before = fs::read(&target_payload).expect("target snapshot");

    let sweep = coordinator::reconcile_registered_roots(&context).expect("reconcile");

    assert!(sweep.removed_game_ids.is_empty());
    assert_eq!(sweep.errors.len(), 1);
    assert!(
        context
            .storage()
            .find_active_game(game.id())
            .expect("active game")
            .is_some()
    );
    assert!(
        !context
            .storage()
            .is_installation_absent(game.id())
            .expect("availability")
    );
    assert!(
        context
            .storage()
            .get_installed_addon(game.id())
            .expect("owner row")
            .is_some()
    );
    assert_eq!(fs::read(&edited_ini).expect("INI retained"), ini_before);
    assert_eq!(
        fs::read(&target_payload).expect("target retained"),
        target_before
    );
    assert!(junction.exists(), "the directory junction remains present");
}

#[test]
fn unknown_payload_under_advisory_excluded_directories_keeps_the_game_present() {
    for (directory, name, bytes) in [
        (".GameData", "unknown.dll", b"unknown game DLL".as_slice()),
        (".GameData", "chunk.pak", b"unknown game archive".as_slice()),
        (
            "Development",
            "unknown.exe",
            b"unknown game executable".as_slice(),
        ),
    ] {
        let temp = tempfile::tempdir().expect("fixture directory");
        let root = temp.path().join("Game");
        let payload_dir = root.join(directory);
        fs::create_dir_all(&payload_dir).expect("payload directory");
        fs::write(
            root.join("ReShade.ini"),
            b"[GENERAL]\r\nUserSetting=keep\r\n",
        )
        .expect("unknown config");
        let payload = payload_dir.join(name);
        fs::write(&payload, bytes).expect("unknown game payload");
        let context = Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let game = game(&root);
        context
            .storage()
            .upsert_game(&game)
            .expect("game registration");
        let before = fs::read(&payload).expect("payload snapshot");

        let report =
            renderpilot_platform_windows::inspect_executable_candidates_complete_strict(&root);
        assert_eq!(report.completeness(), InstallTreeCompleteness::Complete);
        assert!(
            report.structural_files().contains(&payload)
                || report
                    .candidates()
                    .iter()
                    .any(|candidate| candidate.absolute_path == payload),
            "strict scan missed {directory}/{name}"
        );

        let sweep = coordinator::reconcile_registered_roots(&context).expect("reconcile");

        assert!(sweep.removed_game_ids.is_empty(), "{directory}/{name}");
        assert!(sweep.errors.is_empty(), "{directory}/{name}");
        assert!(
            context
                .storage()
                .find_active_game(game.id())
                .expect("active lookup")
                .is_some(),
            "{directory}/{name}"
        );
        assert_eq!(fs::read(&payload).expect("payload retained"), before);
    }
}

#[cfg(windows)]
struct JunctionGuard(std::path::PathBuf);

#[cfg(windows)]
impl Drop for JunctionGuard {
    fn drop(&mut self) {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "rmdir"])
            .arg(&self.0)
            .status();
    }
}

#[cfg(windows)]
fn create_directory_junction(junction: &Path, target: &Path) {
    let output = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(junction)
        .arg(target)
        .output()
        .expect("cmd must create a directory junction");
    assert!(
        output.status.success(),
        "junction creation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn managed_owned_present_bytes_are_ignored_only_while_the_installed_hash_matches() {
    let temp = tempfile::tempdir().expect("fixture directory");
    let root = temp.path().join("Game");
    fs::create_dir_all(&root).expect("game root");
    let file_path = root.join("dxgi.dll");
    fs::write(&file_path, b"installed managed bytes").expect("installed bytes");
    let installed_sha256 = file_sha256(&root, &file_path);
    fs::write(&file_path, b"original managed bytes").expect("original bytes");
    let original_sha256 = file_sha256(&root, &file_path);
    assert_ne!(installed_sha256, original_sha256);
    let managed = ManagedAddonFile::owned(
        path_ref(&file_path),
        ManagedFileBaseline::Present {
            sha256: original_sha256,
        },
        installed_sha256.clone(),
    );
    let mut presence = typed_presence_for_managed(&root, &managed);

    fs::write(&file_path, b"installed managed bytes").expect("installed bytes");
    assert_eq!(
        classify_with_presence(&root, &file_path, &presence),
        super::GamePayloadObservation::ConfirmedAbsent
    );

    fs::write(&file_path, b"original managed bytes").expect("returned original");
    assert_eq!(
        classify_with_presence(&root, &file_path, &presence),
        super::GamePayloadObservation::Present
    );
    fs::write(&file_path, b"user-edited managed bytes").expect("user edit");
    assert_eq!(
        classify_with_presence(&root, &file_path, &presence),
        super::GamePayloadObservation::Present
    );

    fs::write(&file_path, b"installed managed bytes").expect("installed bytes");
    let reused = ManagedAddonFile::reused(path_ref(&file_path), installed_sha256.clone());
    presence = typed_presence_for_managed(&root, &reused);
    assert_eq!(
        classify_with_presence(&root, &file_path, &presence),
        super::GamePayloadObservation::Present
    );

    let unchanged_original = ManagedAddonFile::owned(
        path_ref(&file_path),
        ManagedFileBaseline::Present {
            sha256: installed_sha256.clone(),
        },
        installed_sha256,
    );
    presence = typed_presence_for_managed(&root, &unchanged_original);
    assert_eq!(
        classify_with_presence(&root, &file_path, &presence),
        super::GamePayloadObservation::Present
    );
}

#[test]
fn optiscaler_owned_present_binding_uses_its_installed_receipt_hash() {
    let temp = tempfile::tempdir().expect("fixture directory");
    let root = temp.path().join("Game");
    fs::create_dir_all(&root).expect("game root");
    let file_path = root.join("amd_fsr3.dll");
    fs::write(&file_path, b"installed OptiScaler runtime").expect("installed runtime");
    let installed_sha256 = file_sha256(&root, &file_path);
    fs::write(&file_path, b"retained original FSR entry point").expect("original entry point");
    let original_sha256 = file_sha256(&root, &file_path);
    let binding = OptiScalerModuleRuntimeBinding {
        module: "amd-runtime".to_owned(),
        path: path_ref(&file_path),
        installed: FileReceipt::owned("installed-runtime", installed_sha256)
            .expect("installed receipt"),
        baseline: OptiScalerFileBaseline::Present {
            receipt: FileReceipt::reused("retained-fsr-entry-point", original_sha256)
                .expect("original receipt"),
        },
    };
    let mut presence = super::PresenceProjection::default();
    if let Some(digest) = super::optiscaler_binding_presence_digest(&binding) {
        super::insert_typed_presence(
            &mut presence.typed_sha256,
            &root,
            &path_ref(&root),
            &binding.path,
            digest,
        );
    }

    fs::write(&file_path, b"installed OptiScaler runtime").expect("installed runtime");
    assert_eq!(
        classify_with_presence(&root, &file_path, &presence),
        super::GamePayloadObservation::ConfirmedAbsent
    );
    fs::write(&file_path, b"retained original FSR entry point").expect("returned original");
    assert_eq!(
        classify_with_presence(&root, &file_path, &presence),
        super::GamePayloadObservation::Present
    );
}

#[cfg(windows)]
#[test]
fn absolute_windows_case_and_verbatim_aliases_are_rebased_under_the_exact_root() {
    let temp = tempfile::tempdir().expect("fixture directory");
    let root = temp.path().join("Game");
    fs::create_dir_all(&root).expect("game root");
    let file_path = root.join("dxgi.dll");
    fs::write(&file_path, b"unknown structural DLL").expect("file");
    let root_ref = path_ref(&root);
    let lowercase_path = file_path.to_string_lossy().to_ascii_lowercase();
    let case_alias = PathRef::new(lowercase_path.clone()).expect("case alias");
    let verbatim_alias = PathRef::new(format!(r"\\?\{}", lowercase_path.replace('/', "\\")))
        .expect("verbatim alias");

    assert_eq!(
        super::path_under_root(&root, &root_ref, &case_alias),
        Some(file_path.clone())
    );
    assert_eq!(
        super::path_under_root(&root, &root_ref, &verbatim_alias),
        Some(file_path)
    );
    assert_eq!(
        classify_with_presence(
            &root,
            Path::new(&lowercase_path),
            &super::PresenceProjection::default()
        ),
        super::GamePayloadObservation::Present
    );
}

fn file_sha256(root: &Path, path: &Path) -> Sha256Hash {
    let authority =
        crate::fs::VerifiedDir::open_absolute_components(root, None).expect("root authority");
    let observed = authority
        .observe_descendant(path)
        .expect("file observation")
        .expect("file exists");
    Sha256Hash::new(observed.digest.expect("file digest")).expect("valid digest")
}

fn typed_presence_for_managed(root: &Path, file: &ManagedAddonFile) -> super::PresenceProjection {
    let mut presence = super::PresenceProjection::default();
    if let Some(digest) = super::managed_file_presence_digest(file) {
        super::insert_typed_presence(
            &mut presence.typed_sha256,
            root,
            &path_ref(root),
            file.path(),
            digest,
        );
    }
    presence
}

fn classify_with_presence(
    root: &Path,
    path: &Path,
    presence: &super::PresenceProjection,
) -> super::GamePayloadObservation {
    let authority =
        crate::fs::VerifiedDir::open_absolute_components(root, None).expect("root authority");
    super::classify_structural_files(
        root,
        &path_ref(root),
        &authority,
        &[path.to_owned()],
        &Default::default(),
        presence,
        true,
    )
}
