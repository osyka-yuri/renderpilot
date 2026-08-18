use super::super::SqliteStorage;
use super::*;
use crate::error::storage_error;
use renderpilot_application::GameRepository;
use renderpilot_domain::{
    GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
};

fn store_game(storage: &SqliteStorage, game_id: GameId) {
    let identity = GameIdentity::new(game_id, "Test Game", Launcher::Steam).expect("identity");
    let game = GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new("C:/Games/Test").expect("path"),
    );
    storage.upsert_game(&game).expect("store game");
}

#[test]
fn prepared_row_round_trips_and_commits() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let row = PendingFileMutationRow {
        id: "tx-1".to_owned(),
        game_id: GameId::new("steam:1").expect("id"),
        feature: renderpilot_domain::mutation_features::CATALOG_SWAP.to_owned(),
        subject_id: Some("component:1".to_owned()),
        state: PendingFileMutationState::Prepared,
        manifest_json: r#"{"snapshots":[]}"#.to_owned(),
    };

    store_game(&storage, row.game_id.clone());

    storage.prepare_file_mutation(&row).expect("prepare");
    assert_eq!(
        storage.get_pending_file_mutation("tx-1").expect("get"),
        Some(row)
    );

    storage
        .mark_file_mutation_committed("tx-1")
        .expect("commit");
    assert_eq!(
        storage
            .get_pending_file_mutation("tx-1")
            .expect("get")
            .expect("row")
            .state,
        PendingFileMutationState::Committed
    );
}

#[test]
fn preparing_row_publishes_its_manifest_before_commit() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let mut row = PendingFileMutationRow {
        id: "tx-preparing".to_owned(),
        game_id: GameId::new("steam:2").expect("id"),
        feature: renderpilot_domain::mutation_features::CATALOG_SWAP.to_owned(),
        subject_id: None,
        state: PendingFileMutationState::Preparing,
        manifest_json:
            r#"{"format_version":1,"roots":[],"transaction_dir":"unused","snapshots":[]}"#
                .to_owned(),
    };

    store_game(&storage, row.game_id.clone());

    storage
        .begin_file_mutation_preparation(&BeginFileMutationPreparation {
            id: row.id.clone(),
            game_id: row.game_id.clone(),
            feature: row.feature.clone(),
            subject_id: row.subject_id.clone(),
            initial_manifest_json: row.manifest_json.clone(),
        })
        .expect("reserve");
    assert_eq!(
        storage
            .get_pending_file_mutation(&row.id)
            .expect("read")
            .expect("row")
            .state,
        PendingFileMutationState::Preparing,
        "begin always writes literal Preparing"
    );
    row.manifest_json =
        r#"{"format_version":1,"roots":["C:/game"],"transaction_dir":"C:/tx","snapshots":[]}"#
            .to_owned();
    storage
        .finish_preparing_file_mutation(&row.id, &row.manifest_json)
        .expect("publish");

    let stored = storage
        .get_pending_file_mutation(&row.id)
        .expect("read")
        .expect("row");
    assert_eq!(stored.state, PendingFileMutationState::Prepared);
    assert_eq!(stored.manifest_json, row.manifest_json);
    assert_eq!(
        storage.catalog_readiness(&row.game_id).expect("readiness"),
        super::super::observations::CatalogReadiness::Invalidated {
            authority_epoch: 1,
            reason: "prepared_file_mutation".to_owned(),
            mutation_token: Some(row.id),
        }
    );
}

