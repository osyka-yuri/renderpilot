//! Persist installation availability and move Engine.ini ownership out of
//! the local add-on row.

use renderpilot_application::AppResult;
use renderpilot_domain::{AddonKind, EngineConfigJournal};
use rusqlite::Connection;

use crate::{
    error::{storage_context, storage_error},
    mapping,
};

use super::{
    util::{table_exists, table_has_column},
    version,
};

pub(super) const SOURCE_VERSION: i32 = 21;
pub(super) const TARGET_VERSION: i32 = 22;

const ADDON_TABLE_WITHOUT_ENGINE_JOURNAL: &str = r#"
CREATE TABLE installed_addons_v22 (
    game_id               TEXT    PRIMARY KEY NOT NULL,
    kind                  TEXT    NOT NULL,
    addon_file            TEXT    NOT NULL,
    addon_version         TEXT,
    created_files_json    TEXT    NOT NULL,
    backed_up_files_json  TEXT    NOT NULL,
    managed_files_json    TEXT    NOT NULL DEFAULT '[]',
    tracked_sources_json  TEXT    NOT NULL DEFAULT '[]',
    host_kind             TEXT,
    reshade_channel       TEXT,
    registered_exe_path   TEXT,
    renodx_config_receipt_json TEXT,
    created_at            INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),
    updated_at            INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),
    CHECK (length(trim(game_id)) > 0),
    CHECK (length(trim(kind)) > 0),
    CHECK (length(trim(addon_file)) > 0),
    CHECK (json_valid(created_files_json) AND json_type(created_files_json) = 'array'),
    CHECK (json_valid(backed_up_files_json) AND json_type(backed_up_files_json) = 'array'),
    CHECK (json_valid(managed_files_json) AND json_type(managed_files_json) = 'array'),
    CHECK (json_valid(tracked_sources_json) AND json_type(tracked_sources_json) = 'array'),
    CHECK (host_kind IS NULL OR length(trim(host_kind)) > 0),
    CHECK (reshade_channel IS NULL OR length(trim(reshade_channel)) > 0),
    CHECK (registered_exe_path IS NULL OR length(trim(registered_exe_path)) > 0),
    CHECK (registered_exe_path IS NULL OR instr(registered_exe_path, char(0)) = 0),
    CHECK (renodx_config_receipt_json IS NULL OR
           (json_valid(renodx_config_receipt_json) AND json_type(renodx_config_receipt_json) = 'object')),
    CHECK (created_at >= 0),
    CHECK (updated_at >= created_at)
) STRICT;

INSERT INTO installed_addons_v22 (
    game_id, kind, addon_file, addon_version,
    created_files_json, backed_up_files_json, managed_files_json, tracked_sources_json,
    host_kind, reshade_channel, registered_exe_path, renodx_config_receipt_json,
    created_at, updated_at
)
SELECT game_id, kind, addon_file, addon_version,
       created_files_json, backed_up_files_json, managed_files_json, tracked_sources_json,
       host_kind, reshade_channel, registered_exe_path, renodx_config_receipt_json,
       created_at, updated_at
  FROM installed_addons_v21;

"#;

