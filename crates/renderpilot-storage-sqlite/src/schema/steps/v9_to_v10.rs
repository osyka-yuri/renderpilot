//! Additive released v9→v10 upgrade: managed files + crash-recoverable file
//! mutations.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use super::super::ddl::pending_file_mutations;
use super::super::version;
use super::util::ensure_column;

pub(super) fn apply(connection: &Connection) -> AppResult<()> {
    ensure_column(
        connection,
        "installed_addons",
        "managed_files_json",
        "ALTER TABLE installed_addons ADD COLUMN managed_files_json TEXT NOT NULL DEFAULT '[]' \
         CHECK (json_valid(managed_files_json)) CHECK (json_type(managed_files_json) = 'array')",
    )?;

    // Released v9 has no pending-mutations table. This edge is the only
    // historical upgrade owner for creating its canonical v10 DDL.
    pending_file_mutations::create_for_released_v9_to_v10(connection)?;

    version::write(connection, 10)
}
