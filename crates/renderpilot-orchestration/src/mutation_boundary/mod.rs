//! Process-local mutation boundaries and durable recovery routing.
//!
//! Game locks and the shared Vulkan lock are deliberately process-local. The
//! SQLite pending-mutation singleton is the durable fence within one catalog;
//! it is not a machine-wide lock and does not create a persistent sidecar.

mod game;
mod recovery_route;
mod shared;

/// Proof that the caller owns both boundaries used by a combined game/shared
/// commit. Construction is private to this module, which fixes the global lock
/// order to game first, shared Vulkan second.
pub(crate) struct GameSharedMutationGuards {
    game: crate::game_mutation_lock::GameMutationGuard,
    shared_vulkan: crate::addons::vulkan_lock::SharedVulkanMutationGuard,
}

/// Final boundary selected from a freshly resolved operation plan.
pub(crate) enum GameMutationBoundary {
    Game(crate::game_mutation_lock::GameMutationGuard),
    GameShared(GameSharedMutationGuards),
}

impl GameSharedMutationGuards {
    fn new(
        game: crate::game_mutation_lock::GameMutationGuard,
        shared_vulkan: crate::addons::vulkan_lock::SharedVulkanMutationGuard,
    ) -> Self {
        Self {
            game,
            shared_vulkan,
        }
    }

    pub(crate) fn game(&self) -> &crate::game_mutation_lock::GameMutationGuard {
        &self.game
    }

    pub(crate) fn shared_vulkan(&self) -> &crate::addons::vulkan_lock::SharedVulkanMutationGuard {
        &self.shared_vulkan
    }

    /// Releases the shared boundary while retaining the already-recovered
    /// game boundary for follow-up game-only work.
    pub(crate) fn into_game(self) -> crate::game_mutation_lock::GameMutationGuard {
        let Self {
            game,
            shared_vulkan,
        } = self;
        drop(shared_vulkan);
        game
    }
}

pub(crate) use game::{
    enter_game_mutation_boundaries, enter_game_mutation_boundary,
    enter_game_mutation_boundary_async,
};
pub(crate) use shared::{
    enter_game_shared_mutation_boundary, enter_game_shared_mutation_boundary_async,
    enter_mutation_boundary_async, enter_shared_only_mutation_boundary_async,
};
