use renderpilot_nvapi::setting::SettingContext;
use renderpilot_storage_sqlite::{NvapiProfileCreationCompletion, NvapiVerifiedProfileReceipt};

use super::{
    Context, NvapiError, NvapiProfileState, ProfileTransition, ServiceError,
    classify_profile_transition, generated_profile_name, map_drs, open_drs_session, profile_status,
    read_profile_receipts, validate_game_executable,
};
/// Creates one empty profile only after the full path is proven missing.
pub(in crate::nvapi) fn create_owned_profile(
    context: &Context,
    game_id: &str,
    setting_context: &SettingContext,
) -> Result<(), ServiceError> {
    let path = setting_context
        .effective_exe_path
        .as_deref()
        .ok_or_else(|| {
            ServiceError::command_failed("select an executable before creating an NVIDIA profile")
        })?;
    let (path, _) = validate_game_executable(context, game_id, path)?;
    let path = path.to_string_lossy().replace('\\', "/");
    let status = profile_status(context, game_id, setting_context)?;
    if status.state != NvapiProfileState::Missing || !status.can_create {
        return Err(ServiceError::command_failed(status.detail.unwrap_or_else(
            || "NVIDIA profile creation is unavailable for the current executable state".to_owned(),
        )));
    }
    let session = open_drs_session().map_err(map_drs)?;
    match session.find_profile_by_exe(&path) {
        Err(NvapiError::ExecutableNotFound) => {}
        Err(error) => return Err(map_drs(error)),
        Ok(_) => {
            return Err(ServiceError::command_failed(
                "an NVIDIA profile appeared for this executable; refresh status before creating",
            ));
        }
    }
    let game = crate::nvapi::resolve::load_game_with_context(context, game_id)?;
    let profile_name = generated_profile_name(game.identity().title(), game_id);
    match session.find_profile_by_name(&profile_name) {
        Ok(_) => {
            return Err(ServiceError::command_failed(format!(
                "the generated NVIDIA profile name `{profile_name}` is already in use"
            )));
        }
        Err(NvapiError::ProfileNotFound) => {}
        Err(error) => return Err(map_drs(error)),
    }
    let op_id = ulid::Ulid::generate().to_string();
    let before = serde_json::json!({"binding_path": path, "profile_name": profile_name, "profile_absent": true}).to_string();
    let after = serde_json::json!({"binding_path": path, "profile_name": profile_name, "application_path": path}).to_string();
    context.storage().begin_nvapi_operation(
        &op_id,
        Some(game_id),
        Some(&profile_name),
        "create_profile",
        &before,
        &after,
    )?;
    let profile = match session.create_profile(&profile_name) {
        Ok(profile) => profile,
        Err(error) => {
            drop(session);
            if reconcile_create_operation(context, &op_id, game_id, &profile_name, &path)? {
                return Ok(());
            }
            return Err(map_drs(error));
        }
    };
    if let Err(error) = profile.create_application(&path) {
        drop(session);
        if reconcile_create_operation(context, &op_id, game_id, &profile_name, &path)? {
            return Ok(());
        }
        return Err(map_drs(error));
    }
    let save_result = session.save();
    drop(session);
    if reconcile_create_operation(context, &op_id, game_id, &profile_name, &path)? {
        return Ok(());
    }
    if let Err(error) = save_result {
        return Err(ServiceError::command_failed(format!(
            "NVIDIA profile save did not persist the requested profile; the exact before-state was verified and the journal was canceled: {error}"
        )));
    }
    Err(ServiceError::command_failed(
        "NVIDIA profile creation was not present in a fresh driver session; the journal was canceled",
    ))
}

