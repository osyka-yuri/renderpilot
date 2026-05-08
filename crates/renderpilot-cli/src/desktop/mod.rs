//! Desktop UI facade over the existing CLI/core orchestration.
//!
//! This module exposes JSON-friendly entry points that Tauri commands can call
//! without embedding business logic in the command handlers themselves.

mod catalog;
mod covers;
mod operations;
mod scan;
mod utils;

#[cfg(test)]
mod tests;

/// Best-effort cleanup of orphaned files in the catalog `covers/` directory.
pub fn gc_cover_orphans_on_startup() {
    covers::gc_orphans_on_startup();
}

pub use self::catalog::{
    get_catalog_setting, get_game_cards, get_game_details, list_games, set_catalog_setting,
};
pub use self::covers::{clear_game_cover, fetch_game_cover, set_game_cover};
pub use self::operations::{
    apply_operation, apply_operation_plan, build_swap_plan, rollback_operation,
};
pub use self::scan::{scan_auto_libraries, scan_manual_folder};
