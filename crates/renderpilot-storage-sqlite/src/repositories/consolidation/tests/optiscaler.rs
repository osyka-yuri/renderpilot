//! OptiScaler aggregate migration and inventory tests.

use super::*;
use renderpilot_application::InstalledAddonRepository;
#[test]
fn optiscaler_state_only_partial_is_blocked_and_unchanged() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let destination = game("game:destination", "C:/Games/Example");
    let source = game("manual:child", "C:/Games/Example/D3D12");
    storage.upsert_game(&destination).expect("destination");
    storage.upsert_game(&source).expect("source");
    seed_optiscaler_state(&storage, source.id(), "release");

    let plan = consolidation_plan(&destination, &[&source]);
    let conflicts = storage
        .inspect_consolidation_conflicts(&plan)
        .expect("partial preview");
    assert_optiscaler_blocked(&conflicts);
    let error = storage
        .save_install_scan_with_consolidation(
            ScanWriteUnit {
                game: &destination,
                components: &[],
                artifacts: &[],
                prune_empty_operations: false,
            },
            &plan,
            &conflicts,
        )
        .expect_err("state-only aggregate must block execution");
    assert!(error.message().contains("game_proxy_topologies"));
    assert_game_and_optiscaler_state_remain(&storage, &source);
}

#[test]
fn optiscaler_topology_only_partial_and_invalid_outer_are_blocked() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let destination = game("game:destination", "C:/Games/Example");
    let partial_source = game("manual:partial", "C:/Games/Example/Partial");
    let invalid_source = game("manual:invalid", "C:/Games/Example/Invalid");
    for value in [&destination, &partial_source, &invalid_source] {
        storage.upsert_game(value).expect("game");
    }
    seed_optiscaler_topology(&storage, partial_source.id(), "opti_scaler", None);
    seed_optiscaler_rows(
        &storage,
        invalid_source.id(),
        "release",
        "reshade",
        true,
        true,
        false,
    );

    let partial_plan = consolidation_plan(&destination, &[&partial_source]);
    let partial_conflicts = storage
        .inspect_consolidation_conflicts(&partial_plan)
        .expect("topology-only preview");
    assert_optiscaler_blocked(&partial_conflicts);

    let invalid_plan = consolidation_plan(&destination, &[&invalid_source]);
    let invalid_conflicts = storage
        .inspect_consolidation_conflicts(&invalid_plan)
        .expect("invalid outer preview");
    assert_optiscaler_blocked(&invalid_conflicts);
    assert_game_and_optiscaler_state_remain(&storage, &partial_source);
    assert_game_and_optiscaler_state_remain(&storage, &invalid_source);
}

#[test]
fn optiscaler_payload_row_identity_mismatch_is_invalid_partial() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let destination = game("game:destination", "C:/Games/Example");
    let source = game("manual:child", "C:/Games/Example/D3D12");
    storage.upsert_game(&destination).expect("destination");
    storage.upsert_game(&source).expect("source");
    seed_optiscaler_complete_with_identity_mismatch(&storage, source.id());

    let conflicts = storage
        .inspect_consolidation_conflicts(&consolidation_plan(&destination, &[&source]))
        .expect("identity mismatch preview");
    assert_optiscaler_blocked(&conflicts);
}

#[test]
fn equivalent_complete_optiscaler_aggregates_use_destination_and_rekey_source() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let destination = game("game:destination", "C:/Games/Example");
    let source = game("manual:child", "C:/Games/Example/D3D12");
    storage.upsert_game(&destination).expect("destination");
    storage.upsert_game(&source).expect("source");
    seed_optiscaler_complete(&storage, destination.id(), "release");
    seed_optiscaler_complete(&storage, source.id(), "release");

    let plan = consolidation_plan(&destination, &[&source]);
    let conflicts = storage
        .inspect_consolidation_conflicts(&plan)
        .expect("equivalent preview");
    assert_eq!(
        conflicts.destination_wins_tables,
        vec![
            "game_proxy_topologies".to_owned(),
            "optiscaler_install_states".to_owned()
        ]
    );
    assert!(conflicts.blocking_tables.is_empty());
    storage
        .save_install_scan_with_consolidation(
            ScanWriteUnit {
                game: &destination,
                components: &[],
                artifacts: &[],
                prune_empty_operations: false,
            },
            &plan,
            &conflicts,
        )
        .expect("equivalent aggregate consolidation");

    assert!(!game_exists(&storage, &source));
    assert_eq!(
        optiscaler_topology_id(&storage, &destination),
        "optiscaler:game:destination"
    );
    assert_eq!(
        optiscaler_state_topology_id(&storage, &destination),
        "optiscaler:game:destination"
    );
    assert_eq!(optiscaler_row_count(&storage, source.id()), 0);
    assert_foreign_keys_are_valid(&storage);
}

