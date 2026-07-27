use renderpilot_application::AppResult;
use rusqlite::params;

use crate::error::storage_error;
use crate::sqlite_clock;

use super::SqliteStorage;

impl SqliteStorage {
    /// Loads every reliable source fingerprint once for a background scan batch.
    pub fn list_scan_source_checkpoints(&self) -> AppResult<Vec<(String, String)>> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT source_key, fingerprint FROM scan_source_checkpoints ORDER BY source_key",
                )
                .map_err(storage_error)?;
            let rows = statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(storage_error)?;
            rows.map(|row| row.map_err(storage_error)).collect()
        })
    }

    /// Activates a source fingerprint only after its installation scan succeeds.
    pub fn upsert_scan_source_checkpoint(
        &self,
        source_key: &str,
        fingerprint: &str,
    ) -> AppResult<()> {
        self.with_connection(|connection| {
            let now = sqlite_clock::now_ms(connection)?;
            connection
                .execute(
                    "INSERT INTO scan_source_checkpoints (source_key, fingerprint, updated_at) \
                     VALUES (?1, ?2, ?3) \
                     ON CONFLICT(source_key) DO UPDATE SET \
                         fingerprint = excluded.fingerprint, updated_at = excluded.updated_at",
                    params![source_key, fingerprint, now],
                )
                .map_err(storage_error)?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_round_trip_replaces_one_source() {
        let storage = SqliteStorage::in_memory().expect("storage");
        storage
            .upsert_scan_source_checkpoint("steam:42", "first")
            .expect("first checkpoint");
        storage
            .upsert_scan_source_checkpoint("steam:42", "second")
            .expect("replacement checkpoint");

        assert_eq!(
            storage.list_scan_source_checkpoints().expect("checkpoints"),
            vec![(String::from("steam:42"), String::from("second"))]
        );
    }
}
