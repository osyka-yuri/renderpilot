use super::*;

#[test]
fn independent_engine_owner_is_released_during_explicit_game_removal() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Managed Game");
    let engine_ini = temp.path().join("AppData").join("Engine.ini");
    std::fs::create_dir_all(&install).expect("install");
    std::fs::create_dir_all(engine_ini.parent().expect("Engine.ini parent"))
        .expect("Engine.ini directory");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("seed game");
    seed_engine_owner(&context, game.id(), AddonKind::RenoDx, &engine_ini);
    assert!(
        context
            .storage()
            .engine_config_journal_owner(game.id())
            .expect("canonical owner")
            .is_some()
    );

    remove_game_from_catalog(&context, game.id()).expect("remove with independent owner");

    assert!(
        !engine_ini.exists(),
        "owned target is released via its persisted path"
    );
    assert!(
        context
            .storage()
            .engine_config_journal_owner(game.id())
            .expect("canonical owner")
            .is_none()
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("game")
            .is_none()
    );
}

#[test]
fn pending_independent_engine_owner_is_recovered_then_released() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Managed Game");
    let engine_ini = temp.path().join("AppData").join("Engine.ini");
    std::fs::create_dir_all(&install).expect("install");
    std::fs::create_dir_all(engine_ini.parent().expect("Engine.ini parent"))
        .expect("Engine.ini directory");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("seed game");
    seed_engine_owner(&context, game.id(), AddonKind::RenoDx, &engine_ini);
    seed_pending_engine_release(&context, game.id(), AddonKind::RenoDx, &engine_ini);

    remove_game_from_catalog(&context, game.id()).expect("recover and remove");

    assert!(
        !engine_ini.exists(),
        "the stable contribution is released after recovery"
    );
    assert!(
        context
            .storage()
            .engine_config_journal_owner(game.id())
            .expect("canonical owner")
            .is_none()
    );
}

#[test]
fn independent_engine_target_collision_is_rejected_before_component_rollback() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Managed Game");
    let engine_ini = install.join("Engine.ini");
    let backup = crate::fs::backup_path(&engine_ini).expect("component backup path");
    std::fs::create_dir_all(&install).expect("install");
    std::fs::write(&backup, b"rollback pre-image").expect("backup");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("seed game");
    seed_engine_owner(&context, game.id(), AddonKind::RenoDx, &engine_ini);
    let engine_before = std::fs::read(&engine_ini).expect("Engine.ini");
    let component_id = ComponentId::new("component:engine-target-collision").expect("component id");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_ref(&engine_ini))
            .with_sha256(renderpilot_detection::sha256_bytes(&engine_before).expect("active hash")),
    );
    context
        .storage()
        .replace_components_for_game(game.id(), &[component])
        .expect("component");
    context
        .storage()
        .recover_component_rollback_baseline(
            game.id(),
            &component_id,
            &ComponentRollbackBaseline::new(vec![
                ComponentFile::new(path_ref(&engine_ini)).with_sha256(
                    renderpilot_detection::sha256_file(&backup).expect("baseline hash"),
                ),
            ]),
        )
        .expect("baseline");

    let error = remove_game_from_catalog(&context, game.id())
        .expect_err("overlapping rollback and Engine owner must be rejected");

    assert!(matches!(
        error,
        ServiceError::ManagedCleanupAmbiguous { .. }
    ));
    assert_eq!(
        std::fs::read(&engine_ini).expect("unchanged Engine.ini"),
        engine_before
    );
    assert_eq!(
        std::fs::read(&backup).expect("unchanged backup"),
        b"rollback pre-image"
    );
    assert!(
        context
            .storage()
            .engine_config_journal_owner(game.id())
            .expect("canonical owner")
            .is_some()
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("game")
            .is_some()
    );
}

#[test]
fn independent_engine_pending_stage_collision_is_rejected_before_writes() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Managed Game");
    let engine_ini = install.join("Engine.ini");
    let stage = install.join(".renderpilot-engine-managed-pending-release.stage");
    let backup = crate::fs::backup_path(&stage).expect("component backup path");
    std::fs::create_dir_all(&install).expect("install");
    std::fs::write(&backup, b"rollback pre-image").expect("backup");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("seed game");
    seed_engine_owner(&context, game.id(), AddonKind::RenoDx, &engine_ini);
    seed_pending_engine_release(&context, game.id(), AddonKind::RenoDx, &engine_ini);
    std::fs::write(&stage, b"unrecognized pending stage").expect("stage");
    let stage_before = std::fs::read(&stage).expect("stage bytes");
    let component_id = ComponentId::new("component:engine-stage-collision").expect("component id");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_ref(&stage))
            .with_sha256(renderpilot_detection::sha256_bytes(&stage_before).expect("active hash")),
    );
    context
        .storage()
        .replace_components_for_game(game.id(), &[component])
        .expect("component");
    context
        .storage()
        .recover_component_rollback_baseline(
            game.id(),
            &component_id,
            &ComponentRollbackBaseline::new(vec![
                ComponentFile::new(path_ref(&stage)).with_sha256(
                    renderpilot_detection::sha256_file(&backup).expect("baseline hash"),
                ),
            ]),
        )
        .expect("baseline");

    let error = remove_game_from_catalog(&context, game.id())
        .expect_err("pending stage cannot overlap a component rollback target");

    assert!(matches!(
        error,
        ServiceError::ManagedCleanupAmbiguous { .. }
    ));
    assert_eq!(
        std::fs::read(&stage).expect("unchanged stage"),
        stage_before
    );
    assert_eq!(
        std::fs::read(&backup).expect("unchanged backup"),
        b"rollback pre-image"
    );
    assert!(
        context
            .storage()
            .engine_config_journal_owner(game.id())
            .expect("pending owner")
            .is_some_and(|owner| owner.journal.is_pending())
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("game")
            .is_some()
    );
}