#[test]
fn absent_destination_accepts_one_complete_source_and_rekeys_before_state_insert() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let destination = game("game:destination", "C:/Games/Example");
    let source = game("manual:child", "C:/Games/Example/D3D12");
    storage.upsert_game(&destination).expect("destination");
    storage.upsert_game(&source).expect("source");
    seed_optiscaler_complete(&storage, source.id(), "release");

    let plan = consolidation_plan(&destination, &[&source]);
    let conflicts = storage
        .inspect_consolidation_conflicts(&plan)
        .expect("source-only preview");
    assert!(conflicts.destination_wins_tables.is_empty());
    assert!(conflicts.blocking_tables.is_empty());
    storage
        .save_install_scan_with_consolidation(
            ScanWriteUnit {
                game: &destination,
                components: &[],
                artifacts: &[],
                prune_empty_operations: false,
            },
            &plan,
            &conflicts,
        )
        .expect("source aggregate consolidation");

    assert!(!game_exists(&storage, &source));
    assert_eq!(
        optiscaler_topology_id(&storage, &destination),
        "optiscaler:game:destination"
    );
    assert_eq!(
        optiscaler_state_topology_id(&storage, &destination),
        "optiscaler:game:destination"
    );
    assert_foreign_keys_are_valid(&storage);
}

#[test]
fn differing_complete_optiscaler_aggregates_block_preview_and_execution() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let destination = game("game:destination", "C:/Games/Example");
    let source = game("manual:child", "C:/Games/Example/D3D12");
    storage.upsert_game(&destination).expect("destination");
    storage.upsert_game(&source).expect("source");
    seed_optiscaler_complete(&storage, destination.id(), "release-destination");
    seed_optiscaler_complete(&storage, source.id(), "release-source");

    let plan = consolidation_plan(&destination, &[&source]);
    let conflicts = storage
        .inspect_consolidation_conflicts(&plan)
        .expect("differing preview");
    assert_optiscaler_blocked(&conflicts);
    let error = storage
        .save_install_scan_with_consolidation(
            ScanWriteUnit {
                game: &destination,
                components: &[],
                artifacts: &[],
                prune_empty_operations: false,
            },
            &plan,
            &conflicts,
        )
        .expect_err("differing complete aggregates must block execution");
    assert!(error.message().contains("optiscaler_install_states"));
    assert_game_and_optiscaler_state_remain(&storage, &source);
    assert_game_and_optiscaler_state_remain(&storage, &destination);
}

#[test]
fn multiple_equivalent_sources_are_accepted_but_a_differing_source_blocks() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let destination = game("game:destination", "C:/Games/Example");
    let first = game("manual:first", "C:/Games/Example/First");
    let second = game("manual:second", "C:/Games/Example/Second");
    for value in [&destination, &first, &second] {
        storage.upsert_game(value).expect("game");
    }
    seed_optiscaler_complete(&storage, first.id(), "release");
    seed_optiscaler_complete(&storage, second.id(), "release");

    let plan = consolidation_plan(&destination, &[&first, &second]);
    let conflicts = storage
        .inspect_consolidation_conflicts(&plan)
        .expect("equivalent source preview");
    assert!(conflicts.blocking_tables.is_empty());
    storage
        .save_install_scan_with_consolidation(
            ScanWriteUnit {
                game: &destination,
                components: &[],
                artifacts: &[],
                prune_empty_operations: false,
            },
            &plan,
            &conflicts,
        )
        .expect("equivalent sources consolidation");
    assert!(!game_exists(&storage, &first));
    assert!(!game_exists(&storage, &second));
    assert_eq!(optiscaler_row_count(&storage, destination.id()), 2);
    assert_foreign_keys_are_valid(&storage);

    let storage = SqliteStorage::in_memory().expect("differing storage");
    for value in [&destination, &first, &second] {
        storage.upsert_game(value).expect("game");
    }
    seed_optiscaler_complete(&storage, first.id(), "release");
    seed_optiscaler_complete(&storage, second.id(), "release-other");
    let conflicts = storage
        .inspect_consolidation_conflicts(&plan)
        .expect("differing source preview");
    assert_optiscaler_blocked(&conflicts);
}

