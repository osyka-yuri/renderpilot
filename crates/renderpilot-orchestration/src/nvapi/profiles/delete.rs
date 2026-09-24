use renderpilot_nvapi::setting::SettingContext;

use super::{
    Context, NvapiError, ProfileTransition, ServiceError, classify_profile_transition,
    composition_json, map_drs, open_drs_session, owned_profile_receipt_matches,
    read_profile_receipts, selected_executable_allows_owned_delete, verify_owned_composition,
};
/// Deletes only a profile whose complete current composition equals its local ownership receipt.
pub(in crate::nvapi) fn delete_owned_profile(
    context: &Context,
    game_id: &str,
    setting_context: &SettingContext,
) -> Result<(), ServiceError> {
    let owner = context
        .storage()
        .get_nvapi_owned_profile(game_id)?
        .ok_or_else(|| {
            ServiceError::command_failed("this game has no RenderPilot-owned NVIDIA profile")
        })?;
    if !selected_executable_allows_owned_delete(
        setting_context.effective_exe_path.as_deref(),
        &owner.binding_path,
    )? {
        return Err(ServiceError::command_failed(format!(
            "the profile is bound to {}; select that executable before deleting it while it still exists",
            owner.binding_path
        )));
    }
    if context
        .storage()
        .nvapi_target_has_other_game_claims(&owner.profile_name, game_id)?
    {
        return Err(ServiceError::command_failed(
            "another game still references a setting on this profile; deletion is blocked",
        ));
    }
    let session = open_drs_session().map_err(map_drs)?;
    let profile = session
        .find_profile_by_exe(&owner.binding_path)
        .map_err(map_drs)?;
    verify_owned_composition(&owner, &profile, game_id)?;
    let before = serde_json::json!({
        "binding_path": owner.binding_path,
        "profile_name": owner.profile_name,
        "composition": owner.composition_json,
        "application_witness": owner.application_witness_json,
    })
    .to_string();
    let after = serde_json::json!({"binding_path": owner.binding_path, "profile_name": owner.profile_name, "profile_absent": true}).to_string();
    let op_id = ulid::Ulid::generate().to_string();
    context.storage().begin_nvapi_operation(
        &op_id,
        Some(game_id),
        Some(&owner.profile_name),
        "delete_profile",
        &before,
        &after,
    )?;
    let delete_result = profile.delete_user_profile();
    if let Err(error) = delete_result {
        drop(session);
        if reconcile_delete_operation(context, &op_id, game_id, &owner)? {
            return Ok(());
        }
        return Err(map_drs(error));
    }
    let save_result = session.save();
    drop(session);
    if reconcile_delete_operation(context, &op_id, game_id, &owner)? {
        return Ok(());
    }
    if let Err(error) = save_result {
        return Err(ServiceError::command_failed(format!(
            "NVIDIA profile deletion was not persisted; the exact owned profile remains and the journal was canceled: {error}"
        )));
    }
    Err(ServiceError::command_failed(
        "NVIDIA profile remains in the fresh driver session; the journal was canceled",
    ))
}

fn reconcile_delete_operation(
    context: &Context,
    op_id: &str,
    game_id: &str,
    owner: &renderpilot_storage_sqlite::NvapiOwnedProfileRow,
) -> Result<bool, ServiceError> {
    let session = open_drs_session().map_err(map_drs)?;
    let by_name = match session.find_profile_by_name(&owner.profile_name) {
        Err(NvapiError::ProfileNotFound) => {
            // The exact owned profile identity is gone. DRS may now expose an
            // unrelated predefined or external profile for the same path.
            return match classify_profile_transition(Some(false), Some(true)) {
                ProfileTransition::After => {
                    context.storage().complete_nvapi_profile_deletion(
                        op_id,
                        game_id,
                        &owner.profile_name,
                    )?;
                    Ok(true)
                }
                _ => unreachable!("missing owned profile proves delete after-state"),
            };
        }
        Ok(profile) => profile,
        Err(error) => return Err(map_drs(error)),
    };
    let named_identity = by_name.identity().map_err(map_drs)?;
    if named_identity.name != owner.profile_name || named_identity.is_predefined {
        let observed = serde_json::json!({"identity": named_identity}).to_string();
        return match classify_profile_transition(Some(false), Some(false)) {
            ProfileTransition::Conflict => {
                context
                    .storage()
                    .mark_nvapi_operation_conflict(op_id, &observed)?;
                Err(ServiceError::command_failed(
                    "the remaining NVIDIA profile has a different identity; the operation is marked as a conflict",
                ))
            }
            _ => unreachable!("successful contradictory profile identity proves a conflict"),
        };
    }
    let path_profile = match session.find_profile_by_exe(&owner.binding_path) {
        Ok(profile) => Some(profile),
        Err(NvapiError::ExecutableNotFound) => None,
        Err(error) => return Err(map_drs(error)),
    };
    let Some(path_profile) = path_profile else {
        let applications = by_name.applications().map_err(map_drs)?;
        let settings = by_name.settings().map_err(map_drs)?;
        let composition = composition_json(applications.clone(), settings.clone())?;
        let enum_is_exact_before = composition == owner.composition_json
            && applications.len() == 1
            && renderpilot_domain::normalized_path_key(&applications[0].app_name)
                == renderpilot_domain::normalized_path_key(&owner.binding_path);
        if enum_is_exact_before {
            return match classify_profile_transition(None, None) {
                ProfileTransition::Pending => Err(ServiceError::command_failed(
                    "the owned profile composition still matches, but full-path lookup remains unresolved; the operation stays pending",
                )),
                _ => unreachable!("missing full-path witness is inconclusive"),
            };
        }
        let observed = serde_json::json!({
            "identity": named_identity,
            "applications": applications,
            "settings": settings,
            "binding_path_lookup": "executable_not_found",
        })
        .to_string();
        return match classify_profile_transition(Some(false), Some(false)) {
            ProfileTransition::Conflict => {
                context
                    .storage()
                    .mark_nvapi_operation_conflict(op_id, &observed)?;
                Err(ServiceError::command_failed(
                    "the NVIDIA profile deletion before-state changed; the operation is marked as a conflict",
                ))
            }
            _ => unreachable!("fully observed changed profile composition proves a conflict"),
        };
    };
    let observed = read_profile_receipts(&path_profile)?;
    let exact_before = owned_profile_receipt_matches(
        owner,
        &observed.identity,
        &observed.witness,
        &observed.witness_json,
        &observed.composition,
    );
    if classify_profile_transition(Some(exact_before), Some(false)) == ProfileTransition::Before {
        context.storage().cancel_nvapi_operation(op_id)?;
        return Ok(false);
    }
    let conflict_observation = serde_json::json!({
        "identity": observed.identity,
        "witness": observed.witness,
        "applications": observed.applications,
        "settings": observed.settings,
    })
    .to_string();
    context
        .storage()
        .mark_nvapi_operation_conflict(op_id, &conflict_observation)?;
    Err(ServiceError::command_failed(
        "the NVIDIA profile deletion before-state changed; the operation is marked as a conflict",
    ))
}
