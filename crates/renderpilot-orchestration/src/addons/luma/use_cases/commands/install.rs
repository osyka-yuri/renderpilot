//! Installs Luma Framework from upstream.
//!
//! The command is deliberately only a route selector. The inactive route keeps
//! the established engine lifecycle, while an existing OptiScaler topology is
//! handled by the closed peer transaction in `active`.

mod active;
mod inactive;

#[cfg(test)]
mod tests;

use renderpilot_application::ProxyTopologyRepository;
use renderpilot_domain::{GameId, InstalledAddon};

use super::engine_config;
use crate::addons::reshade::types::ReshadeSourceCatalog;
use crate::net::ProgressObserver;
use crate::{Context, ServiceError};

/// Shared parameters for a Luma install operation.
pub struct InstallRequest<'a> {
    /// Backend context (game repository, addon repository, settings).
    pub context: &'a Context,
    /// The resolved Luma tool catalogue.
    pub manifest: &'a crate::addons::luma::types::LumaManifest,
    /// Independently resolved ReShade source catalogue.
    pub reshade_sources: &'a ReshadeSourceCatalog,
    /// The game to install Luma for.
    pub game_id: &'a GameId,
    /// Typed authority for the final game-file commit.
    pub safety: crate::GameSafetyPermit,
    /// Optional download progress observer.
    pub progress: Option<&'a ProgressObserver<'a>>,
}

/// Installs Luma into `game`, selecting the lifecycle from the topology that
/// was observed while holding the game's mutation boundary. Downloads happen
/// only after the route's read-only phase has completed.
pub async fn install(request: InstallRequest<'_>) -> Result<InstalledAddon, ServiceError> {
    let context = request.context;
    let game_id = request.game_id;
    let manifest = request.manifest;
    let safety = request.safety.clone();
    let topology = {
        let _guard =
            crate::mutation_boundary::enter_game_mutation_boundary_async(context, game_id).await?;
        context.storage().get_proxy_topology(game_id)?
    };

    let installed = match topology {
        Some(topology) => active::install(request, topology).await?,
        None => inactive::install(request).await?,
    };

    if let Err(error) = crate::catalog::refresh_game_components(context, game_id).await {
        tracing::warn!("failed to refresh game components after Luma install: {error}");
    }
    engine_config::reconcile_after_commit(context, manifest, game_id, safety).await?;

    Ok(installed)
}
