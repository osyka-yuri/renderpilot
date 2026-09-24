//! Write and validate NVAPI setting overrides.

use renderpilot_nvapi::NvapiError;
use renderpilot_nvapi::setting::{CatalogReadiness, NvapiSetting, SettingContext};
use renderpilot_storage_sqlite::{
    NvapiGameSettingPreparation, NvapiGlobalSettingPreparation, NvapiSettingOperationCompletion,
    NvapiSettingOperationScope, NvapiSettingState,
};

use super::assemble::known_preset_set;
use super::live::read_pre_state;
use super::session::{map_nvapi_write_error, open_drs_session, warning_to_service_error};
use super::target::{SettingTarget, WriteOp};
use crate::ServiceError;
use crate::dlss::preset_manifest::{bundled_manifest, supported_presets_for};

/// Resolves a revert request against the recorded original state.
///
/// `target` is `"predefined"` (delete the override, restoring the driver
/// default) or `"original"` (restoring a game's exact recorded original state).
pub fn resolve_revert_op(
    target: &SettingTarget<'_>,
    revert_target: &str,
) -> Result<WriteOp, ServiceError> {
    match revert_target {
        "predefined" => Ok(WriteOp::Delete),
        "original" => {
            if target.game_id().is_none() {
                return Err(ServiceError::command_failed(
                    "original revert is not available for global settings",
                ));
            }
            Ok(WriteOp::RestoreOriginal)
        }
        other => Err(ServiceError::command_failed(format!(
            "invalid revert target `{other}`; expected 'predefined' or 'original'"
        ))),
    }
}

fn restore_original_op(present: bool, value: Option<u32>) -> Result<WriteOp, ServiceError> {
    if present {
        value.map(WriteOp::Set).ok_or_else(|| {
            ServiceError::command_failed("the recorded original NVIDIA setting value is missing")
        })
    } else {
        Ok(WriteOp::Delete)
    }
}

/// Validates that `dword` is an allowed value for `setting` given the current DLL version.
///
/// Returns `Err` only when the preset manifest explicitly manages this value
/// and marks it as unsupported for the detected DLL version.
pub fn validate_value_supported(
    setting: &dyn NvapiSetting,
    dword: u32,
    ctx: &SettingContext,
) -> Result<(), ServiceError> {
    ensure_dll_setting_catalog_ready(setting, ctx)?;
    let Some(kind) = setting.dll_kind() else {
        return Ok(());
    };
    let Some(info) = ctx.dlls.get(&kind) else {
        return Ok(());
    };
    let Some(version) = info.version else {
        return Ok(());
    };

    let supported = supported_presets_for(bundled_manifest(kind), &version);
    if supported.is_empty() {
        return Ok(());
    }
    // The manifest only constrains the presets it explicitly lists. Values it
    // does not manage -- the "recommended" sentinel, or a preset letter beyond
    // its table -- are always allowed.
    if known_preset_set(setting, ctx).contains(&dword) && !supported.contains(&dword) {
        return Err(ServiceError::command_failed(format!(
            "value `{}` is not supported for DLL version {} (kind={:?})",
            setting
                .format_wire(dword)
                .unwrap_or_else(|| dword.to_string()),
            version,
            kind
        )));
    }
    Ok(())
}

