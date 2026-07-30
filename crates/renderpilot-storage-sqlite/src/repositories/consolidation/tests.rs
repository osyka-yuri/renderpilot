use renderpilot_application::{ComponentRepository, GameRepository};
use renderpilot_domain::{
    ComponentFile, ComponentId, ComponentKind, GameId, GameIdentity, GameInstallation, GameRuntime,
    Launcher, LibraryComponent, LibraryTechnology, PathRef, Platform, RootAuthority, Swappability,
};

use super::{ComponentRekey, ConsolidationPlan, ConsolidationSource};
use crate::{ScanWriteUnit, SqliteStorage};

#[test]
fn aggregate_consolidation_rekeys_every_scoped_state_category() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let destination = game("game:destination", "C:/Games/Example");
    let source = game("manual:child", "C:/Games/Example/D3D12");
    let destination_component = component(
        "component:destination",
        destination.id(),
        "C:/Games/Example/D3D12/D3D12Core.dll",
    );
    let source_component = component(
        "component:source",
        source.id(),
        "C:/Games/Example/D3D12/D3D12Core.dll",
    );
    storage.upsert_game(&destination).expect("destination game");
    storage.upsert_game(&source).expect("source game");
    storage
        .replace_components_for_game(
            destination.id(),
            std::slice::from_ref(&destination_component),
        )
        .expect("destination component");
    storage
        .replace_components_for_game(source.id(), std::slice::from_ref(&source_component))
        .expect("source component");
    seed_all_scoped_state(&storage);

    let plan = ConsolidationPlan {
        destination_game_id: destination.id().clone(),
        sources: vec![ConsolidationSource {
            source_game_id: source.id().clone(),
            component_rekeys: vec![ComponentRekey {
                source_component_id: source_component.id().as_str().to_owned(),
                destination_component_id: destination_component.id().as_str().to_owned(),
            }],
        }],
    };
    let conflicts = storage
        .inspect_consolidation_conflicts(&plan)
        .expect("conflict preview");
    let report = storage
        .save_install_scan_with_consolidation(
            ScanWriteUnit {
                game: &destination,
                components: std::slice::from_ref(&destination_component),
                artifacts: &[],
                prune_empty_operations: false,
            },
            &plan,
            &conflicts,
        )
        .expect("aggregate consolidation");

    assert_eq!(
        report.consolidation.removed_game_ids,
        vec![source.id().clone()]
    );
    assert!(
        storage
            .find_game(source.id())
            .expect("find source")
            .is_none()
    );
    let connection = storage.connection().expect("connection");
    for table in [
        "operations",
        "operation_items",
        "component_backups",
        "installed_addons",
        "game_covers",
        "nvapi_executable_overrides",
        "nvapi_setting_baselines",
        "game_ui_state",
        "profile_addon_capabilities",
    ] {
        let source_rows: i64 = connection
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE game_id = 'manual:child'"),
                [],
                |row| row.get(0),
            )
            .expect("source scoped count");
        assert_eq!(source_rows, 0, "{table} retained source identity");
    }
    let artifact_owner: String = connection
        .query_row(
            "SELECT source_game_id FROM library_artifacts WHERE id = 'artifact:source'",
            [],
            |row| row.get(0),
        )
        .expect("artifact owner");
    assert_eq!(artifact_owner, "game:destination");
    let operation_component: String = connection
        .query_row(
            "SELECT component_id FROM operation_items WHERE operation_id = 'operation:source'",
            [],
            |row| row.get(0),
        )
        .expect("operation component");
    assert_eq!(operation_component, "component:destination");
    let backup_component: String = connection
        .query_row(
            "SELECT component_id FROM component_backups WHERE game_id = 'game:destination'",
            [],
            |row| row.get(0),
        )
        .expect("backup component");
    assert_eq!(backup_component, "component:destination");
}

