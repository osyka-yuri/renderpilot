use renderpilot_application::AppResult;
use rusqlite::{params, Connection};

use crate::error::storage_context;
use crate::mapping;

const INITIAL_MIGRATION: &str = include_str!("../migrations/0001_initial.sql");
const LIBRARY_ARTIFACTS_TABLE: &str = "library_artifacts";
const LEGACY_ARTIFACT_FILE_JSON_COLUMN: &str = "file_json";
const NORMALIZE_ARTIFACT_TRUST_LEVEL_SQL: &str = "UPDATE library_artifacts
     SET trust_level = COALESCE(trust_level, 'LocalObserved')
     WHERE trust_level IS NULL";
const DEDUPLICATE_ARTIFACTS_SQL: &str = "DELETE FROM library_artifacts
     WHERE sha256 IS NOT NULL
       AND rowid NOT IN (
           SELECT MAX(rowid)
           FROM library_artifacts
           WHERE sha256 IS NOT NULL
           GROUP BY sha256
       )";
const CREATE_ARTIFACT_INDEXES_SQL: &str =
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_library_artifacts_sha256
         ON library_artifacts(sha256);
     CREATE INDEX IF NOT EXISTS idx_library_artifacts_technology
         ON library_artifacts(technology);";

const LIBRARY_ARTIFACT_COLUMN_MIGRATIONS: &[ColumnMigration] = &[
    ColumnMigration::new(
        "file_name",
        "ALTER TABLE library_artifacts ADD COLUMN file_name TEXT",
        "could not add library_artifacts.file_name",
    ),
    ColumnMigration::new(
        "file_path",
        "ALTER TABLE library_artifacts ADD COLUMN file_path TEXT",
        "could not add library_artifacts.file_path",
    ),
    ColumnMigration::new(
        "version",
        "ALTER TABLE library_artifacts ADD COLUMN version TEXT",
        "could not add library_artifacts.version",
    ),
    ColumnMigration::new(
        "sha256",
        "ALTER TABLE library_artifacts ADD COLUMN sha256 TEXT",
        "could not add library_artifacts.sha256",
    ),
    ColumnMigration::new(
        "source_game_id",
        "ALTER TABLE library_artifacts ADD COLUMN source_game_id TEXT REFERENCES games(id) ON DELETE SET NULL",
        "could not add library_artifacts.source_game_id",
    ),
    ColumnMigration::new(
        "trust_level",
        "ALTER TABLE library_artifacts ADD COLUMN trust_level TEXT",
        "could not add library_artifacts.trust_level",
    ),
];

pub(crate) fn apply(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch(INITIAL_MIGRATION)
        .map_err(|error| storage_context("could not apply sqlite migrations", error))?;

    ensure_library_artifact_schema(connection)
}

fn ensure_library_artifact_schema(connection: &Connection) -> AppResult<()> {
    let columns = table_columns(connection, LIBRARY_ARTIFACTS_TABLE)?;

    if columns.is_empty() {
        return Ok(());
    }

    migrate_library_artifact_columns(connection, &columns)?;
    normalize_library_artifact_rows(connection)?;
    create_library_artifact_indexes(connection)?;

    Ok(())
}

fn migrate_library_artifact_columns(connection: &Connection, columns: &[String]) -> AppResult<()> {
    add_missing_library_artifact_columns(connection, columns)?;

    if has_column(columns, LEGACY_ARTIFACT_FILE_JSON_COLUMN) {
        backfill_legacy_artifact_columns(connection)?;
    }

    Ok(())
}

fn add_missing_library_artifact_columns(
    connection: &Connection,
    columns: &[String],
) -> AppResult<()> {
    for migration in LIBRARY_ARTIFACT_COLUMN_MIGRATIONS {
        if has_column(columns, migration.name) {
            continue;
        }

        connection
            .execute_batch(migration.sql)
            .map_err(|error| storage_context(migration.error_context, error))?;
    }

    Ok(())
}

fn normalize_library_artifact_rows(connection: &Connection) -> AppResult<()> {
    connection
        .execute(NORMALIZE_ARTIFACT_TRUST_LEVEL_SQL, [])
        .map_err(|error| storage_context("could not normalize artifact trust level", error))?;

    connection
        .execute(DEDUPLICATE_ARTIFACTS_SQL, [])
        .map_err(|error| storage_context("could not deduplicate legacy artifacts", error))?;

    Ok(())
}

fn create_library_artifact_indexes(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch(CREATE_ARTIFACT_INDEXES_SQL)
        .map_err(|error| storage_context("could not create library_artifacts indexes", error))
}

fn table_columns(connection: &Connection, table_name: &str) -> AppResult<Vec<String>> {
    let mut statement = connection
        .prepare("SELECT name FROM pragma_table_info(?1)")
        .map_err(|error| storage_context("could not inspect sqlite table schema", error))?;
    let rows = statement
        .query_map([table_name], |row| row.get::<_, String>(0))
        .map_err(|error| storage_context("could not read sqlite table schema", error))?;
    let mut columns = Vec::new();

    for row in rows {
        columns.push(
            row.map_err(|error| storage_context("could not read sqlite table column", error))?,
        );
    }

    Ok(columns)
}

fn has_column(columns: &[String], column_name: &str) -> bool {
    columns.iter().any(|column| column == column_name)
}

