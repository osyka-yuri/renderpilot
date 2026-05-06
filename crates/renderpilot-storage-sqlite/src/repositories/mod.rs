mod artifacts;
mod backups;
mod components;
mod games;
mod operations;
mod row_mapping;

use std::sync::Mutex;

use renderpilot_application::{AppResult, ArtifactRepository, ComponentRepository, GameRepository};
use renderpilot_domain::{GameInstallation, GraphicsComponent, LibraryArtifact};
use rusqlite::{Connection, Params};

use crate::error::storage_error;

/// SQLite-backed storage adapter implementing application repository ports.
#[derive(Debug)]
pub struct SqliteStorage {
    pub(crate) connection: Mutex<Connection>,
}

impl SqliteStorage {
    /// Stores one scan result: the game row and the latest component list.
    pub fn save_scan_result(
        &self,
        game: &GameInstallation,
        components: &[GraphicsComponent],
        artifacts: &[LibraryArtifact],
    ) -> AppResult<()> {
        self.upsert_game(game)?;
        self.replace_components_for_game(game.id(), components)?;

        for artifact in artifacts {
            self.upsert_artifact(artifact)?;
        }

        Ok(())
    }

    pub(super) fn query_list<T, P, F>(&self, sql: &str, params: P, map: F) -> AppResult<Vec<T>>
    where
        P: Params,
        F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<AppResult<T>>,
    {
        let connection = self.connection()?;
        let mut statement = connection.prepare(sql).map_err(storage_error)?;
        let rows = statement.query_map(params, map).map_err(storage_error)?;

        row_mapping::collect_rows(rows)
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_application::{
        AppErrorKind, ComponentRepository, GameRepository, OperationItemRecord, OperationKind,
        OperationRecord, OperationRepository, OperationStatus, UnixTimestampMillis,
    };
    use renderpilot_domain::{
        ComponentFile, ComponentId, ComponentKind, GameId, GameIdentity, GameInstallation,
        GameRuntime, GraphicsComponent, GraphicsTechnology, Launcher, OperationId, PathRef,
        Platform, Swappability,
    };

    use super::SqliteStorage;

    #[test]
    fn replace_components_for_game_rolls_back_delete_on_insert_failure() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
        let game = sample_game("game:existing");
        let existing_component = sample_component("component:existing", "game:existing");

        storage.upsert_game(&game).expect("game should be stored");
        storage
            .replace_components_for_game(game.id(), &[existing_component.clone()])
            .expect("initial component set should be stored");

        let error = storage
            .replace_components_for_game(
                game.id(),
                &[sample_component("component:orphan", "game:missing")],
            )
            .expect_err("foreign-key violation should fail");

        assert_eq!(error.kind(), AppErrorKind::StorageFailed);
        assert_eq!(
            storage
                .list_components_for_game(game.id())
                .expect("existing components should remain after rollback"),
            vec![existing_component]
        );
    }

    #[test]
    fn replace_operation_items_rolls_back_delete_on_insert_failure() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
        let game = sample_game("game:existing");
        let component = sample_component("component:existing", "game:existing");
        let operation = sample_operation("operation:existing", "game:existing");
        let existing_item = sample_operation_item("operation:existing", "component:existing");

        storage.upsert_game(&game).expect("game should be stored");
        storage
            .replace_components_for_game(game.id(), &[component])
            .expect("component should be stored");
        storage
            .upsert_operation(&operation)
            .expect("operation should be stored");
        storage
            .replace_operation_items(&operation.id, &[existing_item.clone()])
            .expect("initial operation items should be stored");

        let error = storage
            .replace_operation_items(
                &operation.id,
                &[sample_operation_item(
                    "operation:existing",
                    "component:missing",
                )],
            )
            .expect_err("foreign-key violation should fail");

        assert_eq!(error.kind(), AppErrorKind::StorageFailed);
        assert_eq!(
            storage
                .list_operation_items(&operation.id)
                .expect("existing operation items should remain after rollback"),
            vec![existing_item]
        );
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
        .with_file(ComponentFile::new(
            PathRef::new(format!(
                "C:/Games/Test/{}.dll",
                component_id.replace(':', "_")
            ))
            .expect("component path should be valid"),
        ))
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
            PathRef::new(format!(
                "C:/Games/Test/{}.dll",
                component_id.replace(':', "_")
            ))
            .expect("source path should be valid"),
            OperationStatus::Planned,
        )
    }
}
