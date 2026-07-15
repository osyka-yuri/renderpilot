//! SQLite storage adapter for RenderPilot.
//!
//! This crate owns SQLite schema management, connection pragmas, and repository
//! implementations. Domain types remain SQLite-agnostic.

mod connection;
mod error;
mod mapping;
mod repositories;
mod schema;
mod sqlite_clock;

pub use repositories::file_hash_cache::FileHashCacheRow;
pub use repositories::game_covers::{DeletedGameInfo, GameCoverRecord};
pub use repositories::game_ui_state::GameUiStateRow;
pub use repositories::{
    ComponentBaselineInsert, GameMutationCommit, InstalledAddonMutation, PendingFileMutationRow,
    PendingFileMutationState,
};
pub use repositories::{ScanWriteReport, ScanWriteUnit, SqliteStorage};
