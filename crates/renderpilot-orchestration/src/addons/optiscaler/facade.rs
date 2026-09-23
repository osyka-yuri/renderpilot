//! Public OptiScaler use-case boundary.
//!
//! Presentation layers provide only user input. This facade resolves validated
//! immutable metadata and keeps lifecycle implementation models private.

use std::path::Path;

use renderpilot_domain::{GameId, OptiScalerInstallState};

use super::lifecycle;
use super::types::{OptiScalerAvailability, OptiScalerOperationResult, OptiScalerUpdateCheck};
use crate::net::ProgressObserver;
use crate::{Context, ServiceError};

/// Inputs for a first managed OptiScaler installation.
pub struct InstallOptiScalerRequest<'a> {
    /// Application services.
    pub context: &'a Context,
    /// Fresh game-scoped authority for the final file commit.
    pub safety: crate::GameSafetyPermit,
    /// Optional explicit module selection.
    pub modules: Option<&'a [String]>,
    /// Optional artifact-download progress observer.
    pub progress: Option<&'a ProgressObserver<'a>>,
}

impl InstallOptiScalerRequest<'_> {
    /// Returns the game scope represented by this request's safety permit.
    #[must_use]
    pub fn game_id(&self) -> &GameId {
        self.safety.game_id()
    }
}

/// Inputs for an update or repair.
pub struct UpdateOptiScalerRequest<'a> {
    /// Application services.
    pub context: &'a Context,
    /// Fresh game-scoped authority for the final file commit.
    pub safety: crate::GameSafetyPermit,
    /// Optional artifact-download progress observer.
    pub progress: Option<&'a ProgressObserver<'a>>,
}

impl UpdateOptiScalerRequest<'_> {
    /// Returns the game scope represented by this request's safety permit.
    #[must_use]
    pub fn game_id(&self) -> &GameId {
        self.safety.game_id()
    }
}

/// Inputs for changing the module selection of an existing installation.
pub struct SetOptiScalerModulesRequest<'a> {
    /// Application services.
    pub context: &'a Context,
    /// Fresh game-scoped authority for the final file commit.
    pub safety: crate::GameSafetyPermit,
    /// Explicit module selection.
    pub modules: &'a [String],
    /// Optional artifact-download progress observer.
    pub progress: Option<&'a ProgressObserver<'a>>,
}

impl SetOptiScalerModulesRequest<'_> {
    /// Returns the game scope represented by this request's safety permit.
    #[must_use]
    pub fn game_id(&self) -> &GameId {
        self.safety.game_id()
    }
}

/// Inputs for explicitly moving an installation to another executable.
pub struct RelocateOptiScalerRequest<'a> {
    /// Application services.
    pub context: &'a Context,
    /// Fresh game-scoped authority for the final file commit.
    pub safety: crate::GameSafetyPermit,
    /// Explicit executable selected by the user.
    pub target_exe: &'a Path,
    /// Optional artifact-download progress observer.
    pub progress: Option<&'a ProgressObserver<'a>>,
}

impl RelocateOptiScalerRequest<'_> {
    /// Returns the game scope represented by this request's safety permit.
    #[must_use]
    pub fn game_id(&self) -> &GameId {
        self.safety.game_id()
    }
}

/// Evaluates one game using validated local/remote metadata.
pub async fn availability(
    context: &Context,
    game_id: &GameId,
) -> Result<OptiScalerAvailability, ServiceError> {
    let (manifest, catalog) = tokio::try_join!(
        super::manifest_store::get_or_fetch_manifest(),
        super::compatibility_catalog::get_or_fetch_catalog(),
    )?;
    let result = super::load_availability(context, &manifest, &catalog, game_id).await?;
    Ok(result)
}

/// Installs the selected immutable release and module closure.
pub async fn install(
    request: InstallOptiScalerRequest<'_>,
) -> Result<OptiScalerOperationResult, ServiceError> {
    let context = request.context;
    let game_id = request.game_id().clone();
    let (manifest, catalog) = tokio::try_join!(
        super::manifest_store::get_or_fetch_manifest(),
        super::compatibility_catalog::get_or_fetch_catalog(),
    )?;
    let result = lifecycle::install(lifecycle::InstallOptiScalerRequest {
        context,
        manifest: &manifest,
        catalog: &catalog,
        safety: request.safety,
        modules: request.modules,
        progress: request.progress,
    })
    .await?;

    if let Err(error) = crate::catalog::refresh_game_components(context, &game_id).await {
        tracing::warn!("failed to refresh game components after OptiScaler install: {error}");
    }

    Ok(result)
}

