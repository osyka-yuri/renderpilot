//! Input, owner, commit, and cross-fence validation for the singleton row.

use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::{AddonKind, GameId, InstalledAddon, InstalledAddonHostKind};
use rusqlite::{OptionalExtension, Transaction};

use crate::{error::storage_error, repositories::observations};

use super::super::game_mutations::InstalledAddonMutation;
use super::RESOURCE_KEY;
use super::model::{
    BeginSharedVulkanMutation, PendingSharedVulkanMutationRow, PendingSharedVulkanMutationState,
    SharedCatalogBinding, SharedVulkanMutationCommit, SharedVulkanMutationScope,
};
use super::queries::read_shared_row;

pub(crate) fn validate_prepared_shared_vulkan_mutation_commit_within_transaction(
    transaction: &Transaction<'_>,
    commit: &SharedVulkanMutationCommit<'_>,
) -> AppResult<PendingSharedVulkanMutationRow> {
    validate_owner(commit.scope, commit.game_id)?;
    let row = read_shared_row(transaction)?.ok_or_else(|| {
        AppError::storage_failed(format!(
            "shared Vulkan mutation '{}' is missing or is not prepared",
            commit.id
        ))
    })?;
    ensure_exact_owner(&row, commit.id, commit.scope, commit.game_id)?;
    if row.state != PendingSharedVulkanMutationState::Prepared {
        return Err(AppError::storage_failed(format!(
            "shared Vulkan mutation '{}' is missing or is not prepared",
            commit.id
        )));
    }

    if let Some(game_id) = commit.game_id {
        let catalog_exists = observations::catalog_exists_within_transaction(transaction, game_id)?;
        if matches!(commit.addon, InstalledAddonMutation::ReplaceExpected { .. }) && !catalog_exists
        {
            return Err(AppError::storage_failed(
                "expected-owner shared Vulkan replacement requires an existing catalog game",
            ));
        }
        if catalog_exists {
            match observations::readiness_within_transaction(transaction, game_id)? {
                crate::repositories::observations::CatalogReadiness::Invalidated {
                    mutation_token: Some(token),
                    ..
                } if token == commit.id => {}
                _ => {
                    return Err(AppError::storage_failed(format!(
                        "shared Vulkan mutation '{}' has no matching invalidated catalog authority",
                        commit.id
                    )));
                }
            }
        } else if matches!(commit.addon, InstalledAddonMutation::Keep) {
            return Err(AppError::storage_failed(format!(
                "pre-catalog shared Vulkan mutation '{}' may commit only an add-on lifecycle effect",
                commit.id
            )));
        }
        match commit.addon {
            InstalledAddonMutation::Upsert(addon) if addon.game_id() != game_id => {
                return Err(AppError::storage_failed(format!(
                    "shared Vulkan mutation '{}' add-on owner does not match game {}",
                    commit.id,
                    game_id.as_str()
                )));
            }
            InstalledAddonMutation::Upsert(addon) => {
                super::super::installed_addons::ensure_independent_peer_mutation_allowed(
                    transaction,
                    game_id,
                    addon.kind(),
                )?;
            }
            InstalledAddonMutation::Delete(kind) => {
                super::super::installed_addons::ensure_independent_peer_mutation_allowed(
                    transaction,
                    game_id,
                    kind,
                )?;
            }
            InstalledAddonMutation::ReplaceExpected {
                expected,
                replacement,
            } => {
                validate_expected_shared_vulkan_replacement(
                    transaction,
                    game_id,
                    expected,
                    replacement,
                )?;
            }
            InstalledAddonMutation::Keep => {}
            InstalledAddonMutation::OptiScaler(_) => {
                return Err(AppError::invalid_input(
                    "shared Vulkan mutations cannot commit an OptiScaler aggregate",
                ));
            }
        }
    } else if !matches!(commit.addon, InstalledAddonMutation::Keep) {
        return Err(AppError::invalid_input(
            "shared-only Vulkan mutations cannot change a game add-on",
        ));
    }
    Ok(row)
}

