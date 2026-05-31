//! Desktop UI facade over the existing CLI/core orchestration.
//!
//! This module exposes JSON-friendly entry points that Tauri commands can call
//! without embedding business logic in the command handlers themselves.

mod catalog;
mod covers;
pub(crate) mod dlss;
pub(crate) mod libraries;
mod nvapi;
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
    get_catalog_setting, get_game_details, list_games, query_game_cards, set_catalog_setting,
};
pub use self::covers::{clear_game_cover, fetch_game_cover, set_game_cover};
pub use self::libraries::{
    delete_library, download_library, fetch_libraries_manifest, get_libraries_manifest,
    get_library_states, LibraryManifest, LibraryManifestEntry, LibraryState,
};
pub use self::nvapi::{
    clear_game_executable_override, get_nvapi_setting_state, list_game_executable_candidates,
    list_nvapi_setting_states, list_nvapi_supported_settings, revert_nvapi_setting,
    set_game_executable_override, set_nvapi_setting_value,
};
pub use self::operations::{apply_swap, rollback_component};
pub use self::scan::{scan_auto_libraries, scan_manual_folder};