/// Writes a new value (or deletes the override) for `setting` in the game's NVIDIA driver profile.
///
/// Persists the exact original explicit state and pending DRS intent before it
/// mutates a user profile. The target handle is resolved from the selected full path.
pub fn write_setting_value(
    context: &crate::Context,
    target: &SettingTarget<'_>,
    setting: &dyn NvapiSetting,
    ctx: &SettingContext,
    op: WriteOp,
) -> Result<(), ServiceError> {
    ensure_dll_setting_catalog_ready(setting, ctx)?;
    let session = open_drs_session().map_err(warning_to_service_error)?;

    let profile = target.resolve_profile_for_write(&session, ctx)?;

    let identity = profile
        .identity()
        .map_err(|error| map_nvapi_write_error(error, "profile identity read failed"))?;
    let identity_json = serde_json::to_string(&identity).map_err(|error| {
        ServiceError::command_failed(format!("could not encode NVIDIA profile identity: {error}"))
    })?;
    let profile_name = identity.name;
    let pre = read_pre_state(setting, &profile)?;
    let before_present = pre.is_explicit_in_profile;
    let before_value = before_present.then_some(pre.current);
    let mut resolved_op = op;
    if matches!(op, WriteOp::RestoreOriginal) {
        let game_id = target.game_id().ok_or_else(|| {
            ServiceError::command_failed("original revert is not available for global settings")
        })?;
        let original = context
            .storage()
            .get_nvapi_target_setting_claim(game_id, &profile_name, setting.nvapi_id())?
            .map(|claim| (claim.original_present, claim.original_value))
            .ok_or_else(|| {
                ServiceError::command_failed(
                    "no verified original NVIDIA setting state is recorded for this profile",
                )
            })?;
        resolved_op = restore_original_op(original.0, original.1)?;
    }
    let (expected_present, expected_value) = match resolved_op {
        WriteOp::Set(value) => (true, Some(value)),
        WriteOp::Delete => (false, None),
        WriteOp::RestoreOriginal => unreachable!("resolved above"),
    };
    let application = profile.matched_application();
    let application_json = application
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| {
            ServiceError::command_failed(format!(
                "could not encode NVIDIA application witness: {error}"
            ))
        })?;
    let executable_path = ctx.effective_exe_path.as_deref();
    if let Some(game_id) = target.game_id() {
        let path = executable_path
            .ok_or_else(|| ServiceError::command_failed("no executable detected for game"))?;
        if !application_matches_selected_path(application.as_ref(), path) {
            return Err(ServiceError::command_failed(
                "NVIDIA profile lookup did not match the selected full-path executable",
            ));
        }
        let owned = context.storage().get_nvapi_owned_profile(game_id)?;
        let is_owned = owned
            .as_ref()
            .is_some_and(|row| row.profile_name == profile_name);
        let profile_owner = context
            .storage()
            .get_nvapi_owned_profile_by_name(&profile_name)?;
        ensure_profile_owner_matches_game(game_id, profile_owner.as_ref())?;
        if let Some(owner) = owned.as_ref()
            && (!is_owned
                || renderpilot_domain::normalized_path_key(&owner.binding_path)
                    != renderpilot_domain::normalized_path_key(path))
        {
            return Err(ServiceError::command_failed(
                "the selected NVIDIA profile does not match RenderPilot's confirmed executable binding",
            ));
        }
        let app_json = application_json.as_deref().ok_or_else(|| {
            ServiceError::command_failed("full-path NVIDIA lookup returned no application witness")
        })?;
        let before_composition = if is_owned {
            let composition = profile_composition(&profile)?;
            if serde_json::to_string(&composition).ok().as_deref()
                != owned.as_ref().map(|row| row.composition_json.as_str())
            {
                return Err(ServiceError::command_failed(
                    "RenderPilot-owned NVIDIA profile composition changed externally; resolve the conflict before writing",
                ));
            }
            Some(composition)
        } else {
            None
        };
        let op_id = ulid::Ulid::generate().to_string();
        context
            .storage()
            .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
                op_id: &op_id,
                game_id,
                profile_name: &profile_name,
                target_kind: if is_owned { "renderpilot" } else { "external" },
                profile_is_predefined: identity.is_predefined,
                profile_identity_json: &identity_json,
                executable_path: path,
                application_witness_json: app_json,
                setting_id: setting.nvapi_id(),
                original: NvapiSettingState {
                    present: before_present,
                    value: before_value,
                },
                before: NvapiSettingState {
                    present: before_present,
                    value: before_value,
                },
                after: NvapiSettingState {
                    present: expected_present,
                    value: expected_value,
                },
            })?;

        if let Err(error) = apply_drs_write(&profile, setting, resolved_op) {
            drop(session);
            let outcome = reconcile_setting_write(
                context,
                SettingWriteReconciliation {
                    target,
                    ctx,
                    setting,
                    op_id: &op_id,
                    game_id: Some(game_id),
                    profile_name: &profile_name,
                    before_state: (before_present, before_value),
                    after_state: (expected_present, expected_value),
                    before_composition: before_composition.as_ref(),
                },
            )?;
            return finish_reconciled_write(
                outcome,
                Some(map_nvapi_write_error(error, "setting write failed")),
            );
        }
        let save_result = session.save();
        drop(session);
        let outcome = reconcile_setting_write(
            context,
            SettingWriteReconciliation {
                target,
                ctx,
                setting,
                op_id: &op_id,
                game_id: Some(game_id),
                profile_name: &profile_name,
                before_state: (before_present, before_value),
                after_state: (expected_present, expected_value),
                before_composition: before_composition.as_ref(),
            },
        )?;
        return finish_reconciled_write(
            outcome,
            save_result.err().map(|error| {
                map_nvapi_write_error(
                    error,
                    "save result was uncertain; a fresh NVIDIA session reconciled the receipt",
                )
            }),
        );
    }

    // Global profile writes also use a durable receipt and explicit original state,
    // keyed by the exact base-profile identity returned by DRS.
    let op_id = ulid::Ulid::generate().to_string();
    context
        .storage()
        .prepare_nvapi_global_setting_operation(NvapiGlobalSettingPreparation {
            op_id: &op_id,
            profile_name: &profile_name,
            profile_is_predefined: identity.is_predefined,
            profile_identity_json: &identity_json,
            setting_id: setting.nvapi_id(),
            original: NvapiSettingState {
                present: before_present,
                value: before_value,
            },
            before: NvapiSettingState {
                present: before_present,
                value: before_value,
            },
            after: NvapiSettingState {
                present: expected_present,
                value: expected_value,
            },
        })?;
    if let Err(error) = apply_drs_write(&profile, setting, resolved_op) {
        drop(session);
        let outcome = reconcile_setting_write(
            context,
            SettingWriteReconciliation {
                target,
                ctx,
                setting,
                op_id: &op_id,
                game_id: None,
                profile_name: &profile_name,
                before_state: (before_present, before_value),
                after_state: (expected_present, expected_value),
                before_composition: None,
            },
        )?;
        return finish_reconciled_write(
            outcome,
            Some(map_nvapi_write_error(error, "global setting write failed")),
        );
    }
    let save_result = session.save();
    drop(session);
    let outcome = reconcile_setting_write(
        context,
        SettingWriteReconciliation {
            target,
            ctx,
            setting,
            op_id: &op_id,
            game_id: None,
            profile_name: &profile_name,
            before_state: (before_present, before_value),
            after_state: (expected_present, expected_value),
            before_composition: None,
        },
    )?;
    finish_reconciled_write(
        outcome,
        save_result.err().map(|error| {
            map_nvapi_write_error(
                error,
                "global save result was uncertain; a fresh NVIDIA session reconciled the receipt",
            )
        }),
    )
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::{
        ClaimRestoreCompositionObservation, ClaimRestoreExpectation, ClaimRestoreObservation,
        ReconciledWrite, application_matches_selected_path, application_witness_matches,
        classify_reconciled_write, restore_original_op, validate_claim_restore_observation,
    };
    use crate::nvapi::ops::WriteOp;
    use renderpilot_storage_sqlite::NvapiSettingState;

    #[test]
    fn restoring_absent_original_deletes_the_profile_override() {
        assert_matches!(
            restore_original_op(false, None).expect("absent original"),
            WriteOp::Delete
        );
    }

    #[test]
    fn restoring_explicit_original_sets_its_recorded_value() {
        assert_matches!(
            restore_original_op(true, Some(17)).expect("explicit original"),
            WriteOp::Set(17)
        );
    }

    #[test]
    fn explicit_original_without_a_value_fails_closed() {
        assert!(restore_original_op(true, None).is_err());
    }

    #[test]
    fn game_setting_write_requires_an_exact_selected_executable_match() {
        let application = renderpilot_nvapi::ApplicationIdentity {
            app_name: "C:\\Games\\One\\game.exe".to_owned(),
            user_friendly_name: "Game".to_owned(),
            launcher: String::new(),
            file_in_folder: String::new(),
            flags: 0,
            command_line: String::new(),
            is_predefined: false,
        };

        assert!(application_matches_selected_path(
            Some(&application),
            "c:/games/one/game.exe"
        ));
        assert!(!application_matches_selected_path(
            Some(&application),
            "C:/Games/Other/game.exe"
        ));
        assert!(!application_matches_selected_path(
            Some(&application),
            "game.exe"
        ));
        assert!(!application_matches_selected_path(
            None,
            "C:/Games/One/game.exe"
        ));

        let mut folder_match = application;
        folder_match.app_name = "C:/Games/One".to_owned();
        assert!(!application_matches_selected_path(
            Some(&folder_match),
            "C:/Games/One/game.exe"
        ));
    }

    #[test]
    fn changed_application_witness_does_not_reconcile_a_game_write() {
        let recorded = r#"{"app_name":"C:/Games/One/game.exe"}"#;
        let rebound = r#"{"app_name":"C:/Games/Two/game.exe"}"#;

        let changed_binding = !application_witness_matches(Some(recorded), Some(rebound));
        assert!(changed_binding);
        assert!(!application_witness_matches(None, Some(recorded)));
        assert_eq!(
            classify_reconciled_write(
                true,
                !changed_binding,
                (true, Some(7)),
                (false, None),
                (true, Some(7)),
            ),
            ReconciledWrite::Conflict,
            "the same profile name and expected value cannot hide a changed executable binding",
        );
    }

    #[test]
    fn claim_restore_rejects_rebound_profile_even_when_value_matches() {
        let witness = r#"{"app_name":"C:/Games/One/game.exe"}"#;
        let result = validate_claim_restore_observation(
            &ClaimRestoreExpectation {
                target_id: "RenderPilot - One [id]",
                executable_path: "C:/Games/One/game.exe",
                application_witness_json: witness,
                state: NvapiSettingState {
                    present: true,
                    value: Some(7),
                },
                setting_id: 10,
            },
            &ClaimRestoreObservation {
                profile_name: "RenderPilot - Another [id]",
                is_predefined: false,
                application_witness_json: Some(witness),
                state: NvapiSettingState {
                    present: true,
                    value: Some(7),
                },
                composition: ClaimRestoreCompositionObservation {
                    owner_receipt: None,
                    before_restore: None,
                    after_restore: None,
                },
            },
        );

        assert_eq!(result, Err("profile identity changed"));
    }

    #[test]
    fn claim_restore_allows_only_the_claimed_setting_to_change_in_owned_composition() {
        let app = renderpilot_nvapi::ApplicationIdentity {
            app_name: "C:/Games/One/game.exe".to_owned(),
            user_friendly_name: "Game".to_owned(),
            launcher: String::new(),
            file_in_folder: String::new(),
            flags: 0,
            command_line: String::new(),
            is_predefined: false,
        };
        let witness = serde_json::to_string(&app).expect("witness JSON");
        let before = serde_json::json!({
            "applications": [app],
            "settings": [{"id": 10, "value": 7}, {"id": 11, "value": 2}],
        });
        let after = serde_json::json!({
            "applications": [{
                "app_name": "C:/Games/One/game.exe",
                "user_friendly_name": "Game",
                "launcher": "",
                "file_in_folder": "",
                "flags": 0,
                "command_line": "",
                "is_predefined": false,
            }],
            "settings": [{"id": 11, "value": 2}],
        });
        let before_json = before.to_string();

        let result = validate_claim_restore_observation(
            &ClaimRestoreExpectation {
                target_id: "RenderPilot - One [id]",
                executable_path: "C:/Games/One/game.exe",
                application_witness_json: &witness,
                state: NvapiSettingState {
                    present: false,
                    value: None,
                },
                setting_id: 10,
            },
            &ClaimRestoreObservation {
                profile_name: "RenderPilot - One [id]",
                is_predefined: false,
                application_witness_json: Some(&witness),
                state: NvapiSettingState {
                    present: false,
                    value: None,
                },
                composition: ClaimRestoreCompositionObservation {
                    owner_receipt: Some(&before_json),
                    before_restore: Some(&before_json),
                    after_restore: Some(&after),
                },
            },
        );
        assert_eq!(result, Ok(()));

        let drifted_after = serde_json::json!({
            "applications": after["applications"],
            "settings": [{"id": 11, "value": 3}],
        });
        let drift_result = validate_claim_restore_observation(
            &ClaimRestoreExpectation {
                target_id: "RenderPilot - One [id]",
                executable_path: "C:/Games/One/game.exe",
                application_witness_json: &witness,
                state: NvapiSettingState {
                    present: false,
                    value: None,
                },
                setting_id: 10,
            },
            &ClaimRestoreObservation {
                profile_name: "RenderPilot - One [id]",
                is_predefined: false,
                application_witness_json: Some(&witness),
                state: NvapiSettingState {
                    present: false,
                    value: None,
                },
                composition: ClaimRestoreCompositionObservation {
                    owner_receipt: Some(&before_json),
                    before_restore: Some(&before_json),
                    after_restore: Some(&drifted_after),
                },
            },
        );
        assert_eq!(
            drift_result,
            Err("owned profile composition changed beyond the restored setting")
        );
    }
}

