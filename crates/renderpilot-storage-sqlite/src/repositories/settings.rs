//! Application settings key/value store.

use renderpilot_application::AppResult;
use rusqlite::OptionalExtension;

use crate::error::storage_context;
use crate::sqlite_clock;

use super::SqliteStorage;

const SQL_UPSERT_SETTING: &str = "
    INSERT INTO settings (key, value, created_at, updated_at)
    VALUES (?1, ?2, ?3, ?3)
    ON CONFLICT(key) DO UPDATE SET
        value = excluded.value,
        updated_at = excluded.updated_at
";

const SQL_DELETE_SETTING: &str = "
    DELETE FROM settings
    WHERE key = ?1
";

const SQL_SELECT_SETTING: &str = "
    SELECT value
    FROM settings
    WHERE key = ?1
";

impl SqliteStorage {
    /// Sets a string setting value.
    pub fn set_setting(&self, key: &str, value: &str) -> AppResult<()> {
        self.with_connection(|connection| {
            let updated_at = sqlite_clock::now_ms(connection)?;

            connection
                .execute(SQL_UPSERT_SETTING, (key, value, updated_at))
                .map_err(|error| storage_context("failed to save setting", error))?;

            Ok(())
        })
    }

    /// Deletes a settings row by key. Missing keys are a no-op (SQLite `DELETE` affects zero rows).
    pub fn delete_setting(&self, key: &str) -> AppResult<()> {
        self.with_connection(|connection| {
            connection
                .execute(SQL_DELETE_SETTING, [key])
                .map_err(|error| storage_context("failed to delete setting", error))?;

            Ok(())
        })
    }

    /// Reads a string setting value.
    pub fn get_setting(&self, key: &str) -> AppResult<Option<String>> {
        self.with_connection(|connection| {
            connection
                .query_row(SQL_SELECT_SETTING, [key], |row| row.get(0))
                .optional()
                .map_err(|error| storage_context("failed to read setting", error))
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::repositories::SqliteStorage;

    #[test]
    fn round_trip_set_and_get() {
        let storage = SqliteStorage::in_memory().expect("storage");
        storage
            .set_setting("steamgriddb.api_key", "secret")
            .expect("set");
        assert_eq!(
            storage.get_setting("steamgriddb.api_key").expect("get"),
            Some("secret".to_owned())
        );
    }

    #[test]
    fn get_missing_key_returns_none() {
        let storage = SqliteStorage::in_memory().expect("storage");
        assert_eq!(storage.get_setting("missing.key").expect("get"), None);
    }

    #[test]
    fn upsert_overwrites_existing() {
        let storage = SqliteStorage::in_memory().expect("storage");
        storage.set_setting("k", "first").expect("set first");
        storage.set_setting("k", "second").expect("set second");
        assert_eq!(
            storage.get_setting("k").expect("get"),
            Some("second".to_owned())
        );
    }

    #[test]
    fn delete_then_get_returns_none() {
        let storage = SqliteStorage::in_memory().expect("storage");
        storage.set_setting("k", "value").expect("set");
        storage.delete_setting("k").expect("delete");
        assert_eq!(storage.get_setting("k").expect("get"), None);
    }
}