#[test]
fn pending_mutation_aborts_the_whole_aggregate() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let destination = game("game:destination", "C:/Games/Example");
    let source = game("manual:child", "C:/Games/Example/D3D12");
    let destination_component =
        component("component:destination", destination.id(), "C:/Games/a.dll");
    let source_component = component("component:source", source.id(), "C:/Games/a.dll");
    storage.upsert_game(&destination).expect("destination");
    storage.upsert_game(&source).expect("source");
    storage
        .replace_components_for_game(source.id(), std::slice::from_ref(&source_component))
        .expect("source component");
    storage
        .connection()
        .expect("connection")
        .execute(
            "INSERT INTO pending_file_mutations (
                    id, game_id, feature, state, manifest_json
                ) VALUES ('pending:source', 'manual:child', 'test', 'prepared', '{}')",
            [],
        )
        .expect("pending mutation");

    let plan = ConsolidationPlan {
        destination_game_id: destination.id().clone(),
        sources: vec![ConsolidationSource {
            source_game_id: source.id().clone(),
            component_rekeys: vec![ComponentRekey {
                source_component_id: source_component.id().as_str().to_owned(),
                destination_component_id: destination_component.id().as_str().to_owned(),
            }],
        }],
    };
    let conflicts = storage
        .inspect_consolidation_conflicts(&plan)
        .expect("conflict preview");
    let error = storage
        .save_install_scan_with_consolidation(
            ScanWriteUnit {
                game: &destination,
                components: std::slice::from_ref(&destination_component),
                artifacts: &[],
                prune_empty_operations: false,
            },
            &plan,
            &conflicts,
        )
        .expect_err("pending mutation must abort");
    assert!(
        error.message().contains("pending_file_mutations"),
        "unexpected consolidation error: {error}"
    );
    assert!(
        storage
            .find_game(source.id())
            .expect("find source")
            .is_some()
    );
    assert!(
        storage
            .find_game(destination.id())
            .expect("find destination")
            .is_some(),
        "pre-existing destination remains"
    );
}

#[test]
fn ambiguous_managed_baselines_history_addons_and_nvapi_are_blocking() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let destination = game("game:destination", "C:/Games/Example");
    let source = game("manual:child", "C:/Games/Example/D3D12");
    let destination_component =
        component("component:destination", destination.id(), "C:/Games/a.dll");
    let source_component = component("component:source", source.id(), "C:/Games/a.dll");
    let history_component = component("component:history", source.id(), "C:/Games/history.dll");
    storage.upsert_game(&destination).expect("destination");
    storage.upsert_game(&source).expect("source");
    storage
        .replace_components_for_game(
            destination.id(),
            std::slice::from_ref(&destination_component),
        )
        .expect("destination component");
    storage
        .replace_components_for_game(source.id(), &[source_component.clone(), history_component])
        .expect("source components");
    storage
        .connection()
        .expect("connection")
        .execute_batch(
            r#"
                INSERT INTO component_backups (
                    component_id, game_id, files_json, auxiliary_json, created_at, updated_at
                ) VALUES
                    ('component:destination', 'game:destination', '[]', '[]', 1, 1),
                    ('component:source', 'manual:child', '[{"path":"source"}]', '[]', 1, 1);

                INSERT INTO installed_addons (
                    game_id, kind, addon_file, created_files_json,
                    backed_up_files_json, managed_files_json, tracked_sources_json,
                    created_at, updated_at
                ) VALUES
                    ('game:destination', 'RenoDx', 'destination.addon64',
                     '[]', '[]', '[]', '[]', 1, 1),
                    ('manual:child', 'RenoDx', 'source.addon64',
                     '[]', '[]', '[]', '[]', 1, 1);

                INSERT INTO nvapi_setting_baselines (
                    game_id, setting_key, baseline_dword, baseline_was_predefined,
                    captured_exe, captured_at
                ) VALUES
                    ('game:destination', 'setting', 1, 0, 'game.exe', 1),
                    ('manual:child', 'setting', 2, 0, 'game.exe', 1);

                INSERT INTO operations (
                    id, game_id, kind, status, created_at, updated_at
                ) VALUES ('operation:history', 'manual:child', 'Scan', 'Planned', 1, 1);
                INSERT INTO operation_items (
                    operation_id, game_id, component_id, source_path, status,
                    created_at, updated_at
                ) VALUES (
                    'operation:history', 'manual:child', 'component:history',
                    'C:/Games/history.dll', 'Planned', 1, 1
                );
                "#,
        )
        .expect("conflicting managed state");

    let plan = ConsolidationPlan {
        destination_game_id: destination.id().clone(),
        sources: vec![ConsolidationSource {
            source_game_id: source.id().clone(),
            component_rekeys: vec![ComponentRekey {
                source_component_id: source_component.id().as_str().to_owned(),
                destination_component_id: destination_component.id().as_str().to_owned(),
            }],
        }],
    };

    let conflicts = storage
        .inspect_consolidation_conflicts(&plan)
        .expect("conflict preview");

    for table in [
        "component_backups",
        "installed_addons",
        "nvapi_setting_baselines",
        "operations",
    ] {
        assert!(
            conflicts
                .blocking_tables
                .iter()
                .any(|candidate| candidate == table),
            "{table} must block ambiguous runtime consolidation: {conflicts:?}"
        );
    }
    assert!(conflicts.requires_recovery_bundle());
    assert!(conflicts.has_blocking_conflicts());
}

