//! Luma update route facade.
//!
//! The facade selects the inactive or active topology route from one guarded
//! phase-one snapshot, then keeps that route fixed through preparation and
//! commit.

mod active;
mod inactive;
mod route;

use renderpilot_domain::GameId;

use super::engine_config;
use crate::addons::luma::types::LumaManifest;
use crate::addons::reshade::types::ReshadeSourceCatalog;
use crate::net::ProgressObserver;
use crate::{Context, ServiceError};

/// Complete request for a Luma update or repair.
pub struct UpdateRequest<'a> {
    /// Application services and storage.
    pub context: &'a Context,
    /// Luma release manifest used to resolve the update.
    pub manifest: &'a LumaManifest,
    /// ReShade source catalog used when the host binary must be updated.
    pub reshade_sources: &'a ReshadeSourceCatalog,
    /// Game whose installation is being updated.
    pub game_id: &'a GameId,
    /// Whether to replace the full managed file set even when versions match.
    pub force_full: bool,
    /// Fresh permit authorizing this game-file mutation.
    pub safety: crate::GameSafetyPermit,
    /// Optional download progress observer.
    pub progress: Option<&'a ProgressObserver<'a>>,
}

/// Applies an update to the installed Luma add-on and, when needed, its ReShade
/// host.
pub async fn update(request: UpdateRequest<'_>) -> Result<(), ServiceError> {
    let context = request.context;
    let game_id = request.game_id;
    let manifest = request.manifest;
    let safety = request.safety.clone();
    let phase1 = {
        let guard =
            crate::mutation_boundary::enter_game_mutation_boundary_async(context, game_id).await?;
        route::snapshot_update_route(context, request.manifest, &guard)?
    };

    match phase1 {
        route::UpdatePhase1::Inactive(phase1) => inactive::update(request, *phase1).await?,
        route::UpdatePhase1::Active(phase1) => active::update(request, *phase1).await?,
    }

    if let Err(error) = crate::catalog::refresh_game_components(context, game_id).await {
        tracing::warn!("failed to refresh game components after Luma update: {error}");
    }
    engine_config::reconcile_after_commit(context, manifest, game_id, safety).await?;

    Ok(())
}