fn apply_drs_write(
    profile: &renderpilot_nvapi::Profile<'_>,
    setting: &dyn NvapiSetting,
    op: WriteOp,
) -> Result<(), renderpilot_nvapi::NvapiError> {
    match op {
        WriteOp::Set(dword) => {
            profile.set_dword(setting.nvapi_id(), dword)?;
        }
        WriteOp::Delete => {
            profile.delete_setting(setting.nvapi_id())?;
        }
        WriteOp::RestoreOriginal => unreachable!("resolved before DRS mutation"),
    }
    Ok(())
}

fn read_setting_presence(
    setting: &dyn NvapiSetting,
    profile: &renderpilot_nvapi::Profile<'_>,
) -> Result<(bool, Option<u32>), ServiceError> {
    match profile.get_dword_full(setting.nvapi_id()) {
        Ok(value) => Ok((
            value.is_explicit_in_profile,
            value.is_explicit_in_profile.then_some(value.current),
        )),
        Err(renderpilot_nvapi::NvapiError::GetSettingFailed(code))
            if code == renderpilot_nvapi::NVAPI_SETTING_NOT_FOUND =>
        {
            Ok((false, None))
        }
        Err(error) => Err(ServiceError::command_failed(format!(
            "could not verify NVIDIA setting: {error}"
        ))),
    }
}

