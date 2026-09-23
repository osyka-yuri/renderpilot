//! RenoDX adapter for the shared Unreal Engine.ini service.

use renderpilot_domain::{AddonKind, GameId};

use crate::addons::availability_pipeline;
use crate::addons::engine_config::service;
use crate::addons::engine_config::{EngineIniRecipe, EngineIniRecipeSet};
use crate::addons::records;
use crate::addons::renodx::game_context::analyze_and_resolve;
use crate::addons::renodx::matcher::RenoDxResolution;
use crate::addons::renodx::types::RenoDxManifest;
use crate::{Context, ServiceError};

/// Applies/reapplies the current typed RenoDX Engine.ini guidance under the
/// same game mutation boundary as the other tool commands.  The manifest and
/// target are resolved server-side; clients cannot supply either path or INI.
pub async fn apply(
    context: &Context,
    manifest: &RenoDxManifest,
    game_id: &GameId,
    safety: crate::GameSafetyPermit,
) -> Result<(), ServiceError> {
    let guard =
        crate::mutation_boundary::enter_game_mutation_boundary_async(context, game_id).await?;
    crate::FileSafetyAuthority::new().authorize_game_commit(
        context,
        crate::addons::mutation_features::RENODX_INSTALL,
        &guard,
        &safety,
        || apply_for_installed(context, manifest, game_id),
    )
}

/// Reconciles typed RenoDX Engine.ini guidance after a committed install or
/// update.  The operation is best-effort by contract: an Engine.ini failure is
/// logged and never rolls back the already committed add-on lifecycle.
pub(crate) async fn reconcile_after_commit(
    context: &Context,
    manifest: &RenoDxManifest,
    game_id: &GameId,
    safety: crate::GameSafetyPermit,
) -> Result<(), ServiceError> {
    let result: Result<(), ServiceError> = async {
        let guard =
            crate::mutation_boundary::enter_game_mutation_boundary_async(context, game_id).await?;
        let preflight = availability_pipeline::preflight(
            context,
            game_id,
            AddonKind::RenoDx,
            manifest,
            analyze_and_resolve,
        )?;
        if records::record_of_kind(context, game_id, AddonKind::RenoDx)?.is_none() {
            return Ok(());
        }
        let Some(recipes) = recipes_for_resolution(&preflight.resolution)? else {
            return Ok(());
        };
        crate::FileSafetyAuthority::new().authorize_game_commit(
            context,
            crate::addons::mutation_features::RENODX_INSTALL,
            &guard,
            &safety,
            || {
                service::apply_resolved(
                    context.storage(),
                    game_id,
                    AddonKind::RenoDx,
                    &preflight.engine_config_resolution,
                    Some(&recipes),
                    &format!("renodx-engine-reconcile-{}", ulid::Ulid::generate()),
                )
                .map_err(|error| ServiceError::command_failed(error.to_string()))?;
                Ok(())
            },
        )?;
        Ok(())
    }
    .await;
    if let Err(error) = result {
        tracing::warn!("RenoDX Engine.ini reconcile failed after committed lifecycle: {error}");
    }
    Ok(())
}

/// Applies/reapplies only backend-derived typed guidance for an installed
/// RenoDX record.  The caller must already hold the game mutation boundary.
pub(crate) fn apply_for_installed(
    context: &Context,
    manifest: &RenoDxManifest,
    game_id: &GameId,
) -> Result<(), ServiceError> {
    let record = records::record_of_kind(context, game_id, AddonKind::RenoDx)?
        .ok_or_else(|| ServiceError::invalid_input("RenoDX is not installed"))?;
    service::recover_pending(context.storage(), game_id, AddonKind::RenoDx)
        .map_err(|error| ServiceError::command_failed(error.to_string()))?;
    let preflight = availability_pipeline::preflight(
        context,
        game_id,
        AddonKind::RenoDx,
        manifest,
        analyze_and_resolve,
    )?;
    let recipes = recipes_for_resolution(&preflight.resolution)?
        .ok_or_else(|| ServiceError::invalid_input("RenoDX has no typed Engine.ini recipe"))?;
    service::apply_resolved(
        context.storage(),
        game_id,
        record.kind(),
        &preflight.engine_config_resolution,
        Some(&recipes),
        &format!("renodx-engine-apply-{}", ulid::Ulid::generate()),
    )
    .map_err(|error| ServiceError::command_failed(error.to_string()))?
    .ok_or_else(|| {
        ServiceError::invalid_input("Unreal Engine.ini target is not currently ready")
    })?;
    Ok(())
}

fn recipes_for_resolution(
    resolution: &RenoDxResolution,
) -> Result<Option<EngineIniRecipeSet>, ServiceError> {
    let guidance = match resolution {
        RenoDxResolution::Installable(plan) => &plan.guidance,
        RenoDxResolution::External {
            file_install: Some(plan),
            ..
        } => &plan.guidance,
        _ => return Ok(None),
    };
    let recipes = guidance
        .iter()
        .filter_map(|item| {
            item.engine_ini
                .as_ref()
                .map(|recipe| recipe.to_engine_config_recipe(&item.id))
        })
        .collect::<Result<Vec<EngineIniRecipe>, _>>()
        .map_err(|error| ServiceError::invalid_input(error.to_string()))?;
    if recipes.is_empty() {
        return Ok(None);
    }
    EngineIniRecipeSet::from_recipes(recipes.iter())
        .map(Some)
        .map_err(|error| ServiceError::invalid_input(error.to_string()))
}
