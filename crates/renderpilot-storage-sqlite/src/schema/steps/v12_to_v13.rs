use renderpilot_application::AppResult;
use rusqlite::Connection;

use super::super::{ddl, version};

/// Adds the two durable performance caches shipped together in schema v13.
pub(super) fn apply(connection: &Connection) -> AppResult<()> {
    ddl::profile_addon_capabilities::apply(connection)?;
    ddl::scan_source_checkpoints::apply(connection)?;
    version::write(connection, 13)
}
