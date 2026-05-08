mod artifacts;
mod backups;
mod catalog_select_sql;
mod columns;
mod components;
pub mod file_hash_cache;
mod games;
mod operations;
mod row_mapping;

use std::sync::Mutex;

use renderpilot_application::AppResult;
use renderpilot_domain::{GameInstallation, GraphicsComponent, LibraryArtifact};
use rusqlite::{Connection, Params, Transaction};

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
            persist_scan_result_in_transaction(transaction, game, components, artifacts)
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

fn persist_scan_result_in_transaction(
    transaction: &Transaction<'_>,
    game: &GameInstallation,
    components: &[GraphicsComponent],
    artifacts: &[LibraryArtifact],
) -> AppResult<()> {
    games::upsert_game_within_transaction(transaction, game)?;
    components::replace_components_for_game_within_transaction(transaction, game.id(), components)?;
    artifacts::upsert_artifacts_within_transaction(transaction, artifacts)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use renderpilot_application::{
        AppError, AppErrorKind, ComponentRepository, GameRepository, OperationItemRecord,
        OperationJournalEntry, OperationKind, OperationRecord, OperationRepository,
        OperationStatus, UnixTimestampMillis,
    };
    use renderpilot_domain::{
        ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
        GameIdentity, GameInstallation, GameRuntime, GraphicsComponent, GraphicsTechnology,
        Launcher, LibraryArtifact, OperationId, PathRef, Platform, Sha256Hash, Swappability,
    };

    use super::SqliteStorage;

    const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    const GAME_EXISTING: &str = "game:existing";
    const GAME_MISSING: &str = "game:missing";
    const GAME_SCAN_ROLLBACK: &str = "game:scan-rollback";

    const COMPONENT_EXISTING: &str = "component:existing";
    const COMPONENT_MISSING: &str = "component:missing";
    const COMPONENT_ORPHAN: &str = "component:orphan";
    const COMPONENT_SCAN_ROLLBACK: &str = "component:scan-rollback";

    const OPERATION_EXISTING: &str = "operation:existing";
    const OPERATION_NEW: &str = "operation:new";

    #[test]
    fn save_scan_result_rolls_back_game_and_components_when_artifact_insert_fails() {
        let fixture = StorageFixture::new();

        let game = sample_game(GAME_SCAN_ROLLBACK);
        let component = sample_component(COMPONENT_SCAN_ROLLBACK, game.id().as_str());

        let invalid_artifact = sample_artifact(
            "artifact:bad-source-game",
            "C:/Games/Test/nvngx_dlss.dll",
            HASH_A,
        )
        .with_source_game_id(game_id(GAME_MISSING));

        let error = fixture
            .storage
            .save_scan_result(&game, &[component], &[invalid_artifact])
            .expect_err("invalid artifact FK should fail the whole scan transaction");

        assert_storage_failed(&error);
        fixture.assert_game_absent(game.id());
        fixture.assert_components_empty(game.id());
    }

    #[test]
    fn replace_components_for_game_keeps_existing_rows_when_replacement_fails() {
        let fixture = StorageFixture::new();

        let game = sample_game(GAME_EXISTING);
        let existing_component = sample_component(COMPONENT_EXISTING, game.id().as_str());
        let invalid_component = sample_component(COMPONENT_ORPHAN, GAME_MISSING);

        fixture.store_game(&game);
        fixture.replace_components(game.id(), std::slice::from_ref(&existing_component));

        let error = fixture
            .storage
            .replace_components_for_game(game.id(), &[invalid_component])
            .expect_err("invalid replacement should fail");

        assert_storage_failed(&error);
        fixture.assert_components(game.id(), vec![existing_component]);
    }

    #[test]
    fn save_operation_entry_rolls_back_item_replace_on_insert_failure() {
        let fixture = StorageFixture::new();

        let game = sample_game(GAME_EXISTING);
        let component = sample_component(COMPONENT_EXISTING, game.id().as_str());
        let operation = sample_operation(OPERATION_EXISTING, game.id().as_str());
        let existing_item = sample_operation_item(OPERATION_EXISTING, COMPONENT_EXISTING);

        fixture.store_game(&game);
        fixture.replace_components(game.id(), &[component]);
        fixture.save_operation_entry_parts(&operation, std::slice::from_ref(&existing_item));

        let invalid_item = sample_operation_item(OPERATION_EXISTING, COMPONENT_MISSING);

        let invalid_entry = OperationJournalEntry::try_new(operation.clone(), vec![invalid_item])
            .expect("journal entry should be valid");

        let error = fixture
            .storage
            .save_operation_entry(&invalid_entry)
            .expect_err("foreign-key violation should fail");

        assert_storage_failed(&error);
        fixture.assert_operation_items(&operation.id, vec![existing_item]);
    }

    #[test]
    fn save_operation_entry_rolls_back_operation_when_items_fail() {
        let fixture = StorageFixture::new();

        let game = sample_game(GAME_EXISTING);
        let component = sample_component(COMPONENT_EXISTING, game.id().as_str());
        let operation = sample_operation(OPERATION_NEW, game.id().as_str());
        let invalid_item = sample_operation_item(OPERATION_NEW, COMPONENT_MISSING);

        fixture.store_game(&game);
        fixture.replace_components(game.id(), &[component]);

        let entry = OperationJournalEntry::try_new(operation.clone(), vec![invalid_item])
            .expect("journal entry should be valid");

        let error = fixture
            .storage
            .save_operation_entry(&entry)
            .expect_err("foreign-key violation should fail");

        assert_storage_failed(&error);
        fixture.assert_operation_absent(&operation.id);
    }

    struct StorageFixture {
        storage: SqliteStorage,
    }

    impl StorageFixture {
        #[track_caller]
        fn new() -> Self {
            Self {
                storage: SqliteStorage::in_memory().expect("in-memory sqlite should open"),
            }
        }

        #[track_caller]
        fn store_game(&self, game: &GameInstallation) {
            self.storage
                .upsert_game(game)
                .expect("game should be stored");
        }

        #[track_caller]
        fn replace_components(&self, game_id: &GameId, components: &[GraphicsComponent]) {
            self.storage
                .replace_components_for_game(game_id, components)
                .expect("component set should be stored");
        }

        #[track_caller]
        fn save_operation_entry_parts(
            &self,
            operation: &OperationRecord,
            items: &[OperationItemRecord],
        ) {
            let entry = OperationJournalEntry::try_new(operation.clone(), items.to_vec())
                .expect("operation journal entry should be valid");
            self.storage
                .save_operation_entry(&entry)
                .expect("operation journal entry should be stored");
        }

        #[track_caller]
        fn assert_game_absent(&self, game_id: &GameId) {
            assert_eq!(
                self.storage
                    .find_game(game_id)
                    .expect("find_game should succeed"),
                None,
                "game row should be rolled back",
            );
        }

        #[track_caller]
        fn assert_operation_absent(&self, operation_id: &OperationId) {
            assert_eq!(
                self.storage
                    .find_operation_entry(operation_id)
                    .expect("find_operation_entry should succeed"),
                None,
                "operation row should be rolled back",
            );
        }

        #[track_caller]
        fn assert_components_empty(&self, game_id: &GameId) {
            assert!(
                self.storage
                    .list_components_for_game(game_id)
                    .expect("components should list")
                    .is_empty(),
                "components should be rolled back",
            );
        }

        #[track_caller]
        fn assert_components(&self, game_id: &GameId, expected: Vec<GraphicsComponent>) {
            assert_eq!(
                self.storage
                    .list_components_for_game(game_id)
                    .expect("components should list"),
                expected,
            );
        }

        #[track_caller]
        fn assert_operation_items(
            &self,
            operation_id: &OperationId,
            expected: Vec<OperationItemRecord>,
        ) {
            let entry = self
                .storage
                .find_operation_entry(operation_id)
                .expect("find_operation_entry should succeed")
                .expect("operation should exist");
            assert_eq!(entry.items(), expected.as_slice());
        }
    }

    #[track_caller]
    fn assert_storage_failed(error: &AppError) {
        assert_eq!(error.kind(), &AppErrorKind::StorageFailed);
    }

    fn sample_game(id: &str) -> GameInstallation {
        let identity = GameIdentity::new(game_id(id), "Test Game", Launcher::Steam)
            .expect("game identity should be valid");

        GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            path_ref("C:/Games/Test"),
        )
    }

    fn sample_component(component_id: &str, game_id: &str) -> GraphicsComponent {
        GraphicsComponent::new(
            component_id_from(component_id),
            game_id_from(game_id),
            ComponentKind::NativeLibrary,
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
        )
        .with_file(ComponentFile::new(component_path(component_id)))
    }

    fn sample_operation(operation_id: &str, game_id: &str) -> OperationRecord {
        OperationRecord::new(
            operation_id_from(operation_id),
            game_id_from(game_id),
            OperationKind::Scan,
            OperationStatus::Planned,
            UnixTimestampMillis::new(1).expect("timestamp should be valid"),
        )
    }

    fn sample_operation_item(operation_id: &str, component_id: &str) -> OperationItemRecord {
        OperationItemRecord::new(
            operation_id_from(operation_id),
            component_id_from(component_id),
            component_path(component_id),
            OperationStatus::Planned,
        )
    }

    fn sample_artifact(id: &str, path: &str, sha256: &str) -> LibraryArtifact {
        LibraryArtifact::new(
            ArtifactId::new(id).expect("artifact id should be valid"),
            GraphicsTechnology::DlssSuperResolution,
            "nvngx_dlss.dll",
            ComponentFile::new(path_ref(path)).with_sha256(sha256_hash(sha256)),
            ArtifactTrustLevel::LocalObserved,
        )
        .expect("artifact should be valid")
        .with_source("scan-folder")
        .expect("artifact source should be valid")
    }

    fn component_path(component_id: &str) -> PathRef {
        path_ref(format!(
            "C:/Games/Test/{}.dll",
            component_id.replace(':', "_"),
        ))
    }

    fn game_id(id: &str) -> GameId {
        GameId::new(id).expect("game id should be valid")
    }

    fn game_id_from(id: &str) -> GameId {
        game_id(id)
    }

    fn component_id_from(id: &str) -> ComponentId {
        ComponentId::new(id).expect("component id should be valid")
    }

    fn operation_id_from(id: &str) -> OperationId {
        OperationId::new(id).expect("operation id should be valid")
    }

    fn path_ref(path: impl Into<String>) -> PathRef {
        PathRef::new(path.into()).expect("path should be valid")
    }

    fn sha256_hash(value: &str) -> Sha256Hash {
        Sha256Hash::new(value).expect("sha256 should be valid")
    }
}
