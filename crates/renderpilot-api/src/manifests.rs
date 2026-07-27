//! Coordinated remote CDN manifest refresh for the desktop GUI.

use super::utils::to_json;
use renderpilot_orchestration::manifests::{ManifestRefreshPolicy, refresh_remote_manifests};

/// Typed transport payload plus the invalidation fact needed by the desktop
/// coordinator. Keeping this decision in the API avoids inspecting JSON in
/// the Tauri layer.
pub struct RemoteManifestRefreshOutput {
    /// Serialized public refresh report returned to the frontend.
    pub json: serde_json::Value,
    /// Whether the authoritative replacement-library catalog refreshed.
    pub library_catalog_refreshed: bool,
}

/// Force-refreshes manifests and returns the typed invalidation metadata used
/// by the startup/refresh coordinator.
pub async fn refresh_remote_manifests_forced_output()
-> Result<RemoteManifestRefreshOutput, crate::ApiError> {
    let report = refresh_remote_manifests(ManifestRefreshPolicy::Forced).await;
    let library_catalog_refreshed = report.libraries_changed;
    Ok(RemoteManifestRefreshOutput {
        json: to_json(report)?,
        library_catalog_refreshed,
    })
}
