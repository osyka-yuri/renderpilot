use renderpilot_application::{GameRepository, InstalledAddonRepository};
use renderpilot_domain::{
    GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
};
use tempfile::tempdir;

use super::*;

#[test]
fn legacy_proxy_owner_uses_canonical_install_root_for_external_classification() {
    let db = tempdir().expect("database root");
    let game_root = tempdir().expect("game root");
    let external_addons = tempdir().expect("external AddonPath");
    let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:external-proxy-root").expect("game id");
    let game = GameInstallation::new(
        GameIdentity::new(game_id.clone(), "External Proxy Root", Launcher::Manual)
            .expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(game_root.path().to_string_lossy()).expect("game root path"),
    );
    context.storage().upsert_game(&game).expect("store game");

    let external_record = InstalledAddon::new(
        game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new(
            external_addons
                .path()
                .join("renodx-game.addon64")
                .to_string_lossy(),
        )
        .expect("external payload path"),
    );
    context
        .storage()
        .upsert_installed_addon(&external_record)
        .expect("store legacy owner");
    let persisted_external_record = context
        .storage()
        .get_installed_addon(&game_id)
        .expect("reload persisted owner")
        .expect("owner exists");

    let owner = resolve_inactive_external_proxy_owner(
        &context,
        &game_id,
        Path::new(game.install_path().as_str()),
    )
    .expect("resolve legacy external owner")
    .expect("legacy None host is treated as Proxy");
    assert_eq!(owner.record, persisted_external_record);

    let nested_executable_root = game_root.path().join("Binaries").join("Win64");
    std::fs::create_dir_all(&nested_executable_root).expect("nested executable root");
    let game_local_record = InstalledAddon::new(
        game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new(
            game_root
                .path()
                .join("AddonPath")
                .join("renodx-game.addon64")
                .to_string_lossy(),
        )
        .expect("game-local payload path"),
    );
    context
        .storage()
        .upsert_installed_addon(&game_local_record)
        .expect("replace stored owner");

    assert!(
        resolve_inactive_external_proxy_owner(
            &context,
            &game_id,
            Path::new(game.install_path().as_str()),
        )
        .expect("resolve against canonical install root")
        .is_none(),
        "a nested executable directory must not make a game-local AddonPath external"
    );
    assert!(
        resolve_inactive_external_proxy_owner(&context, &game_id, &nested_executable_root)
            .expect("illustrate the old exe-parent boundary")
            .is_some(),
        "the fixture distinguishes the installation root from a nested exe parent"
    );
}
