//! One-shot UI intents for supported cleanup of absent games.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, PoisonError};

use renderpilot_domain::GameId;
use renderpilot_platform_windows::vulkan_layer::SharedVulkanLayerObservation;
use renderpilot_storage_sqlite::{
    LocalCleanupOwners, NvapiPendingOperationRow, PendingFileMutationRow,
    PendingSharedVulkanMutationRow,
};

use crate::catalog::installation_lifecycle::residue::RetiredRootObservation;

/// Current evidence used to reject an action whose proposal has gone stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetiredLeftoversObservation {
    pub(crate) owners: LocalCleanupOwners,
    pub(crate) root: RetiredRootObservation,
    pub(crate) pending_files: Vec<PendingFileMutationRow>,
    pub(crate) pending_shared_vulkan: Option<PendingSharedVulkanMutationRow>,
    pub(crate) pending_nvapi: Vec<NvapiPendingOperationRow>,
    pub(crate) native_shared_vulkan: Option<Result<SharedVulkanLayerObservation, String>>,
}

/// Opaque process-local token bound to the evidence shown by List.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IssuedRetiredLeftoversIntent {
    pub(crate) token: String,
    pub(crate) semantic_revision: String,
}

/// At most one current action token per game.
#[derive(Debug, Default)]
pub(crate) struct RetiredLeftoversIntentCache {
    intents: Mutex<HashMap<GameId, IssuedRetiredLeftoversIntent>>,
}

impl RetiredLeftoversIntentCache {
    pub(crate) fn issue(&self, game_id: GameId, intent: IssuedRetiredLeftoversIntent) {
        self.intents
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(game_id, intent);
    }

    pub(crate) fn consume(&self, game_id: &GameId) -> Option<IssuedRetiredLeftoversIntent> {
        self.intents
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(game_id)
    }

    pub(crate) fn retain_games(&self, game_ids: &HashSet<GameId>) {
        self.intents
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|game_id, _| game_ids.contains(game_id));
    }
}
