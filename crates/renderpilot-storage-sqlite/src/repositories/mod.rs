mod artifacts;
mod backups;
mod components;
pub mod file_hash_cache;
mod games;
mod operations;
mod row_mapping;

use std::sync::Mutex;

use renderpilot_application::AppResult;
use renderpilot_domain::{GameInstallation, GraphicsComponent, LibraryArtifact};
use rusqlite::{Connection, Params};

use crate::error::storage_error;

/// SQLite-backed storage adapter implementing application repository ports.
#[derive(Debug)]
pub struct SqliteStorage {
    pub(crate) connection: Mutex<Connection>,
}

impl SqliteStorage {
    /// Stores a complete scan result atomically in one database transaction.
    ///
    /// The scan result is persisted as one unit:
    ///
    /// - the game row is inserted or updated;
    /// - previous components for the game are replaced;
    /// - discovered artifacts are upserted.
    ///
    /// If any step fails, the whole scan result is rolled back. This prevents
    /// partially updated catalog state after failed scans.
    ///
    /// Row-level helpers in `games`, `components`, and `artifacts` intentionally
    /// accept an existing connection/transaction and must not start their own
    /// transactions. This keeps the scan write as one atomic unit.
    pub fn save_scan_result(
        &self,
        game: &GameInstallation,
        components: &[GraphicsComponent],
        artifacts: &[LibraryArtifact],
    ) -> AppResult<()> {
        self.with_transaction(|transaction| {
            save_scan_result_in_transaction(transaction, game, components, artifacts)
        })
    }

    pub(super) fn query_list<T, P, F>(&self, sql: &str, params: P, map: F) -> AppResult<Vec<T>>
    where
        P: Params,
        F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<AppResult<T>>,
    {
        let connection = self.connection()?;
        let mut statement = connection.prepare_cached(sql).map_err(storage_error)?;
        let rows = statement.query_map(params, map).map_err(storage_error)?;

        row_mapping::collect_rows(rows)
    }
}

