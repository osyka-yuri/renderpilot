use super::*;

use renderpilot_domain::{
    EngineConfigJournal, EngineConfigReceipt, TrackedSource, TrackedSourceRole,
};

fn stable_engine_journal(path: &str) -> EngineConfigJournal {
    EngineConfigJournal {
        stable: Some(EngineConfigReceipt {
            schema_version: 1,
            path: path.to_owned(),
            file_created: false,
            encoding: "utf8".to_owned(),
            before_digest: "0".repeat(64),
            after_digest: "1".repeat(64),
            recipe_fingerprint: "2".repeat(64),
            contributions: Vec::new(),
            created_headers: Vec::new(),
            created_header_prefixes: Vec::new(),
            created_header_groups: Vec::new(),
            created_header_ordinals: Vec::new(),
        }),
        pending: None,
    }
}

fn seed_engine_journal(storage: &SqliteStorage, game_id: &GameId, journal: &EngineConfigJournal) {
    storage
        .connection
        .lock()
        .expect("connection")
        .execute(
            "INSERT INTO game_engine_config_journals
                (game_id, addon_kind, journal_json, created_at, updated_at)
             VALUES (?1, 'luma', ?2, 1, 1)",
            rusqlite::params![
                game_id.as_str(),
                serde_json::to_string(journal).expect("journal json")
            ],
        )
        .expect("seed canonical Engine.ini owner");
}

fn commit_expected_replacement(
    storage: &SqliteStorage,
    game_id: &GameId,
    expected: &InstalledAddon,
    replacement: &InstalledAddon,
    mutation_id: Option<&str>,
) -> renderpilot_application::AppResult<()> {
    storage.commit_game_mutation(GameMutationCommit {
        game_id,
        component_set: None,
        baseline_mutations: &[],
        addon: InstalledAddonMutation::ReplaceExpected {
            expected,
            replacement,
        },
        mutation_id,
    })
}

#[test]
fn commit_game_mutation_rolls_back_addon_when_mutation_mark_fails() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = GameId::new("steam:mutation-atomic").expect("id");
    let addon = InstalledAddon::new(
        game_id.clone(),
        AddonKind::Luma,
        PathRef::new(r"C:\Games\Test\Luma-Game.addon").expect("path"),
    );

    storage
        .commit_game_mutation(GameMutationCommit {
            game_id: &game_id,
            component_set: None,
            baseline_mutations: &[],
            addon: InstalledAddonMutation::Upsert(&addon),
            mutation_id: Some("missing-tx"),
        })
        .expect_err("missing mutation id must fail the whole commit");

    assert!(
        storage
            .get_installed_addon(&game_id)
            .expect("query")
            .is_none(),
        "addon upsert must roll back when mutation mark fails"
    );
}

#[test]
fn commit_game_mutation_marks_prepared_mutation_with_addon_upsert() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = GameId::new("steam:mutation-ok").expect("id");
    storage
        .upsert_game(&test_game(game_id.clone()))
        .expect("store game");
    let addon = InstalledAddon::new(
        game_id.clone(),
        AddonKind::Luma,
        PathRef::new(r"C:\Games\Test\Luma-Game.addon").expect("path"),
    );
    storage
        .prepare_file_mutation(&PendingFileMutationRow {
            id: "tx-ok".to_owned(),
            game_id: game_id.clone(),
            feature: renderpilot_domain::mutation_features::LUMA_UPDATE.to_owned(),
            subject_id: None,
            state: PendingFileMutationState::Preparing,
            manifest_json: r#"{"snapshots":[]}"#.to_owned(),
        })
        .expect("prepare");
    storage
        .finish_preparing_file_mutation("tx-ok", r#"{"snapshots":[]}"#)
        .expect("finish prepare");

    storage
        .commit_game_mutation(GameMutationCommit {
            game_id: &game_id,
            component_set: Some(&[]),
            baseline_mutations: &[],
            addon: InstalledAddonMutation::Upsert(&addon),
            mutation_id: Some("tx-ok"),
        })
        .expect("commit");

    assert!(
        storage
            .get_installed_addon(&game_id)
            .expect("query")
            .is_some()
    );
    assert_eq!(
        storage
            .get_pending_file_mutation("tx-ok")
            .expect("get")
            .expect("row")
            .state,
        PendingFileMutationState::Committed
    );
    assert_eq!(
        storage.catalog_readiness(&game_id).expect("readiness"),
        CatalogReadiness::Invalidated {
            authority_epoch: 1,
            reason: "prepared_file_mutation".to_owned(),
            mutation_token: Some("tx-ok".to_owned()),
        }
    );
}

