//! The catalog's portable path-tag transition.
//!
//! Game installation paths are deliberately external and must not be rewritten
//! when a portable root is copied or moved. Portable data paths are derived by
//! `RuntimePathsV1` and authenticated by the supervisor's migration receipt;
//! virtual UI paths are never persisted. The schema stamp therefore records
//! the released boundary without mutating user-owned external values.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use super::super::{ddl::portable_path_tags, version};

pub(super) const SOURCE_VERSION: i32 = 15;
pub(super) const TARGET_VERSION: i32 = 16;

pub(super) fn apply(connection: &Connection) -> AppResult<()> {
    portable_path_tags::apply(connection)?;
    version::write(connection, TARGET_VERSION)
}
