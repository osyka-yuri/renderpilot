//! Coordinated remote CDN manifest refresh for the desktop GUI.

use super::utils::{JsonResult, to_json};
use renderpilot_orchestration::manifests::{ManifestRefreshPolicy, refresh_remote_manifests};

/// Force-refreshes all remote manifests (libraries, RenoDX, Luma, ReShade)
/// subject to the process-local cooldown gate. Always returns a report JSON
/// (partial CDN failures are encoded per-kind, not as a hard error).
pub async fn refresh_remote_manifests_forced() -> JsonResult {
    let report = refresh_remote_manifests(ManifestRefreshPolicy::Forced).await;
    to_json(report)
}