/// Updates a managed installation to the selected stable release.
pub async fn update(
    request: UpdateOptiScalerRequest<'_>,
) -> Result<OptiScalerOperationResult, ServiceError> {
    mutate(request, false).await
}

/// Reapplies every managed invariant without changing user-owned drift.
pub async fn repair(
    request: UpdateOptiScalerRequest<'_>,
) -> Result<OptiScalerOperationResult, ServiceError> {
    mutate(request, true).await
}

async fn mutate(
    request: UpdateOptiScalerRequest<'_>,
    repair: bool,
) -> Result<OptiScalerOperationResult, ServiceError> {
    let context = request.context;
    let game_id = request.game_id().clone();
    let (manifest, catalog) = tokio::try_join!(
        super::manifest_store::get_or_fetch_manifest(),
        super::compatibility_catalog::get_or_fetch_catalog(),
    )?;
    let request_inner = lifecycle::UpdateOptiScalerRequest {
        context,
        manifest: &manifest,
        catalog: &catalog,
        safety: request.safety,
        progress: request.progress,
    };
    let result = if repair {
        lifecycle::repair(request_inner).await?
    } else {
        lifecycle::update(request_inner).await?
    };

    if let Err(error) = crate::catalog::refresh_game_components(context, &game_id).await {
        tracing::warn!("failed to refresh game components after OptiScaler mutate: {error}");
    }

    Ok(result)
}

/// Applies an explicit validated module selection.
pub async fn set_modules(
    request: SetOptiScalerModulesRequest<'_>,
) -> Result<OptiScalerOperationResult, ServiceError> {
    let context = request.context;
    let game_id = request.game_id().clone();
    let (manifest, catalog) = tokio::try_join!(
        super::manifest_store::get_or_fetch_manifest(),
        super::compatibility_catalog::get_or_fetch_catalog(),
    )?;
    let result = lifecycle::set_modules(lifecycle::SetOptiScalerModulesRequest {
        context,
        manifest: &manifest,
        catalog: &catalog,
        safety: request.safety,
        modules: request.modules,
        progress: request.progress,
    })
    .await?;

    if let Err(error) = crate::catalog::refresh_game_components(context, &game_id).await {
        tracing::warn!("failed to refresh game components after OptiScaler set_modules: {error}");
    }

    Ok(result)
}

/// Relocates a managed installation to an explicitly selected executable.
pub async fn relocate(
    request: RelocateOptiScalerRequest<'_>,
) -> Result<OptiScalerOperationResult, ServiceError> {
    let context = request.context;
    let game_id = request.game_id().clone();
    let (manifest, catalog) = tokio::try_join!(
        super::manifest_store::get_or_fetch_manifest(),
        super::compatibility_catalog::get_or_fetch_catalog(),
    )?;
    let result = lifecycle::relocate(lifecycle::RelocateOptiScalerRequest {
        context,
        manifest: &manifest,
        catalog: &catalog,
        safety: request.safety,
        target_exe: request.target_exe,
        progress: request.progress,
    })
    .await?;

    if let Err(error) = crate::catalog::refresh_game_components(context, &game_id).await {
        tracing::warn!("failed to refresh game components after OptiScaler relocate: {error}");
    }

    Ok(result)
}

/// Checks one managed installation for release or integrity drift.
pub async fn check_update(
    context: &Context,
    game_id: &GameId,
) -> Result<OptiScalerUpdateCheck, ServiceError> {
    let (manifest, catalog) = tokio::try_join!(
        super::manifest_store::get_or_fetch_manifest(),
        super::compatibility_catalog::get_or_fetch_catalog(),
    )?;
    lifecycle::check_update(context, &manifest, &catalog, game_id).await
}

/// Checks every managed OptiScaler installation using one manifest snapshot.
pub async fn check_updates(
    context: &Context,
) -> Result<Vec<(GameId, OptiScalerUpdateCheck)>, ServiceError> {
    let manifest = super::manifest_store::get_or_fetch_manifest().await?;
    super::check_updates_with_manifest(context, &manifest).await
}

/// Returns managed state without exposing repositories to presentation code.
pub fn status(
    context: &Context,
    game_id: &GameId,
) -> Result<Option<OptiScalerInstallState>, ServiceError> {
    super::stored_status(context, game_id)
}

/// Uninstall and recovery intentionally use only persisted local state.
pub async fn uninstall(
    context: &Context,
    game_id: &GameId,
) -> Result<OptiScalerOperationResult, ServiceError> {
    let result = lifecycle::uninstall(context, game_id).await?;

    if let Err(error) = crate::catalog::refresh_game_components(context, game_id).await {
        tracing::warn!("failed to refresh game components after OptiScaler uninstall: {error}");
    }

    Ok(result)
}
