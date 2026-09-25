//! Shared fixtures for consolidation tests.

use super::*;

pub(super) fn consolidation_plan(
    destination: &renderpilot_domain::GameInstallation,
    sources: &[&renderpilot_domain::GameInstallation],
) -> ConsolidationPlan {
    ConsolidationPlan {
        destination_game_id: destination.id().clone(),
        sources: sources
            .iter()
            .map(|source| ConsolidationSource {
                source_game_id: source.id().clone(),
                component_rekeys: Vec::new(),
            })
            .collect(),
    }
}

pub(super) fn assert_optiscaler_blocked(conflicts: &super::super::ConsolidationConflictSummary) {
    assert!(
        conflicts
            .blocking_tables
            .contains(&"game_proxy_topologies".to_owned())
    );
    assert!(
        conflicts
            .blocking_tables
            .contains(&"optiscaler_install_states".to_owned())
    );
}

pub(super) fn seed_optiscaler_complete(
    storage: &SqliteStorage,
    game_id: &GameId,
    release_id: &str,
) {
    seed_optiscaler_rows(
        storage,
        game_id,
        release_id,
        "opti_scaler",
        true,
        true,
        false,
    );
}

pub(super) fn seed_optiscaler_state(storage: &SqliteStorage, game_id: &GameId, release_id: &str) {
    seed_optiscaler_rows(
        storage,
        game_id,
        release_id,
        "opti_scaler",
        true,
        false,
        false,
    );
}

pub(super) fn seed_optiscaler_topology(
    storage: &SqliteStorage,
    game_id: &GameId,
    outer_implementation: &str,
    payload_game_id: Option<&str>,
) {
    seed_optiscaler_rows(
        storage,
        game_id,
        "release",
        outer_implementation,
        false,
        true,
        payload_game_id.is_some(),
    );
    if let Some(payload_game_id) = payload_game_id {
        let connection = storage.connection().expect("connection");
        let topology_json = topology_json(game_id, payload_game_id, "opti_scaler");
        connection
            .execute(
                "UPDATE game_proxy_topologies SET topology_json = :topology_json WHERE game_id = :game_id",
                named_params! {
                    ":topology_json": topology_json,
                    ":game_id": game_id.as_str(),
                },
            )
            .expect("topology payload");
    }
}

pub(super) fn seed_optiscaler_complete_with_identity_mismatch(
    storage: &SqliteStorage,
    game_id: &GameId,
) {
    seed_optiscaler_rows(storage, game_id, "release", "opti_scaler", true, true, true);
    let connection = storage.connection().expect("connection");
    connection
        .execute(
            "UPDATE game_proxy_topologies SET topology_json = :topology_json WHERE game_id = :game_id",
            named_params! {
                ":topology_json": topology_json(game_id, "manual:wrong", "opti_scaler"),
                ":game_id": game_id.as_str(),
            },
        )
        .expect("topology identity mismatch");
}

pub(super) fn seed_optiscaler_rows(
    storage: &SqliteStorage,
    game_id: &GameId,
    release_id: &str,
    outer_implementation: &str,
    state_present: bool,
    topology_present: bool,
    payload_game_id_differs: bool,
) {
    let connection = storage.connection().expect("connection");
    if state_present {
        let configuration_baseline_json = crate::repositories::optiscaler_states::codec::encode(
            &renderpilot_domain::OptiScalerConfigurationBaseline::absent(),
        )
        .expect("configuration baseline");
        connection
            .execute_batch("PRAGMA foreign_keys = OFF;")
            .expect("disable foreign keys for partial fixture");
        connection
            .execute(
                "INSERT INTO optiscaler_install_states (
                    game_id, release_id, manifest_revision, archive_sha256, source,
                    target_exe_path, target_dir, modules_json, release_files_json,
                    runtime_bindings_json, directory_receipts_json, proxy_topology_id,
                    config_schema, config_base_release, adoption_state, prerequisite_binding,
                    created_at, updated_at,
                    configuration_baseline_json
                ) VALUES (
                    :game_id, :release_id, 'manifest', NULL, NULL,
                    :target_exe_path, :target_dir, '[\"core\"]', :release_files_json,
                    '[]', '[]', :proxy_topology_id,
                    1, :config_base_release, 'managed', 'none', 1, 1,
                    :configuration_baseline_json
                )",
                named_params! {
                    ":game_id": game_id.as_str(),
                    ":release_id": release_id,
                    ":target_exe_path": format!("{}/Game.exe", target_dir()),
                    ":target_dir": target_dir(),
                    ":release_files_json": release_files_json(),
                    ":proxy_topology_id": topology_id(game_id),
                    ":config_base_release": release_id,
                    ":configuration_baseline_json": configuration_baseline_json,
                },
            )
            .expect("OptiScaler state");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("restore foreign keys");
    }
    if topology_present {
        let payload_id = if payload_game_id_differs {
            "manual:wrong"
        } else {
            game_id.as_str()
        };
        connection
            .execute(
                "INSERT INTO game_proxy_topologies (
                    game_id, id, topology_json, created_at, updated_at
                ) VALUES (:game_id, :id, :topology_json, 1, 1)",
                named_params! {
                    ":game_id": game_id.as_str(),
                    ":id": topology_id(game_id),
                    ":topology_json": topology_json(game_id, payload_id, outer_implementation),
                },
            )
            .expect("proxy topology");
    }
}

