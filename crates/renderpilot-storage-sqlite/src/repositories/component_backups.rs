//! Persistence for component swap baselines (the pre-swap original file set).
//!
//! The `.bak` sidecars on disk hold the original *bytes*; this table holds their
//! *identity* so an N-to-1 rollback can restore exactly the right files even when
//! the active bundle was renamed (1→N upgrades) or re-swapped (A→B→C).
//!
//! Follows the `method()` / `method_within_transaction()` pattern: the public
//! method wraps in a fresh transaction; the `pub(super)` variant accepts an
//! existing one for composition in `game_mutations.rs`.

use std::collections::HashMap;

use renderpilot_application::AppResult;
use renderpilot_domain::{
    ComponentFile, ComponentId, ComponentRollbackBaseline, D3d12ExecutableBaseline,
    D3d12ExecutableIdentity, GameId, PathRef, Sha256Hash,
};
use rusqlite::{OptionalExtension, Transaction, named_params};
use serde::{Deserialize, Serialize};

use crate::{error::storage_error, mapping, sqlite_clock};

use super::SqliteStorage;

const SELECT_BACKUP_SQL: &str = "
    SELECT files_json, auxiliary_json
    FROM component_backups
    WHERE component_id = :component_id
";

const SELECT_BACKUPS_FOR_GAME_SQL: &str = "
    SELECT component_id, files_json, auxiliary_json
    FROM component_backups
    WHERE game_id = :game_id
";

const SELECT_ALL_BACKUPS_SQL: &str = "
    SELECT component_id, files_json, auxiliary_json
    FROM component_backups
    ORDER BY component_id
";

const CAPTURE_BACKUP_SQL: &str = "
    INSERT INTO component_backups
        (component_id, game_id, files_json, auxiliary_json, created_at, updated_at)
    VALUES
        (:component_id, :game_id, :files_json, :auxiliary_json, :now_ms, :now_ms)
";

const UPDATE_AUXILIARY_SQL: &str = "
    UPDATE component_backups
    SET auxiliary_json = :auxiliary_json,
        updated_at = :now_ms
    WHERE component_id = :component_id
";

const DELETE_BACKUP_SQL: &str = "
    DELETE FROM component_backups
    WHERE component_id = :component_id
";

/// Compatibility representation stored in the v12 `auxiliary_json` column.
///
/// The tagged array is intentionally private to SQLite. The domain aggregate
/// exposes an explicit optional D3D12 baseline and cannot represent duplicates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "baseline", rename_all = "snake_case")]
enum AuxiliaryBaselineWire {
    D3d12Executable {
        executable_path: PathRef,
        original_sha256: Sha256Hash,
        original_sdk_version: u32,
        expected_active_sha256: Sha256Hash,
        expected_active_sdk_version: u32,
    },
}

impl From<&D3d12ExecutableBaseline> for AuxiliaryBaselineWire {
    fn from(baseline: &D3d12ExecutableBaseline) -> Self {
        Self::D3d12Executable {
            executable_path: baseline.executable_path().clone(),
            original_sha256: baseline.original().sha256().clone(),
            original_sdk_version: baseline.original().sdk_version(),
            expected_active_sha256: baseline.expected_active().sha256().clone(),
            expected_active_sdk_version: baseline.expected_active().sdk_version(),
        }
    }
}

impl AuxiliaryBaselineWire {
    fn into_d3d12(self) -> D3d12ExecutableBaseline {
        let Self::D3d12Executable {
            executable_path,
            original_sha256,
            original_sdk_version,
            expected_active_sha256,
            expected_active_sdk_version,
        } = self;
        D3d12ExecutableBaseline::new(
            executable_path,
            D3d12ExecutableIdentity::new(original_sdk_version, original_sha256),
            D3d12ExecutableIdentity::new(expected_active_sdk_version, expected_active_sha256),
        )
    }
}

fn auxiliary_json(d3d12_executable: Option<&D3d12ExecutableBaseline>) -> AppResult<String> {
    let wire = d3d12_executable
        .map(AuxiliaryBaselineWire::from)
        .into_iter()
        .collect::<Vec<_>>();
    mapping::serialize_json(&wire)
}

impl SqliteStorage {
    /// Returns every persisted component baseline in one query.
    pub fn list_all_component_backups(
        &self,
    ) -> AppResult<HashMap<ComponentId, ComponentRollbackBaseline>> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare_cached(SELECT_ALL_BACKUPS_SQL)
                .map_err(storage_error)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(storage_error)?;

