//! Add the singleton shared-Vulkan durable mutation fence.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_context;

use super::super::{ddl::pending_shared_vulkan_mutations, version};

pub(super) const SOURCE_VERSION: i32 = 17;
pub(super) const TARGET_VERSION: i32 = 18;

pub(super) fn apply(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch(&pending_shared_vulkan_mutations::create_table_sql())
        .map_err(|error| {
            storage_context("could not add pending shared Vulkan mutation table", error)
        })?;
    version::write(connection, TARGET_VERSION)
}
