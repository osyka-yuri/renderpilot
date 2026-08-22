use renderpilot_domain::GameId;

use crate::{Context, ServiceError, game_mutation_lock};

pub(crate) fn enter_game_mutation_boundary(
    context: &Context,
    game_id: &GameId,
) -> Result<game_mutation_lock::GameMutationGuard, ServiceError> {
    let guard = game_mutation_lock::blocking_lock(game_id);
    crate::file_mutation::recover_pending(context, &guard)?;
    super::recovery_route::recover_pending_shared_for_game_blocking(context, &guard)?;
    crate::addons::reconcile_legacy_managed_files_locked(context, &guard, game_id)?;
    Ok(guard)
}

pub(crate) fn enter_game_mutation_boundaries(
    context: &Context,
    game_ids: impl IntoIterator<Item = GameId>,
) -> Result<game_mutation_lock::GameMutationGuardSet, ServiceError> {
    let mut game_ids = game_ids.into_iter().collect::<Vec<_>>();
    game_ids.sort();
    game_ids.dedup();

    let mut guards = Vec::with_capacity(game_ids.len());
    for game_id in game_ids {
        guards.push(game_mutation_lock::blocking_lock(&game_id));
    }
    for guard in &guards {
        crate::file_mutation::recover_pending(context, guard)?;
    }
    for guard in &guards {
        super::recovery_route::recover_pending_shared_for_game_blocking(context, guard)?;
    }
    for guard in &guards {
        crate::addons::reconcile_legacy_managed_files_locked(context, guard, guard.game_id())?;
    }
    Ok(game_mutation_lock::GameMutationGuardSet::from_guards(
        guards,
    ))
}

pub(crate) async fn enter_game_mutation_boundary_async(
    context: &Context,
    game_id: &GameId,
) -> Result<game_mutation_lock::GameMutationGuard, ServiceError> {
    let guard = game_mutation_lock::lock(game_id).await;
    crate::file_mutation::recover_pending(context, &guard)?;
    super::recovery_route::recover_pending_shared_for_game_async(context, &guard).await?;
    crate::addons::reconcile_legacy_managed_files_locked(context, &guard, game_id)?;
    Ok(guard)
}
