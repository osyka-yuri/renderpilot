//! Wall-clock reads from SQLite for `created_at` / `updated_at` columns.
//!
//! SQLite is the catalog clock source. Row timestamps use
//! `unixepoch('subsec') * 1000`, so values produced inside a single SQL
//! statement observe one coherent clock and do not race Rust `SystemTime`
//! against SQLite defaults.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_context;

const SQLITE_NOW_MS_QUERY: &str = "SELECT CAST(unixepoch('subsec') * 1000 AS INTEGER)";

/// Current catalog time in Unix milliseconds, as reported by SQLite.
pub(crate) fn now_ms(connection: &Connection) -> AppResult<i64> {
    connection
        .query_row(SQLITE_NOW_MS_QUERY, [], |row| row.get::<_, i64>(0))
        .map_err(|error| storage_context("could not read SQLite clock in Unix milliseconds", error))
}

/// Returns an `operations.updated_at` value that cannot violate
/// `updated_at >= created_at`.
///
/// This is needed when the domain row already carries a Rust-side `created_at`
/// timestamp, for example from plan builders using `SystemTime`.
#[must_use]
pub(crate) fn operation_updated_at_ms(sqlite_now_ms: i64, operation_created_at_ms: i64) -> i64 {
    sqlite_now_ms.max(operation_created_at_ms)
}

#[cfg(test)]
mod tests {
    use super::{now_ms, operation_updated_at_ms};
    use rusqlite::Connection;

    #[test]
    fn now_ms_reads_sqlite_clock_in_unix_milliseconds() {
        let connection = Connection::open_in_memory().expect("in-memory sqlite connection");

        let timestamp = now_ms(&connection).expect("sqlite clock timestamp");

        assert!(timestamp > 0);
    }

    #[test]
    fn operation_updated_at_uses_created_at_when_sqlite_clock_trails() {
        assert_eq!(operation_updated_at_ms(100, 200), 200);
    }

    #[test]
    fn operation_updated_at_uses_sqlite_clock_when_it_is_not_behind() {
        assert_eq!(operation_updated_at_ms(300, 200), 300);
        assert_eq!(operation_updated_at_ms(200, 200), 200);
    }
}