#[test]
fn illegal_state_transitions_are_rejected() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let row = PendingFileMutationRow {
        id: "tx-illegal".to_owned(),
        game_id: GameId::new("steam:3").expect("id"),
        feature: renderpilot_domain::mutation_features::CATALOG_SWAP.to_owned(),
        subject_id: None,
        state: PendingFileMutationState::Preparing,
        manifest_json: r#"{"snapshots":[]}"#.to_owned(),
    };
    store_game(&storage, row.game_id.clone());
    storage.prepare_file_mutation(&row).expect("reserve");

    storage
        .mark_file_mutation_committed("tx-illegal")
        .expect_err("cannot commit from preparing");
    assert_eq!(
        storage
            .get_pending_file_mutation("tx-illegal")
            .expect("get")
            .expect("row")
            .state,
        PendingFileMutationState::Preparing
    );

    storage
        .finish_preparing_file_mutation("tx-illegal", r#"{"snapshots":[]}"#)
        .expect("preparing -> prepared");
    storage
        .finish_preparing_file_mutation("tx-illegal", r#"{"snapshots":[]}"#)
        .expect_err("cannot finish preparing twice");
    storage
        .mark_file_mutation_committed("tx-illegal")
        .expect("prepared -> committed");
    storage
        .mark_file_mutation_committed("tx-illegal")
        .expect_err("cannot commit twice");
    assert_eq!(
        storage
            .get_pending_file_mutation("tx-illegal")
            .expect("get")
            .expect("row")
            .state,
        PendingFileMutationState::Committed
    );
}

#[test]
fn resolution_fence_is_idempotent_and_repairs_wrong_authority_once() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = GameId::new("steam:fence").expect("id");
    store_game(&storage, game_id.clone());
    storage
        .begin_file_mutation_preparation(&BeginFileMutationPreparation {
            id: "tx-fence".to_owned(),
            game_id: game_id.clone(),
            feature: "test".to_owned(),
            subject_id: None,
            initial_manifest_json: r#"{"snapshots":[]}"#.to_owned(),
        })
        .expect("begin");
    storage
        .finish_preparing_file_mutation("tx-fence", r#"{"snapshots":[]}"#)
        .expect("finish");

    let _first = storage
        .fence_prepared_file_mutation_resolution(&game_id, "tx-fence")
        .expect("matching fence");
    let _second = storage
        .fence_prepared_file_mutation_resolution(&game_id, "tx-fence")
        .expect("idempotent matching fence");
    assert_eq!(
        storage
            .catalog_readiness(&game_id)
            .expect("readiness")
            .authority_epoch(),
        1
    );

    storage
        .with_connection(|connection| {
            connection
                .execute(
                    "UPDATE catalog_scan_authority SET mutation_token = 'wrong' WHERE game_id = ?1",
                    [game_id.as_str()],
                )
                .map_err(storage_error)?;
            Ok(())
        })
        .expect("corrupt authority fixture");
    let _repair = storage
        .fence_prepared_file_mutation_resolution(&game_id, "tx-fence")
        .expect("repair fence");
    let _repeat = storage
        .fence_prepared_file_mutation_resolution(&game_id, "tx-fence")
        .expect("repaired fence is idempotent");
    assert_eq!(
        storage.catalog_readiness(&game_id).expect("readiness"),
        super::super::observations::CatalogReadiness::Invalidated {
            authority_epoch: 2,
            reason: "recovery".to_owned(),
            mutation_token: Some("tx-fence".to_owned()),
        }
    );
}

#[test]
fn resolution_fence_rejects_missing_or_wrong_game_authority() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = GameId::new("steam:fence-missing").expect("id");
    store_game(&storage, game_id.clone());
    storage
        .fence_prepared_file_mutation_resolution(&game_id, "missing")
        .expect_err("missing prepared row must stop");

    let row = PendingFileMutationRow {
        id: "tx-other-game".to_owned(),
        game_id: GameId::new("steam:other-game").expect("other id"),
        feature: "test".to_owned(),
        subject_id: None,
        state: PendingFileMutationState::Prepared,
        manifest_json: r#"{"snapshots":[]}"#.to_owned(),
    };
    storage.prepare_file_mutation(&row).expect("fixture row");
    storage
        .fence_prepared_file_mutation_resolution(&game_id, &row.id)
        .expect_err("wrong game must stop before any authority write");
}

#[test]
fn pre_catalog_finish_and_fence_preserve_total_absence() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = GameId::new("steam:pre-catalog-fence").expect("id");
    storage
        .begin_file_mutation_preparation(&BeginFileMutationPreparation {
            id: "tx-pre-catalog".to_owned(),
            game_id: game_id.clone(),
            feature: "test".to_owned(),
            subject_id: None,
            initial_manifest_json: r#"{"snapshots":[]}"#.to_owned(),
        })
        .expect("begin without catalog");
    storage
        .finish_preparing_file_mutation("tx-pre-catalog", r#"{"snapshots":[]}"#)
        .expect("finish without catalog");

    let fence = storage
        .fence_prepared_file_mutation_resolution(&game_id, "tx-pre-catalog")
        .expect("fence without catalog");
    storage
        .complete_prepared_file_mutation_restored(fence)
        .expect("complete without catalog");
    assert!(
        storage.catalog_readiness(&game_id).is_err(),
        "both catalog rows must remain absent for a pre-catalog mutation"
    );
}