            let mut backups = HashMap::new();
            for row in rows {
                let (component_id, files_json, auxiliary_json) = row.map_err(storage_error)?;
                backups.insert(
                    mapping::component_id(component_id)?,
                    rollback_baseline(&files_json, &auxiliary_json)?,
                );
            }
            Ok(backups)
        })
    }

    /// Returns the recorded pre-swap baseline files for a component, if any.
    ///
    /// `Some` only means the identity row exists. Orchestration must still
    /// confirm that the corresponding sidecars or unchanged live bytes exist.
    pub fn get_component_backup(
        &self,
        component_id: &ComponentId,
    ) -> AppResult<Option<ComponentRollbackBaseline>> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare_cached(SELECT_BACKUP_SQL)
                .map_err(storage_error)?;

            let row: Option<(String, String)> = statement
                .query_row(
                    named_params! { ":component_id": component_id.as_str() },
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(storage_error)?;

            match row {
                Some((files_json, auxiliary_json)) => {
                    Ok(Some(rollback_baseline(&files_json, &auxiliary_json)?))
                }
                None => Ok(None),
            }
        })
    }

    /// Returns every persisted component baseline for a game in one query.
    pub fn component_backups_for_game(
        &self,
        game_id: &GameId,
    ) -> AppResult<HashMap<ComponentId, ComponentRollbackBaseline>> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare_cached(SELECT_BACKUPS_FOR_GAME_SQL)
                .map_err(storage_error)?;

            let rows = statement
                .query_map(named_params! { ":game_id": game_id.as_str() }, |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(storage_error)?;

            let mut backups = HashMap::new();
            for row in rows {
                let (component_id, files_json, auxiliary_json) = row.map_err(storage_error)?;
                backups.insert(
                    mapping::component_id(component_id)?,
                    rollback_baseline(&files_json, &auxiliary_json)?,
                );
            }
            Ok(backups)
        })
    }

    /// Explicitly sets the backup baseline for a component outside of a normal swap.
    /// Used by the scan process to automatically recover orphaned `.bak` files.
    pub fn recover_component_backup(
        &self,
        game_id: &GameId,
        component_id: &ComponentId,
        files: &[ComponentFile],
    ) -> AppResult<()> {
        self.recover_component_rollback_baseline(
            game_id,
            component_id,
            &ComponentRollbackBaseline::new(files.to_vec()),
        )
    }

    /// Recovers one complete rollback aggregate after database loss.
    pub fn recover_component_rollback_baseline(
        &self,
        game_id: &GameId,
        component_id: &ComponentId,
        baseline: &ComponentRollbackBaseline,
    ) -> AppResult<()> {
        self.with_transaction(|transaction| {
            if let Some(existing) =
                load_component_backup_within_transaction(transaction, component_id)?
            {
                return if existing == *baseline {
                    Ok(())
                } else {
                    Err(storage_error(
                        "recovered component rollback baseline conflicts with the immutable original",
                    ))
                };
            }
            capture_component_backup_within_transaction(
                transaction,
                game_id,
                component_id,
                baseline,
            )
        })
    }

    /// Attaches a missing D3D12 executable baseline to a recovered component aggregate.
    ///
    /// Recovery is idempotent for the same auxiliary kind. It never replaces an
    /// existing original identity.
    pub fn recover_component_d3d12_executable_baseline(
        &self,
        component_id: &ComponentId,
        executable: &D3d12ExecutableBaseline,
    ) -> AppResult<()> {
        self.with_transaction(|transaction| {
            let current = load_component_backup_within_transaction(transaction, component_id)?
                .ok_or_else(|| storage_error("component rollback baseline does not exist"))?;
            if let Some(existing) = current.d3d12_executable() {
                return if existing == executable {
                    Ok(())
                } else {
                    Err(storage_error(
                        "recovered D3D12 executable baseline conflicts with the immutable original",
                    ))
                };
            }
            capture_component_d3d12_executable_within_transaction(
                transaction,
                component_id,
                executable,
            )
        })
    }
}

pub(super) fn capture_component_backup_within_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
    component_id: &ComponentId,
    baseline: &ComponentRollbackBaseline,
) -> AppResult<()> {
    let now_ms = sqlite_clock::now_ms(transaction)?;
    let files_json = mapping::serialize_json(baseline.files())?;
    let auxiliary_json = auxiliary_json(baseline.d3d12_executable())?;

    transaction
        .prepare_cached(CAPTURE_BACKUP_SQL)
        .map_err(storage_error)?
        .execute(named_params! {
            ":component_id": component_id.as_str(),
            ":game_id": game_id.as_str(),
            ":files_json": files_json,
            ":auxiliary_json": auxiliary_json,
            ":now_ms": now_ms,
        })
        .map_err(storage_error)?;

    Ok(())
}