pub(super) fn apply(connection: &Connection) -> AppResult<()> {
    if !table_exists(connection, "games")? {
        return version::write(connection, TARGET_VERSION);
    }

    connection
        .execute_batch(super::super::ddl::installation_availability::SQL)
        .map_err(|error| storage_context("could not add installation availability", error))?;
    connection
        .execute(
            "INSERT OR IGNORE INTO installation_availability (game_id, state, revision)
             SELECT id, 'active', 0 FROM games",
            [],
        )
        .map_err(storage_error)?;
    connection
        .execute_batch(super::super::ddl::engine_config_journals::SQL)
        .map_err(|error| storage_context("could not add canonical Engine.ini journals", error))?;

    if table_exists(connection, "installed_addons")?
        && table_has_column(connection, "installed_addons", "engine_config_journal_json")?
    {
        validate_legacy_journals(connection)?;
        connection
            .execute_batch(
                "DROP TRIGGER IF EXISTS trg_installed_addons_touch_updated_at;
                 ALTER TABLE installed_addons RENAME TO installed_addons_v21;",
            )
            .map_err(|error| storage_context("could not preserve v21 add-on rows", error))?;
        connection
            .execute_batch(ADDON_TABLE_WITHOUT_ENGINE_JOURNAL)
            .map_err(|error| {
                storage_context("could not rebuild add-on rows without journal", error)
            })?;
        transfer_legacy_journals(connection)?;
        connection
            .execute_batch(
                "DROP TABLE installed_addons_v21;
                 ALTER TABLE installed_addons_v22 RENAME TO installed_addons;
                 CREATE TRIGGER trg_installed_addons_touch_updated_at
                 AFTER UPDATE ON installed_addons
                 FOR EACH ROW
                 WHEN NEW.updated_at = OLD.updated_at
                 BEGIN
                     UPDATE installed_addons
                        SET updated_at = max(
                            CAST(unixepoch('subsec') * 1000 AS INTEGER),
                            OLD.updated_at + 1
                        )
                      WHERE game_id = NEW.game_id;
                 END;",
            )
            .map_err(|error| storage_context("could not finish v22 add-on table", error))?;
    }

    version::write(connection, TARGET_VERSION)
}

fn validate_legacy_journals(connection: &Connection) -> AppResult<()> {
    let mut statement = connection
        .prepare(
            "SELECT game_id, kind, engine_config_journal_json
               FROM installed_addons
              WHERE engine_config_journal_json IS NOT NULL
              ORDER BY game_id",
        )
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
    for row in rows {
        let (game_id, kind_text, raw) = row.map_err(storage_error)?;
        let kind: AddonKind = mapping::enum_from_text(&kind_text)?;
        let journal: EngineConfigJournal = mapping::deserialize_json(&raw)?;
        journal.validate_for_kind(kind).map_err(|error| {
            storage_error(format!("invalid Engine.ini journal for {game_id}: {error}"))
        })?;
        if journal.is_empty() || mapping::serialize_json(&journal)? != raw {
            return Err(storage_error(format!(
                "noncanonical Engine.ini journal for {game_id}; refusing lossy migration"
            )));
        }
    }
    Ok(())
}