#[test]
fn cleanup_only_resolution_removes_the_row_and_keeps_catalog_invalidated() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = GameId::new("steam:cleanup-only-resolution").expect("id");
    store_game(&storage, game_id.clone());
    storage
        .begin_file_mutation_preparation(&BeginFileMutationPreparation {
            id: "tx-cleanup-only".to_owned(),
            game_id: game_id.clone(),
            feature: "test".to_owned(),
            subject_id: None,
            initial_manifest_json: r#"{"snapshots":[]}"#.to_owned(),
        })
        .expect("begin");
    storage
        .finish_preparing_file_mutation("tx-cleanup-only", r#"{"snapshots":[]}"#)
        .expect("finish");

    let fence = storage
        .fence_prepared_file_mutation_resolution(&game_id, "tx-cleanup-only")
        .expect("fence");
    storage
        .complete_prepared_file_mutation_without_restore(fence)
        .expect("complete cleanup-only");

    assert!(
        storage
            .get_pending_file_mutation("tx-cleanup-only")
            .expect("row lookup")
            .is_none()
    );
    assert_eq!(
        storage.catalog_readiness(&game_id).expect("readiness"),
        super::super::observations::CatalogReadiness::Invalidated {
            authority_epoch: 1,
            reason: "prepared_file_mutation".to_owned(),
            mutation_token: Some("tx-cleanup-only".to_owned()),
        }
    );
}

#[test]
fn mixed_catalog_binding_stops_preparation_before_state_change() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let present_game = GameId::new("steam:mixed-game").expect("id");
    store_game(&storage, present_game.clone());
    storage
        .with_connection(|connection| {
            connection
                .execute(
                    "DELETE FROM catalog_scan_authority WHERE game_id = ?1",
                    [present_game.as_str()],
                )
                .map_err(storage_error)?;
            Ok(())
        })
        .expect("remove authority fixture");
    storage
        .begin_file_mutation_preparation(&BeginFileMutationPreparation {
            id: "tx-mixed-game".to_owned(),
            game_id: present_game,
            feature: "test".to_owned(),
            subject_id: None,
            initial_manifest_json: r#"{"snapshots":[]}"#.to_owned(),
        })
        .expect("begin");
    storage
        .finish_preparing_file_mutation("tx-mixed-game", r#"{"snapshots":[]}"#)
        .expect_err("game without authority is corruption");
    assert_eq!(
        storage
            .get_pending_file_mutation("tx-mixed-game")
            .expect("row")
            .expect("reserved row")
            .state,
        PendingFileMutationState::Preparing
    );

    let absent_game = GameId::new("steam:mixed-authority").expect("id");
    storage
        .begin_file_mutation_preparation(&BeginFileMutationPreparation {
            id: "tx-mixed-authority".to_owned(),
            game_id: absent_game.clone(),
            feature: "test".to_owned(),
            subject_id: None,
            initial_manifest_json: r#"{"snapshots":[]}"#.to_owned(),
        })
        .expect("begin");
    storage
        .with_connection(|connection| {
            connection
                .execute_batch("PRAGMA foreign_keys = OFF")
                .map_err(storage_error)?;
            let insert = connection.execute(
                "INSERT INTO catalog_scan_authority
                        (game_id, readiness, authority_epoch, invalidation_reason,
                         mutation_token, completed_at, updated_at)
                     VALUES (?1, 'never_completed', 0, NULL, NULL, NULL, 0)",
                [absent_game.as_str()],
            );
            connection
                .execute_batch("PRAGMA foreign_keys = ON")
                .map_err(storage_error)?;
            insert.map_err(storage_error)?;
            Ok(())
        })
        .expect("insert authority-only corruption fixture");
    storage
        .finish_preparing_file_mutation("tx-mixed-authority", r#"{"snapshots":[]}"#)
        .expect_err("authority without game is corruption");
    assert_eq!(
        storage
            .get_pending_file_mutation("tx-mixed-authority")
            .expect("row")
            .expect("reserved row")
            .state,
        PendingFileMutationState::Preparing
    );
}

#[test]
fn rust_state_strings_match_sql_check_constraint() {
    // Keep in sync with CHECK (state IN (...)) in schema/ddl/pending_file_mutations.
    let allowed = ["preparing", "prepared", "committed"];
    for state in [
        PendingFileMutationState::Preparing,
        PendingFileMutationState::Prepared,
        PendingFileMutationState::Committed,
    ] {
        assert!(
            allowed.contains(&state.as_str()),
            "state {:?} missing from SQL CHECK set",
            state
        );
        assert_eq!(
            state
                .as_str()
                .parse::<PendingFileMutationState>()
                .expect("round-trip"),
            state
        );
    }
    assert!("done".parse::<PendingFileMutationState>().is_err());
}