pub(super) fn target_dir() -> String {
    "C:/Games/Consolidation".to_owned()
}

pub(super) fn topology_id(game_id: &GameId) -> String {
    format!("optiscaler:{game_id}")
}

pub(super) fn release_files_json() -> String {
    serde_json::json!([{
        "path": format!("{}/OptiScaler.ini", target_dir()),
        "installed": {
            "identity": "config-identity",
            "digest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "ownership": "owned"
        },
        "role": "configuration",
        "cleanup": "remove_if_unchanged",
        "baseline": { "kind": "absent" }
    }])
    .to_string()
}

pub(super) fn topology_json(
    game_id: &GameId,
    payload_game_id: &str,
    outer_implementation: &str,
) -> String {
    let root = format!("{}/dxgi.dll", target_dir());
    serde_json::json!({
        "id": topology_id(game_id),
        "game_id": payload_game_id,
        "root_slot": root,
        "outer": {
            "implementation": outer_implementation,
            "path": root,
            "receipt": {
                "identity": "outer-identity",
                "digest": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "ownership": "owned"
            }
        },
        "downstream": null,
        "downstream_origin": null,
        "root_prestate": "absent"
    })
    .to_string()
}

pub(super) fn game_exists(
    storage: &SqliteStorage,
    game: &renderpilot_domain::GameInstallation,
) -> bool {
    storage.find_game(game.id()).expect("find game").is_some()
}

pub(super) fn optiscaler_row_count(storage: &SqliteStorage, game_id: &GameId) -> i64 {
    let connection = storage.connection().expect("connection");
    connection
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM optiscaler_install_states WHERE game_id = :game_id) +
                (SELECT COUNT(*) FROM game_proxy_topologies WHERE game_id = :game_id)",
            named_params! { ":game_id": game_id.as_str() },
            |row| row.get(0),
        )
        .expect("OptiScaler row count")
}

pub(super) fn optiscaler_topology_id(
    storage: &SqliteStorage,
    game: &renderpilot_domain::GameInstallation,
) -> String {
    let connection = storage.connection().expect("connection");
    connection
        .query_row(
            "SELECT id FROM game_proxy_topologies WHERE game_id = :game_id",
            named_params! { ":game_id": game.id().as_str() },
            |row| row.get(0),
        )
        .expect("topology id")
}

pub(super) fn optiscaler_state_topology_id(
    storage: &SqliteStorage,
    game: &renderpilot_domain::GameInstallation,
) -> String {
    let connection = storage.connection().expect("connection");
    connection
        .query_row(
            "SELECT proxy_topology_id FROM optiscaler_install_states WHERE game_id = :game_id",
            named_params! { ":game_id": game.id().as_str() },
            |row| row.get(0),
        )
        .expect("state topology id")
}

pub(super) fn assert_game_and_optiscaler_state_remain(
    storage: &SqliteStorage,
    game: &renderpilot_domain::GameInstallation,
) {
    assert!(game_exists(storage, game));
    assert!(optiscaler_row_count(storage, game.id()) > 0);
}

pub(super) fn assert_foreign_keys_are_valid(storage: &SqliteStorage) {
    let connection = storage.connection().expect("connection");
    let violations: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .expect("foreign key check");
    assert_eq!(violations, 0);
}

pub(super) fn seed_all_scoped_state(storage: &SqliteStorage) {
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
                INSERT INTO game_ui_state (game_id, is_favorite, is_hidden, updated_at)
                VALUES ('manual:child', 1, 0, 1);
                INSERT INTO profile_addon_capabilities (
                    game_id, addon_kind, source_revision, updated_at
                ) VALUES ('manual:child', 'RenoDx', 'revision', 1);
                "#,
            )
            .expect("seed scoped state");
}

pub(super) fn game(id: &str, path: &str) -> GameInstallation {
    GameInstallation::new(
        GameIdentity::new(GameId::new(id).expect("game id"), "Game", Launcher::Manual)
            .expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(path).expect("path"),
    )
    .with_root_authority(RootAuthority::Legacy)
}

pub(super) fn component(id: &str, game_id: &GameId, path: &str) -> LibraryComponent {
    LibraryComponent::new(
        ComponentId::new(id).expect("component id"),
        game_id.clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
    )
    .with_file(ComponentFile::new(PathRef::new(path).expect("path")))
}
