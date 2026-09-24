use renderpilot_application::AppResult;
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;

use super::pending::{ensure_no_overlapping_nvapi_operations, require_pending_operation};
use super::{
    NvapiGameSettingPreparation, NvapiGlobalSettingPreparation, NvapiSettingOperationCompletion,
    NvapiSettingOperationScope, NvapiSettingState, NvapiTargetSettingClaimRow,
};
use crate::{
    SqliteStorage,
    error::{storage_context, storage_error},
};
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SettingBeforeReceipt {
    Game(GameSettingBeforeReceipt),
    Global(GlobalSettingBeforeReceipt),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GameSettingBeforeReceipt {
    setting_id: u32,
    present: bool,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    value: Option<u32>,
    claim_was_new: bool,
    reference_was_new: bool,
    executable_path: String,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    reference_previous_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GlobalSettingBeforeReceipt {
    setting_id: u32,
    present: bool,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    value: Option<u32>,
    claim_was_new: bool,
    reference_was_new: bool,
}

fn deserialize_required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

impl SettingBeforeReceipt {
    fn validate(&self, has_game_id: bool) -> Result<(), &'static str> {
        match self {
            Self::Game(receipt) => {
                if !has_game_id {
                    return Err("game setting receipt has no game scope");
                }
                if receipt.executable_path.trim().is_empty() {
                    return Err("game setting receipt has an empty executable path");
                }
                if receipt.present != receipt.value.is_some() {
                    return Err("game setting receipt has inconsistent presence and value");
                }
                if receipt.reference_was_new == receipt.reference_previous_path.is_some() {
                    return Err("game setting receipt has inconsistent previous reference");
                }
            }
            Self::Global(receipt) => {
                if has_game_id {
                    return Err("global setting receipt has a game scope");
                }
                if receipt.reference_was_new {
                    return Err("global setting receipt cannot create a game reference");
                }
                if receipt.present != receipt.value.is_some() {
                    return Err("global setting receipt has inconsistent presence and value");
                }
            }
        }
        Ok(())
    }
}

// -----------------------------------------------------------------------------
impl SqliteStorage {
    /// Counts active game participants for one shared profile setting claim.
    pub fn count_nvapi_setting_claim_refs(
        &self,
        target_id: &str,
        setting_id: u32,
    ) -> AppResult<u32> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT count(*) FROM nvapi_game_claim_refs
                  WHERE target_id = ?1 AND setting_id = ?2",
                    params![target_id, setting_id],
                    |row| row.get(0),
                )
                .map_err(storage_error)
        })
    }

    /// Checks whether another game participates in any setting on an owned profile.
    pub fn nvapi_target_has_other_game_claims(
        &self,
        target_id: &str,
        game_id: &str,
    ) -> AppResult<bool> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT EXISTS(
                    SELECT 1 FROM nvapi_game_claim_refs
                     WHERE target_id = ?1 AND game_id != ?2
                )",
                    params![target_id, game_id],
                    |row| row.get(0),
                )
                .map_err(storage_error)
        })
    }
}

impl SqliteStorage {
    /// Returns every active DRS claim held by one game.
    pub fn list_nvapi_setting_claims_for_game(
        &self,
        game_id: &str,
    ) -> AppResult<Vec<NvapiTargetSettingClaimRow>> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT r.game_id, c.target_id, c.setting_id, r.executable_path,
                        c.original_present, c.original_value,
                        c.expected_present, c.expected_value
                   FROM nvapi_game_claim_refs r
                   JOIN nvapi_target_setting_claims c
                     ON c.target_id = r.target_id AND c.setting_id = r.setting_id
                  WHERE r.game_id = ?1 ORDER BY c.target_id, c.setting_id",
                )
                .map_err(|error| storage_context("could not prepare NVAPI claim list", error))?;
            let rows = statement
                .query_map([game_id], |row| {
                    Ok(NvapiTargetSettingClaimRow {
                        game_id: row.get(0)?,
                        target_id: row.get(1)?,
                        setting_id: row.get(2)?,
                        executable_path: row.get(3)?,
                        original_present: row.get::<_, i32>(4)? != 0,
                        original_value: row.get(5)?,
                        expected_present: row.get::<_, i32>(6)? != 0,
                        expected_value: row.get(7)?,
                    })
                })
                .map_err(|error| storage_context("could not read NVAPI claims", error))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| storage_context("could not map NVAPI claims", error))
        })
    }
}

