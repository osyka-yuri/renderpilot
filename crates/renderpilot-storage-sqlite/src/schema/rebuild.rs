//! Apply the composed catalog baseline.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_context;

use super::ddl;

pub(super) fn apply_baseline(connection: &Connection) -> AppResult<()> {
    let baseline = ddl::compose_baseline();
    connection
        .execute_batch(&baseline)
        .map_err(|error| storage_context("could not apply sqlite catalog baseline", error))
}
