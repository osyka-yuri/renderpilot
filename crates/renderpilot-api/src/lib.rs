//! GUI presentation facade for RenderPilot.
//!
//! Owns GUI DTO structs, `serde_json::Value` response building, and parsing of
//! GUI string ids into domain ids.

mod error;
pub use error::ApiError;

/// Serves image bytes for cover requests made via the `rp-cover://` URI scheme.
///
/// Handles paths of the form `/<url-encoded-game-id>`.
#[must_use]
pub fn cover_asset_protocol_response(
    context: &renderpilot_orchestration::Context,
    request_path: &str,
) -> http::Response<Vec<u8>> {
    renderpilot_orchestration::covers::cover_protocol_http_response(context, request_path)
}

/// Response served when a cover request cannot be handled (for example, when the
/// shared context is unavailable). Lets callers degrade gracefully without
/// constructing HTTP responses themselves.
#[must_use]
pub fn cover_unavailable_response() -> http::Response<Vec<u8>> {
    renderpilot_orchestration::covers::cover_unavailable_response()
}

pub(crate) mod catalog;
pub(crate) mod covers;
pub(crate) mod dlss_indicator;
pub(crate) mod libraries;
pub(crate) mod nvapi;
pub(crate) mod operations;
pub(crate) mod scan;
pub(crate) mod utils;

/// Best-effort cleanup of orphaned files in the catalog `covers/` directory.
pub fn gc_cover_orphans_on_startup(context: &renderpilot_orchestration::Context) {
    covers::gc_orphans_on_startup(context);
}

pub use self::catalog::{
    get_catalog_setting, get_game_details, list_games, query_game_cards, set_catalog_setting,
    set_game_favorite, set_game_hidden, QueryGameCardsRequest,
};
pub use self::covers::{clear_game_cover, fetch_game_cover, set_game_cover};
pub use self::dlss_indicator::{get_dlss_indicator_state, set_dlss_indicator_enabled};
pub use self::libraries::{
    delete_library, download_artifact, download_library, fetch_libraries_manifest,
    get_libraries_manifest, get_library_states, DownloadProgress, LibraryManifest,
    LibraryManifestEntry, LibraryState, ProgressObserver,
};
pub use self::nvapi::{
    clear_game_executable_override, get_nvapi_setting_state, list_game_executable_candidates,
    list_nvapi_setting_states, list_nvapi_supported_settings, revert_nvapi_setting,
    set_game_executable_override, set_nvapi_setting_value,
};
pub use self::operations::{apply_swap, rollback_component};
pub use self::scan::{scan_auto_libraries, scan_manual_folder};