#[test]
fn different_kind_engine_owner_coexists_with_local_addon_removal() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Managed Game");
    let addon_path = install.join("renodx.addon64");
    let engine_ini = temp.path().join("AppData").join("Engine.ini");
    std::fs::create_dir_all(&install).expect("install");
    std::fs::create_dir_all(engine_ini.parent().expect("Engine.ini parent"))
        .expect("Engine.ini directory");
    std::fs::write(&addon_path, b"local RenoDX").expect("addon");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("seed game");
    seed_engine_owner(&context, game.id(), AddonKind::Luma, &engine_ini);
    context
        .storage()
        .upsert_installed_addon(&InstalledAddon::new(
            game.id().clone(),
            AddonKind::RenoDx,
            path_ref(&addon_path),
        ))
        .expect("local RenoDX");
    assert_eq!(
        context
            .storage()
            .get_installed_addon(game.id())
            .expect("local addon")
            .expect("addon")
            .kind(),
        AddonKind::RenoDx
    );

    remove_game_from_catalog(&context, game.id()).expect("remove mixed owners");

    assert!(
        !addon_path.exists(),
        "the local RenoDX installation is removed"
    );
    assert!(
        !engine_ini.exists(),
        "the independent Luma contribution is released"
    );
    assert!(
        context
            .storage()
            .engine_config_journal_owner(game.id())
            .expect("canonical owner")
            .is_none()
    );
}

#[test]
fn matching_kind_engine_owner_is_consumed_by_addon_uninstall() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Managed Game");
    let addon_path = install.join("renodx.addon64");
    let engine_ini = temp.path().join("AppData").join("Engine.ini");
    std::fs::create_dir_all(&install).expect("install");
    std::fs::create_dir_all(engine_ini.parent().expect("Engine.ini parent"))
        .expect("Engine.ini directory");
    std::fs::write(&addon_path, b"local RenoDX").expect("addon");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("seed game");
    seed_engine_owner(&context, game.id(), AddonKind::RenoDx, &engine_ini);
    context
        .storage()
        .upsert_installed_addon(&InstalledAddon::new(
            game.id().clone(),
            AddonKind::RenoDx,
            path_ref(&addon_path),
        ))
        .expect("local RenoDX");

    remove_game_from_catalog(&context, game.id()).expect("remove matching owners");

    assert!(!addon_path.exists(), "the local add-on uninstall ran");
    assert!(
        !engine_ini.exists(),
        "the matching add-on release consumed the owner"
    );
    assert!(
        context
            .storage()
            .engine_config_journal_owner(game.id())
            .expect("canonical owner")
            .is_none()
    );
}

fn seed_engine_owner(
    context: &crate::Context,
    game_id: &renderpilot_domain::GameId,
    kind: AddonKind,
    path: &std::path::Path,
) {
    let recipe = crate::addons::engine_config::EngineIniRecipe::new(
        "managed.cleanup.test",
        1,
        vec![crate::addons::engine_config::EngineIniEntry {
            section: "SystemSettings".to_owned(),
            key: "r.AllowHDR".to_owned(),
            value: "1".to_owned(),
        }],
    )
    .expect("recipe");
    let recipes =
        crate::addons::engine_config::EngineIniRecipeSet::from_recipes([&recipe]).expect("recipes");
    crate::addons::engine_config::service::apply(
        context.storage(),
        game_id,
        kind,
        path,
        &recipes,
        "managed-cleanup-test-apply",
    )
    .expect("apply Engine.ini owner");
}

fn seed_pending_engine_release(
    context: &crate::Context,
    game_id: &renderpilot_domain::GameId,
    kind: AddonKind,
    path: &std::path::Path,
) {
    let storage = context.storage();
    let raw = storage
        .engine_config_journal_token(game_id, kind)
        .expect("read token")
        .expect("stable token");
    let stable: EngineConfigJournal = serde_json::from_str(&raw).expect("stable journal");
    let receipt = stable.stable.expect("stable receipt");
    let current_bytes = std::fs::read(path).expect("Engine.ini");
    let pending = EngineConfigJournal {
        stable: Some(receipt.clone()),
        pending: Some(EngineConfigTransition {
            operation_id: "managed-pending-release".to_owned(),
            stage_name: ".renderpilot-engine-managed-pending-release.stage".to_owned(),
            prior: Some(receipt),
            after: None,
            before_digest: renderpilot_detection::sha256_bytes(&current_bytes)
                .expect("before digest")
                .as_str()
                .to_owned(),
            after_digest: renderpilot_detection::sha256_bytes(&[])
                .expect("after digest")
                .as_str()
                .to_owned(),
        }),
    };
    assert!(
        storage
            .compare_and_swap_engine_config_journal(game_id, kind, Some(&raw), Some(&pending))
            .expect("persist pending")
    );
}