fn profile_composition(
    profile: &renderpilot_nvapi::Profile<'_>,
) -> Result<serde_json::Value, ServiceError> {
    let mut applications = profile
        .applications()
        .map_err(|error| map_nvapi_write_error(error, "application composition read failed"))?;
    applications.sort_by_cached_key(|application| application.app_name.to_lowercase());
    let mut settings = profile
        .settings()
        .map_err(|error| map_nvapi_write_error(error, "setting composition read failed"))?;
    settings.sort_by_key(|setting| setting.id);
    Ok(serde_json::json!({
        "applications": applications,
        "settings": settings,
    }))
}

fn application_matches_selected_path(
    application: Option<&renderpilot_nvapi::ApplicationIdentity>,
    selected_path: &str,
) -> bool {
    application.is_some_and(|application| {
        renderpilot_domain::normalized_path_key(&application.app_name)
            == renderpilot_domain::normalized_path_key(selected_path)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReconciledWrite {
    Before,
    After,
    Conflict,
}

#[derive(Clone, Copy)]
struct SettingWriteReconciliation<'request, 'game> {
    target: &'request SettingTarget<'game>,
    ctx: &'request SettingContext,
    setting: &'request dyn NvapiSetting,
    op_id: &'request str,
    game_id: Option<&'request str>,
    profile_name: &'request str,
    before_state: (bool, Option<u32>),
    after_state: (bool, Option<u32>),
    before_composition: Option<&'request serde_json::Value>,
}

fn reconcile_setting_write(
    context: &crate::Context,
    request: SettingWriteReconciliation<'_, '_>,
) -> Result<ReconciledWrite, ServiceError> {
    let SettingWriteReconciliation {
        target,
        ctx,
        setting,
        op_id,
        game_id,
        profile_name,
        before_state: (before_present, before_value),
        after_state: (after_present, after_value),
        before_composition,
    } = request;
    // Reconciliation never reuses the session that attempted the write. A
    // newly loaded DRS session is the persisted-state observation boundary.
    let session = open_drs_session().map_err(warning_to_service_error)?;
    let profile = target.resolve_profile_for_write(&session, ctx)?;
    let identity = profile.identity().map_err(|error| {
        map_nvapi_write_error(error, "profile identity during reconciliation failed")
    })?;
    let observed = read_setting_presence(setting, &profile)?;
    let owner = if game_id.is_some() {
        context
            .storage()
            .get_nvapi_owned_profile_by_name(profile_name)?
    } else {
        None
    };
    let mut application_binding_matches = game_id.is_none();
    if let Some(game_id) = game_id {
        let observed_witness = profile.matched_application();
        let expected_witness = ctx
            .effective_exe_path
            .as_deref()
            .map(|path| {
                context
                    .storage()
                    .get_nvapi_application_witness(profile_name, path)
            })
            .transpose()?
            .flatten();
        let observed_witness_json = observed_witness
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| ServiceError::command_failed(error.to_string()))?;
        let path_matches = ctx.effective_exe_path.as_deref().is_some_and(|path| {
            observed_witness.as_ref().is_some_and(|witness| {
                renderpilot_domain::normalized_path_key(&witness.app_name)
                    == renderpilot_domain::normalized_path_key(path)
            })
        });
        application_binding_matches = path_matches
            && application_witness_matches(
                expected_witness.as_deref(),
                observed_witness_json.as_deref(),
            )
            && owner.as_ref().is_none_or(|owner| {
                Some(owner.application_witness_json.as_str()) == observed_witness_json.as_deref()
            });
        if owner.as_ref().is_some_and(|owner| owner.game_id != game_id) {
            mark_setting_conflict(
                context,
                op_id,
                &serde_json::json!({"owner_game_id": owner.as_ref().map(|owner| &owner.game_id), "request_game_id": game_id}),
            )?;
            return Ok(ReconciledWrite::Conflict);
        }
    }
    let outcome = classify_reconciled_write(
        identity.name == profile_name,
        application_binding_matches,
        observed,
        (before_present, before_value),
        (after_present, after_value),
    );
    if outcome == ReconciledWrite::Conflict {
        mark_setting_conflict(
            context,
            op_id,
            &serde_json::json!({
                "profile_name": identity.name,
                "application_binding_matches": application_binding_matches,
                "present": observed.0,
                "value": observed.1,
            }),
        )?;
        return Ok(ReconciledWrite::Conflict);
    }
    let after_composition = if owner.is_some() {
        Some(profile_composition(&profile)?)
    } else {
        None
    };
    if outcome == ReconciledWrite::After {
        if let (Some(owner), Some(actual), Some(before)) = (
            owner.as_ref(),
            after_composition.as_ref(),
            before_composition,
        ) {
            let before_json = before.to_string();
            if owner.composition_json.as_str() != before_json.as_str()
                || !composition_matches_except_setting(
                    &owner.composition_json,
                    actual,
                    setting.nvapi_id(),
                )?
            {
                mark_setting_conflict(context, op_id, &serde_json::json!({"composition": actual}))?;
                return Ok(ReconciledWrite::Conflict);
            }
        }
        let identity_json = serde_json::to_string(&identity)
            .map_err(|error| ServiceError::command_failed(error.to_string()))?;
        let composition_json = after_composition.as_ref().map(serde_json::Value::to_string);
        context
            .storage()
            .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
                op_id,
                scope: match game_id {
                    Some(game_id) => NvapiSettingOperationScope::Game { game_id },
                    None => NvapiSettingOperationScope::Global,
                },
                target_id: profile_name,
                setting_id: setting.nvapi_id(),
                expected: NvapiSettingState {
                    present: after_present,
                    value: after_value,
                },
                profile_identity_json: &identity_json,
                composition_json: composition_json.as_deref(),
            })?;
        return Ok(ReconciledWrite::After);
    }
    if outcome == ReconciledWrite::Before {
        if let (Some(owner), Some(actual), Some(before)) = (
            owner.as_ref(),
            after_composition.as_ref(),
            before_composition,
        ) {
            let before_json = before.to_string();
            if owner.composition_json.as_str() != before_json.as_str() || actual != before {
                mark_setting_conflict(context, op_id, &serde_json::json!({"composition": actual}))?;
                return Ok(ReconciledWrite::Conflict);
            }
        }
        context.storage().cancel_nvapi_setting_operation(op_id)?;
        return Ok(ReconciledWrite::Before);
    }
    unreachable!("all non-conflict reconciliation outcomes are handled above")
}