fn save_scan_result_in_transaction(
    connection: &Connection,
    game: &GameInstallation,
    components: &[GraphicsComponent],
    artifacts: &[LibraryArtifact],
) -> AppResult<()> {
    games::upsert_game_in_connection(connection, game)?;
    components::replace_components_for_game_in_connection(connection, game.id(), components)?;
    artifacts::upsert_artifacts_in_connection(connection, artifacts)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use renderpilot_application::{
        AppErrorKind, ComponentRepository, GameRepository, OperationItemRecord, OperationKind,
        OperationRecord, OperationRepository, OperationStatus, UnixTimestampMillis,
    };
    use renderpilot_domain::{
        ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
        GameIdentity, GameInstallation, GameRuntime, GraphicsComponent, GraphicsTechnology,
        Launcher, LibraryArtifact, OperationId, PathRef, Platform, Sha256Hash, Swappability,
    };

    use super::SqliteStorage;

    const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[test]
    fn save_scan_result_rolls_back_game_and_components_when_artifact_insert_fails() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");

        let game = sample_game("game:scan-rollback");
        let component = sample_component("component:scan-rollback", game.id().as_str());

        let invalid_artifact = sample_artifact(
            "artifact:bad-source-game",
            "C:/Games/Test/nvngx_dlss.dll",
            HASH_A,
        )
        .with_source_game_id(GameId::new("game:missing").expect("game id should be valid"));

        let error = storage
            .save_scan_result(&game, &[component], &[invalid_artifact])
            .expect_err("invalid artifact FK should fail the whole scan transaction");

        assert_storage_failed(&error);

        assert_eq!(
            storage
                .find_game(game.id())
                .expect("find_game should succeed"),
            None,
            "game row should be rolled back",
        );

        assert!(
            storage
                .list_components_for_game(game.id())
                .expect("components should list")
                .is_empty(),
            "components should be rolled back",
        );
    }

    #[test]
    fn replace_components_for_game_keeps_existing_rows_when_replacement_fails() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");

        let game = sample_game("game:existing");
        let existing_component = sample_component("component:existing", "game:existing");
        let invalid_component = sample_component("component:orphan", "game:missing");

        store_game(&storage, &game);
        replace_components(&storage, game.id(), &[existing_component.clone()]);

        let error = storage
            .replace_components_for_game(game.id(), &[invalid_component])
            .expect_err("invalid replacement should fail");

        assert_storage_failed(&error);

        assert_eq!(
            storage
                .list_components_for_game(game.id())
                .expect("existing components should remain after rollback"),
            vec![existing_component],
        );
    }

    #[test]
    fn replace_operation_items_rolls_back_delete_on_insert_failure() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");

        let game = sample_game("game:existing");
        let component = sample_component("component:existing", "game:existing");
        let operation = sample_operation("operation:existing", "game:existing");
        let existing_item = sample_operation_item("operation:existing", "component:existing");

        store_game(&storage, &game);
        replace_components(&storage, game.id(), &[component]);
        store_operation(&storage, &operation);
        replace_operation_items(&storage, &operation.id, &[existing_item.clone()]);

        let error = storage
            .replace_operation_items(
                &operation.id,
                &[sample_operation_item(
                    "operation:existing",
                    "component:missing",
                )],
            )
            .expect_err("foreign-key violation should fail");

        assert_storage_failed(&error);

        assert_eq!(
            storage
                .list_operation_items(&operation.id)
                .expect("existing operation items should remain after rollback"),
            vec![existing_item],
        );
    }

    fn assert_storage_failed(error: &renderpilot_application::AppError) {
        assert_eq!(error.kind(), AppErrorKind::StorageFailed);
    }

    fn store_game(storage: &SqliteStorage, game: &GameInstallation) {
        storage.upsert_game(game).expect("game should be stored");
    }

    fn replace_components(
        storage: &SqliteStorage,
        game_id: &GameId,
        components: &[GraphicsComponent],
    ) {
        storage
            .replace_components_for_game(game_id, components)
            .expect("component set should be stored");
    }

    fn store_operation(storage: &SqliteStorage, operation: &OperationRecord) {
        storage
            .upsert_operation(operation)
            .expect("operation should be stored");
    }

    fn replace_operation_items(
        storage: &SqliteStorage,
        operation_id: &OperationId,
        items: &[OperationItemRecord],
    ) {
        storage
            .replace_operation_items(operation_id, items)
            .expect("operation items should be stored");
    }

    fn sample_game(id: &str) -> GameInstallation {
        let identity = GameIdentity::new(
            GameId::new(id).expect("game id should be valid"),
            "Test Game",
            Launcher::Steam,
        )
        .expect("game identity should be valid");

        GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new("C:/Games/Test").expect("install path should be valid"),
        )
    }

    fn sample_component(component_id: &str, game_id: &str) -> GraphicsComponent {
        GraphicsComponent::new(
            ComponentId::new(component_id).expect("component id should be valid"),
            GameId::new(game_id).expect("game id should be valid"),
            ComponentKind::NativeLibrary,
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
        )
        .with_file(ComponentFile::new(component_path(component_id)))
    }

    fn sample_operation(operation_id: &str, game_id: &str) -> OperationRecord {
        OperationRecord::new(
            OperationId::new(operation_id).expect("operation id should be valid"),
            GameId::new(game_id).expect("game id should be valid"),
            OperationKind::Scan,
            OperationStatus::Planned,
            UnixTimestampMillis::new(1).expect("timestamp should be valid"),
        )
    }

    fn sample_operation_item(operation_id: &str, component_id: &str) -> OperationItemRecord {
        OperationItemRecord::new(
            OperationId::new(operation_id).expect("operation id should be valid"),
            ComponentId::new(component_id).expect("component id should be valid"),
            component_path(component_id),
            OperationStatus::Planned,
        )
    }

    fn sample_artifact(id: &str, path: &str, sha256: &str) -> LibraryArtifact {
        LibraryArtifact::new(
            ArtifactId::new(id).expect("artifact id should be valid"),
            GraphicsTechnology::DlssSuperResolution,
            "nvngx_dlss.dll",
            ComponentFile::new(PathRef::new(path).expect("artifact path should be valid"))
                .with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid")),
            ArtifactTrustLevel::LocalObserved,
        )
        .expect("artifact should be valid")
        .with_source("scan-folder")
        .expect("artifact source should be valid")
    }

    fn component_path(component_id: &str) -> PathRef {
        PathRef::new(format!(
            "C:/Games/Test/{}.dll",
            component_id.replace(':', "_"),
        ))
        .expect("component path should be valid")
    }
}
