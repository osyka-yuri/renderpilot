//! Add durable RenderPilot ownership and mutation receipts for NVAPI DRS.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use super::util::table_exists;
use super::version;

pub(super) const SOURCE_VERSION: i32 = 20;
pub(super) const TARGET_VERSION: i32 = 21;

pub(super) fn apply(connection: &Connection) -> AppResult<()> {
    if !table_exists(connection, "games")? {
        return version::write(connection, TARGET_VERSION);
    }
    connection
        .execute_batch(super::super::ddl::nvapi_profile_management::baseline_sql())
        .map_err(|error| {
            crate::error::storage_context("could not add NVAPI profile ownership tables", error)
        })?;
    connection
        .execute_batch("DROP TABLE IF EXISTS nvapi_setting_baselines")
        .map_err(|error| {
            crate::error::storage_context("could not remove obsolete NVAPI baselines", error)
        })?;
    version::write(connection, TARGET_VERSION)
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    #[test]
    fn v20_upgrade_discards_obsolete_baselines_and_adds_owner_tables() {
        let connection = Connection::open_in_memory().expect("sqlite");
        connection
            .execute_batch(
                "CREATE TABLE games (id TEXT PRIMARY KEY NOT NULL) STRICT;
                 CREATE TABLE nvapi_setting_baselines (
                     game_id TEXT NOT NULL,
                     setting_key TEXT NOT NULL,
                     baseline_dword INTEGER NOT NULL,
                     baseline_was_predefined INTEGER NOT NULL,
                     predefined_dword INTEGER,
                     captured_exe TEXT NOT NULL,
                     captured_at INTEGER NOT NULL,
                     PRIMARY KEY (game_id, setting_key)
                 ) STRICT;
                 INSERT INTO games VALUES ('manual:legacy');
                 INSERT INTO nvapi_setting_baselines VALUES
                     ('manual:legacy', 'dlss_sr_render_preset', 3, 0, 0, 'game.exe', 7);
                 PRAGMA user_version = 20;",
            )
            .expect("v20 fixture");

        super::apply(&connection).expect("v20 to v21");
        let old_table_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='nvapi_setting_baselines')",
                [],
                |row| row.get(0),
            )
            .expect("check old baseline table");
        assert!(!old_table_exists, "obsolete baseline table must be removed");

        let version: i32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("schema version");
        assert_eq!(version, 21);

        let game = connection
            .execute("DELETE FROM games WHERE id='manual:legacy'", [])
            .expect("old baseline must not block game deletion");
        assert_eq!(game, 1);
    }
}
