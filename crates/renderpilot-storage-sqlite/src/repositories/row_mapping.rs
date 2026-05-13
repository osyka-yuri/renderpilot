use renderpilot_application::{
    AppResult, BackupId, BackupRecord, OperationItemRecord, OperationKind, OperationRecord,
    OperationStatus, UnixTimestampMillis,
};
use renderpilot_domain::{
    ComponentFile, GameIdentity, GameInstallation, GraphicsComponent, LibraryArtifact, PathRef,
};
use rusqlite::types::FromSql;

use crate::{
    error::{invalid_row, storage_error},
    mapping,
};

use super::columns;

pub(super) fn game_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AppResult<GameInstallation>> {
    read_domain_row::<GameRow>(row)
}

pub(super) fn component_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AppResult<GraphicsComponent>> {
    read_domain_row::<ComponentRow>(row)
}

pub(super) fn artifact_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AppResult<LibraryArtifact>> {
    read_domain_row::<ArtifactRow>(row)
}

pub(super) fn operation_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AppResult<OperationRecord>> {
    read_domain_row::<OperationRow>(row)
}

pub(super) fn operation_item_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AppResult<OperationItemRecord>> {
    read_domain_row::<OperationItemRow>(row)
}

pub(super) fn backup_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AppResult<BackupRecord>> {
    read_domain_row::<BackupRow>(row)
}

