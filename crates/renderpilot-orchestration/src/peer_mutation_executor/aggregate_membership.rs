//! Executor ceremony for an aggregate-only reused-claim membership change.
//!
//! This route has no filesystem mutation and no durable pending row.  It still
//! uses the same game lock and two-observation CAS discipline as physical peer
//! routes: observe, prepare in storage, observe again, then consume the
//! storage-owned permit.

#[cfg(test)]
use std::cell::RefCell;

use renderpilot_storage_sqlite::{MetadataAggregatePreparation, MetadataAggregateTransition};

use crate::ServiceError;
use crate::addons::peer_lifecycle::PeerAggregateMembershipPackage;
use crate::game_mutation_lock::GameMutationGuard;

use super::PeerMutationExecutor;
use super::read_guards::observe_read_guard_requirements;

impl PeerMutationExecutor {
    /// Commits one guarded aggregate-only reused-claim membership change.
    ///
    /// The package is consumed by this ceremony.  Callers cannot replace its
    /// roots, aggregate images, contract, or requirements between preparation
    /// and the SQLite CAS commit.
    pub(crate) fn commit_reused_claim_membership(
        &self,
        guard: &GameMutationGuard,
        package: PeerAggregateMembershipPackage,
    ) -> Result<(), ServiceError> {
        let package = package.into_commit_inputs();
        authenticate_guard(guard, &package)?;

        let initial_read_guards = observe_read_guard_requirements(package.read_guards())?;
        let mutation = self.build_metadata_aggregate_mutation(
            guard,
            package.before_peer(),
            package.after_peer(),
            Some(package.unchanged_topology()),
        )?;
        let permit = self
            .runtime
            .prepare_metadata_aggregate(MetadataAggregatePreparation::new(
                mutation,
                MetadataAggregateTransition::ReusedClaimMembership {
                    canonical_game_root: package.canonical_game_root(),
                    sealed_roots: package.sealed_roots(),
                    initial_read_guards: &initial_read_guards,
                },
            ))?;

        #[cfg(test)]
        run_test_between_observation_hook();

        // This second observation is deliberately adjacent to permit
        // consumption.  Any live reused-claim drift fails closed in storage;
        // no physical endpoint or pending-mutation fallback is involved.
        let final_read_guards = observe_read_guard_requirements(package.read_guards())?;
        let committed = self
            .runtime
            .commit_metadata_aggregate(permit, final_read_guards)?;
        if let Err(error) = self.runtime.cleanup_metadata_aggregate(committed) {
            tracing::warn!(
                "peer reused-claim membership committed but cleanup is pending: {error}"
            );
        }
        Ok(())
    }
}

fn authenticate_guard(
    guard: &GameMutationGuard,
    package: &crate::addons::peer_lifecycle::PeerAggregateMembershipCommitInputs,
) -> Result<(), ServiceError> {
    let game_id = guard.game_id();
    if game_id != package.game_id()
        || game_id != package.after_peer().game_id()
        || game_id != &package.unchanged_topology().game_id
    {
        return Err(ServiceError::invalid_input(
            "peer aggregate membership images must belong to the guarded game",
        ));
    }
    Ok(())
}

#[cfg(test)]
type BetweenObservationHook = Box<dyn FnOnce() + Send + 'static>;

#[cfg(test)]
thread_local! {
    static BETWEEN_OBSERVATION_HOOK: RefCell<Option<BetweenObservationHook>> = RefCell::new(None);
}

#[cfg(test)]
fn set_between_observation_hook(hook: impl FnOnce() + Send + 'static) {
    BETWEEN_OBSERVATION_HOOK.with(|hooks| {
        assert!(hooks.borrow_mut().replace(Box::new(hook)).is_none());
    });
}

#[cfg(test)]
fn clear_between_observation_hook() {
    BETWEEN_OBSERVATION_HOOK.with(|hooks| hooks.borrow_mut().take());
}

#[cfg(test)]
fn run_test_between_observation_hook() {
    let hook = BETWEEN_OBSERVATION_HOOK.with(|hooks| hooks.borrow_mut().take());
    if let Some(hook) = hook {
        hook();
    }
}

#[cfg(test)]
mod tests;
