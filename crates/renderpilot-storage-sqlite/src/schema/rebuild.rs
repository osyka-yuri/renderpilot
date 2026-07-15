//! Drop all user schema objects and re-apply the composed baseline.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_context;

use super::CURRENT_SCHEMA_VERSION;
use super::ddl;
use super::objects::drop_user_schema_objects;
use super::validation::validate_catalog_schema;
use super::version;

pub(super) fn reset_catalog_schema(connection: &Connection) -> AppResult<()> {
    drop_user_schema_objects(connection)?;
    apply_baseline(connection)?;
    version::write(connection, CURRENT_SCHEMA_VERSION)?;
    validate_catalog_schema(connection)
}

pub(super) fn apply_baseline(connection: &Connection) -> AppResult<()> {
    let baseline = ddl::compose_baseline();
    connection
        .execute_batch(&baseline)
        .map_err(|error| storage_context("could not apply sqlite catalog baseline", error))
}