impl SqliteStorage {
    /// Cancels an intent that has been observed in its exact pre-mutation state.
    /// Newly created claims and references are removed with the journal row.
    pub fn cancel_nvapi_setting_operation(&self, op_id: &str) -> AppResult<()> {
        self.with_transaction(|transaction| {
            let before: Option<(Option<String>, Option<String>, String)> = transaction
                .query_row(
                    "SELECT game_id, target_id, before_json
                   FROM pending_drs_operations WHERE op_id = ?1 AND kind = 'setting'",
                    [op_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
                .map_err(storage_error)?;
            let Some((game_id, target_id, before_json)) = before else {
                return Err(renderpilot_application::AppError::storage_failed(
                    "pending DRS setting receipt was not found",
                ));
            };
            let before: SettingBeforeReceipt = serde_json::from_str(&before_json)
                .map_err(|error| storage_context("invalid DRS before-state receipt", error))?;
            before
                .validate(game_id.is_some())
                .map_err(renderpilot_application::AppError::storage_failed)?;
            let (setting_id, claim_was_new) = match &before {
                SettingBeforeReceipt::Game(receipt) => (receipt.setting_id, receipt.claim_was_new),
                SettingBeforeReceipt::Global(receipt) => {
                    (receipt.setting_id, receipt.claim_was_new)
                }
            };
            let target_id = target_id.as_deref().ok_or_else(|| {
                renderpilot_application::AppError::storage_failed(
                    "setting receipt has no NVIDIA target scope",
                )
            })?;
            match (&before, game_id.as_deref()) {
                (SettingBeforeReceipt::Game(receipt), Some(game_id)) => {
                    if receipt.reference_was_new {
                        transaction
                            .execute(
                                "DELETE FROM nvapi_game_claim_refs
                              WHERE game_id = ?1 AND target_id = ?2 AND setting_id = ?3",
                                params![game_id, target_id, setting_id],
                            )
                            .map_err(storage_error)?;
                    } else if let Some(previous_path) = receipt.reference_previous_path.as_deref() {
                        transaction
                            .execute(
                                "UPDATE nvapi_game_claim_refs SET executable_path = ?4
                              WHERE game_id = ?1 AND target_id = ?2 AND setting_id = ?3",
                                params![game_id, target_id, setting_id, previous_path],
                            )
                            .map_err(storage_error)?;
                    }
                }
                (SettingBeforeReceipt::Global(_), None) => {}
                _ => unreachable!("scope was validated with the setting receipt"),
            }
            if claim_was_new {
                transaction
                    .execute(
                        "DELETE FROM nvapi_target_setting_claims
                      WHERE target_id = ?1 AND setting_id = ?2
                        AND NOT EXISTS (
                            SELECT 1 FROM nvapi_game_claim_refs
                             WHERE target_id = ?1 AND setting_id = ?2
                        )",
                        params![target_id, setting_id],
                    )
                    .map_err(storage_error)?;
            }
            if let SettingBeforeReceipt::Game(receipt) = &before {
                transaction
                    .execute(
                        "DELETE FROM nvapi_drs_target_app_witnesses
                      WHERE target_id = ?1 AND lower(executable_path) = lower(?2)
                        AND NOT EXISTS (
                            SELECT 1 FROM nvapi_game_claim_refs
                             WHERE target_id = ?1 AND lower(executable_path) = lower(?2)
                        )",
                        params![target_id, receipt.executable_path],
                    )
                    .map_err(storage_error)?;
            }
            transaction
                .execute(
                    "DELETE FROM nvapi_drs_targets
                  WHERE target_id = ?1 AND kind = 'external'
                    AND NOT EXISTS (SELECT 1 FROM nvapi_target_setting_claims WHERE target_id = ?1)
                    AND NOT EXISTS (SELECT 1 FROM nvapi_owned_profiles WHERE profile_name = ?1)",
                    [target_id],
                )
                .map_err(storage_error)?;
            transaction
                .execute(
                    "DELETE FROM pending_drs_operations WHERE op_id = ?1",
                    [op_id],
                )
                .map_err(storage_error)?;
            Ok(())
        })
    }
}

impl SqliteStorage {
    /// Atomically records a target witness, shared original setting state,
    /// game participation, and pre-mutation intent before calling NVAPI.
    pub fn prepare_nvapi_setting_operation(
        &self,
        preparation: NvapiGameSettingPreparation<'_>,
    ) -> AppResult<()> {
        let NvapiGameSettingPreparation {
            op_id,
            game_id,
            profile_name,
            target_kind,
            profile_is_predefined,
            profile_identity_json,
            executable_path,
            application_witness_json,
            setting_id,
            original,
            before,
            after,
        } = preparation;
        let NvapiSettingState {
            present: original_present,
            value: original_value,
        } = original;
        let NvapiSettingState {
            present: before_present,
            value: before_value,
        } = before;
        let NvapiSettingState {
            present: after_present,
            value: after_value,
        } = after;
        let target_id = profile_name;
        let after_json = serde_json::json!({
            "setting_id": setting_id,
            "present": after_present,
            "value": after_value,
        })
        .to_string();
        self.with_transaction(|transaction| {
            ensure_no_overlapping_nvapi_operations(
                transaction,
                Some(target_id),
                &serde_json::json!({"executable_path": executable_path}).to_string(),
                &after_json,
            )?;
            transaction.execute(
                "INSERT INTO nvapi_drs_targets
                    (target_id, profile_name, profile_is_predefined, kind, identity_json)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(profile_name) DO NOTHING",
                params![target_id, profile_name, i32::from(profile_is_predefined), target_kind, profile_identity_json],
            ).map_err(|error| storage_context("could not persist NVIDIA target identity", error))?;
            let identity: (String, i32, String) = transaction.query_row(
                "SELECT target_id, profile_is_predefined, identity_json
                   FROM nvapi_drs_targets WHERE profile_name = ?1",
                [profile_name],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            ).map_err(storage_error)?;
            if identity.0 != target_id || identity.1 != i32::from(profile_is_predefined)
                || identity.2 != profile_identity_json
            {
                return Err(renderpilot_application::AppError::storage_failed(
                    "NVIDIA profile identity changed since its last confirmed receipt",
                ));
            }
            transaction.execute(
                "INSERT INTO nvapi_drs_target_app_witnesses
                    (target_id, executable_path, application_json)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(target_id, executable_path) DO NOTHING",
                params![target_id, executable_path, application_witness_json],
            ).map_err(|error| storage_context("could not persist NVIDIA application witness", error))?;
            let witness: String = transaction.query_row(
                "SELECT application_json FROM nvapi_drs_target_app_witnesses
                  WHERE target_id = ?1 AND executable_path = ?2",
                params![target_id, executable_path],
                |row| row.get(0),
            ).map_err(storage_error)?;
            if witness != application_witness_json {
                return Err(renderpilot_application::AppError::storage_failed(
                    "NVIDIA executable binding witness changed since confirmation",
                ));
            }
            let existing = transaction.query_row(
                "SELECT original_present, original_value, expected_present, expected_value
                   FROM nvapi_target_setting_claims
                  WHERE target_id = ?1 AND setting_id = ?2",
                params![target_id, setting_id],
                |row| Ok((
                    row.get::<_, i32>(0)? != 0,
                    row.get::<_, Option<u32>>(1)?,
                    row.get::<_, i32>(2)? != 0,
                    row.get::<_, Option<u32>>(3)?,
                )),
            ).optional().map_err(storage_error)?;
            let claim_was_new = existing.is_none();
            if let Some((_, _, expected_present, expected_value)) = existing
                && (expected_present != before_present || expected_value != before_value)
            {
                return Err(renderpilot_application::AppError::storage_failed(
                    "NVIDIA setting changed outside RenderPilot; resolve the conflict before writing",
                ));
            }
            if claim_was_new {
                transaction.execute(
                    "INSERT INTO nvapi_target_setting_claims
                        (target_id, setting_id, original_present, original_value,
                         expected_present, expected_value)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        target_id,
                        setting_id,
                        i32::from(original_present),
                        original_value,
                        i32::from(before_present),
                        before_value,
                    ],
                ).map_err(|error| storage_context("could not persist original NVIDIA setting state", error))?;
            }
            let reference_previous_path: Option<String> = transaction.query_row(
                "SELECT executable_path FROM nvapi_game_claim_refs
                  WHERE game_id = ?1 AND target_id = ?2 AND setting_id = ?3",
                params![game_id, target_id, setting_id],
                |row| row.get(0),
            ).optional().map_err(storage_error)?;
            let reference_was_new = reference_previous_path.is_none();
            transaction.execute(
                "INSERT INTO nvapi_game_claim_refs (game_id, target_id, setting_id, executable_path)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(game_id, target_id, setting_id)
                 DO UPDATE SET executable_path = excluded.executable_path",
                params![game_id, target_id, setting_id, executable_path],
            ).map_err(|error| storage_context("could not persist NVIDIA setting participation", error))?;
            let before_json = serde_json::json!({
                "setting_id": setting_id,
                "executable_path": executable_path,
                "reference_previous_path": reference_previous_path,
                "present": before_present,
                "value": before_value,
                "claim_was_new": claim_was_new,
                "reference_was_new": reference_was_new,
            }).to_string();
            transaction.execute(
                "INSERT INTO pending_drs_operations
                    (op_id, game_id, target_id, kind, before_json, after_json, phase)
                 VALUES (?1, ?2, ?3, 'setting', ?4, ?5, 'intent')",
                params![op_id, game_id, target_id, before_json, after_json],
            ).map_err(|error| storage_context("could not persist NVIDIA setting intent", error))?;
            Ok(())
        })
    }

    /// Persists the original state and intent for a global profile setting, scoped to
    /// the exact base-profile identity and setting id.
    pub fn prepare_nvapi_global_setting_operation(
        &self,
        preparation: NvapiGlobalSettingPreparation<'_>,
    ) -> AppResult<()> {
        let NvapiGlobalSettingPreparation {
            op_id,
            profile_name,
            profile_is_predefined,
            profile_identity_json,
            setting_id,
            original,
            before,
            after,
        } = preparation;
        let NvapiSettingState {
            present: original_present,
            value: original_value,
        } = original;
        let NvapiSettingState {
            present: before_present,
            value: before_value,
        } = before;
        let NvapiSettingState {
            present: after_present,
            value: after_value,
        } = after;
        let target_id = profile_name;
        let before_json = serde_json::json!({
            "setting_id": setting_id,
            "present": before_present,
            "value": before_value,
        })
        .to_string();
        let after_json = serde_json::json!({
            "setting_id": setting_id,
            "present": after_present,
            "value": after_value,
        })
        .to_string();
        self.with_transaction(|transaction| {
            ensure_no_overlapping_nvapi_operations(transaction, Some(target_id), &before_json, &after_json)?;
            transaction.execute(
                "INSERT INTO nvapi_drs_targets
                    (target_id, profile_name, profile_is_predefined, kind, identity_json)
                 VALUES (?1, ?1, ?2, 'external', ?3)
                 ON CONFLICT(profile_name) DO NOTHING",
                params![target_id, i32::from(profile_is_predefined), profile_identity_json],
            ).map_err(storage_error)?;
            let identity: (String, i32, String) = transaction.query_row(
                "SELECT target_id, profile_is_predefined, identity_json
                   FROM nvapi_drs_targets WHERE profile_name = ?1",
                [profile_name],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            ).map_err(storage_error)?;
            if identity.0 != target_id
                || identity.1 != i32::from(profile_is_predefined)
                || identity.2 != profile_identity_json
            {
                return Err(renderpilot_application::AppError::storage_failed(
                    "NVIDIA global profile identity changed since its last verified state",
                ));
            }
            let existing = transaction.query_row(
                "SELECT original_present, original_value, expected_present, expected_value
                   FROM nvapi_target_setting_claims
                  WHERE target_id = ?1 AND setting_id = ?2",
                params![target_id, setting_id],
                |row| Ok((
                    row.get::<_, i32>(0)? != 0,
                    row.get::<_, Option<u32>>(1)?,
                    row.get::<_, i32>(2)? != 0,
                    row.get::<_, Option<u32>>(3)?,
                )),
            ).optional().map_err(storage_error)?;
            let claim_was_new = existing.is_none();
            if let Some((_, _, expected_present, expected_value)) = existing
                && (expected_present != before_present || expected_value != before_value)
            {
                return Err(renderpilot_application::AppError::storage_failed(
                    "global NVIDIA setting changed outside RenderPilot; resolve the conflict before writing",
                ));
            }
            if claim_was_new {
                transaction.execute(
                    "INSERT INTO nvapi_target_setting_claims
                        (target_id, setting_id, original_present, original_value,
                         expected_present, expected_value)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![target_id, setting_id, i32::from(original_present), original_value,
                        i32::from(before_present), before_value],
                ).map_err(storage_error)?;
            }
            let before_json = serde_json::json!({
                "setting_id": setting_id,
                "present": before_present,
                "value": before_value,
                "claim_was_new": claim_was_new,
                "reference_was_new": false,
            }).to_string();
            transaction.execute(
                "INSERT INTO pending_drs_operations
                    (op_id, game_id, target_id, kind, before_json, after_json, phase)
                 VALUES (?1, NULL, ?2, 'setting', ?3, ?4, 'intent')",
                params![op_id, target_id, before_json, after_json],
            ).map_err(storage_error)?;
            Ok(())
        })
    }

    /// Publishes a setting's verified expected state and receipt after reload
    /// confirmed the requested driver state.
    pub fn finish_nvapi_setting_operation(
        &self,
        completion: NvapiSettingOperationCompletion<'_>,
    ) -> AppResult<()> {
        let NvapiSettingOperationCompletion {
            op_id,
            scope,
            target_id,
            setting_id,
            expected,
            profile_identity_json,
            composition_json,
        } = completion;
        let game_id = match scope {
            NvapiSettingOperationScope::Game { game_id } => Some(game_id),
            NvapiSettingOperationScope::Global => None,
        };
        let NvapiSettingState {
            present: expected_present,
            value: expected_value,
        } = expected;
        self.with_transaction(|transaction| {
            require_pending_operation(transaction, op_id, "setting", game_id, Some(target_id))?;
            transaction
                .execute(
                    "UPDATE nvapi_target_setting_claims
                    SET expected_present = ?3, expected_value = ?4,
                        updated_at = CAST(unixepoch('subsec') * 1000 AS INTEGER)
                  WHERE target_id = ?1 AND setting_id = ?2",
                    params![
                        target_id,
                        setting_id,
                        i32::from(expected_present),
                        expected_value
                    ],
                )
                .map_err(storage_error)?;
            transaction
                .execute(
                    "UPDATE nvapi_drs_targets SET identity_json = ?2
                  WHERE target_id = ?1",
                    params![target_id, profile_identity_json],
                )
                .map_err(storage_error)?;
            if let Some(composition) = composition_json {
                transaction
                    .execute(
                        "UPDATE nvapi_owned_profiles
                        SET composition_json = ?2,
                            updated_at = CAST(unixepoch('subsec') * 1000 AS INTEGER)
                      WHERE profile_name = ?1",
                        params![target_id, composition],
                    )
                    .map_err(storage_error)?;
            }
            let removed = transaction
                .execute(
                    "DELETE FROM pending_drs_operations
                  WHERE op_id = ?1 AND kind = 'setting' AND phase != 'conflict'
                    AND game_id IS ?2 AND target_id = ?3",
                    params![op_id, game_id, target_id],
                )
                .map_err(storage_error)?;
            if removed != 1 {
                return Err(renderpilot_application::AppError::storage_failed(
                    "NVIDIA setting intent could not be finalized",
                ));
            }
            Ok(())
        })
    }

    /// Reads a shared setting claim by driver profile identity and setting id.
    pub fn get_nvapi_target_setting_claim(
        &self,
        game_id: &str,
        target_id: &str,
        setting_id: u32,
    ) -> AppResult<Option<NvapiTargetSettingClaimRow>> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT r.game_id, c.target_id, c.setting_id, r.executable_path,
                        c.original_present, c.original_value,
                        c.expected_present, c.expected_value
                   FROM nvapi_game_claim_refs r
                   JOIN nvapi_target_setting_claims c
                     ON c.target_id = r.target_id AND c.setting_id = r.setting_id
                  WHERE r.game_id = ?1 AND r.target_id = ?2 AND r.setting_id = ?3",
                    params![game_id, target_id, setting_id],
                    |row| {
                        Ok(NvapiTargetSettingClaimRow {
                            game_id: row.get(0)?,
                            target_id: row.get(1)?,
                            setting_id: row.get(2)?,
                            executable_path: row.get(3)?,
                            original_present: row.get::<_, i32>(4)? != 0,
                            original_value: row.get(5)?,
                            expected_present: row.get::<_, i32>(6)? != 0,
                            expected_value: row.get(7)?,
                        })
                    },
                )
                .optional()
                .map_err(storage_error)
        })
    }

    /// Reads the owned original state for one global profile setting.
    pub fn get_nvapi_global_setting_claim(
        &self,
        target_id: &str,
        setting_id: u32,
    ) -> AppResult<Option<(bool, Option<u32>)>> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT original_present, original_value
                   FROM nvapi_target_setting_claims
                  WHERE target_id = ?1 AND setting_id = ?2",
                    params![target_id, setting_id],
                    |row| Ok((row.get::<_, i32>(0)? != 0, row.get(1)?)),
                )
                .optional()
                .map_err(storage_error)
        })
    }

    /// Releases one game's reference after its last expected driver value was
    /// restored. Shared claims remain until the final participant releases.
    pub fn release_nvapi_setting_claim(
        &self,
        game_id: &str,
        target_id: &str,
        setting_id: u32,
    ) -> AppResult<()> {
        self.with_transaction(|transaction| {
            transaction
                .execute(
                    "DELETE FROM nvapi_game_claim_refs
                  WHERE game_id = ?1 AND target_id = ?2 AND setting_id = ?3",
                    params![game_id, target_id, setting_id],
                )
                .map_err(storage_error)?;
            transaction
                .execute(
                    "DELETE FROM nvapi_target_setting_claims
                  WHERE target_id = ?1 AND setting_id = ?2
                    AND NOT EXISTS (
                        SELECT 1 FROM nvapi_game_claim_refs
                         WHERE target_id = ?1 AND setting_id = ?2
                    )",
                    params![target_id, setting_id],
                )
                .map_err(storage_error)?;
            transaction
                .execute(
                    "DELETE FROM nvapi_drs_target_app_witnesses
                  WHERE target_id = ?1
                    AND NOT EXISTS (SELECT 1 FROM nvapi_target_setting_claims WHERE target_id = ?1)
                    AND NOT EXISTS (SELECT 1 FROM nvapi_owned_profiles WHERE profile_name = ?1)",
                    [target_id],
                )
                .map_err(storage_error)?;
            transaction
                .execute(
                    "DELETE FROM nvapi_drs_targets
                  WHERE target_id = ?1 AND kind = 'external'
                    AND NOT EXISTS (SELECT 1 FROM nvapi_target_setting_claims WHERE target_id = ?1)
                    AND NOT EXISTS (SELECT 1 FROM nvapi_owned_profiles WHERE profile_name = ?1)",
                    [target_id],
                )
                .map_err(storage_error)?;
            Ok(())
        })
    }
}
