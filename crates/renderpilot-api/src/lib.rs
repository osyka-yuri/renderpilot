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
pub(crate) mod luma;
pub(crate) mod manifests;
pub(crate) mod nvapi;
pub(crate) mod operations;
pub(crate) mod renodx;
pub(crate) mod scan;
pub(crate) mod utils;

/// Best-effort cleanup of orphaned files in the catalog `covers/` directory.
pub fn gc_cover_orphans_on_startup(context: &renderpilot_orchestration::Context) {
    covers::gc_orphans_on_startup(context);
}

pub use self::catalog::{
    QueryGameCardsRequest, get_catalog_setting, get_game_details, list_games, query_game_cards,
    set_catalog_setting, set_game_favorite, set_game_hidden,
};
pub use self::covers::{clear_game_cover, fetch_game_cover, set_game_cover};
pub use self::dlss_indicator::{get_dlss_indicator_state, set_dlss_indicator_enabled};
pub use self::libraries::{
    DownloadProgress, LibraryManifest, LibraryManifestEntry, LibraryState, ProgressObserver,
    delete_library, download_artifact, download_library, fetch_libraries_manifest,
    get_libraries_manifest, get_library_states,
};
pub use self::luma::{
    luma_availability, luma_check_update, luma_install, luma_uninstall, luma_update,
};
pub use self::manifests::refresh_remote_manifests_forced;
pub use self::nvapi::{
    clear_game_executable_override, get_nvapi_setting_state, list_game_executable_candidates,
    list_global_nvapi_setting_states, list_nvapi_setting_states, list_nvapi_supported_settings,
    resolve_game_executable, revert_global_nvapi_setting, revert_nvapi_setting,
    set_game_executable_override, set_global_nvapi_setting_value, set_nvapi_setting_value,
};
pub use self::operations::{apply_swap, rollback_component};
pub use self::renodx::{
    renodx_apply_vulkan_layer, renodx_availability, renodx_check_update,
    renodx_dlss_fix_availability, renodx_install, renodx_install_dlss_fix,
    renodx_install_from_file, renodx_remove_vulkan_layer, renodx_switch_reshade_channel,
    renodx_uninstall, renodx_uninstall_dlss_fix, renodx_update,
    renodx_vulkan_layer_management_status, renodx_vulkan_layer_status,
};
pub use self::scan::{scan_auto_libraries, scan_manual_folder};