#[test]
fn metadata_only_mutation_leaves_a_complete_authority_unchanged() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = GameId::new("steam:metadata-only").expect("id");
    let game = test_game(game_id.clone());
    complete_game_scan(&storage, &game);

    storage
        .commit_game_mutation(GameMutationCommit {
            game_id: &game_id,
            component_set: None,
            baseline_mutations: &[],
            addon: InstalledAddonMutation::Keep,
            mutation_id: None,
        })
        .expect("metadata-only mutation");

    let CatalogReadiness::Complete(ready) = storage.catalog_readiness(&game_id).expect("readiness")
    else {
        panic!("metadata-only mutation must preserve Complete authority");
    };
    assert_eq!(ready.game_id(), &game_id);
    assert_eq!(ready.authority_epoch(), 1);
}

#[test]
fn component_set_without_file_mutation_invalidates_authority() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = GameId::new("steam:component-set").expect("id");
    let game = test_game(game_id.clone());
    complete_game_scan(&storage, &game);

    storage
        .commit_game_mutation(GameMutationCommit {
            game_id: &game_id,
            component_set: Some(&[]),
            baseline_mutations: &[],
            addon: InstalledAddonMutation::Keep,
            mutation_id: None,
        })
        .expect("component mutation");

    assert_eq!(
        storage.catalog_readiness(&game_id).expect("readiness"),
        CatalogReadiness::Invalidated {
            authority_epoch: 2,
            reason: "game_mutation_component_set".to_owned(),
            mutation_token: None,
        }
    );
}

#[test]
fn mutation_id_requires_same_game_prepared_row_with_matching_invalidation() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = GameId::new("steam:mutation-conditions").expect("id");
    let other_game_id = GameId::new("steam:mutation-other-game").expect("id");
    let game = test_game(game_id.clone());
    let other_game = test_game(other_game_id.clone());
    complete_game_scan(&storage, &game);
    complete_game_scan(&storage, &other_game);
    prepare_mutation(&storage, &other_game_id, "tx-other-game");

    storage
        .commit_game_mutation(GameMutationCommit {
            game_id: &game_id,
            component_set: Some(&[]),
            baseline_mutations: &[],
            addon: InstalledAddonMutation::Keep,
            mutation_id: Some("tx-other-game"),
        })
        .expect_err("a different game's prepared mutation cannot commit");

    storage
        .prepare_file_mutation(&PendingFileMutationRow {
            id: "tx-without-invalidation".to_owned(),
            game_id: game_id.clone(),
            feature: renderpilot_domain::mutation_features::LUMA_UPDATE.to_owned(),
            subject_id: None,
            state: PendingFileMutationState::Prepared,
            manifest_json: r#"{"snapshots":[]}"#.to_owned(),
        })
        .expect("fixture prepared row");
    storage
        .commit_game_mutation(GameMutationCommit {
            game_id: &game_id,
            component_set: None,
            baseline_mutations: &[],
            addon: InstalledAddonMutation::Keep,
            mutation_id: Some("tx-without-invalidation"),
        })
        .expect_err("prepared state without matching invalidation cannot commit");
    let CatalogReadiness::Complete(ready) = storage.catalog_readiness(&game_id).expect("readiness")
    else {
        panic!("rejected mutation must preserve Complete authority");
    };
    assert_eq!(ready.game_id(), &game_id);
    assert_eq!(ready.authority_epoch(), 1);
}

