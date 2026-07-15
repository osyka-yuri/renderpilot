//! Additive upgrade: RenoDX host metadata + shared_artifacts.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use super::super::ddl::shared_artifacts;
use super::super::version;
use super::util::ensure_installed_addons_column;

pub(super) fn apply(connection: &Connection) -> AppResult<()> {
    ensure_installed_addons_column(
        connection,
        "host_kind",
        "ALTER TABLE installed_addons ADD COLUMN host_kind TEXT",
    )?;
    ensure_installed_addons_column(
        connection,
        "reshade_channel",
        "ALTER TABLE installed_addons ADD COLUMN reshade_channel TEXT",
    )?;
    ensure_installed_addons_column(
        connection,
        "registered_exe_path",
        "ALTER TABLE installed_addons ADD COLUMN registered_exe_path TEXT",
    )?;

    shared_artifacts::apply(connection)?;

    version::write(connection, 9)
}