pub(super) fn update_component_d3d12_executable_state_within_transaction(
    transaction: &Transaction<'_>,
    component_id: &ComponentId,
    expected_active: &D3d12ExecutableIdentity,
) -> AppResult<()> {
    let current = load_component_backup_within_transaction(transaction, component_id)?
        .ok_or_else(|| storage_error("component rollback baseline does not exist"))?;
    let updated = current
        .with_expected_d3d12_identity(expected_active.clone())
        .ok_or_else(|| storage_error("D3D12 executable baseline does not exist"))?;

    let now_ms = sqlite_clock::now_ms(transaction)?;
    let auxiliary_json = auxiliary_json(updated.d3d12_executable())?;
    let changed = transaction
        .prepare_cached(UPDATE_AUXILIARY_SQL)
        .map_err(storage_error)?
        .execute(named_params! {
            ":component_id": component_id.as_str(),
            ":auxiliary_json": auxiliary_json,
            ":now_ms": now_ms,
        })
        .map_err(storage_error)?;
    if changed != 1 {
        return Err(storage_error("component rollback baseline does not exist"));
    }
    Ok(())
}

pub(super) fn capture_component_d3d12_executable_within_transaction(
    transaction: &Transaction<'_>,
    component_id: &ComponentId,
    executable: &D3d12ExecutableBaseline,
) -> AppResult<()> {
    let current = load_component_backup_within_transaction(transaction, component_id)?
        .ok_or_else(|| storage_error("component rollback baseline does not exist"))?;
    if current.d3d12_executable().is_some() {
        return Err(storage_error(
            "D3D12 executable baseline original has already been captured",
        ));
    }
    let now_ms = sqlite_clock::now_ms(transaction)?;
    let auxiliary_json = auxiliary_json(Some(executable))?;
    let changed = transaction
        .prepare_cached(UPDATE_AUXILIARY_SQL)
        .map_err(storage_error)?
        .execute(named_params! {
            ":component_id": component_id.as_str(),
            ":auxiliary_json": auxiliary_json,
            ":now_ms": now_ms,
        })
        .map_err(storage_error)?;
    if changed != 1 {
        return Err(storage_error("component rollback baseline does not exist"));
    }
    Ok(())
}

pub(super) fn delete_component_backup_within_transaction(
    transaction: &Transaction<'_>,
    component_id: &ComponentId,
) -> AppResult<()> {
    transaction
        .prepare_cached(DELETE_BACKUP_SQL)
        .map_err(storage_error)?
        .execute(named_params! { ":component_id": component_id.as_str() })
        .map_err(storage_error)?;

    Ok(())
}

