//! Post-scan / startup refresh of per-game add-on capability flags.

use std::sync::Arc;

use renderpilot_orchestration::Context;
use renderpilot_orchestration::addons::capabilities;

/// Rebuilds catalog add-on capability flags from cached tool manifests.
///
/// Best-effort: failures are logged and never fail the caller (scan / startup).
pub(crate) async fn refresh_catalog_addon_capabilities(context: Arc<Context>) {
    let loaded = capabilities::load_capability_probes().await;
    if loaded.is_empty() {
        return;
    }

    let refresh = tauri::async_runtime::spawn_blocking(move || loaded.refresh(&context)).await;

    match refresh {
        Ok(Ok(())) => {}
        Ok(Err(error)) => log::warn!("failed to rebuild catalog add-on capabilities: {error}"),
        Err(error) => log::warn!("catalog add-on capability task failed: {error}"),
    }
}
