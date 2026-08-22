//! Durable coordination for the singleton shared RenoDX Vulkan layer.
//!
//! The row is a database fence for the filesystem transaction owned by the
//! orchestration layer. It never claims that the filesystem itself is ACID;
//! it records enough exact identity to make recovery deterministic and keeps
//! game-scoped catalog publication behind the same durable boundary.

mod commit;
mod model;
mod preparation;
mod queries;
mod recovery;
mod reservation;
mod validation;

#[cfg(test)]
mod tests;

pub use model::{
    BeginSharedVulkanMutation, PendingSharedVulkanMutationRow, PendingSharedVulkanMutationState,
    PreparedSharedVulkanMutationResolutionFence, SharedArtifactMutation,
    SharedVulkanMutationCommit, SharedVulkanMutationReservation, SharedVulkanMutationScope,
};

pub(crate) use validation::{
    assert_no_shared_mutation_for_game_within_transaction,
    assert_no_shared_mutation_id_within_transaction,
};

pub(crate) const RESOURCE_KEY: &str = crate::schema::SHARED_VULKAN_RESOURCE_KEY;
