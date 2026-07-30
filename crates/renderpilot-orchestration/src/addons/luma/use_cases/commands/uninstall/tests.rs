use std::path::Path;

use renderpilot_application::{ComponentRepository, GameRepository, InstalledAddonRepository};
use renderpilot_domain::{
    AddonKind, ComponentFile, ComponentId, ComponentKind, GameId, GameIdentity, GameInstallation,
    GameRuntime, InstalledAddon, Launcher, LibraryComponent, LibraryTechnology, ManagedAddonFile,
    ManagedFileBaseline, PathRef, Platform, Swappability,
};
use tempfile::tempdir;

use super::uninstall;
use crate::Context;
use crate::ServiceError;
use crate::addons::records;
fn path_ref(path: &Path) -> PathRef {
    PathRef::new(path.to_string_lossy().into_owned()).expect("path")
}

fn seed_game(context: &Context, game_id: &GameId, root: &Path) {
    let identity = GameIdentity::new(game_id.clone(), "Test", Launcher::Manual).expect("identity");
    let game = GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        path_ref(root),
    );
    context.storage().upsert_game(&game).expect("game");
}

fn seed_dlss_component(
    context: &Context,
    game_id: &GameId,
    live: &Path,
    baseline: &[ComponentFile],
) -> ComponentId {
    let component_id =
        ComponentId::new(format!("component:{}:dlss", game_id.as_str())).expect("component id");
    let current = ComponentFile::new(path_ref(live))
        .with_sha256(renderpilot_detection::sha256_file(live).expect("live hash"));
    let component = LibraryComponent::new(
        component_id.clone(),
        game_id.clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
    )
    .with_file(current);
    context
        .storage()
        .replace_components_for_game(game_id, &[component])
        .expect("component");
    context
        .storage()
        .recover_component_backup(game_id, &component_id, baseline)
        .expect("backup");
    component_id
}

#[test]
fn uninstall_reports_not_installed_for_a_renodx_record_and_leaves_it_untouched() {
    let db_dir = tempdir().expect("db dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:1091500").expect("game id");
    let renodx_record = InstalledAddon::new(
        game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new(r"C:\Games\Test\renodx-test.addon64").expect("path"),
    );
    context
        .storage()
        .upsert_installed_addon(&renodx_record)
        .expect("seed renodx record");

    let error = uninstall(&context, &game_id).expect_err("luma uninstall must be refused");
    assert!(matches!(error, ServiceError::InvalidInput(_)));

    let still_present = records::foreign_record(&context, &game_id, AddonKind::Luma)
        .expect("get")
        .expect("the renodx record must survive untouched");
    assert_eq!(still_present.kind(), AddonKind::RenoDx);
}

#[test]
fn uninstall_reports_not_installed_when_nothing_is_installed() {
    let db_dir = tempdir().expect("db dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:1091501").expect("game id");

    let error = uninstall(&context, &game_id).expect_err("nothing installed");
    assert!(matches!(error, ServiceError::InvalidInput(_)));
}

#[test]
fn uninstall_removes_files_then_deletes_the_db_row() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:1091502").expect("game id");
    let addon = game_dir.path().join("Luma-Game.addon");
    std::fs::write(&addon, b"addon").expect("write addon");
    let payload = game_dir.path().join("Luma").join("Global");
    std::fs::create_dir_all(&payload).expect("mkdir");
    let shader = payload.join("A.hlsl");
    std::fs::write(&shader, b"technique {}").expect("write shader");

    let record = InstalledAddon::from_parts(
        game_id.clone(),
        AddonKind::Luma,
        PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
        None,
        vec![
            PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
            PathRef::new(shader.to_string_lossy().into_owned()).expect("path"),
        ],
        Vec::new(),
        Vec::new(),
    )
    .expect("record");
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("seed");

    uninstall(&context, &game_id).expect("uninstall");

    assert!(!addon.exists());
    assert!(!shader.exists());
    assert!(
        records::record_of_kind(&context, &game_id, AddonKind::Luma)
            .expect("get")
            .is_none(),
        "DB row must be deleted only after successful file uninstall"
    );
}

#[test]
fn uninstall_keeps_the_db_row_when_file_removal_fails() {
    // A tracked created path that is a non-empty directory cannot be removed
    // via remove_file -- file uninstall fails. The row must stay so UI/DB agree.
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:1091503").expect("game id");
    let addon = game_dir.path().join("Luma-Game.addon");
    std::fs::write(&addon, b"addon").expect("write addon");
    let stuck = game_dir.path().join("Luma-stuck.addon");
    std::fs::create_dir_all(stuck.join("nested")).expect("mkdir stuck");
    std::fs::write(stuck.join("nested").join("x"), b"x").expect("nested file");

    let record = InstalledAddon::from_parts(
        game_id.clone(),
        AddonKind::Luma,
        PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
        None,
        vec![
            PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
            PathRef::new(stuck.to_string_lossy().into_owned()).expect("path"),
        ],
        Vec::new(),
        Vec::new(),
    )
    .expect("record");
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("seed");

    let _error = uninstall(&context, &game_id).expect_err("file removal must fail");

    let still = records::record_of_kind(&context, &game_id, AddonKind::Luma)
        .expect("get")
        .expect("row must survive file-uninstall failure");
    assert_eq!(still.kind(), AddonKind::Luma);
}