pub(super) fn collect_rows<T, F>(rows: rusqlite::MappedRows<'_, F>) -> AppResult<Vec<T>>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<AppResult<T>>,
{
    rows.map(|row| row.map_err(storage_error)?)
        .collect::<AppResult<Vec<_>>>()
}

trait SqliteRowReadExt {
    fn read<T>(&self, column: &'static str) -> rusqlite::Result<T>
    where
        T: FromSql;
}

impl SqliteRowReadExt for rusqlite::Row<'_> {
    fn read<T>(&self, column: &'static str) -> rusqlite::Result<T>
    where
        T: FromSql,
    {
        self.get(column)
    }
}

trait DomainRow: Sized {
    type Domain;

    fn from_sqlite_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self>;
    fn into_domain(self) -> AppResult<Self::Domain>;
}

fn read_domain_row<R>(row: &rusqlite::Row<'_>) -> rusqlite::Result<AppResult<R::Domain>>
where
    R: DomainRow,
{
    R::from_sqlite_row(row).map(R::into_domain)
}

fn operation_kind(value: String) -> AppResult<OperationKind> {
    OperationKind::from_storage(value).map_err(invalid_row)
}

fn operation_status(value: String) -> AppResult<OperationStatus> {
    OperationStatus::from_storage(value).map_err(invalid_row)
}

fn timestamp_millis(value: i64) -> AppResult<UnixTimestampMillis> {
    UnixTimestampMillis::new(value).map_err(invalid_row)
}

fn backup_id(value: String) -> AppResult<BackupId> {
    BackupId::new(value).map_err(invalid_row)
}

fn with_optional<T, V>(
    value: T,
    option: Option<V>,
    apply: impl FnOnce(T, V) -> AppResult<T>,
) -> AppResult<T> {
    match option {
        Some(option) => apply(value, option),
        None => Ok(value),
    }
}

#[derive(Debug)]
struct GameRow {
    id: String,
    title: String,
    launcher: String,
    external_id: Option<String>,
    platform: String,
    runtime: String,
    install_path: String,
    executable_candidates_json: String,
}

impl DomainRow for GameRow {
    type Domain = GameInstallation;

    fn from_sqlite_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        use columns::projection::game as g;

        Ok(Self {
            id: row.read(g::ID)?,
            title: row.read(g::TITLE)?,
            launcher: row.read(g::LAUNCHER)?,
            external_id: row.read(g::EXTERNAL_ID)?,
            platform: row.read(g::PLATFORM)?,
            runtime: row.read(g::RUNTIME)?,
            install_path: row.read(g::INSTALL_PATH)?,
            executable_candidates_json: row.read(g::EXECUTABLE_CANDIDATES_JSON)?,
        })
    }

    fn into_domain(self) -> AppResult<GameInstallation> {
        let Self {
            id,
            title,
            launcher,
            external_id,
            platform,
            runtime,
            install_path,
            executable_candidates_json,
        } = self;

        let identity = game_identity(id, title, launcher, external_id)?;
        let platform = mapping::platform(platform)?;
        let runtime = mapping::runtime(runtime)?;
        let install_path = mapping::path_ref(install_path)?;
        let executable_candidates =
            mapping::deserialize_json::<Vec<PathRef>>(&executable_candidates_json)?;

        Ok(executable_candidates.into_iter().fold(
            GameInstallation::new(identity, platform, runtime, install_path),
            GameInstallation::with_executable_candidate,
        ))
    }
}

fn game_identity(
    id: String,
    title: String,
    launcher: String,
    external_id: Option<String>,
) -> AppResult<GameIdentity> {
    let identity = GameIdentity::new(mapping::game_id(id)?, title, mapping::launcher(launcher)?)
        .map_err(invalid_row)?;

    with_optional(identity, external_id, |identity, external_id| {
        identity.with_external_id(external_id).map_err(invalid_row)
    })
}

#[derive(Debug)]
struct ComponentRow {
    id: String,
    game_id: String,
    kind: String,
    technology: String,
    swappability: String,
    files_json: String,
}

impl DomainRow for ComponentRow {
    type Domain = GraphicsComponent;

    fn from_sqlite_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        use columns::projection::component as c;

        Ok(Self {
            id: row.read(c::ID)?,
            game_id: row.read(c::GAME_ID)?,
            kind: row.read(c::KIND)?,
            technology: row.read(c::TECHNOLOGY)?,
            swappability: row.read(c::SWAPPABILITY)?,
            files_json: row.read(c::FILES_JSON)?,
        })
    }

    fn into_domain(self) -> AppResult<GraphicsComponent> {
        let Self {
            id,
            game_id,
            kind,
            technology,
            swappability,
            files_json,
        } = self;

        let component = GraphicsComponent::new(
            mapping::component_id(id)?,
            mapping::game_id(game_id)?,
            mapping::component_kind(kind)?,
            mapping::graphics_technology(technology)?,
            mapping::swappability(swappability)?,
        );
        let files = mapping::component_files(files_json)?;

        Ok(files
            .into_iter()
            .fold(component, GraphicsComponent::with_file))
    }
}

#[derive(Debug)]
struct ArtifactRow {
    id: String,
    technology: String,
    file_name: String,
    file_path: String,
    version: Option<String>,
    sha256: String,
    source: Option<String>,
    source_game_id: Option<String>,
    trust_level: String,
}

impl DomainRow for ArtifactRow {
    type Domain = LibraryArtifact;

    fn from_sqlite_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        use columns::projection::artifact as a;

        Ok(Self {
            id: row.read(a::ID)?,
            technology: row.read(a::TECHNOLOGY)?,
            file_name: row.read(a::FILE_NAME)?,
            file_path: row.read(a::FILE_PATH)?,
            version: row.read(a::VERSION)?,
            sha256: row.read(a::SHA256)?,
            source: row.read(a::SOURCE)?,
            source_game_id: row.read(a::SOURCE_GAME_ID)?,
            trust_level: row.read(a::TRUST_LEVEL)?,
        })
    }

    fn into_domain(self) -> AppResult<LibraryArtifact> {
        let Self {
            id,
            technology,
            file_name,
            file_path,
            version,
            sha256,
            source,
            source_game_id,
            trust_level,
        } = self;

        let file = component_file(file_path, sha256, version)?;
        let artifact = LibraryArtifact::new(
            mapping::artifact_id(id)?,
            mapping::graphics_technology(technology)?,
            file_name,
            file,
            mapping::artifact_trust_level(trust_level)?,
        )
        .map_err(invalid_row)?;

        let artifact = with_optional(artifact, source, |artifact, source| {
            artifact.with_source(source).map_err(invalid_row)
        })?;

        with_optional(artifact, source_game_id, |artifact, source_game_id| {
            Ok(artifact.with_source_game_id(mapping::game_id(source_game_id)?))
        })
    }
}

