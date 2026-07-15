//! Additive upgrade: managed_files + crash-recoverable file mutations.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use super::super::ddl::pending_file_mutations;
use super::super::version;
use super::util::ensure_installed_addons_column;

pub(super) fn apply(connection: &Connection) -> AppResult<()> {
    ensure_installed_addons_column(
        connection,
        "managed_files_json",
        "ALTER TABLE installed_addons ADD COLUMN managed_files_json TEXT NOT NULL DEFAULT '[]' \
         CHECK (json_valid(managed_files_json)) CHECK (json_type(managed_files_json) = 'array')",
    )?;

    // Ensure correct shape (not bare IF NOT EXISTS): WIP catalogs may already
    // have a table whose CHECK rejects `preparing`.
    pending_file_mutations::ensure_correct_shape(connection)?;

    version::write(connection, 10)
}
