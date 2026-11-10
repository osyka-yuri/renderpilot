//! Write and validate NVAPI setting overrides.

use renderpilot_nvapi::setting::{CatalogReadiness, NvapiSetting, SettingContext};
use renderpilot_storage_sqlite::{
    NvapiGameSettingPreparation, NvapiGlobalSettingPreparation, NvapiSettingOperationCompletion,
    NvapiSettingOperationScope, NvapiSettingState, SqliteStorage,
};

use super::super::profile_composition::{composition_value, reconcile_owned_setting};
use super::assemble::known_preset_set;
use super::live::read_pre_state;
use super::session::{map_nvapi_write_error, open_drs_session, warning_to_service_error};
use super::target::{SettingTarget, WriteOp};
use crate::ServiceError;
use crate::dlss::preset_manifest::{bundled_manifest, supported_presets_for};

/// Resolves a revert request against the recorded original state.
///
/// `target` specifies whether the setting is game-scoped or global.
/// `revert_target` is `"predefined"` (delete the override, restoring the driver
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

pub(super) fn restore_original_op(
    present: bool,
    value: Option<u32>,
) -> Result<WriteOp, ServiceError> {
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

/// Scope parameters required for game-specific setting preparation.
#[derive(Debug, Clone, Copy)]
pub(super) struct GamePreparationScope<'a> {
    pub game_id: &'a str,
    pub target_kind: &'a str,
    pub executable_path: &'a str,
    pub application_witness_json: &'a str,
    pub owned_before_composition_json: Option<&'a str>,
}

/// Parameters for dispatching setting operation preparation to the appropriate storage preparation entry point.
#[derive(Debug, Clone, Copy)]
pub(super) struct SettingPreparationDispatch<'a> {
    pub target: &'a SettingTarget<'a>,
    pub op_id: &'a str,
    pub profile_name: &'a str,
    pub profile_is_predefined: bool,
    pub profile_identity_json: &'a str,
    pub setting_id: u32,
    pub before: NvapiSettingState,
    pub op: WriteOp,
    pub game_scope: Option<GamePreparationScope<'a>>,
}