#[test]
fn conflicting_managed_state_between_two_sources_is_blocking() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let destination = game("game:destination", "C:/Games/Example");
    let first_source = game("manual:first", "C:/Games/Example/First");
    let second_source = game("manual:second", "C:/Games/Example/Second");
    for game in [&destination, &first_source, &second_source] {
        storage.upsert_game(game).expect("game");
    }
    storage
        .connection()
        .expect("connection")
        .execute_batch(
            r#"
                INSERT INTO installed_addons (
                    game_id, kind, addon_file, created_files_json,
                    backed_up_files_json, managed_files_json, tracked_sources_json,
                    created_at, updated_at
                ) VALUES
                    ('manual:first', 'RenoDx', 'first.addon64',
                     '[]', '[]', '[]', '[]', 1, 1),
                    ('manual:second', 'RenoDx', 'second.addon64',
                     '[]', '[]', '[]', '[]', 1, 1);
                "#,
        )
        .expect("conflicting source state");

    let plan = ConsolidationPlan {
        destination_game_id: destination.id().clone(),
        sources: vec![
            ConsolidationSource {
                source_game_id: first_source.id().clone(),
                component_rekeys: Vec::new(),
            },
            ConsolidationSource {
                source_game_id: second_source.id().clone(),
                component_rekeys: Vec::new(),
            },
        ],
    };
    let conflicts = storage
        .inspect_consolidation_conflicts(&plan)
        .expect("conflict preview");
    assert_eq!(
        conflicts.blocking_tables,
        vec!["installed_addons".to_owned()]
    );

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
        .expect_err("ambiguous source state must fail closed");
    assert!(error.message().contains("installed_addons"));
    assert!(
        storage
            .find_game(first_source.id())
            .expect("first source")
            .is_some()
    );
    assert!(
        storage
            .find_game(second_source.id())
            .expect("second source")
            .is_some()
    );
}

#[test]
fn component_rekeys_are_one_to_one_across_the_whole_plan() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let destination = game("game:destination", "C:/Games/Example");
    let first_source = game("manual:first", "C:/Games/Example/First");
    let second_source = game("manual:second", "C:/Games/Example/Second");
    let destination_component =
        component("component:destination", destination.id(), "C:/Games/a.dll");
    let first_component = component("component:first", first_source.id(), "C:/Games/a.dll");
    let second_component = component("component:second", second_source.id(), "C:/Games/a.dll");
    for game in [&destination, &first_source, &second_source] {
        storage.upsert_game(game).expect("game");
    }
    storage
        .replace_components_for_game(
            destination.id(),
            std::slice::from_ref(&destination_component),
        )
        .expect("destination component");
    storage
        .replace_components_for_game(first_source.id(), std::slice::from_ref(&first_component))
        .expect("first component");
    storage
        .replace_components_for_game(second_source.id(), std::slice::from_ref(&second_component))
        .expect("second component");

    let plan = ConsolidationPlan {
        destination_game_id: destination.id().clone(),
        sources: vec![
            ConsolidationSource {
                source_game_id: first_source.id().clone(),
                component_rekeys: vec![ComponentRekey {
                    source_component_id: first_component.id().as_str().to_owned(),
                    destination_component_id: destination_component.id().as_str().to_owned(),
                }],
            },
            ConsolidationSource {
                source_game_id: second_source.id().clone(),
                component_rekeys: vec![ComponentRekey {
                    source_component_id: second_component.id().as_str().to_owned(),
                    destination_component_id: destination_component.id().as_str().to_owned(),
                }],
            },
        ],
    };

    let error = storage
        .inspect_consolidation_conflicts(&plan)
        .expect_err("cross-source destination reuse must be rejected");
    assert!(error.message().contains("whole consolidation plan"));
}

#[test]
fn component_rekey_rejects_a_destination_owned_by_another_game() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let destination = game("game:destination", "C:/Games/Example");
    let source = game("manual:child", "C:/Games/Example/D3D12");
    let unrelated = game("game:unrelated", "C:/Games/Other");
    let source_component = component(
        "component:source",
        source.id(),
        "C:/Games/Example/D3D12/a.dll",
    );
    let unrelated_component = component("component:other", unrelated.id(), "C:/Games/Other/a.dll");
    for game in [&destination, &source, &unrelated] {
        storage.upsert_game(game).expect("game");
    }
    storage
        .replace_components_for_game(source.id(), std::slice::from_ref(&source_component))
        .expect("source component");
    storage
        .replace_components_for_game(unrelated.id(), std::slice::from_ref(&unrelated_component))
        .expect("unrelated component");
    let plan = ConsolidationPlan {
        destination_game_id: destination.id().clone(),
        sources: vec![ConsolidationSource {
            source_game_id: source.id().clone(),
            component_rekeys: vec![ComponentRekey {
                source_component_id: source_component.id().as_str().to_owned(),
                destination_component_id: unrelated_component.id().as_str().to_owned(),
            }],
        }],
    };
    let conflicts = storage
        .inspect_consolidation_conflicts(&plan)
        .expect("preview");

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
        .expect_err("foreign component ownership must fail");

    assert!(error.message().contains("does not belong"));
    assert!(storage.find_game(source.id()).expect("source").is_some());
}