fn application_witness_matches(expected: Option<&str>, observed: Option<&str>) -> bool {
    expected.is_some() && expected == observed
}

fn classify_reconciled_write(
    identity_matches: bool,
    application_binding_matches: bool,
    observed: (bool, Option<u32>),
    before: (bool, Option<u32>),
    after: (bool, Option<u32>),
) -> ReconciledWrite {
    if !identity_matches || !application_binding_matches {
        ReconciledWrite::Conflict
    } else if observed == after {
        ReconciledWrite::After
    } else if observed == before {
        ReconciledWrite::Before
    } else {
        ReconciledWrite::Conflict
    }
}

struct ClaimRestoreExpectation<'a> {
    target_id: &'a str,
    executable_path: &'a str,
    application_witness_json: &'a str,
    state: NvapiSettingState,
    setting_id: u32,
}

struct ClaimRestoreCompositionObservation<'a> {
    owner_receipt: Option<&'a str>,
    before_restore: Option<&'a str>,
    after_restore: Option<&'a serde_json::Value>,
}

struct ClaimRestoreObservation<'a> {
    profile_name: &'a str,
    is_predefined: bool,
    application_witness_json: Option<&'a str>,
    state: NvapiSettingState,
    composition: ClaimRestoreCompositionObservation<'a>,
}