#[test]
fn uninstall_cascades_swap_and_restores_exact_owned_baseline() {
    let db = tempdir().expect("db");
    let game = tempdir().expect("game");
    let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:luma-cascade-present").expect("id");
    seed_game(&context, &game_id, game.path());
    let addon = game.path().join("Luma-Game.addon");
    let live = game.path().join("nvngx_dlss.dll");
    let sidecar = game.path().join("nvngx_dlss.dll.bak");
    std::fs::write(&addon, b"addon").expect("addon");
    std::fs::write(&live, b"catalog-overlay").expect("live");
    std::fs::write(&sidecar, b"exact-original").expect("sidecar");
    let original_hash = renderpilot_detection::sha256_file(&sidecar).expect("hash");
    let baseline = vec![ComponentFile::new(path_ref(&live)).with_sha256(original_hash.clone())];
    let component_id = seed_dlss_component(&context, &game_id, &live, &baseline);
    let binding = ManagedAddonFile::owned(
        path_ref(&live),
        ManagedFileBaseline::Present {
            sha256: original_hash,
        },
        renderpilot_detection::sha256_file(&live).expect("live hash"),
    );
    let record = InstalledAddon::new(game_id.clone(), AddonKind::Luma, path_ref(&addon))
        .try_with_managed_files(vec![binding])
        .expect("valid binding");
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("record");

    uninstall(&context, &game_id).expect("uninstall");

    assert_eq!(std::fs::read(&live).unwrap(), b"exact-original");
    assert!(!sidecar.exists());
    assert!(
        context
            .storage()
            .get_component_backup(&component_id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn uninstall_of_owned_absent_path_removes_swap_and_sidecar_claim() {
    let db = tempdir().expect("db");
    let game = tempdir().expect("game");
    let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:luma-cascade-absent").expect("id");
    seed_game(&context, &game_id, game.path());
    let addon = game.path().join("Luma-Game.addon");
    let live = game.path().join("nvngx_dlss.dll");
    std::fs::write(&addon, b"addon").expect("addon");
    std::fs::write(&live, b"catalog-overlay").expect("live");
    let component_id = seed_dlss_component(&context, &game_id, &live, &[]);
    let binding = ManagedAddonFile::owned(
        path_ref(&live),
        ManagedFileBaseline::Absent,
        renderpilot_detection::sha256_file(&live).expect("hash"),
    );
    let record = InstalledAddon::new(game_id.clone(), AddonKind::Luma, path_ref(&addon))
        .try_with_managed_files(vec![binding])
        .expect("valid binding");
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("record");

    uninstall(&context, &game_id).expect("uninstall");

    assert!(!live.exists());
    assert!(!game.path().join("nvngx_dlss.dll.bak").exists());
    assert!(
        context
            .storage()
            .get_component_backup(&component_id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn uninstall_reused_dlss_leaves_independent_library_swap_intact() {
    let db = tempdir().expect("db");
    let game = tempdir().expect("game");
    let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:luma-reused-swap").expect("id");
    seed_game(&context, &game_id, game.path());
    let addon = game.path().join("Luma-Game.addon");
    let live = game.path().join("nvngx_dlss.dll");
    let sidecar = game.path().join("nvngx_dlss.dll.bak");
    std::fs::write(&addon, b"addon").expect("addon");
    std::fs::write(&live, b"independent-library-swap").expect("live");
    std::fs::write(&sidecar, b"game-original").expect("sidecar");
    let original_hash = renderpilot_detection::sha256_file(&sidecar).expect("hash");
    let baseline = vec![ComponentFile::new(path_ref(&live)).with_sha256(original_hash)];
    let component_id = seed_dlss_component(&context, &game_id, &live, &baseline);
    let live_hash = renderpilot_detection::sha256_file(&live).expect("live hash");
    let binding = ManagedAddonFile::reused(path_ref(&live), live_hash);
    let record = InstalledAddon::new(game_id.clone(), AddonKind::Luma, path_ref(&addon))
        .try_with_managed_files(vec![binding])
        .expect("valid binding");
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("record");

    uninstall(&context, &game_id).expect("uninstall");

    assert_eq!(std::fs::read(&live).unwrap(), b"independent-library-swap");
    assert_eq!(std::fs::read(&sidecar).unwrap(), b"game-original");
    assert!(
        context
            .storage()
            .get_component_backup(&component_id)
            .unwrap()
            .is_some()
    );
}

#[test]
fn public_luma_uninstall_waits_at_the_game_mutation_boundary() {
    let context = std::sync::Arc::new(Context::from_storage(
        renderpilot_storage_sqlite::SqliteStorage::in_memory().expect("storage"),
    ));
    let game_id = GameId::new(format!("manual:luma-lock-{}", ulid::Ulid::generate())).expect("id");
    let held = crate::game_mutation_lock::blocking_lock(&game_id);
    let (attempt_tx, attempt_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    crate::game_mutation_lock::set_lock_attempt_hook(&game_id, attempt_tx);

    let worker_context = std::sync::Arc::clone(&context);
    let worker_game = game_id;
    let worker = std::thread::spawn(move || {
        done_tx
            .send(uninstall(&worker_context, &worker_game))
            .expect("report result");
    });

    attempt_rx.recv().expect("entrypoint reached lock");
    assert!(matches!(
        done_rx.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    drop(held);
    assert!(done_rx.recv().expect("completed").is_err());
    worker.join().expect("worker");
}
