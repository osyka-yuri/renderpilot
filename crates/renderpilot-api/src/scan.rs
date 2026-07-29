//! Catalog scan transport facade.

mod add_game;
mod auto;
mod remove_game;

pub use add_game::{add_game, inspect_game_install};
pub use auto::{AutoScanOutput, scan_auto_libraries, scan_auto_libraries_background_output};
pub use remove_game::remove_game_from_catalog;