fn validate_claim_restore_observation(
    expected: &ClaimRestoreExpectation<'_>,
    observed: &ClaimRestoreObservation<'_>,
) -> Result<(), &'static str> {
    if observed.profile_name != expected.target_id
        || (observed.composition.owner_receipt.is_some() && observed.is_predefined)
    {
        return Err("profile identity changed");
    }
    if !application_witness_matches(
        Some(expected.application_witness_json),
        observed.application_witness_json,
    ) {
        return Err("full-path application witness changed");
    }
    let witness: renderpilot_nvapi::ApplicationIdentity = serde_json::from_str(
        observed
            .application_witness_json
            .ok_or("full-path application witness is missing")?,
    )
    .map_err(|_| "full-path application witness is invalid")?;
    if renderpilot_domain::normalized_path_key(&witness.app_name)
        != renderpilot_domain::normalized_path_key(expected.executable_path)
    {
        return Err("full-path application binding changed");
    }
    if observed.state != expected.state {
        return Err("restored setting presence or value changed");
    }
    match (
        observed.composition.owner_receipt,
        observed.composition.before_restore,
        observed.composition.after_restore,
    ) {
        (Some(owner), Some(before), Some(after)) => {
            if owner != before {
                return Err("owned profile composition changed before restore");
            }
            if !composition_matches_except_setting(owner, after, expected.setting_id)
                .unwrap_or(false)
            {
                return Err("owned profile composition changed beyond the restored setting");
            }
        }
        (None, None, None) => {}
        _ => return Err("owned profile composition could not be verified"),
    }
    Ok(())
}

fn composition_matches_except_setting(
    before_json: &str,
    after: &serde_json::Value,
    setting_id: u32,
) -> Result<bool, ServiceError> {
    let mut before: serde_json::Value = serde_json::from_str(before_json).map_err(|error| {
        ServiceError::command_failed(format!("invalid NVIDIA composition receipt: {error}"))
    })?;
    let mut after = after.clone();
    for composition in [&mut before, &mut after] {
        if let Some(settings) = composition
            .get_mut("settings")
            .and_then(serde_json::Value::as_array_mut)
        {
            settings.retain(|setting| {
                setting.get("id").and_then(serde_json::Value::as_u64) != Some(u64::from(setting_id))
            });
        }
    }
    Ok(before == after)
}

fn mark_setting_conflict(
    context: &crate::Context,
    op_id: &str,
    observed: &serde_json::Value,
) -> Result<(), ServiceError> {
    context
        .storage()
        .mark_nvapi_operation_conflict(op_id, &observed.to_string())?;
    Ok(())
}

fn finish_reconciled_write(
    outcome: ReconciledWrite,
    write_error: Option<ServiceError>,
) -> Result<(), ServiceError> {
    match outcome {
        ReconciledWrite::After => Ok(()),
        ReconciledWrite::Before => Err(write_error.unwrap_or_else(|| {
            ServiceError::command_failed(
                "the requested NVIDIA setting was not saved; the exact before-state was verified",
            )
        })),
        ReconciledWrite::Conflict => Err(write_error.unwrap_or_else(|| {
            ServiceError::command_failed(
                "NVIDIA setting state matches neither the recorded before-state nor requested value; the operation is marked as a conflict",
            )
        })),
    }
}

pub(super) fn ensure_profile_owner_matches_game(
    game_id: &str,
    owner: Option<&renderpilot_storage_sqlite::NvapiOwnedProfileRow>,
) -> Result<(), ServiceError> {
    if let Some(owner) = owner
        && owner.game_id != game_id
    {
        return Err(ServiceError::command_failed(format!(
            "NVIDIA profile `{}` is owned by game {}; this game cannot change its settings",
            owner.profile_name, owner.game_id
        )));
    }
    Ok(())
}