#[test]
fn optiscaler_move_rolls_back_and_deletes_source_state_before_topology() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let destination = game("game:destination", "C:/Games/Example");
    let source = game("manual:child", "C:/Games/Example/D3D12");
    storage.upsert_game(&destination).expect("destination");
    storage.upsert_game(&source).expect("source");
    seed_optiscaler_complete(&storage, source.id(), "release");
    let source_plan = ConsolidationSource {
        source_game_id: source.id().clone(),
        component_rekeys: Vec::new(),
    };

    let error = storage
        .with_transaction(|transaction| {
            move_optiscaler_aggregate(transaction, destination.id(), &source_plan)?;
            Err::<(), _>(AppError::storage_failed("test rollback"))
        })
        .expect_err("transaction must roll back after OptiScaler move");
    assert!(error.message().contains("test rollback"));
    assert_eq!(optiscaler_row_count(&storage, source.id()), 2);
    assert_eq!(optiscaler_row_count(&storage, destination.id()), 0);
    assert_foreign_keys_are_valid(&storage);
}

#[test]
fn optiscaler_inventory_includes_direct_paths_and_json_paths_from_both_tables() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let destination = game("game:destination", "C:/Games/Example");
    let source = game("manual:child", "C:/Games/Example/D3D12");
    storage.upsert_game(&destination).expect("destination");
    storage.upsert_game(&source).expect("source");
    seed_optiscaler_complete(&storage, source.id(), "release");

    let paths = storage
        .list_consolidation_recovery_file_paths(&consolidation_plan(&destination, &[&source]))
        .expect("inventory");
    let paths = paths
        .iter()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect::<Vec<_>>();
    for expected in [
        format!("{}/Game.exe", target_dir()),
        target_dir(),
        format!("{}/OptiScaler.ini", target_dir()),
        format!("{}/dxgi.dll", target_dir()),
    ] {
        assert!(
            paths.iter().any(|path| path == &expected),
            "missing {expected}: {paths:?}"
        );
    }
}

#[test]
fn renodx_inventory_extracts_only_the_typed_ini_path_from_a_receipt() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let destination = game("game:destination", "C:/Games/Example");
    let source = game("manual:child", "C:/Games/Example/D3D12");
    storage.upsert_game(&destination).expect("destination");
    storage.upsert_game(&source).expect("source");
    let addon = renderpilot_domain::InstalledAddon::new(
        source.id().clone(),
        renderpilot_domain::AddonKind::RenoDx,
        renderpilot_domain::PathRef::new("C:/Games/Example/D3D12/renodx.addon64")
            .expect("addon path"),
    )
    .with_renodx_config_receipt(Some(renderpilot_domain::RenoDxConfigReceipt::new(
        renderpilot_domain::PathRef::new("C:/Games/Example/D3D12/ReShade.ini").expect("ini path"),
        renderpilot_domain::RenoDxSetPathBaseline::Present {
            value: "C:/Users/user/secret.txt".to_owned(),
        },
        true,
        renderpilot_domain::RenoDxSetPathValue::One,
    )))
    .expect("receipt");
    storage.upsert_installed_addon(&addon).expect("addon");

    let paths = storage
        .list_consolidation_recovery_file_paths(&consolidation_plan(&destination, &[&source]))
        .expect("inventory");
    let matches_path = |expected: &str| {
        paths
            .iter()
            .any(|path| path.to_string_lossy().replace('\\', "/") == expected)
    };
    assert!(matches_path("C:/Games/Example/D3D12/ReShade.ini"));
    assert!(!matches_path("C:/Users/user/secret.txt"));
}
