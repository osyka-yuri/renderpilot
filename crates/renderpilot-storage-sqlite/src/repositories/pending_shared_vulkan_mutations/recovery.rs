//! Recovery fences and completion of prepared shared Vulkan mutations.

use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::GameId;
use rusqlite::params;

use crate::{error::storage_error, repositories::observations};

use super::super::SqliteStorage;
use super::RESOURCE_KEY;
use super::model::{
    PendingSharedVulkanMutationState, PreparedSharedVulkanMutationResolutionFence,
    SharedCatalogBinding, SharedVulkanMutationScope,
};
use super::queries::read_shared_row;
use super::validation::{ensure_exact_owner, validate_catalog_binding, validate_owner};

impl SqliteStorage {
    /// Captures a typed fence before recovery removes a prepared row.
    pub fn fence_prepared_shared_vulkan_mutation_resolution(
        &self,
        id: &str,
        scope: SharedVulkanMutationScope,
        game_id: Option<&GameId>,
    ) -> AppResult<PreparedSharedVulkanMutationResolutionFence> {
        validate_owner(scope, game_id)?;
        self.with_transaction(|transaction| {
            let row = read_shared_row(transaction)?.ok_or_else(|| {
                AppError::storage_failed(format!(
                    "prepared shared Vulkan mutation '{id}' is missing"
                ))
            })?;
            ensure_exact_owner(&row, id, scope, game_id)?;
            if row.state != PendingSharedVulkanMutationState::Prepared {
                return Err(AppError::storage_failed(format!(
                    "shared Vulkan mutation '{id}' is not prepared"
                )));
            }
            let catalog_binding = if let Some(game_id) = game_id {
                if !observations::catalog_exists_within_transaction(transaction, game_id)? {
                    SharedCatalogBinding::Absent
                } else {
                    match observations::readiness_within_transaction(transaction, game_id)? {
                        crate::repositories::observations::CatalogReadiness::Invalidated {
                            authority_epoch,
                            mutation_token: Some(token),
                            ..
                        } if token == id => SharedCatalogBinding::Invalidated { authority_epoch },
                        _ => {
                            return Err(AppError::storage_failed(format!(
                                "prepared shared Vulkan mutation '{id}' has no matching invalidated catalog authority"
                            )));
                        }
                    }
                }
            } else {
                SharedCatalogBinding::Absent
            };
            Ok(PreparedSharedVulkanMutationResolutionFence {
                id: id.to_owned(),
                scope,
                game_id: game_id.cloned(),
                catalog_binding,
            })
        })
    }

    /// Completes recovery after the caller restored exact pre-mutation state.
    pub fn complete_prepared_shared_vulkan_mutation_restored(
        &self,
        fence: PreparedSharedVulkanMutationResolutionFence,
    ) -> AppResult<()> {
        self.complete_prepared_shared_vulkan_mutation_resolution(fence)
    }

    /// Completes cleanup-only recovery while retaining any catalog invalidation.
    pub fn complete_prepared_shared_vulkan_mutation_without_restore(
        &self,
        fence: PreparedSharedVulkanMutationResolutionFence,
    ) -> AppResult<()> {
        self.complete_prepared_shared_vulkan_mutation_resolution(fence)
    }

    fn complete_prepared_shared_vulkan_mutation_resolution(
        &self,
        fence: PreparedSharedVulkanMutationResolutionFence,
    ) -> AppResult<()> {
        let PreparedSharedVulkanMutationResolutionFence {
            id,
            scope,
            game_id,
            catalog_binding,
        } = fence;
        self.with_transaction(|transaction| {
            let row = read_shared_row(transaction)?.ok_or_else(|| {
                AppError::storage_failed(format!(
                    "prepared shared Vulkan mutation '{}' changed before recovery completion",
                    id
                ))
            })?;
            ensure_exact_owner(&row, &id, scope, game_id.as_ref())?;
            if row.state != PendingSharedVulkanMutationState::Prepared {
                return Err(AppError::storage_failed(format!(
                    "shared Vulkan mutation '{}' changed before recovery completion",
                    id
                )));
            }
            validate_catalog_binding(transaction, &id, game_id.as_ref(), catalog_binding)?;
            let deleted = transaction
                .execute(
                    "DELETE FROM pending_shared_vulkan_mutations
                     WHERE resource_key = ?1 AND id = ?2 AND scope = ?3 AND state = 'prepared'
                       AND ((game_id IS NULL AND ?4 IS NULL) OR game_id = ?4)",
                    params![
                        RESOURCE_KEY,
                        id,
                        scope.as_str(),
                        game_id.as_ref().map(GameId::as_str)
                    ],
                )
                .map_err(storage_error)?;
            if deleted != 1 {
                return Err(AppError::storage_failed(format!(
                    "shared Vulkan mutation '{}' changed before recovery completion",
                    id
                )));
            }
            Ok(())
        })
    }
}