/// Rejects a DLL-dependent mutation until the game has a current complete
/// catalog projection. This must run before DRS access or claim writes.
pub(crate) fn ensure_dll_setting_catalog_ready(
    setting: &dyn NvapiSetting,
    ctx: &SettingContext,
) -> Result<(), ServiceError> {
    if setting.dll_kind().is_some() && ctx.catalog_readiness == CatalogReadiness::NotReady {
        return Err(ServiceError::NvapiCatalogNotReady);
    }
    Ok(())
}

/// Restores every setting claim for one game and releases its catalog references.
///
/// Shared profile settings are restored only when this game owns the final
/// reference. The claim and pending journal fence the driver mutation until it
/// has been verified in a fresh DRS session.
pub(crate) fn restore_game_setting_claims(
    context: &crate::Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    game_id: &str,
) -> Result<(), ServiceError> {
    if guard.game_id().as_str() != game_id {
        return Err(ServiceError::command_failed(
            "NVAPI setting restore requires the matching game mutation boundary",
        ));
    }
    let _drs_guard = super::super::game_session::lock_drs_operations()?;
    let claims = context
        .storage()
        .list_nvapi_setting_claims_for_game(game_id)?;
    if claims.is_empty() {
        return Ok(());
    }
    restore_nvapi_claims(context, game_id, claims)?;
    Ok(())
}