fn component_file(
    file_path: String,
    sha256: String,
    version: Option<String>,
) -> AppResult<ComponentFile> {
    let file = ComponentFile::new(mapping::path_ref(file_path)?)
        .with_sha256(mapping::sha256_hash(sha256)?);

    with_optional(file, version, |file, version| {
        Ok(file.with_version(mapping::version(version)?))
    })
}

#[derive(Debug)]
struct OperationRow {
    id: String,
    game_id: String,
    kind: String,
    status: String,
    created_at: i64,
    completed_at: Option<i64>,
    metadata_json: Option<String>,
}

impl DomainRow for OperationRow {
    type Domain = OperationRecord;

    fn from_sqlite_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        use columns::projection::operation as o;

        Ok(Self {
            id: row.read(o::ID)?,
            game_id: row.read(o::GAME_ID)?,
            kind: row.read(o::KIND)?,
            status: row.read(o::STATUS)?,
            created_at: row.read(o::CREATED_AT)?,
            completed_at: row.read(o::COMPLETED_AT)?,
            metadata_json: row.read(o::METADATA_JSON)?,
        })
    }

    fn into_domain(self) -> AppResult<OperationRecord> {
        let Self {
            id,
            game_id,
            kind,
            status,
            created_at,
            completed_at,
            metadata_json,
        } = self;

        let operation = OperationRecord::new(
            mapping::operation_id(id)?,
            mapping::game_id(game_id)?,
            operation_kind(kind)?,
            operation_status(status)?,
            timestamp_millis(created_at)?,
        );

        let operation = with_optional(operation, completed_at, |operation, completed_at| {
            Ok(operation.with_completed_at(timestamp_millis(completed_at)?))
        })?;

        with_optional(operation, metadata_json, |operation, metadata_json| {
            Ok(operation.with_metadata_json(mapping::metadata_json(metadata_json)?))
        })
    }
}

#[derive(Debug)]
struct OperationItemRow {
    operation_id: String,
    component_id: String,
    artifact_id: Option<String>,
    source_path: String,
    target_path: Option<String>,
    status: String,
    metadata_json: Option<String>,
}

impl DomainRow for OperationItemRow {
    type Domain = OperationItemRecord;

    fn from_sqlite_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        use columns::projection::operation_item as i;

        Ok(Self {
            operation_id: row.read(i::OPERATION_ID)?,
            component_id: row.read(i::COMPONENT_ID)?,
            artifact_id: row.read(i::ARTIFACT_ID)?,
            source_path: row.read(i::SOURCE_PATH)?,
            target_path: row.read(i::TARGET_PATH)?,
            status: row.read(i::STATUS)?,
            metadata_json: row.read(i::METADATA_JSON)?,
        })
    }

    fn into_domain(self) -> AppResult<OperationItemRecord> {
        let Self {
            operation_id,
            component_id,
            artifact_id,
            source_path,
            target_path,
            status,
            metadata_json,
        } = self;

        let item = OperationItemRecord::new(
            mapping::operation_id(operation_id)?,
            mapping::component_id(component_id)?,
            mapping::path_ref(source_path)?,
            operation_status(status)?,
        );

        let item = with_optional(item, artifact_id, |item, artifact_id| {
            Ok(item.with_artifact_id(mapping::artifact_id(artifact_id)?))
        })?;

        let item = with_optional(item, target_path, |item, target_path| {
            Ok(item.with_target_path(mapping::path_ref(target_path)?))
        })?;

        with_optional(item, metadata_json, |item, metadata_json| {
            Ok(item.with_metadata_json(mapping::metadata_json(metadata_json)?))
        })
    }
}

