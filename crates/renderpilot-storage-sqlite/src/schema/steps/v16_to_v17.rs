//! Replace weak global scan caches with owner-scoped observations and authority.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_context;

use super::super::{ddl::observations, validation, version};

pub(super) const SOURCE_VERSION: i32 = 16;
pub(super) const TARGET_VERSION: i32 = 17;

const DROP_OBSOLETE_OBJECTS: &str = r#"
DROP TRIGGER IF EXISTS trg_file_hash_cache_touch_updated_at;
DROP INDEX IF EXISTS idx_file_hash_cache_updated_at;
DROP TABLE IF EXISTS file_hash_cache;
DROP TRIGGER IF EXISTS trg_scan_source_checkpoints_touch_updated_at;
DROP TABLE IF EXISTS scan_source_checkpoints;
"#;

/// A v16 cache never proves a complete scan; every legacy game starts
/// fail-closed. Any durable mutation, including a cleaned-but-not-yet-deleted
/// committed marker, has a deterministic invalidation token.
pub(super) fn apply(connection: &Connection) -> AppResult<()> {
    observations::apply(connection)?;
    connection
        .execute_batch(
            r#"
INSERT OR IGNORE INTO catalog_scan_authority (game_id, readiness, authority_epoch, updated_at)
SELECT id, 'never_completed', 0, updated_at FROM games
;
UPDATE catalog_scan_authority
SET readiness = 'invalidated', authority_epoch = authority_epoch + 1,
    invalidation_reason = 'v17_pending_file_mutation',
    mutation_token = (SELECT p.id FROM pending_file_mutations AS p
                      WHERE p.game_id = catalog_scan_authority.game_id
                      ORDER BY p.created_at, p.id LIMIT 1),
    completed_at = NULL,
    updated_at = max(CAST(unixepoch('subsec') * 1000 AS INTEGER), updated_at + 1)
WHERE EXISTS (SELECT 1 FROM pending_file_mutations AS p
              WHERE p.game_id = catalog_scan_authority.game_id
                AND p.state IN ('preparing', 'prepared', 'committed'));
"#,
        )
        .map_err(|error| storage_context("could not seed v17 scan authority", error))?;
    validation::validate_database_integrity(connection)?;
    connection
        .execute_batch(DROP_OBSOLETE_OBJECTS)
        .map_err(|error| storage_context("could not remove v16 weak scan caches", error))?;
    validation::validate_database_integrity(connection)?;
    version::write(connection, TARGET_VERSION)
}
