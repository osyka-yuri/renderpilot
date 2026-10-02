use renderpilot_domain::GameId;

use crate::{Context, ServiceError, game_mutation_lock};

/// Acquires the per-game boundary without running pending file/shared
/// recovery or legacy managed-file reconciliation.
///
/// Read-only observation flows use this entry and decide explicitly whether
/// their own catalog writes are safe after examining the installation.
pub(crate) fn enter_game_observation_boundary(
    game_id: &GameId,
) -> game_mutation_lock::GameMutationGuard {
    game_mutation_lock::blocking_lock(game_id)
}

/// Acquires multiple observation boundaries in deterministic identity order.
#[cfg(any(windows, test))]
pub(crate) fn enter_game_observation_boundaries<'a>(
    game_ids: impl IntoIterator<Item = &'a GameId>,
) -> game_mutation_lock::GameMutationGuardSet {
    let mut game_ids = game_ids.into_iter().collect::<Vec<_>>();
    game_ids.sort();
    game_ids.dedup();

    let guards = game_ids
        .into_iter()
        .map(game_mutation_lock::blocking_lock)
        .collect();
    game_mutation_lock::GameMutationGuardSet::from_guards(guards)
}

/// Async counterpart to [`enter_game_observation_boundary`].
pub(crate) async fn enter_game_observation_boundary_async(
    game_id: &GameId,
) -> game_mutation_lock::GameMutationGuard {
    game_mutation_lock::lock(game_id).await
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_boundary_holds_only_the_game_lock() {
        let game_id = GameId::new("manual:observation-boundary").expect("game id");
        let guard = enter_game_observation_boundary(&game_id);

        assert_eq!(guard.game_id(), &game_id);
        assert!(crate::game_mutation_lock::try_lock(&game_id).is_none());
    }

    #[test]
    fn observation_boundaries_sort_and_deduplicate_ids() {
        let a = GameId::new("game:observation-a").expect("game id");
        let b = GameId::new("game:observation-b").expect("game id");
        let guards = enter_game_observation_boundaries([&b, &a, &b]);

        assert_eq!(guards.game_ids().collect::<Vec<_>>(), vec![&a, &b]);
    }

    #[tokio::test]
    async fn async_observation_boundary_holds_only_the_game_lock() {
        let game_id = GameId::new("manual:async-observation-boundary").expect("game id");
        let guard = enter_game_observation_boundary_async(&game_id).await;

        assert_eq!(guard.game_id(), &game_id);
        assert!(crate::game_mutation_lock::try_lock(&game_id).is_none());
    }
}