fn backfill_legacy_artifact_columns(connection: &Connection) -> AppResult<()> {
    let mut statement = connection
        .prepare(
            "SELECT id, file_json
             FROM library_artifacts
             WHERE file_name IS NULL OR file_path IS NULL OR sha256 IS NULL",
        )
        .map_err(|error| storage_context("could not prepare legacy artifact migration", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| storage_context("could not read legacy artifact rows", error))?;

    for row in rows {
        let (id, file_json) =
            row.map_err(|error| storage_context("could not decode legacy artifact row", error))?;
        let legacy_artifact = LegacyArtifactColumns::from_file_json(file_json)?;

        connection
            .execute(
                "UPDATE library_artifacts
                 SET file_name = COALESCE(file_name, ?1),
                     file_path = COALESCE(file_path, ?2),
                     version = COALESCE(version, ?3),
                     sha256 = COALESCE(sha256, ?4),
                     trust_level = COALESCE(trust_level, 'LocalObserved')
                 WHERE id = ?5",
                params![
                    legacy_artifact.file_name,
                    legacy_artifact.file_path,
                    legacy_artifact.version,
                    legacy_artifact.sha256,
                    id
                ],
            )
            .map_err(|error| storage_context("could not migrate legacy artifact row", error))?;
    }

    Ok(())
}

struct LegacyArtifactColumns {
    file_name: String,
    file_path: String,
    version: Option<String>,
    sha256: String,
}

impl LegacyArtifactColumns {
    fn from_file_json(file_json: String) -> AppResult<Self> {
        let file = mapping::component_file(file_json)?;
        let file_name = std::path::Path::new(file.path().as_str())
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                crate::error::storage_error("legacy artifact path is missing a file name")
            })?
            .to_owned();
        let sha256 = file
            .sha256()
            .ok_or_else(|| crate::error::storage_error("legacy artifact sha256 is missing"))?
            .as_str()
            .to_owned();

        Ok(Self {
            file_name,
            file_path: file.path().as_str().to_owned(),
            version: file.version().map(|version| version.as_str().to_owned()),
            sha256,
        })
    }
}

struct ColumnMigration {
    name: &'static str,
    sql: &'static str,
    error_context: &'static str,
}

impl ColumnMigration {
    const fn new(name: &'static str, sql: &'static str, error_context: &'static str) -> Self {
        Self {
            name,
            sql,
            error_context,
        }
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{ComponentFile, GraphicsTechnology, PathRef, Sha256Hash, Version};
    use rusqlite::{params, Connection};

    use super::{apply, table_columns, LIBRARY_ARTIFACTS_TABLE};
    use crate::mapping;

    #[test]
    fn apply_backfills_legacy_artifact_columns() {
        let connection = Connection::open_in_memory().expect("sqlite should open");
        create_legacy_artifacts_table(&connection);

        let file_json = legacy_file_json(
            "C:/Games/GameA/nvngx_dlss.dll",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        );
        let technology = mapping::enum_to_text(GraphicsTechnology::DlssSuperResolution)
            .expect("technology should serialize");

        connection
            .execute(
                "INSERT INTO library_artifacts (id, technology, file_json, source, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 1)",
                params!["artifact:one", technology, file_json, "scan-folder"],
            )
            .expect("legacy artifact row should insert");

        apply(&connection).expect("schema migration should succeed");

        let columns = table_columns(&connection, LIBRARY_ARTIFACTS_TABLE)
            .expect("library_artifacts columns should load");

        assert!(columns.iter().any(|column| column == "file_name"));
        assert!(columns.iter().any(|column| column == "sha256"));

        let row = connection
            .query_row(
                "SELECT file_name, file_path, version, sha256, trust_level
                 FROM library_artifacts
                 WHERE id = ?1",
                ["artifact:one"],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .expect("migrated artifact row should load");

        assert_eq!(row.0, "nvngx_dlss.dll");
        assert_eq!(row.1, "C:/Games/GameA/nvngx_dlss.dll");
        assert_eq!(row.2.as_deref(), Some("3.7.20"));
        assert_eq!(
            row.3,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(row.4, "LocalObserved");
    }

    #[test]
    fn apply_deduplicates_legacy_rows_by_sha256() {
        let connection = Connection::open_in_memory().expect("sqlite should open");
        create_legacy_artifacts_table(&connection);

        let technology = mapping::enum_to_text(GraphicsTechnology::DlssSuperResolution)
            .expect("technology should serialize");
        let first_file_json = legacy_file_json(
            "C:/Games/GameA/nvngx_dlss.dll",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );
        let second_file_json = legacy_file_json(
            "C:/Games/GameB/nvngx_dlss.dll",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );

        connection
            .execute(
                "INSERT INTO library_artifacts (id, technology, file_json, source, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 1)",
                params![
                    "artifact:first",
                    technology.clone(),
                    first_file_json,
                    "scan-folder"
                ],
            )
            .expect("first legacy artifact row should insert");
        connection
            .execute(
                "INSERT INTO library_artifacts (id, technology, file_json, source, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 2)",
                params![
                    "artifact:second",
                    technology,
                    second_file_json,
                    "scan-folder"
                ],
            )
            .expect("second legacy artifact row should insert");

        apply(&connection).expect("schema migration should succeed");

        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM library_artifacts", [], |row| {
                row.get(0)
            })
            .expect("artifact count should load");

        assert_eq!(count, 1);
    }

    fn create_legacy_artifacts_table(connection: &Connection) {
        connection
            .execute_batch(
                "CREATE TABLE library_artifacts (
                    id TEXT PRIMARY KEY NOT NULL,
                    technology TEXT NOT NULL,
                    file_json TEXT NOT NULL,
                    source TEXT,
                    updated_at INTEGER NOT NULL
                );",
            )
            .expect("legacy library_artifacts table should be created");
    }

    fn legacy_file_json(path: &str, sha256: &str) -> String {
        let file = ComponentFile::new(PathRef::new(path).expect("path should be valid"))
            .with_version(Version::parse("3.7.20").expect("version should be valid"))
            .with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid"));

        mapping::serialize_json(&file).expect("component file should serialize")
    }
}
