//! Post-scan / startup refresh of per-game add-on capability flags.

use std::sync::Arc;

use renderpilot_orchestration::Context;
use renderpilot_orchestration::addons::capabilities;
use renderpilot_orchestration::domain::GameId;

/// Rebuilds catalog add-on capability flags from cached tool manifests.
///
/// Callers decide how to surface failures; startup records them as best-effort issues.
pub(crate) async fn refresh_catalog_addon_capabilities(
    context: Arc<Context>,
) -> Result<bool, String> {
    let loaded = capabilities::load_capability_probes().await;
    if loaded.is_empty() {
        return Ok(false);
    }

    let refresh = tauri::async_runtime::spawn_blocking(move || loaded.refresh(&context)).await;

    match refresh {
        Ok(Ok(changed)) => Ok(changed),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(format!("worker failed: {error}")),
    }
}

/// Refreshes durable capability rows only for the game whose executable facts changed.
pub(crate) async fn refresh_game_catalog_addon_capabilities(
    context: Arc<Context>,
    game_id: GameId,
) -> bool {
    let loaded = capabilities::load_capability_probes().await;
    if loaded.is_empty() {
        return false;
    }

    let refresh =
        tauri::async_runtime::spawn_blocking(move || loaded.refresh_game(&context, &game_id)).await;

    match refresh {
        Ok(Ok(changed)) => changed,
        Ok(Err(error)) => {
            log::warn!("failed to refresh game add-on capabilities: {error}");
            false
        }
        Err(error) => {
            log::warn!("game capability task failed: {error}");
            false
        }
    }
}