#[test]
fn changed_conflict_preview_aborts_before_scan_write() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let destination = game("game:destination", "C:/Games/Example");
    let source = game("manual:child", "C:/Games/Example/D3D12");
    storage.upsert_game(&destination).expect("destination");
    storage.upsert_game(&source).expect("source");

    let plan = ConsolidationPlan {
        destination_game_id: destination.id().clone(),
        sources: vec![ConsolidationSource {
            source_game_id: source.id().clone(),
            component_rekeys: Vec::new(),
        }],
    };
    let preview = storage
        .inspect_consolidation_conflicts(&plan)
        .expect("empty conflict preview");
    assert!(!preview.requires_recovery_bundle());

    storage
        .upsert_game_cover(destination.id(), "destination.webp")
        .expect("destination cover");
    storage
        .upsert_game_cover(source.id(), "source.webp")
        .expect("source cover");

    let error = storage
        .save_install_scan_with_consolidation(
            ScanWriteUnit {
                game: &destination,
                components: &[],
                artifacts: &[],
                prune_empty_operations: false,
            },
            &plan,
            &preview,
        )
        .expect_err("stale conflict preview must abort");

    assert!(error.message().contains("conflict state changed"));
    assert!(
        storage
            .find_game(source.id())
            .expect("find source")
            .is_some(),
        "source must remain after a stale preview"
    );
}

fn seed_all_scoped_state(storage: &SqliteStorage) {
    storage
            .connection()
            .expect("connection")
            .execute_batch(
                r#"
                INSERT INTO operations (
                    id, game_id, kind, status, created_at, updated_at
                ) VALUES (
                    'operation:source', 'manual:child', 'Scan', 'Planned', 1, 1
                );
                INSERT INTO operation_items (
                    operation_id, game_id, component_id, source_path, status,
                    created_at, updated_at
                ) VALUES (
                    'operation:source', 'manual:child', 'component:source',
                    'C:/Games/Example/D3D12/D3D12Core.dll', 'Planned', 1, 1
                );
                INSERT INTO component_backups (
                    component_id, game_id, files_json, auxiliary_json, created_at, updated_at
                ) VALUES (
                    'component:source', 'manual:child', '[]', '[]', 1, 1
                );
                INSERT INTO library_artifacts (
                    id, technology, file_name, files_json, metadata_json, source,
                    source_game_id, trust_level, created_at, updated_at
                ) VALUES (
                    'artifact:source', 'dlss_super_resolution', 'D3D12Core.dll',
                    '[{"path":"C:/Games/Example/D3D12/D3D12Core.dll",
                       "sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}]',
                    '{}', 'scan-folder', 'manual:child', 'user_imported', 1, 1
                );
                INSERT INTO installed_addons (
                    game_id, kind, addon_file, created_files_json,
                    backed_up_files_json, managed_files_json, tracked_sources_json,
                    created_at, updated_at
                ) VALUES (
                    'manual:child', 'RenoDx', 'addon.addon64', '[]', '[]', '[]', '[]', 1, 1
                );
                INSERT INTO game_covers (game_id, file_name, updated_at)
                VALUES ('manual:child', 'source.webp', 1);
                INSERT INTO nvapi_executable_overrides (
                    game_id, selected_path, selected_basename, updated_at
                ) VALUES ('manual:child', 'C:/Games/game.exe', 'game.exe', 1);
                INSERT INTO nvapi_setting_baselines (
                    game_id, setting_key, baseline_dword, baseline_was_predefined,
                    captured_exe, captured_at
                ) VALUES ('manual:child', 'setting', 1, 0, 'game.exe', 1);
                INSERT INTO game_ui_state (game_id, is_favorite, is_hidden, updated_at)
                VALUES ('manual:child', 1, 0, 1);
                INSERT INTO profile_addon_capabilities (
                    game_id, addon_kind, source_revision, updated_at
                ) VALUES ('manual:child', 'RenoDx', 'revision', 1);
                "#,
            )
            .expect("seed scoped state");
}

fn game(id: &str, path: &str) -> GameInstallation {
    GameInstallation::new(
        GameIdentity::new(GameId::new(id).expect("game id"), "Game", Launcher::Manual)
            .expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(path).expect("path"),
    )
    .with_root_authority(RootAuthority::Legacy)
}

fn component(id: &str, game_id: &GameId, path: &str) -> LibraryComponent {
    LibraryComponent::new(
        ComponentId::new(id).expect("component id"),
        game_id.clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
    )
    .with_file(ComponentFile::new(PathRef::new(path).expect("path")))
}