fn reconcile_create_operation(
    context: &Context,
    op_id: &str,
    game_id: &str,
    profile_name: &str,
    path: &str,
) -> Result<bool, ServiceError> {
    let session = open_drs_session().map_err(map_drs)?;
    let by_name = match session.find_profile_by_name(profile_name) {
        Ok(profile) => Some(profile),
        Err(NvapiError::ProfileNotFound) => None,
        Err(error) => return Err(map_drs(error)),
    };
    let by_path = match session.find_profile_by_exe(path) {
        Ok(profile) => Some(profile),
        Err(NvapiError::ExecutableNotFound) => None,
        Err(error) => return Err(map_drs(error)),
    };
    let Some(named) = by_name else {
        let Some(path_profile) = by_path else {
            return match classify_profile_transition(Some(true), Some(false)) {
                ProfileTransition::Before => {
                    context.storage().cancel_nvapi_operation(op_id)?;
                    Ok(false)
                }
                _ => unreachable!("exact missing observations prove the create before-state"),
            };
        };
        let identity = path_profile.identity().map_err(map_drs)?;
        if identity.name == profile_name {
            return match classify_profile_transition(None, None) {
                ProfileTransition::Pending => Err(ServiceError::command_failed(
                    "profile-name and full-path lookups disagree about the requested profile; the operation remains pending",
                )),
                _ => unreachable!("inconsistent profile lookups are incomplete evidence"),
            };
        }
        let observed = serde_json::json!({
            "expected_profile_name": profile_name,
            "profile_name": identity.name,
            "binding_path": path,
        })
        .to_string();
        return match classify_profile_transition(Some(false), Some(false)) {
            ProfileTransition::Conflict => {
                context
                    .storage()
                    .mark_nvapi_operation_conflict(op_id, &observed)?;
                Err(ServiceError::command_failed(
                    "the requested profile identity is absent while this executable resolves to a DRS profile; the operation is marked as a conflict",
                ))
            }
            _ => unreachable!("successful mismatched path identity proves a conflict"),
        };
    };
    let named_identity = named.identity().map_err(map_drs)?;
    if named_identity.name != profile_name || named_identity.is_predefined {
        let observed = serde_json::json!({"identity": named_identity}).to_string();
        return match classify_profile_transition(Some(false), Some(false)) {
            ProfileTransition::Conflict => {
                context
                    .storage()
                    .mark_nvapi_operation_conflict(op_id, &observed)?;
                Err(ServiceError::command_failed(
                    "fresh NVIDIA session returned a different profile identity; the operation is marked as a conflict",
                ))
            }
            _ => unreachable!("successful contradictory identity proves a conflict"),
        };
    }

    let Some(path_profile) = by_path else {
        let applications = named.applications().map_err(map_drs)?;
        let settings = named.settings().map_err(map_drs)?;
        let enumeration_matches_after = applications.len() == 1
            && renderpilot_domain::normalized_path_key(&applications[0].app_name)
                == renderpilot_domain::normalized_path_key(path)
            && settings.is_empty();
        if enumeration_matches_after {
            return match classify_profile_transition(None, None) {
                ProfileTransition::Pending => Err(ServiceError::command_failed(
                    "the profile composition is visible but full-path lookup has not confirmed its binding; the operation remains pending",
                )),
                _ => unreachable!("missing path witness leaves the create unresolved"),
            };
        }
        let observed = serde_json::json!({
            "profile_name": named_identity.name,
            "applications": applications,
            "settings": settings,
            "path_lookup": "executable_not_found",
        })
        .to_string();
        return match classify_profile_transition(Some(false), Some(false)) {
            ProfileTransition::Conflict => {
                context
                    .storage()
                    .mark_nvapi_operation_conflict(op_id, &observed)?;
                Err(ServiceError::command_failed(
                    "the NVIDIA profile composition contradicts the requested binding; the operation is marked as a conflict",
                ))
            }
            _ => unreachable!("fully observed mismatching composition proves a conflict"),
        };
    };
    let observed = read_profile_receipts(&path_profile)?;
    let exact_after = observed.identity.name == profile_name
        && !observed.identity.is_predefined
        && observed.applications.len() == 1
        && observed.applications[0] == observed.witness
        && renderpilot_domain::normalized_path_key(&observed.witness.app_name)
            == renderpilot_domain::normalized_path_key(path)
        && observed.settings.is_empty();
    match classify_profile_transition(Some(false), Some(exact_after)) {
        ProfileTransition::Conflict => {
            let conflict_observation = serde_json::json!({
                "profile_name": observed.identity.name,
                "applications": observed.applications,
                "settings": observed.settings,
            })
            .to_string();
            context
                .storage()
                .mark_nvapi_operation_conflict(op_id, &conflict_observation)?;
            return Err(ServiceError::command_failed(
                "fresh NVIDIA session does not match the requested profile; the operation is marked as a conflict",
            ));
        }
        ProfileTransition::After => {}
        ProfileTransition::Before | ProfileTransition::Pending => {
            return Err(ServiceError::command_failed(
                "fresh NVIDIA profile observations do not uniquely prove the requested after-state; the operation remains pending",
            ));
        }
    }
    let identity_json = serde_json::to_string(&observed.identity)
        .map_err(|error| ServiceError::command_failed(error.to_string()))?;
    let witness_json = observed.witness_json;
    let composition = observed.composition;
    context
        .storage()
        .complete_nvapi_profile_creation(NvapiProfileCreationCompletion {
            op_id,
            game_id,
            binding_path: path,
            profile: NvapiVerifiedProfileReceipt {
                profile_name,
                profile_identity_json: &identity_json,
                application_witness_json: &witness_json,
                composition_json: &composition,
            },
        })?;
    Ok(true)
}
