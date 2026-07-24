//! Typed auxiliary files in the component rollback aggregate.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use super::super::version;
use super::util::{ensure_column, table_has_column};

pub(super) fn apply(connection: &Connection) -> AppResult<()> {
    // Released v11 catalogs always contain this table. If a malformed or
    // pre-release catalog does not, still advance the additive chain so the
    // current-schema validator can route it through the normal rebuild path.
    if !table_has_column(connection, "component_backups", "component_id")? {
        return version::write(connection, 12);
    }

    ensure_column(
        connection,
        "component_backups",
        "auxiliary_json",
        "ALTER TABLE component_backups ADD COLUMN auxiliary_json TEXT NOT NULL DEFAULT '[]' \
         CHECK (json_valid(auxiliary_json)) CHECK (json_type(auxiliary_json) = 'array')",
    )?;
    version::write(connection, 12)
}