fn load_component_backup_within_transaction(
    transaction: &Transaction<'_>,
    component_id: &ComponentId,
) -> AppResult<Option<ComponentRollbackBaseline>> {
    let row = transaction
        .prepare_cached(SELECT_BACKUP_SQL)
        .map_err(storage_error)?
        .query_row(
            named_params! { ":component_id": component_id.as_str() },
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(storage_error)?;
    row.map(|(files, auxiliary)| rollback_baseline(&files, &auxiliary))
        .transpose()
}

fn rollback_baseline(
    files_json: &str,
    auxiliary_json: &str,
) -> AppResult<ComponentRollbackBaseline> {
    let files = mapping::component_files(files_json)?;
    let auxiliary_files: Vec<AuxiliaryBaselineWire> = mapping::deserialize_json(auxiliary_json)?;
    if auxiliary_files.len() > 1 {
        return Err(storage_error(
            "component rollback baseline contains duplicate D3D12 executables",
        ));
    }
    Ok(ComponentRollbackBaseline::from_parts(
        files,
        auxiliary_files
            .into_iter()
            .next()
            .map(AuxiliaryBaselineWire::into_d3d12),
    ))
}

#[cfg(test)]
mod tests {
    use renderpilot_application::GameRepository;
    use renderpilot_domain::{
        D3d12ExecutableBaseline, D3d12ExecutableIdentity, GameIdentity, GameInstallation,
        GameRuntime, Launcher, PathRef, Platform, Sha256Hash,
    };

    use super::*;
    use crate::{ComponentBaselineMutation, GameMutationCommit, InstalledAddonMutation};

    #[test]
    fn component_backups_for_game_returns_complete_rows_for_only_that_game() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game_a = game("steam:1");
        let game_b = game("steam:2");
        storage.upsert_game(&game_a).expect("game a");
        storage.upsert_game(&game_b).expect("game b");

        let component_a = ComponentId::new("component:a").expect("component a");
        let component_b = ComponentId::new("component:b").expect("component b");
        let other_component = ComponentId::new("component:other").expect("other component");
        let baseline_a = baseline("C:/Games/Test/a.dll");
        let baseline_b = baseline("C:/Games/Test/b.dll");
        storage
            .recover_component_backup(game_a.id(), &component_a, &baseline_a)
            .expect("baseline a");
        storage
            .recover_component_backup(game_a.id(), &component_b, &baseline_b)
            .expect("baseline b");
        storage
            .recover_component_backup(game_b.id(), &other_component, &baseline("D:/other.dll"))
            .expect("other baseline");

        let backups = storage
            .component_backups_for_game(game_a.id())
            .expect("game backups");
        assert_eq!(backups.len(), 2);
        assert_eq!(
            backups
                .get(&component_a)
                .map(ComponentRollbackBaseline::files),
            Some(baseline_a.as_slice())
        );
        assert_eq!(
            backups
                .get(&component_b)
                .map(ComponentRollbackBaseline::files),
            Some(baseline_b.as_slice())
        );
        assert!(!backups.contains_key(&other_component));
    }

    #[test]
    fn recovery_never_overwrites_the_first_component_baseline() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game = game("steam:immutable");
        storage.upsert_game(&game).expect("game");
        let component = ComponentId::new("component:immutable").expect("component");
        let original = baseline("C:/Games/Test/original.dll");
        let later = baseline("C:/Games/Test/later.dll");

        storage
            .recover_component_backup(game.id(), &component, &original)
            .expect("first capture");
        storage
            .recover_component_backup(game.id(), &component, &original)
            .expect("identical recovery is idempotent");
        let error = storage
            .recover_component_backup(game.id(), &component, &later)
            .expect_err("a conflicting recovery must be rejected");
        assert!(
            error
                .to_string()
                .contains("conflicts with the immutable original")
        );

        assert_eq!(
            storage
                .get_component_backup(&component)
                .expect("query")
                .expect("baseline")
                .files(),
            original
        );
    }

    #[test]
    fn auxiliary_recovery_is_idempotent_but_never_replaces_original_identity() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game = game("steam:auxiliary-immutable");
        storage.upsert_game(&game).expect("game");
        let component = ComponentId::new("component:auxiliary-immutable").expect("component");
        storage
            .recover_component_backup(
                game.id(),
                &component,
                &baseline("C:/Games/Test/D3D12Core.dll"),
            )
            .expect("component baseline");

        let original = D3d12ExecutableBaseline::new(
            PathRef::new("C:/Games/Test/game.exe").expect("path"),
            D3d12ExecutableIdentity::new(606, Sha256Hash::new("a".repeat(64)).expect("hash")),
            D3d12ExecutableIdentity::new(619, Sha256Hash::new("b".repeat(64)).expect("hash")),
        );
        storage
            .recover_component_d3d12_executable_baseline(&component, &original)
            .expect("first recovery");
        storage
            .recover_component_d3d12_executable_baseline(&component, &original)
            .expect("same recovery is idempotent");

        let conflicting = D3d12ExecutableBaseline::new(
            PathRef::new("C:/Games/Test/game.exe").expect("path"),
            D3d12ExecutableIdentity::new(607, Sha256Hash::new("c".repeat(64)).expect("hash")),
            D3d12ExecutableIdentity::new(620, Sha256Hash::new("d".repeat(64)).expect("hash")),
        );
        assert!(
            storage
                .recover_component_d3d12_executable_baseline(&component, &conflicting)
                .is_err(),
            "recovery must not replace a previously captured original"
        );
        assert_eq!(
            storage
                .get_component_backup(&component)
                .expect("query")
                .expect("baseline")
                .d3d12_executable(),
            Some(&original)
        );
    }

    #[test]
    fn private_v12_wire_round_trips_into_the_explicit_domain_model() {
        let files_json = "[]";
        let auxiliary_json = serde_json::json!([{
            "kind": "d3d12_executable",
            "baseline": {
                "executable_path": "C:/Games/Test/game.exe",
                "original_sha256": "a".repeat(64),
                "original_sdk_version": 606,
                "expected_active_sha256": "b".repeat(64),
                "expected_active_sdk_version": 619
            }
        }])
        .to_string();

        let baseline = rollback_baseline(files_json, &auxiliary_json).expect("wire baseline");
        let executable = baseline.d3d12_executable().expect("executable");
        assert_eq!(executable.original().sdk_version(), 606);
        assert_eq!(executable.expected_active().sdk_version(), 619);
        let encoded = auxiliary_json_for_test(executable);
        assert_eq!(
            rollback_baseline(files_json, &encoded).expect("round trip"),
            baseline
        );
    }

    #[test]
    fn private_v12_wire_rejects_duplicate_d3d12_records() {
        let record = serde_json::json!({
            "kind": "d3d12_executable",
            "baseline": {
                "executable_path": "C:/Games/Test/game.exe",
                "original_sha256": "a".repeat(64),
                "original_sdk_version": 606,
                "expected_active_sha256": "b".repeat(64),
                "expected_active_sdk_version": 619
            }
        });
        let json = serde_json::to_string(&vec![record.clone(), record]).expect("json");
        assert!(rollback_baseline("[]", &json).is_err());
    }

    #[test]
    fn state_mutation_cannot_replace_original_executable_identity() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game = game("steam:state-only");
        storage.upsert_game(&game).expect("game");
        let component = ComponentId::new("component:state-only").expect("component");
        let original = D3d12ExecutableIdentity::new(606, Sha256Hash::new("a".repeat(64)).unwrap());
        let baseline = ComponentRollbackBaseline::new(Vec::new()).with_d3d12_executable(
            D3d12ExecutableBaseline::new(
                PathRef::new("C:/Games/Test/game.exe").unwrap(),
                original.clone(),
                original.clone(),
            ),
        );
        storage
            .recover_component_rollback_baseline(game.id(), &component, &baseline)
            .expect("capture");
        let active = D3d12ExecutableIdentity::new(619, Sha256Hash::new("b".repeat(64)).unwrap());
        storage
            .commit_game_mutation(GameMutationCommit {
                game_id: game.id(),
                component_set: None,
                baseline_mutations: &[ComponentBaselineMutation::UpdateD3d12ExecutableState {
                    component_id: &component,
                    expected_active: &active,
                }],
                addon: InstalledAddonMutation::Keep,
                mutation_id: None,
            })
            .expect("state update");
        let updated = storage
            .get_component_backup(&component)
            .expect("query")
            .expect("baseline");
        let executable = updated.d3d12_executable().expect("executable");
        assert_eq!(executable.original(), &original);
        assert_eq!(executable.expected_active(), &active);
    }

    #[test]
    fn state_mutation_rolls_back_when_the_durable_commit_marker_fails() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game = game("steam:state-atomic");
        storage.upsert_game(&game).expect("game");
        let component = ComponentId::new("component:state-atomic").expect("component");
        let original = D3d12ExecutableIdentity::new(
            606,
            Sha256Hash::new("a".repeat(64)).expect("original hash"),
        );
        let baseline = ComponentRollbackBaseline::new(Vec::new()).with_d3d12_executable(
            D3d12ExecutableBaseline::new(
                PathRef::new("C:/Games/Test/game.exe").expect("executable path"),
                original.clone(),
                original.clone(),
            ),
        );
        storage
            .recover_component_rollback_baseline(game.id(), &component, &baseline)
            .expect("capture");

        let uncommitted_active = D3d12ExecutableIdentity::new(
            619,
            Sha256Hash::new("b".repeat(64)).expect("active hash"),
        );
        storage
            .commit_game_mutation(GameMutationCommit {
                game_id: game.id(),
                component_set: None,
                baseline_mutations: &[ComponentBaselineMutation::UpdateD3d12ExecutableState {
                    component_id: &component,
                    expected_active: &uncommitted_active,
                }],
                addon: InstalledAddonMutation::Keep,
                mutation_id: Some("missing-durable-mutation"),
            })
            .expect_err("missing durable marker must roll back the whole transaction");

        let persisted = storage
            .get_component_backup(&component)
            .expect("query")
            .expect("baseline");
        let executable = persisted.d3d12_executable().expect("executable");
        assert_eq!(executable.original(), &original);
        assert_eq!(
            executable.expected_active(),
            &original,
            "an uncommitted active identity must never escape the transaction"
        );
    }

    fn auxiliary_json_for_test(executable: &D3d12ExecutableBaseline) -> String {
        auxiliary_json(Some(executable)).expect("auxiliary json")
    }

    fn game(id: &str) -> GameInstallation {
        GameInstallation::new(
            GameIdentity::new(
                GameId::new(id).expect("game id"),
                format!("Game {id}"),
                Launcher::Manual,
            )
            .expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(format!("C:/Games/{id}")).expect("install path"),
        )
    }

    fn baseline(path: &str) -> Vec<ComponentFile> {
        vec![ComponentFile::new(
            PathRef::new(path).expect("component path"),
        )]
    }
}
