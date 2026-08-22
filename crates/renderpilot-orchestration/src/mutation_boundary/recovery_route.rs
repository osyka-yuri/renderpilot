use renderpilot_domain::GameId;
use renderpilot_storage_sqlite::SharedVulkanMutationScope;

use crate::{Context, ServiceError, game_mutation_lock};

fn pending_shared_owner_is(context: &Context, game_id: &GameId) -> Result<bool, ServiceError> {
    Ok(context
        .storage()
        .pending_shared_vulkan_mutation()?
        .is_some_and(|row| {
            row.scope == SharedVulkanMutationScope::GameShared
                && row.game_id.as_ref() == Some(game_id)
        }))
}

fn recover_pending_locked(context: &Context) -> Result<(), ServiceError> {
    let registry = crate::addons::renodx::platform::vulkan::native_registry();
    crate::addons::shared_vulkan_mutation::recover_pending(context, registry)
}

pub(crate) fn recover_pending_shared_for_game_blocking(
    context: &Context,
    game: &game_mutation_lock::GameMutationGuard,
) -> Result<(), ServiceError> {
    if !pending_shared_owner_is(context, game.game_id())? {
        return Ok(());
    }
    let _shared = crate::addons::vulkan_lock::blocking_shared_vulkan_lock();
    if pending_shared_owner_is(context, game.game_id())? {
        recover_pending_locked(context)?;
    }
    Ok(())
}

pub(crate) async fn recover_pending_shared_for_game_async(
    context: &Context,
    game: &game_mutation_lock::GameMutationGuard,
) -> Result<(), ServiceError> {
    if !pending_shared_owner_is(context, game.game_id())? {
        return Ok(());
    }
    let _shared = crate::addons::vulkan_lock::shared_vulkan_lock().await;
    if pending_shared_owner_is(context, game.game_id())? {
        recover_pending_locked(context)?;
    }
    Ok(())
}

pub(crate) async fn recover_pending_before_shared_lock(
    context: &Context,
) -> Result<(), ServiceError> {
    let Some(owner) = context
        .storage()
        .pending_shared_vulkan_mutation()?
        .and_then(|row| match row.scope {
            SharedVulkanMutationScope::GameShared => row.game_id,
            SharedVulkanMutationScope::SharedOnly => None,
        })
    else {
        return Ok(());
    };
    let _owner_guard = super::game::enter_game_mutation_boundary_async(context, &owner).await?;
    Ok(())
}

pub(crate) async fn recover_foreign_shared_mutation(
    context: &Context,
    owner: &GameId,
) -> Result<(), ServiceError> {
    let _owner_guard = super::game::enter_game_mutation_boundary_async(context, owner).await?;
    Ok(())
}

pub(crate) fn recover_foreign_shared_mutation_blocking(
    context: &Context,
    owner: &GameId,
) -> Result<(), ServiceError> {
    let _owner_guard = super::game::enter_game_mutation_boundary(context, owner)?;
    Ok(())
}

pub(crate) fn pending_shared_owner(context: &Context) -> Result<Option<GameId>, ServiceError> {
    Ok(context
        .storage()
        .pending_shared_vulkan_mutation()?
        .and_then(|row| match row.scope {
            SharedVulkanMutationScope::GameShared => row.game_id,
            SharedVulkanMutationScope::SharedOnly => None,
        }))
}

pub(crate) fn recover_pending_now(context: &Context) -> Result<(), ServiceError> {
    recover_pending_locked(context)
}
