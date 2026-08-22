use renderpilot_domain::GameId;

use crate::{Context, ServiceError};

pub(crate) async fn enter_game_shared_mutation_boundary_async(
    context: &Context,
    game_id: &GameId,
) -> Result<super::GameSharedMutationGuards, ServiceError> {
    loop {
        let game = super::game::enter_game_mutation_boundary_async(context, game_id).await?;
        let shared_vulkan = crate::addons::vulkan_lock::shared_vulkan_lock().await;
        let foreign_owner = context
            .storage()
            .pending_shared_vulkan_mutation()?
            .and_then(|row| match row.scope {
                renderpilot_storage_sqlite::SharedVulkanMutationScope::GameShared
                    if row.game_id.as_ref() != Some(game_id) =>
                {
                    row.game_id
                }
                _ => None,
            });
        if let Some(owner) = foreign_owner {
            drop(shared_vulkan);
            drop(game);
            super::recovery_route::recover_foreign_shared_mutation(context, &owner).await?;
            continue;
        }
        super::recovery_route::recover_pending_now(context)?;
        return Ok(super::GameSharedMutationGuards::new(game, shared_vulkan));
    }
}

pub(crate) fn enter_game_shared_mutation_boundary(
    context: &Context,
    game_id: &GameId,
) -> Result<super::GameSharedMutationGuards, ServiceError> {
    loop {
        let game = super::game::enter_game_mutation_boundary(context, game_id)?;
        let shared_vulkan = crate::addons::vulkan_lock::blocking_shared_vulkan_lock();
        let foreign_owner = context
            .storage()
            .pending_shared_vulkan_mutation()?
            .and_then(|row| match row.scope {
                renderpilot_storage_sqlite::SharedVulkanMutationScope::GameShared
                    if row.game_id.as_ref() != Some(game_id) =>
                {
                    row.game_id
                }
                _ => None,
            });
        if let Some(owner) = foreign_owner {
            drop(shared_vulkan);
            drop(game);
            super::recovery_route::recover_foreign_shared_mutation_blocking(context, &owner)?;
            continue;
        }
        super::recovery_route::recover_pending_now(context)?;
        return Ok(super::GameSharedMutationGuards::new(game, shared_vulkan));
    }
}

pub(crate) async fn enter_shared_only_mutation_boundary_async(
    context: &Context,
) -> Result<crate::addons::vulkan_lock::SharedVulkanMutationGuard, ServiceError> {
    loop {
        super::recovery_route::recover_pending_before_shared_lock(context).await?;
        let shared = crate::addons::vulkan_lock::shared_vulkan_lock().await;
        if super::recovery_route::pending_shared_owner(context)?.is_some() {
            drop(shared);
            continue;
        }
        super::recovery_route::recover_pending_now(context)?;
        return Ok(shared);
    }
}

pub(crate) async fn enter_mutation_boundary_async(
    context: &Context,
    game_id: &GameId,
    include_shared_vulkan: bool,
) -> Result<super::GameMutationBoundary, ServiceError> {
    if include_shared_vulkan {
        Ok(super::GameMutationBoundary::GameShared(
            enter_game_shared_mutation_boundary_async(context, game_id).await?,
        ))
    } else {
        Ok(super::GameMutationBoundary::Game(
            super::game::enter_game_mutation_boundary_async(context, game_id).await?,
        ))
    }
}