fn restore_nvapi_claims(
    context: &crate::Context,
    game_id: &str,
    claims: Vec<renderpilot_storage_sqlite::NvapiTargetSettingClaimRow>,
) -> Result<(), ServiceError> {
    for claim in claims {
        let references = context
            .storage()
            .count_nvapi_setting_claim_refs(&claim.target_id, claim.setting_id)?;
        if references > 1 {
            context.storage().release_nvapi_setting_claim(
                game_id,
                &claim.target_id,
                claim.setting_id,
            )?;
            continue;
        }

        if !crate::dlss::settings_catalog::catalog()
            .iter()
            .any(|setting| setting.nvapi_id == claim.setting_id)
        {
            return Err(ServiceError::command_failed(format!(
                "NVAPI claim {} uses unsupported setting id {}",
                claim.target_id, claim.setting_id
            )));
        }
        let session = open_drs_session().map_err(warning_to_service_error)?;
        let profile = session
            .find_profile_by_exe(&claim.executable_path)
            .map_err(|error| {
                ServiceError::command_failed(format!(
                    "NVIDIA profile for claimed executable {} could not be resolved: {error}",
                    claim.executable_path
                ))
            })?;
        let identity = profile.identity().map_err(|error| {
            ServiceError::command_failed(format!("could not verify NVIDIA claim target: {error}"))
        })?;
        let witness = profile.matched_application().ok_or_else(|| {
            ServiceError::command_failed("NVIDIA claim target has no full-path application witness")
        })?;
        let witness_json = serde_json::to_string(&witness)
            .map_err(|error| ServiceError::command_failed(error.to_string()))?;
        let recorded_witness = context
            .storage()
            .get_nvapi_application_witness(&claim.target_id, &claim.executable_path)?;
        if identity.name != claim.target_id || recorded_witness.as_deref() != Some(&witness_json) {
            return Err(ServiceError::command_failed(format!(
                "NVIDIA claim target or executable witness changed for {}; the claim was kept for review",
                claim.executable_path
            )));
        }
        let owner = context
            .storage()
            .get_nvapi_owned_profile_by_name(&identity.name)?;
        let before_composition = if let Some(owner) = owner.as_ref() {
            if owner.game_id != game_id
                || owner.state != "active"
                || renderpilot_domain::normalized_path_key(&owner.binding_path)
                    != renderpilot_domain::normalized_path_key(&claim.executable_path)
                || owner.application_witness_json != witness_json
            {
                return Err(ServiceError::command_failed(format!(
                    "owned NVIDIA claim target or binding changed for {}; the claim was kept for review",
                    claim.executable_path
                )));
            }
            let composition = profile_composition(&profile)?;
            let composition_json = composition.to_string();
            if composition_json != owner.composition_json {
                return Err(ServiceError::command_failed(format!(
                    "owned NVIDIA profile composition changed for {}; the claim was kept for review",
                    claim.executable_path
                )));
            }
            Some(owner.composition_json.as_str())
        } else {
            None
        };
        let live = read_explicit_setting(&profile, claim.setting_id)?;
        if live != (claim.expected_present, claim.expected_value) {
            return Err(ServiceError::command_failed(format!(
                "NVIDIA setting {} changed outside RenderPilot; its claim was retained and must be reconciled before removing this game",
                claim.setting_id
            )));
        }

        let identity_json = serde_json::to_string(&identity)
            .map_err(|error| ServiceError::command_failed(error.to_string()))?;
        let operation_id = ulid::Ulid::generate().to_string();
        let target_kind = if owner.is_some() {
            "renderpilot"
        } else {
            "external"
        };
        context
            .storage()
            .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
                op_id: &operation_id,
                game_id,
                profile_name: &identity.name,
                target_kind,
                profile_is_predefined: identity.is_predefined,
                profile_identity_json: &identity_json,
                executable_path: &claim.executable_path,
                application_witness_json: &witness_json,
                setting_id: claim.setting_id,
                original: NvapiSettingState {
                    present: claim.original_present,
                    value: claim.original_value,
                },
                before: NvapiSettingState {
                    present: claim.expected_present,
                    value: claim.expected_value,
                },
                after: NvapiSettingState {
                    present: claim.original_present,
                    value: claim.original_value,
                },
            })?;
        if claim.original_present {
            profile
                .set_dword(
                    claim.setting_id,
                    claim.original_value.ok_or_else(|| {
                        ServiceError::command_failed(
                            "NVAPI claim has an explicit original value missing from its receipt",
                        )
                    })?,
                )
                .map_err(|error| map_nvapi_write_error(error, "claim restore failed"))?;
        } else {
            profile
                .delete_setting(claim.setting_id)
                .map_err(|error| map_nvapi_write_error(error, "claim restore delete failed"))?;
        }
        session.save().map_err(|error| ServiceError::command_failed(format!(
            "NVIDIA claim restore has an uncertain save result; the journal remains pending: {error}"
        )))?;
        drop(session);

        let verify_session = open_drs_session().map_err(warning_to_service_error)?;
        let verified = verify_session
            .find_profile_by_exe(&claim.executable_path)
            .map_err(|error| {
                ServiceError::command_failed(format!(
                    "could not verify restored NVIDIA claim: {error}"
                ))
            })?;
        let actual = read_explicit_setting(&verified, claim.setting_id)?;
        let verified_identity = verified
            .identity()
            .map_err(|error| ServiceError::command_failed(error.to_string()))?;
        let verified_witness = verified
            .matched_application()
            .map(|witness| {
                serde_json::to_string(&witness).map_err(|error| {
                    ServiceError::command_failed(format!(
                        "could not encode verified application witness: {error}"
                    ))
                })
            })
            .transpose()?;
        let verified_composition = if owner.is_some() {
            Some(profile_composition(&verified)?)
        } else {
            None
        };
        let observation_error = validate_claim_restore_observation(
            &ClaimRestoreExpectation {
                target_id: &claim.target_id,
                executable_path: &claim.executable_path,
                application_witness_json: &witness_json,
                state: NvapiSettingState {
                    present: claim.original_present,
                    value: claim.original_value,
                },
                setting_id: claim.setting_id,
            },
            &ClaimRestoreObservation {
                profile_name: &verified_identity.name,
                is_predefined: verified_identity.is_predefined,
                application_witness_json: verified_witness.as_deref(),
                state: NvapiSettingState {
                    present: actual.0,
                    value: actual.1,
                },
                composition: ClaimRestoreCompositionObservation {
                    owner_receipt: owner.as_ref().map(|owner| owner.composition_json.as_str()),
                    before_restore: before_composition,
                    after_restore: verified_composition.as_ref(),
                },
            },
        );
        if let Err(reason) = observation_error {
            let observed = serde_json::json!({
                "profile_name": verified_identity.name,
                "is_predefined": verified_identity.is_predefined,
                "application_witness": verified_witness,
                "present": actual.0,
                "value": actual.1,
                "composition": verified_composition,
                "reason": reason,
            })
            .to_string();
            context
                .storage()
                .mark_nvapi_operation_conflict(&operation_id, &observed)?;
            return Err(ServiceError::command_failed(format!(
                "restored NVIDIA claim could not be verified ({reason}); the claim and operation are retained as a conflict"
            )));
        }
        let verified_identity_json = serde_json::to_string(&verified_identity)
            .map_err(|error| ServiceError::command_failed(error.to_string()))?;
        let composition_json = verified_composition
            .as_ref()
            .map(serde_json::Value::to_string);
        context
            .storage()
            .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
                op_id: &operation_id,
                scope: NvapiSettingOperationScope::Game { game_id },
                target_id: &claim.target_id,
                setting_id: claim.setting_id,
                expected: NvapiSettingState {
                    present: claim.original_present,
                    value: claim.original_value,
                },
                profile_identity_json: &verified_identity_json,
                composition_json: composition_json.as_deref(),
            })?;
        context.storage().release_nvapi_setting_claim(
            game_id,
            &claim.target_id,
            claim.setting_id,
        )?;
    }
    Ok(())
}

fn read_explicit_setting(
    profile: &renderpilot_nvapi::Profile<'_>,
    setting_id: u32,
) -> Result<(bool, Option<u32>), ServiceError> {
    match profile.get_dword_full(setting_id) {
        Ok(state) => Ok((
            state.is_explicit_in_profile,
            state.is_explicit_in_profile.then_some(state.current),
        )),
        Err(NvapiError::GetSettingFailed(code))
            if code == renderpilot_nvapi::NVAPI_SETTING_NOT_FOUND =>
        {
            Ok((false, None))
        }
        Err(error) => Err(ServiceError::command_failed(format!(
            "could not read NVIDIA setting state: {error}"
        ))),
    }
}