/// Dispatches setting operation preparation to the appropriate storage preparation entry point.
///
/// Explicit Set and Delete operations use authorized preparation so a user can
/// reapply a setting after the live driver state has drifted. Automatic claim
/// cleanup continues to use the strict storage preparation path.
pub(super) fn dispatch_setting_preparation(
    storage: &SqliteStorage,
    dispatch: SettingPreparationDispatch<'_>,
) -> Result<NvapiSettingState, ServiceError> {
    let SettingPreparationDispatch {
        target,
        op_id,
        profile_name,
        profile_is_predefined,
        profile_identity_json,
        setting_id,
        before,
        op,
        game_scope,
    } = dispatch;

    let after = match op {
        WriteOp::Set(value) => NvapiSettingState {
            present: true,
            value: Some(value),
        },
        WriteOp::Delete => NvapiSettingState {
            present: false,
            value: None,
        },
        WriteOp::RestoreOriginal => {
            return Err(ServiceError::command_failed(
                "WriteOp::RestoreOriginal must be resolved before preparation",
            ));
        }
    };
    match target {
        SettingTarget::Global => {
            storage.prepare_nvapi_authorized_global_setting_operation(
                NvapiGlobalSettingPreparation {
                    op_id,
                    profile_name,
                    profile_is_predefined,
                    profile_identity_json,
                    setting_id,
                    original: before,
                    before,
                    after,
                },
            )?;
        }
        SettingTarget::Game { game_id } => {
            let scope = game_scope.ok_or_else(|| {
                ServiceError::command_failed("game setting preparation requires game scope details")
            })?;
            if scope.game_id != *game_id {
                return Err(ServiceError::command_failed(
                    "game id mismatch in setting preparation",
                ));
            }
            storage.prepare_nvapi_authorized_game_setting_operation(
                NvapiGameSettingPreparation {
                    op_id,
                    game_id,
                    profile_name,
                    target_kind: scope.target_kind,
                    profile_is_predefined,
                    profile_identity_json,
                    executable_path: scope.executable_path,
                    application_witness_json: scope.application_witness_json,
                    setting_id,
                    original: before,
                    before,
                    after,
                },
                scope.owned_before_composition_json,
            )?;
        }
    }
    Ok(after)
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
    let before = NvapiSettingState {
        present: pre.is_explicit_in_profile,
        value: pre.is_explicit_in_profile.then_some(pre.current),
    };
    let setting_id = setting.nvapi_id();
    let mut resolved_op = op;
    let mut before_composition = None;
    let before_composition_json;
    let application_json;
    let storage = context.storage();
    let game_scope = if let Some(game_id) = target.game_id() {
        let path = ctx
            .effective_exe_path
            .as_deref()
            .ok_or_else(|| ServiceError::command_failed("no executable detected for game"))?;
        let owned = storage.get_nvapi_owned_profile(game_id)?;
        let is_owned = owned
            .as_ref()
            .is_some_and(|row| row.profile_name == profile_name);
        let profile_owner = storage.get_nvapi_owned_profile_by_name(&profile_name)?;
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
        application_json =
            setting_application_witness_json(profile.matched_application().as_ref())?;
        let app_json = application_json.as_deref().ok_or_else(|| {
            ServiceError::command_failed(
                "full-path NVIDIA profile lookup returned no application witness",
            )
        })?;
        if matches!(op, WriteOp::RestoreOriginal) {
            let original = storage.get_nvapi_bound_setting_original(
                game_id, &profile_name, setting_id, path, identity.is_predefined, app_json,
            )?
            .ok_or_else(|| ServiceError::command_failed(
                "no verified original NVIDIA setting state is recorded for this executable binding",
            ))?;
            resolved_op = restore_original_op(original.present, original.value)?;
        }
        before_composition = is_owned
            .then(|| profile_composition(&profile))
            .transpose()?;
        before_composition_json = before_composition
            .as_ref()
            .map(serde_json::Value::to_string);
        Some(GamePreparationScope {
            game_id,
            target_kind: if is_owned { "renderpilot" } else { "external" },
            executable_path: path,
            application_witness_json: app_json,
            owned_before_composition_json: before_composition_json.as_deref(),
        })
    } else {
        None
    };

    let op_id = ulid::Ulid::generate().to_string();
    let after = dispatch_setting_preparation(
        storage,
        SettingPreparationDispatch {
            target,
            op_id: &op_id,
            profile_name: &profile_name,
            profile_is_predefined: identity.is_predefined,
            profile_identity_json: &identity_json,
            setting_id,
            before,
            op: resolved_op,
            game_scope,
        },
    )?;
    let write_error = apply_drs_write(&profile, setting, resolved_op)
        .map_err(|error| map_nvapi_write_error(error, "setting write failed"))
        .and_then(|()| {
            session.save().map_err(|error| {
                map_nvapi_write_error(
                    error,
                    "save result was uncertain; a fresh NVIDIA session reconciled the receipt",
                )
            })
        })
        .err();
    drop(session);
    let outcome = reconcile_setting_write(
        context,
        SettingWriteReconciliation {
            target,
            ctx,
            setting,
            op_id: &op_id,
            profile_name: &profile_name,
            before_state: (before.present, before.value),
            after_state: (after.present, after.value),
            before_composition: before_composition.as_ref(),
        },
    )?;
    finish_reconciled_write(outcome, write_error)
}

#[cfg(test)]
mod tests;

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

pub(super) fn profile_composition(
    profile: &renderpilot_nvapi::Profile<'_>,
) -> Result<serde_json::Value, ServiceError> {
    let applications = profile
        .applications()
        .map_err(|error| map_nvapi_write_error(error, "application composition read failed"))?;
    let settings = profile
        .settings()
        .map_err(|error| map_nvapi_write_error(error, "setting composition read failed"))?;
    Ok(composition_value(applications, settings))
}

