//! Desktop UI facade over the existing CLI/core orchestration.
//!
//! This module exposes JSON-friendly entry points that Tauri commands can call
//! without embedding business logic in the command handlers themselves.

mod catalog;
mod operations;
mod scan;
mod utils;

#[cfg(test)]
mod tests;

pub use self::catalog::{get_game_cards, get_game_details, list_games};
pub use self::operations::{
    apply_operation, apply_operation_plan, build_swap_plan, rollback_operation,
};
pub use self::scan::{scan_auto_libraries, scan_manual_folder};