fn validate_expected_shared_vulkan_replacement(
    transaction: &Transaction<'_>,
    game_id: &GameId,
    expected: &InstalledAddon,
    replacement: &InstalledAddon,
) -> AppResult<()> {
    if expected.game_id() != game_id || replacement.game_id() != game_id {
        return Err(AppError::invalid_input(
            "expected shared Vulkan owner and replacement must match the reserved game",
        ));
    }
    if !matches!(expected.kind(), AddonKind::RenoDx | AddonKind::Luma)
        || expected.host_kind() != Some(InstalledAddonHostKind::Proxy)
        || replacement.kind() != AddonKind::RenoDx
        || replacement.host_kind() != Some(InstalledAddonHostKind::SharedVulkanLayer)
    {
        return Err(AppError::invalid_input(
            "shared Vulkan expected-owner replacement requires a RenoDX/Luma Proxy owner and a RenoDX SharedVulkanLayer replacement",
        ));
    }

    super::super::installed_addons::ensure_independent_peer_mutation_allowed(
        transaction,
        game_id,
        expected.kind(),
    )?;
    super::super::installed_addons::ensure_independent_peer_mutation_allowed(
        transaction,
        game_id,
        replacement.kind(),
    )?;
    Ok(())
}

pub(crate) fn assert_no_shared_mutation_for_game_within_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
) -> AppResult<()> {
    let pending: Option<String> = transaction
        .query_row(
            "SELECT id FROM pending_shared_vulkan_mutations
             WHERE resource_key = ?1 AND scope = 'game_shared' AND game_id = ?2",
            [RESOURCE_KEY, game_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    if let Some(id) = pending {
        return Err(storage_error(format!(
            "game {} has pending shared Vulkan mutation '{id}'; file mutation is blocked",
            game_id.as_str()
        )));
    }
    Ok(())
}

pub(crate) fn assert_no_shared_mutation_id_within_transaction(
    transaction: &Transaction<'_>,
    mutation_id: &str,
) -> AppResult<()> {
    let pending: Option<String> = transaction
        .query_row(
            "SELECT id FROM pending_shared_vulkan_mutations
             WHERE resource_key = ?1 AND id = ?2",
            [RESOURCE_KEY, mutation_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    if pending.is_some() {
        return Err(storage_error(format!(
            "mutation id '{mutation_id}' is already used by a shared Vulkan mutation"
        )));
    }
    Ok(())
}

pub(super) fn assert_no_pending_file_mutation_id_within_transaction(
    transaction: &Transaction<'_>,
    mutation_id: &str,
) -> AppResult<()> {
    let pending: Option<String> = transaction
        .query_row(
            "SELECT id FROM pending_file_mutations WHERE id = ?1",
            [mutation_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    if pending.is_some() {
        return Err(AppError::storage_failed(format!(
            "shared Vulkan mutation id '{mutation_id}' is already used by a file mutation"
        )));
    }
    Ok(())
}

pub(super) fn assert_no_pending_file_mutation_within_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
) -> AppResult<()> {
    let pending: Option<String> = transaction
        .query_row(
            "SELECT id FROM pending_file_mutations WHERE game_id = ?1 LIMIT 1",
            [game_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    if let Some(id) = pending {
        return Err(storage_error(format!(
            "game {} has pending file mutation '{id}'; shared Vulkan mutation is blocked",
            game_id.as_str()
        )));
    }
    Ok(())
}

pub(super) fn ensure_exact_owner(
    row: &PendingSharedVulkanMutationRow,
    id: &str,
    scope: SharedVulkanMutationScope,
    game_id: Option<&GameId>,
) -> AppResult<()> {
    if row.id != id || row.scope != scope || row.game_id.as_ref() != game_id {
        return Err(AppError::storage_failed(format!(
            "shared Vulkan mutation '{id}' owner or scope does not match the reservation"
        )));
    }
    Ok(())
}

pub(super) fn validate_owner(
    scope: SharedVulkanMutationScope,
    game_id: Option<&GameId>,
) -> AppResult<()> {
    match (scope, game_id) {
        (SharedVulkanMutationScope::SharedOnly, None)
        | (SharedVulkanMutationScope::GameShared, Some(_)) => Ok(()),
        (SharedVulkanMutationScope::SharedOnly, Some(_)) => Err(AppError::invalid_input(
            "shared-only Vulkan mutations cannot have a game owner",
        )),
        (SharedVulkanMutationScope::GameShared, None) => Err(AppError::invalid_input(
            "game-shared Vulkan mutations require a game owner",
        )),
    }
}

pub(super) fn validate_begin(begin: &BeginSharedVulkanMutation) -> AppResult<()> {
    validate_owner(begin.scope, begin.game_id.as_ref())?;
    for (field, value) in [
        ("id", begin.id.as_str()),
        ("feature", begin.feature.as_str()),
    ] {
        if value.trim().is_empty() || value.contains('\0') {
            return Err(AppError::invalid_input(format!(
                "shared Vulkan mutation {field} is invalid"
            )));
        }
    }
    validate_manifest_object(
        &begin.initial_manifest_json,
        "initial shared Vulkan mutation manifest",
    )?;
    validate_manifest_object(
        &begin.root_capabilities_json,
        "shared Vulkan mutation root capabilities",
    )?;
    Ok(())
}

pub(super) fn validate_manifest(manifest_json: &str) -> AppResult<()> {
    validate_manifest_object(manifest_json, "shared Vulkan mutation manifest")?;
    Ok(())
}

fn validate_manifest_object(
    manifest_json: &str,
    context: &str,
) -> AppResult<serde_json::Map<String, serde_json::Value>> {
    let manifest: serde_json::Value = serde_json::from_str(manifest_json)
        .map_err(|error| AppError::storage_failed(format!("invalid {context}: {error}")))?;
    manifest
        .as_object()
        .cloned()
        .ok_or_else(|| AppError::storage_failed(format!("{context} must be a JSON object")))
}

pub(super) fn delete_shared_state(
    transaction: &Transaction<'_>,
    id: &str,
    state: PendingSharedVulkanMutationState,
) -> AppResult<()> {
    let deleted = transaction
        .execute(
            "DELETE FROM pending_shared_vulkan_mutations
             WHERE resource_key = ?1 AND id = ?2 AND state = ?3",
            [RESOURCE_KEY, id, state.as_str()],
        )
        .map_err(storage_error)?;
    if deleted != 1 {
        return Err(AppError::storage_failed(format!(
            "shared Vulkan mutation '{id}' is missing or is not {}",
            state.as_str()
        )));
    }
    Ok(())
}

pub(super) fn validate_catalog_binding(
    transaction: &Transaction<'_>,
    id: &str,
    game_id: Option<&GameId>,
    catalog_binding: SharedCatalogBinding,
) -> AppResult<()> {
    match (game_id, catalog_binding) {
        (None, SharedCatalogBinding::Absent) => Ok(()),
        (Some(game_id), SharedCatalogBinding::Absent) => {
            if observations::catalog_exists_within_transaction(transaction, game_id)? {
                Err(AppError::storage_failed(format!(
                    "shared Vulkan mutation '{id}' catalog binding changed before recovery completion"
                )))
            } else {
                Ok(())
            }
        }
        (Some(game_id), SharedCatalogBinding::Invalidated { authority_epoch }) => {
            match observations::readiness_within_transaction(transaction, game_id)? {
                crate::repositories::observations::CatalogReadiness::Invalidated {
                    authority_epoch: current_epoch,
                    mutation_token: Some(token),
                    ..
                } if current_epoch == authority_epoch && token == id => Ok(()),
                _ => Err(AppError::storage_failed(format!(
                    "shared Vulkan mutation '{id}' catalog authority changed before recovery completion"
                ))),
            }
        }
        _ => Err(AppError::storage_failed(format!(
            "shared Vulkan mutation '{id}' catalog binding changed before recovery completion"
        ))),
    }
}
