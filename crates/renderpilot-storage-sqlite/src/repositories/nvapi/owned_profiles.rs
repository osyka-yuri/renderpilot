use renderpilot_application::AppResult;
use rusqlite::{OptionalExtension, params};

use super::pending::{delete_pending_nvapi_operation, require_pending_operation};
use super::{
    NvapiOwnedProfileRow, NvapiProfileCreationCompletion, NvapiProfileMoveCompletion,
    NvapiVerifiedProfileReceipt,
};
use crate::{
    SqliteStorage,
    error::{storage_context, storage_error},
};
impl SqliteStorage {
    /// Returns RenderPilot's owned profile record for one game, if present.
    pub fn get_nvapi_owned_profile(
        &self,
        game_id: &str,
    ) -> AppResult<Option<NvapiOwnedProfileRow>> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT game_id, profile_name, binding_path, application_witness_json,
                        composition_json, state, updated_at
                   FROM nvapi_owned_profiles WHERE game_id = ?1",
                    [game_id],
                    |row| {
                        Ok(NvapiOwnedProfileRow {
                            game_id: row.get(0)?,
                            profile_name: row.get(1)?,
                            binding_path: row.get(2)?,
                            application_witness_json: row.get(3)?,
                            composition_json: row.get(4)?,
                            state: row.get(5)?,
                            updated_at: row.get(6)?,
                        })
                    },
                )
                .optional()
                .map_err(|error| storage_context("could not read owned NVIDIA profile", error))
        })
    }

    /// Looks up an owned profile by its normalized executable binding path.
    pub fn get_nvapi_owned_profile_by_binding_path(
        &self,
        binding_path: &str,
    ) -> AppResult<Option<NvapiOwnedProfileRow>> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT game_id, profile_name, binding_path, application_witness_json,
                            composition_json, state, updated_at
                       FROM nvapi_owned_profiles WHERE lower(binding_path) = lower(?1)",
                    [binding_path.replace('\\', "/")],
                    |row| {
                        Ok(NvapiOwnedProfileRow {
                            game_id: row.get(0)?,
                            profile_name: row.get(1)?,
                            binding_path: row.get(2)?,
                            application_witness_json: row.get(3)?,
                            composition_json: row.get(4)?,
                            state: row.get(5)?,
                            updated_at: row.get(6)?,
                        })
                    },
                )
                .optional()
                .map_err(storage_error)
        })
    }

    /// Looks up a RenderPilot-owned profile by its exact DRS name.
    pub fn get_nvapi_owned_profile_by_name(
        &self,
        profile_name: &str,
    ) -> AppResult<Option<NvapiOwnedProfileRow>> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT game_id, profile_name, binding_path, application_witness_json,
                        composition_json, state, updated_at
                   FROM nvapi_owned_profiles WHERE profile_name = ?1",
                    [profile_name],
                    |row| {
                        Ok(NvapiOwnedProfileRow {
                            game_id: row.get(0)?,
                            profile_name: row.get(1)?,
                            binding_path: row.get(2)?,
                            application_witness_json: row.get(3)?,
                            composition_json: row.get(4)?,
                            state: row.get(5)?,
                            updated_at: row.get(6)?,
                        })
                    },
                )
                .optional()
                .map_err(storage_error)
        })
    }

    /// Reports catalog rows that prevent auto-pruning or deletion of this game.
    pub fn game_has_nvapi_durable_state(&self, game_id: &str) -> AppResult<bool> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT EXISTS(
                    SELECT 1 FROM nvapi_owned_profiles WHERE game_id = ?1
                    UNION ALL SELECT 1 FROM nvapi_game_claim_refs WHERE game_id = ?1
                    UNION ALL SELECT 1 FROM pending_drs_operations WHERE game_id = ?1
                )",
                    [game_id],
                    |row| row.get(0),
                )
                .map_err(storage_error)
        })
    }

    /// Reads the last verified NVDRS_APPLICATION witness for one target/path.
    pub fn get_nvapi_application_witness(
        &self,
        target_id: &str,
        executable_path: &str,
    ) -> AppResult<Option<String>> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT application_json FROM nvapi_drs_target_app_witnesses
                  WHERE target_id = ?1 AND lower(executable_path) = lower(?2)",
                    params![target_id, executable_path.replace('\\', "/")],
                    |row| row.get(0),
                )
                .optional()
                .map_err(storage_error)
        })
    }

    /// Stores a fully verified RenderPilot profile and removes its create
    /// intent in the same catalog transaction.
    pub fn complete_nvapi_profile_creation(
        &self,
        completion: NvapiProfileCreationCompletion<'_>,
    ) -> AppResult<()> {
        let NvapiProfileCreationCompletion {
            op_id,
            game_id,
            binding_path,
            profile:
                NvapiVerifiedProfileReceipt {
                    profile_name,
                    profile_identity_json,
                    application_witness_json,
                    composition_json,
                },
        } = completion;
        self.with_transaction(|transaction| {
            require_pending_operation(transaction, op_id, "create_profile", Some(game_id), Some(profile_name))?;
            transaction.execute(
                "INSERT INTO nvapi_drs_targets
                    (target_id, profile_name, profile_is_predefined, kind, identity_json)
                 VALUES (?1, ?1, 0, 'renderpilot', ?2)",
                params![profile_name, profile_identity_json],
            ).map_err(|error| storage_context("could not finalize owned NVIDIA target", error))?;
            transaction.execute(
                "INSERT INTO nvapi_drs_target_app_witnesses
                    (target_id, executable_path, application_json)
                 VALUES (?1, ?2, ?3)",
                params![profile_name, binding_path, application_witness_json],
            ).map_err(|error| storage_context("could not finalize NVIDIA application witness", error))?;
            transaction.execute(
                "INSERT INTO nvapi_owned_profiles
                    (game_id, profile_name, binding_path, application_witness_json, composition_json)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![game_id, profile_name, binding_path, application_witness_json, composition_json],
            ).map_err(|error| storage_context("could not finalize owned NVIDIA profile", error))?;
            delete_pending_nvapi_operation(transaction, op_id, "create_profile", Some(game_id), Some(profile_name))?;
            Ok(())
        })
    }

    /// Publishes an explicitly confirmed executable move and the shared RenoDX
    /// executable override atomically after DRS has been saved and verified.
    pub fn complete_nvapi_profile_move(
        &self,
        completion: NvapiProfileMoveCompletion<'_>,
    ) -> AppResult<()> {
        let NvapiProfileMoveCompletion {
            op_id,
            game_id,
            binding_path,
            binding_basename,
            select_automatically,
            profile:
                NvapiVerifiedProfileReceipt {
                    profile_name,
                    profile_identity_json,
                    application_witness_json,
                    composition_json,
                },
        } = completion;
        self.with_transaction(|transaction| {
            require_pending_operation(
                transaction,
                op_id,
                "move_profile",
                Some(game_id),
                Some(profile_name),
            )?;
            transaction
                .execute(
                    "UPDATE nvapi_owned_profiles
                    SET binding_path = ?3, application_witness_json = ?4,
                        composition_json = ?5, state = 'active',
                        updated_at = CAST(unixepoch('subsec') * 1000 AS INTEGER)
                  WHERE game_id = ?1 AND profile_name = ?2",
                    params![
                        game_id,
                        profile_name,
                        binding_path,
                        application_witness_json,
                        composition_json
                    ],
                )
                .map_err(storage_error)?;
            transaction
                .execute(
                    "DELETE FROM nvapi_drs_target_app_witnesses WHERE target_id = ?1",
                    [profile_name],
                )
                .map_err(storage_error)?;
            transaction
                .execute(
                    "UPDATE nvapi_game_claim_refs SET executable_path = ?3
                  WHERE game_id = ?1 AND target_id = ?2",
                    params![game_id, profile_name, binding_path],
                )
                .map_err(storage_error)?;
            transaction
                .execute(
                    "INSERT INTO nvapi_drs_target_app_witnesses
                    (target_id, executable_path, application_json)
                 VALUES (?1, ?2, ?3)",
                    params![profile_name, binding_path, application_witness_json],
                )
                .map_err(storage_error)?;
            transaction
                .execute(
                    "UPDATE nvapi_drs_targets SET identity_json = ?2
                  WHERE target_id = ?1 AND kind = 'renderpilot'",
                    params![profile_name, profile_identity_json],
                )
                .map_err(storage_error)?;
            if select_automatically {
                transaction
                    .execute(
                        "DELETE FROM nvapi_executable_overrides WHERE game_id = ?1",
                        [game_id],
                    )
                    .map_err(storage_error)?;
            } else {
                transaction
                    .execute(
                        "INSERT INTO nvapi_executable_overrides
                        (game_id, selected_path, selected_basename, updated_at)
                     VALUES (?1, ?2, ?3, CAST(unixepoch('subsec') * 1000 AS INTEGER))
                     ON CONFLICT(game_id) DO UPDATE SET
                        selected_path = excluded.selected_path,
                        selected_basename = excluded.selected_basename,
                        updated_at = excluded.updated_at",
                        params![game_id, binding_path, binding_basename],
                    )
                    .map_err(storage_error)?;
            }
            delete_pending_nvapi_operation(
                transaction,
                op_id,
                "move_profile",
                Some(game_id),
                Some(profile_name),
            )?;
            Ok(())
        })
    }

    /// Removes the durable owner receipt after the owned DRS profile has been
    /// deleted and verified absent.
    pub fn complete_nvapi_profile_deletion(
        &self,
        op_id: &str,
        game_id: &str,
        profile_name: &str,
    ) -> AppResult<()> {
        self.with_transaction(|transaction| {
            require_pending_operation(
                transaction,
                op_id,
                "delete_profile",
                Some(game_id),
                Some(profile_name),
            )?;
            let outside_claims: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                    SELECT 1 FROM nvapi_game_claim_refs
                     WHERE target_id = ?1 AND game_id != ?2
                )",
                    params![profile_name, game_id],
                    |row| row.get(0),
                )
                .map_err(storage_error)?;
            if outside_claims {
                return Err(renderpilot_application::AppError::storage_failed(
                    "RenderPilot profile has setting claims owned by another game",
                ));
            }
            transaction
                .execute(
                    "DELETE FROM nvapi_game_claim_refs WHERE game_id = ?1 AND target_id = ?2",
                    params![game_id, profile_name],
                )
                .map_err(storage_error)?;
            transaction
                .execute(
                    "DELETE FROM nvapi_target_setting_claims WHERE target_id = ?1",
                    [profile_name],
                )
                .map_err(storage_error)?;
            transaction
                .execute(
                    "DELETE FROM nvapi_owned_profiles WHERE game_id = ?1 AND profile_name = ?2",
                    params![game_id, profile_name],
                )
                .map_err(storage_error)?;
            transaction
                .execute(
                    "DELETE FROM nvapi_drs_target_app_witnesses WHERE target_id = ?1",
                    [profile_name],
                )
                .map_err(storage_error)?;
            transaction
                .execute(
                    "DELETE FROM nvapi_drs_targets WHERE target_id = ?1 AND kind = 'renderpilot'",
                    [profile_name],
                )
                .map_err(storage_error)?;
            delete_pending_nvapi_operation(
                transaction,
                op_id,
                "delete_profile",
                Some(game_id),
                Some(profile_name),
            )?;
            Ok(())
        })
    }
}