#[test]
fn expected_owner_replacement_commits_same_and_different_kind_without_touching_engine_owner() {
    for (suffix, replacement_kind) in [
        ("same-kind", AddonKind::Luma),
        ("cross-kind", AddonKind::RenoDx),
    ] {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game_id = GameId::new(format!("manual:replace-{suffix}")).expect("id");
        let game = test_game(game_id.clone());
        storage.upsert_game(&game).expect("game");
        let old = InstalledAddon::new(
            game_id.clone(),
            AddonKind::Luma,
            PathRef::new(format!("D:/External/{suffix}-old.addon")).expect("external old payload"),
        )
        .with_host_kind(renderpilot_domain::InstalledAddonHostKind::Proxy)
        .with_created_file(
            PathRef::new(format!("{}/dxgi.dll", game.install_path().as_str())).expect("old proxy"),
        )
        .with_tracked_source(TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://example.invalid/reshade.zip",
            None,
            "host-digest",
        ));
        storage.upsert_installed_addon(&old).expect("old owner");
        let journal = stable_engine_journal(&format!("C:/Users/Shared/{suffix}-Engine.ini"));
        seed_engine_journal(&storage, &game_id, &journal);

        storage
            .mark_installation_absent(&game, AuthorityCas::new(0))
            .expect("mark absent");
        let absent_readiness = storage
            .catalog_readiness(&game_id)
            .expect("absence readiness");
        storage
            .collect_absent_installation(
                &game,
                AuthorityCas::new(absent_readiness.authority_epoch()),
            )
            .expect("strip local proxy while retaining external owner");
        let after_collection = storage
            .catalog_readiness(&game_id)
            .expect("collection authority");
        storage
            .save_complete_scan_write_unit(CompleteScanWriteUnit {
                game: &game,
                components: &[],
                artifacts: &[],
                observations: &[],
                authority: AuthorityCas::new(after_collection.authority_epoch()),
                prune_empty_operations: false,
            })
            .expect("reactivate on complete scan before explicit install");
        assert!(
            !storage
                .is_installation_absent(&game_id)
                .expect("active after reappearance")
        );
        assert!(
            storage
                .get_installed_addon(&game_id)
                .expect("retained external file owner")
                .is_some()
        );
        let expected = storage
            .get_installed_addon(&game_id)
            .expect("post-reactivation owner read")
            .expect("persisted external owner");
        prepare_mutation(&storage, &game_id, "replace-owner");
        let replacement = InstalledAddon::new(
            game_id.clone(),
            replacement_kind,
            PathRef::new(format!("D:/External/{suffix}.addon")).expect("new payload"),
        );

        commit_expected_replacement(
            &storage,
            &game_id,
            &expected,
            &replacement,
            Some("replace-owner"),
        )
        .expect("prepared exact owner replacement");

        let current = storage
            .get_installed_addon(&game_id)
            .expect("replacement read")
            .expect("replacement owner");
        assert_eq!(current.kind(), replacement_kind);
        assert_eq!(current.addon_file(), replacement.addon_file());
        let journal_owner = storage
            .engine_config_journal_owner(&game_id)
            .expect("canonical Engine owner")
            .expect("Engine owner survives file-row replacement");
        assert_eq!(journal_owner.kind, AddonKind::Luma);
        assert_eq!(journal_owner.journal, journal);
        assert_eq!(
            storage
                .get_pending_file_mutation("replace-owner")
                .expect("pending lookup")
                .expect("row")
                .state,
            PendingFileMutationState::Committed
        );
    }
}

#[test]
fn expected_owner_replacement_rejects_missing_fence_and_game_mismatch() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = GameId::new("manual:replace-guards").expect("id");
    let other_id = GameId::new("manual:replace-guards-other").expect("other id");
    let game = test_game(game_id.clone());
    let other_game = test_game(other_id.clone());
    storage
        .upsert_games(&[game.clone(), other_game])
        .expect("games");
    let old = InstalledAddon::new(
        game_id.clone(),
        AddonKind::Luma,
        PathRef::new("C:/Games/replace-guards/Luma.addon").expect("old payload"),
    );
    storage.upsert_installed_addon(&old).expect("old owner");
    let journal = stable_engine_journal("C:/Games/replace-guards/Engine.ini");
    seed_engine_journal(&storage, &game_id, &journal);
    let expected = storage
        .get_installed_addon(&game_id)
        .expect("owner read")
        .expect("persisted owner");
    let replacement = InstalledAddon::new(
        game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new("D:/External/replacement.addon64").expect("new payload"),
    );
    storage
        .mark_installation_absent(&game, AuthorityCas::new(0))
        .expect("mark absent");

    commit_expected_replacement(&storage, &game_id, &expected, &replacement, None)
        .expect_err("no prepared file mutation is not an owner replacement fence");
    assert_eq!(
        storage
            .get_installed_addon(&game_id)
            .expect("old owner remains"),
        Some(expected.clone())
    );

    let wrong_game_owner = InstalledAddon::new(
        other_id.clone(),
        AddonKind::Luma,
        PathRef::new("C:/Games/other/Luma.addon").expect("other payload"),
    );
    let wrong_game_replacement = InstalledAddon::new(
        other_id,
        AddonKind::RenoDx,
        PathRef::new("D:/External/other.addon64").expect("other replacement"),
    );
    commit_expected_replacement(
        &storage,
        &game_id,
        &wrong_game_owner,
        &wrong_game_replacement,
        Some("unused-mutation"),
    )
    .expect_err("both typed records must belong to the commit game");
    assert_eq!(
        storage
            .get_installed_addon(&game_id)
            .expect("owner preserved"),
        Some(expected)
    );
    assert_eq!(
        storage
            .engine_config_journal_owner(&game_id)
            .expect("Engine owner read")
            .expect("Engine owner preserved")
            .journal,
        journal
    );
}

