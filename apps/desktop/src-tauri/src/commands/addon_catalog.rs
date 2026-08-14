//! Post-scan / startup refresh of per-game add-on capability flags.

use std::sync::Arc;

use crate::{
    backend_diagnostics,
    diagnostic_event::{BackendDiagnosticEvent, CapabilityOperation},
};
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
            report_game_capability_refresh_failure(GameCapabilityRefreshFailure::Service, &error);
            false
        }
        Err(error) => {
            report_game_capability_refresh_failure(GameCapabilityRefreshFailure::Task, &error);
            false
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GameCapabilityRefreshFailure {
    Service,
    Task,
}

impl GameCapabilityRefreshFailure {
    const fn console_prefix(self) -> &'static str {
        match self {
            Self::Service => "failed to refresh game add-on capabilities",
            Self::Task => "game capability task failed",
        }
    }
}

fn report_game_capability_refresh_failure(
    failure: GameCapabilityRefreshFailure,
    error: &impl std::fmt::Display,
) {
    log::warn!("{}: {error}", failure.console_prefix());
    backend_diagnostics::record(BackendDiagnosticEvent::capability_failure(
        CapabilityOperation::RefreshGameCatalogAddonCapabilities,
    ));
}

#[cfg(test)]
mod tests {
    use super::GameCapabilityRefreshFailure;

    #[test]
    fn per_game_capability_failures_keep_distinct_console_classification() {
        assert_eq!(
            GameCapabilityRefreshFailure::Service.console_prefix(),
            "failed to refresh game add-on capabilities"
        );
        assert_eq!(
            GameCapabilityRefreshFailure::Task.console_prefix(),
            "game capability task failed"
        );
    }
}