#[derive(Debug)]
struct BackupRow {
    id: String,
    operation_id: String,
    game_id: String,
    component_id: Option<String>,
    original_path: String,
    backup_path: String,
    sha256: Option<String>,
    created_at: i64,
    metadata_json: Option<String>,
}

impl DomainRow for BackupRow {
    type Domain = BackupRecord;

    fn from_sqlite_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        use columns::projection::backup as b;

        Ok(Self {
            id: row.read(b::ID)?,
            operation_id: row.read(b::OPERATION_ID)?,
            game_id: row.read(b::GAME_ID)?,
            component_id: row.read(b::COMPONENT_ID)?,
            original_path: row.read(b::ORIGINAL_PATH)?,
            backup_path: row.read(b::BACKUP_PATH)?,
            sha256: row.read(b::SHA256)?,
            created_at: row.read(b::CREATED_AT)?,
            metadata_json: row.read(b::METADATA_JSON)?,
        })
    }

    fn into_domain(self) -> AppResult<BackupRecord> {
        let Self {
            id,
            operation_id,
            game_id,
            component_id,
            original_path,
            backup_path,
            sha256,
            created_at,
            metadata_json,
        } = self;

        let backup = BackupRecord::new(
            backup_id(id)?,
            mapping::operation_id(operation_id)?,
            mapping::game_id(game_id)?,
            mapping::path_ref(original_path)?,
            mapping::path_ref(backup_path)?,
            timestamp_millis(created_at)?,
        );

        let backup = with_optional(backup, component_id, |backup, component_id| {
            Ok(backup.with_component_id(mapping::component_id(component_id)?))
        })?;

        let backup = with_optional(backup, sha256, |backup, sha256| {
            Ok(backup.with_sha256(mapping::sha256_hash(sha256)?))
        })?;

        with_optional(backup, metadata_json, |backup, metadata_json| {
            Ok(backup.with_metadata_json(mapping::metadata_json(metadata_json)?))
        })
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_application::{AppErrorKind, BackupRecord, OperationItemRecord};
    use renderpilot_domain::ComponentId;
    use rusqlite::Connection;

    use super::{
        backup_from_row, columns::projection, component_from_row, game_from_row,
        operation_item_from_row, DomainRow, OperationRow,
    };

    #[test]
    fn game_mapping_is_independent_of_column_order() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE games (
                id TEXT, title TEXT, launcher TEXT, external_id TEXT,
                platform TEXT, runtime TEXT, install_path TEXT,
                executable_candidates_json TEXT
            )",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO games VALUES ('game:1', 'Title', 'Manual', NULL, 'Windows', 'NativeWindows', 'C:/Games/Test', '[]')",
            [],
        )
        .unwrap();

        let sql = format!(
            "SELECT
                title AS {},
                launcher AS {},
                id AS {},
                external_id AS {},
                runtime AS {},
                platform AS {},
                executable_candidates_json AS {},
                install_path AS {}
            FROM games",
            projection::game::TITLE,
            projection::game::LAUNCHER,
            projection::game::ID,
            projection::game::EXTERNAL_ID,
            projection::game::RUNTIME,
            projection::game::PLATFORM,
            projection::game::EXECUTABLE_CANDIDATES_JSON,
            projection::game::INSTALL_PATH,
        );

        let mut stmt = conn.prepare(&sql).unwrap();
        let game = stmt.query_row([], game_from_row).unwrap().unwrap();

        assert_eq!(game.identity().title(), "Title");
        assert_eq!(game.id().as_str(), "game:1");
    }

    #[test]
    fn component_mapping_uses_component_game_id_when_game_id_collides() {
        let conn = Connection::open_in_memory().unwrap();
        let sql = format!(
            "SELECT
                'game:wrong' AS game_id,
                'game:correct' AS {},
                'component:c1' AS {},
                'NativeLibrary' AS {},
                'dlss_super_resolution' AS {},
                'Swappable' AS {},
                '[]' AS {}
            ",
            projection::component::GAME_ID,
            projection::component::ID,
            projection::component::KIND,
            projection::component::TECHNOLOGY,
            projection::component::SWAPPABILITY,
            projection::component::FILES_JSON,
        );

        let mut stmt = conn.prepare(&sql).unwrap();
        let component = stmt.query_row([], component_from_row).unwrap().unwrap();

        assert_eq!(component.game_id().as_str(), "game:correct");
    }

    #[test]
    fn operation_item_mapping_uses_item_ids_when_operation_and_component_collide() {
        let conn = Connection::open_in_memory().unwrap();
        let sql = format!(
            "SELECT
                'operation:wrong' AS operation_id,
                'component:wrong' AS component_id,
                'operation:correct' AS {},
                'component:correct' AS {},
                NULL AS {},
                'C:/Games/Test/file.dll' AS {},
                NULL AS {},
                'planned' AS {},
                NULL AS {}
            ",
            projection::operation_item::OPERATION_ID,
            projection::operation_item::COMPONENT_ID,
            projection::operation_item::ARTIFACT_ID,
            projection::operation_item::SOURCE_PATH,
            projection::operation_item::TARGET_PATH,
            projection::operation_item::STATUS,
            projection::operation_item::METADATA_JSON,
        );

        let mut stmt = conn.prepare(&sql).unwrap();
        let item: OperationItemRecord = stmt
            .query_row([], operation_item_from_row)
            .unwrap()
            .unwrap();

        assert_eq!(item.operation_id.as_str(), "operation:correct");
        assert_eq!(item.component_id.as_str(), "component:correct");
    }

    #[test]
    fn backup_mapping_uses_backup_aliases_when_foreign_tables_collide() {
        let conn = Connection::open_in_memory().unwrap();
        let sql = format!(
            "SELECT
                'operation:wrong' AS {},
                'operation:correct' AS {},
                'game:wrong' AS {},
                'game:correct' AS {},
                'component:wrong' AS {},
                'component:correct' AS {},
                'backup:b1' AS {},
                'C:/orig' AS {},
                'C:/bak' AS {},
                NULL AS {},
                42 AS {},
                NULL AS {}
            ",
            projection::operation::ID,
            projection::backup::OPERATION_ID,
            projection::game::ID,
            projection::backup::GAME_ID,
            projection::component::ID,
            projection::backup::COMPONENT_ID,
            projection::backup::ID,
            projection::backup::ORIGINAL_PATH,
            projection::backup::BACKUP_PATH,
            projection::backup::SHA256,
            projection::backup::CREATED_AT,
            projection::backup::METADATA_JSON,
        );

        let mut stmt = conn.prepare(&sql).unwrap();
        let backup: BackupRecord = stmt.query_row([], backup_from_row).unwrap().unwrap();

        assert_eq!(backup.operation_id.as_str(), "operation:correct");
        assert_eq!(backup.game_id.as_str(), "game:correct");
        assert_eq!(
            backup.component_id,
            Some(ComponentId::new("component:correct").unwrap())
        );
        assert_eq!(backup.id.as_str(), "backup:b1");
    }

    #[test]
    fn build_operation_rejects_invalid_metadata_json() {
        let error = OperationRow {
            id: "operation-1".to_owned(),
            game_id: "game-1".to_owned(),
            kind: "scan".to_owned(),
            status: "planned".to_owned(),
            created_at: 1,
            completed_at: None,
            metadata_json: Some("{".to_owned()),
        }
        .into_domain()
        .unwrap_err();

        assert_eq!(error.kind(), &AppErrorKind::StorageFailed);
        assert!(error.message().contains("invalid sqlite row"));
        assert!(error.message().contains("metadata json must be valid JSON"));
    }
}