pub(super) fn setting_application_witness_json(
    application: Option<&renderpilot_nvapi::ApplicationIdentity>,
) -> Result<Option<String>, ServiceError> {
    let Some(application) = application else {
        return Ok(None);
    };
    if application.app_name.is_empty() {
        return Ok(None);
    }
    serde_json::to_string(application)
        .map(Some)
        .map_err(|error| {
            ServiceError::command_failed(format!(
                "could not encode NVIDIA application witness: {error}"
            ))
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
        profile_name,
        before_state: (before_present, before_value),
        after_state: (after_present, after_value),
        before_composition,
    } = request;
    let game_id = target.game_id();
    // Reconciliation never reuses the session that attempted the write. A
    // newly loaded DRS session is the persisted-state observation boundary.
    let session = open_drs_session().map_err(warning_to_service_error)?;
    let profile = target.resolve_profile_for_write(&session, ctx)?;
    let identity = profile.identity().map_err(|error| {
        map_nvapi_write_error(error, "profile identity during reconciliation failed")
    })?;
    let recorded_profile_status = context
        .storage()
        .get_nvapi_target_profile_is_predefined(profile_name)?;
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
        let observed_witness_json = setting_application_witness_json(observed_witness.as_ref())?;
        application_binding_matches = application_witness_matches(
            expected_witness.as_deref(),
            observed_witness_json.as_deref(),
        ) && owner.as_ref().is_none_or(|owner| {
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
        stable_profile_identity_matches(
            profile_name,
            recorded_profile_status,
            &identity.name,
            identity.is_predefined,
        ),
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
                "profile_is_predefined": identity.is_predefined,
                "recorded_profile_is_predefined": recorded_profile_status,
                "application_binding_matches": application_binding_matches,
                "present": observed.0,
                "value": observed.1,
            }),
        )?;
        return Ok(ReconciledWrite::Conflict);
    }
    let verified_owned_composition = match (owner, before_composition) {
        (Some(owner), Some(before)) => {
            let actual = profile_composition(&profile)?;
            match reconcile_owned_setting(
                &owner.composition_json,
                before,
                &actual,
                setting.nvapi_id(),
                outcome == ReconciledWrite::After,
            ) {
                Ok(composition) => composition,
                Err(error) => {
                    mark_setting_conflict(
                        context,
                        op_id,
                        &serde_json::json!({
                            "composition": actual,
                            "reconciliation_error": error,
                        }),
                    )?;
                    return Ok(ReconciledWrite::Conflict);
                }
            }
        }
        (None, None) => None,
        _ => {
            mark_setting_conflict(
                context,
                op_id,
                &serde_json::json!({"reconciliation_error": "NVIDIA profile ownership changed"}),
            )?;
            return Ok(ReconciledWrite::Conflict);
        }
    };
    if outcome == ReconciledWrite::After {
        let identity_json = serde_json::to_string(&identity)
            .map_err(|error| ServiceError::command_failed(error.to_string()))?;
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
                composition_json: verified_owned_composition.as_deref(),
            })?;
        return Ok(ReconciledWrite::After);
    }
    if outcome == ReconciledWrite::Before {
        context.storage().cancel_nvapi_setting_operation(op_id)?;
        return Ok(ReconciledWrite::Before);
    }
    unreachable!("all non-conflict reconciliation outcomes are handled above")
}

pub(super) fn application_witness_matches(expected: Option<&str>, observed: Option<&str>) -> bool {
    expected.is_some_and(|witness| !witness.is_empty()) && expected == observed
}

pub(super) fn stable_profile_identity_matches(
    expected_name: &str,
    recorded_is_predefined: Option<bool>,
    observed_name: &str,
    observed_is_predefined: bool,
) -> bool {
    expected_name == observed_name && recorded_is_predefined == Some(observed_is_predefined)
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
