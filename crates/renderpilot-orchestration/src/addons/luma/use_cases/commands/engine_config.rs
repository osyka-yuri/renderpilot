//! Luma adapter for the shared Unreal Engine.ini service.

use renderpilot_domain::{AddonKind, GameId};

use crate::addons::availability_pipeline;
use crate::addons::engine_config::EngineIniRecipeSet;
use crate::addons::engine_config::service;
use crate::addons::luma::game_context::analyze_and_resolve;
use crate::addons::luma::matcher::LumaResolution;
use crate::addons::luma::types::LumaManifest;
use crate::addons::records;
use crate::{Context, ServiceError};

/// Applies/reapplies the current typed Luma Engine.ini guidance under the
/// normal game mutation boundary.  Manifest, project identity and target are
/// all resolved on the server.
pub async fn apply(
    context: &Context,
    manifest: &LumaManifest,
    game_id: &GameId,
    safety: crate::GameSafetyPermit,
) -> Result<(), ServiceError> {
    let guard =
        crate::mutation_boundary::enter_game_mutation_boundary_async(context, game_id).await?;
    crate::FileSafetyAuthority::new().authorize_game_commit(
        context,
        crate::addons::mutation_features::LUMA_INSTALL,
        &guard,
        &safety,
        || apply_for_installed(context, manifest, game_id),
    )
}

/// Best-effort post-commit reconciliation for typed Luma Unreal guidance.
pub(crate) async fn reconcile_after_commit(
    context: &Context,
    manifest: &LumaManifest,
    game_id: &GameId,
    safety: crate::GameSafetyPermit,
) -> Result<(), ServiceError> {
    let result: Result<(), ServiceError> = async {
        let guard =
            crate::mutation_boundary::enter_game_mutation_boundary_async(context, game_id).await?;
        let preflight = availability_pipeline::preflight(
            context,
            game_id,
            AddonKind::Luma,
            manifest,
            analyze_and_resolve,
        )?;
        if records::record_of_kind(context, game_id, AddonKind::Luma)?.is_none() {
            return Ok(());
        }
        let Some(recipes) = recipes_for_resolution(&preflight.resolution)? else {
            return Ok(());
        };
        crate::FileSafetyAuthority::new().authorize_game_commit(
            context,
            crate::addons::mutation_features::LUMA_INSTALL,
            &guard,
            &safety,
            || {
                service::apply_resolved(
                    context.storage(),
                    game_id,
                    AddonKind::Luma,
                    &preflight.engine_config_resolution,
                    Some(&recipes),
                    &format!("luma-engine-reconcile-{}", ulid::Ulid::generate()),
                )
                .map_err(|error| ServiceError::command_failed(error.to_string()))?;
                Ok(())
            },
        )?;
        Ok(())
    }
    .await;
    if let Err(error) = result {
        tracing::warn!("Luma Engine.ini reconcile failed after committed lifecycle: {error}");
    }
    Ok(())
}

/// Applies/reapplies only server-resolved typed guidance for an installed Luma
/// record.  No client path or recipe is accepted.
pub(crate) fn apply_for_installed(
    context: &Context,
    manifest: &LumaManifest,
    game_id: &GameId,
) -> Result<(), ServiceError> {
    let record = records::record_of_kind(context, game_id, AddonKind::Luma)?
        .ok_or_else(|| ServiceError::invalid_input("Luma is not installed"))?;
    service::recover_pending(context.storage(), game_id, AddonKind::Luma)
        .map_err(|error| ServiceError::command_failed(error.to_string()))?;
    let preflight = availability_pipeline::preflight(
        context,
        game_id,
        AddonKind::Luma,
        manifest,
        analyze_and_resolve,
    )?;
    let recipes = recipes_for_resolution(&preflight.resolution)?
        .ok_or_else(|| ServiceError::invalid_input("Luma has no typed Engine.ini recipe"))?;
    service::apply_resolved(
        context.storage(),
        game_id,
        record.kind(),
        &preflight.engine_config_resolution,
        Some(&recipes),
        &format!("luma-engine-apply-{}", ulid::Ulid::generate()),
    )
    .map_err(|error| ServiceError::command_failed(error.to_string()))?
    .ok_or_else(|| {
        ServiceError::invalid_input("Unreal Engine.ini target is not currently ready")
    })?;
    Ok(())
}

fn recipes_for_resolution(
    resolution: &LumaResolution,
) -> Result<Option<EngineIniRecipeSet>, ServiceError> {
    let guidance = match resolution {
        LumaResolution::Installable(plan) => &plan.guidance,
        _ => return Ok(None),
    };
    let mut recipes = guidance
        .iter()
        .filter_map(|item| item.engine_ini.as_ref())
        .peekable();
    if recipes.peek().is_none() {
        return Ok(None);
    }
    EngineIniRecipeSet::from_recipes(recipes)
        .map(Some)
        .map_err(|error| ServiceError::invalid_input(error.to_string()))
}