fn transfer_legacy_journals(connection: &Connection) -> AppResult<()> {
    // The v21 source table is retained until the exact journal payloads have
    // been copied into the independent owner table.
    connection
        .execute(
            "INSERT INTO game_engine_config_journals
                (game_id, addon_kind, journal_json, created_at, updated_at)
             SELECT game_id, kind, engine_config_journal_json, created_at, updated_at
               FROM installed_addons_v21
              WHERE engine_config_journal_json IS NOT NULL",
            [],
        )
        .map_err(|error| {
            storage_context("could not transfer canonical Engine.ini journals", error)
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{EngineConfigJournal, EngineConfigReceipt};
    use rusqlite::Connection;

    use super::apply;

    const V21_FIXTURE: &str = r#"
        PRAGMA foreign_keys = ON;
        CREATE TABLE games (id TEXT PRIMARY KEY NOT NULL) STRICT;
        INSERT INTO games VALUES ('manual:legacy');
        CREATE TABLE installed_addons (
            game_id TEXT PRIMARY KEY NOT NULL,
            kind TEXT NOT NULL,
            addon_file TEXT NOT NULL,
            addon_version TEXT,
            created_files_json TEXT NOT NULL,
            backed_up_files_json TEXT NOT NULL,
            managed_files_json TEXT NOT NULL DEFAULT '[]',
            tracked_sources_json TEXT NOT NULL DEFAULT '[]',
            host_kind TEXT,
            reshade_channel TEXT,
            registered_exe_path TEXT,
            renodx_config_receipt_json TEXT,
            engine_config_journal_json TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        ) STRICT;
        INSERT INTO installed_addons VALUES
            ('manual:legacy', 'renodx', 'C:/Game/renodx.addon64', NULL,
             '["C:/Game/renodx.addon64"]', '[]', '[]', '[]', NULL, NULL, NULL,
             NULL, NULL, 7, 9);
        CREATE TRIGGER trg_installed_addons_touch_updated_at
        AFTER UPDATE ON installed_addons FOR EACH ROW
        WHEN NEW.updated_at = OLD.updated_at
        BEGIN
            UPDATE installed_addons SET updated_at = max(OLD.updated_at + 1, 1)
             WHERE game_id = NEW.game_id;
        END;
        PRAGMA user_version = 21;
    "#;

    #[test]
    fn v21_migration_preserves_raw_journal_and_addon_timestamps() {
        let connection = Connection::open_in_memory().expect("sqlite");
        connection.execute_batch(V21_FIXTURE).expect("v21 fixture");
        let raw = valid_journal_raw();
        connection
            .execute(
                "UPDATE installed_addons SET engine_config_journal_json = ?1 WHERE game_id='manual:legacy'",
                [&raw],
            )
            .expect("v21 journal");

        apply(&connection).expect("v21 to v22");

        let row: (String, i64, i64) = connection
            .query_row(
                "SELECT journal_json, created_at, updated_at
                   FROM game_engine_config_journals WHERE game_id='manual:legacy'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("migrated journal");
        assert_eq!(row, (raw, 7, 10));

        let (created, updated): (i64, i64) = connection
            .query_row(
                "SELECT created_at, updated_at FROM installed_addons WHERE game_id='manual:legacy'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("migrated local row");
        assert_eq!((created, updated), (7, 10));
        let active: String = connection
            .query_row(
                "SELECT state FROM installation_availability WHERE game_id='manual:legacy'",
                [],
                |row| row.get(0),
            )
            .expect("legacy games start active");
        assert_eq!(active, "active");
        let columns: Vec<String> = connection
            .prepare("SELECT name FROM pragma_table_info('installed_addons')")
            .expect("columns")
            .query_map([], |row| row.get(0))
            .expect("column rows")
            .collect::<Result<_, _>>()
            .expect("column values");
        assert!(
            !columns
                .iter()
                .any(|column| column == "engine_config_journal_json")
        );
        let foreign_key_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_foreign_key_list('game_engine_config_journals')",
                [],
                |row| row.get(0),
            )
            .expect("journal has no game FK");
        assert_eq!(foreign_key_count, 0);
    }

    #[test]
    fn migration_rejects_noncanonical_engine_journal() {
        let connection = Connection::open_in_memory().expect("sqlite");
        connection.execute_batch(V21_FIXTURE).expect("v21 fixture");
        let noncanonical = format!("{} ", valid_journal_raw());
        connection
            .execute(
                "UPDATE installed_addons SET engine_config_journal_json = ?1 WHERE game_id='manual:legacy'",
                [&noncanonical],
            )
            .expect("v21 journal");

        let error = apply(&connection).expect_err("migration must reject changed raw token");
        assert!(
            error
                .to_string()
                .contains("noncanonical Engine.ini journal")
        );
        let version: i32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        assert_eq!(version, 21);
        let old_value: String = connection
            .query_row(
                "SELECT engine_config_journal_json FROM installed_addons WHERE game_id='manual:legacy'",
                [],
                |row| row.get(0),
            )
            .expect("source row remains intact");
        assert_eq!(old_value, noncanonical);
    }

    fn valid_journal_raw() -> String {
        let receipt = EngineConfigReceipt {
            schema_version: 1,
            path: "C:/Game/Engine.ini".to_owned(),
            file_created: false,
            encoding: "utf8".to_owned(),
            before_digest: "0".repeat(64),
            after_digest: "1".repeat(64),
            recipe_fingerprint: "2".repeat(64),
            contributions: Vec::new(),
            created_headers: Vec::new(),
            created_header_prefixes: Vec::new(),
            created_header_groups: Vec::new(),
            created_header_ordinals: Vec::new(),
        };
        crate::mapping::serialize_json(&EngineConfigJournal {
            stable: Some(receipt),
            pending: None,
        })
        .expect("canonical journal")
    }
}