#[test]
fn expected_owner_replacement_rejects_exact_record_and_kind_drift() {
    for (suffix, drift_sql) in [
        (
            "record-drift",
            "UPDATE installed_addons SET addon_version = 'changed' WHERE game_id = ?1",
        ),
        (
            "kind-drift",
            "UPDATE installed_addons SET kind = 'renodx' WHERE game_id = ?1",
        ),
    ] {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game_id = GameId::new(format!("manual:{suffix}")).expect("id");
        let game = test_game(game_id.clone());
        storage.upsert_game(&game).expect("game");
        let old = InstalledAddon::new(
            game_id.clone(),
            AddonKind::Luma,
            PathRef::new(format!("{}/Luma.addon", game.install_path().as_str()))
                .expect("old payload"),
        );
        storage.upsert_installed_addon(&old).expect("old owner");
        let journal =
            stable_engine_journal(&format!("{}/Engine.ini", game.install_path().as_str()));
        seed_engine_journal(&storage, &game_id, &journal);
        let expected = storage
            .get_installed_addon(&game_id)
            .expect("owner read")
            .expect("persisted owner");
        storage
            .mark_installation_absent(&game, AuthorityCas::new(0))
            .expect("mark absent");
        prepare_mutation(&storage, &game_id, "replace-after-drift");
        let replacement = InstalledAddon::new(
            game_id.clone(),
            AddonKind::RenoDx,
            PathRef::new("D:/External/replacement.addon64").expect("new payload"),
        );
        storage
            .connection
            .lock()
            .expect("connection")
            .execute(drift_sql, [game_id.as_str()])
            .expect("introduce exact owner drift after preparation");

        commit_expected_replacement(
            &storage,
            &game_id,
            &expected,
            &replacement,
            Some("replace-after-drift"),
        )
        .expect_err("prepared replacement must re-read the exact current kind and record");
        let current = storage
            .get_installed_addon(&game_id)
            .expect("current owner read")
            .expect("drifted owner is preserved");
        assert_eq!(
            current.kind(),
            if suffix == "kind-drift" {
                AddonKind::RenoDx
            } else {
                AddonKind::Luma
            }
        );
        assert_eq!(
            storage
                .engine_config_journal_owner(&game_id)
                .expect("Engine owner read")
                .expect("Engine owner preserved")
                .journal,
            journal
        );
    }
}

#[test]
fn expected_owner_replacement_rolls_back_file_owner_and_engine_journal_on_catalog_failure() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = GameId::new("manual:replace-rollback").expect("id");
    let game = test_game(game_id.clone());
    storage.upsert_game(&game).expect("game");
    let old = InstalledAddon::new(
        game_id.clone(),
        AddonKind::Luma,
        PathRef::new("C:/Games/replace-rollback/Luma.addon").expect("old payload"),
    );
    storage.upsert_installed_addon(&old).expect("old owner");
    let journal = stable_engine_journal("C:/Games/replace-rollback/Engine.ini");
    seed_engine_journal(&storage, &game_id, &journal);
    let expected = storage
        .get_installed_addon(&game_id)
        .expect("owner read")
        .expect("persisted owner");
    storage
        .mark_installation_absent(&game, AuthorityCas::new(0))
        .expect("mark absent");
    prepare_mutation(&storage, &game_id, "replace-rollback");
    let replacement = InstalledAddon::new(
        game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new("D:/External/replacement.addon64").expect("new payload"),
    );
    storage
        .connection
        .lock()
        .expect("connection")
        .execute_batch(
            "CREATE TRIGGER reject_replacement
             BEFORE INSERT ON installed_addons
             WHEN NEW.kind = 'renodx'
             BEGIN SELECT RAISE(ABORT, 'fixture rejects replacement row'); END;",
        )
        .expect("install in-memory failure trigger");

    commit_expected_replacement(
        &storage,
        &game_id,
        &expected,
        &replacement,
        Some("replace-rollback"),
    )
    .expect_err("catalog row failure rolls back the expected owner delete");

    assert_eq!(
        storage
            .get_installed_addon(&game_id)
            .expect("old owner restored"),
        Some(expected)
    );
    assert_eq!(
        storage
            .engine_config_journal_owner(&game_id)
            .expect("Engine owner read")
            .expect("Engine owner remains")
            .journal,
        journal
    );
    assert_eq!(
        storage
            .get_pending_file_mutation("replace-rollback")
            .expect("pending lookup")
            .expect("prepared row remains")
            .state,
        PendingFileMutationState::Prepared
    );
}
