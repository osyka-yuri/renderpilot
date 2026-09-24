use renderpilot_nvapi::setting::SettingContext;

use super::{
    Context, NvapiError, NvapiProfileState, NvapiProfileStatusDto, ServiceError, composition_json,
    has_pending_for_path, map_drs, matching_pending, open_drs_session, pending_operation_detail,
    selected_executable_allows_owned_delete, verify_owned_composition,
};
/// Reads the live selected executable's DRS status without creating or changing anything.
pub(in crate::nvapi) fn profile_status(
    context: &Context,
    game_id: &str,
    setting_context: &SettingContext,
) -> Result<NvapiProfileStatusDto, ServiceError> {
    let owned = context.storage().get_nvapi_owned_profile(game_id)?;
    let path = setting_context.effective_exe_path.as_deref();
    let mut result = NvapiProfileStatusDto::new(
        path.map(str::to_owned),
        owned.as_ref().map(|row| row.binding_path.clone()),
    );
    result.owned_by_this_game = owned.is_some();
    let Some(path) = path else {
        result.state = NvapiProfileState::NoExecutable;
        result.detail = Some("Select a game executable to inspect its NVIDIA profile.".to_owned());
        if let Some(owner) = owned.as_ref() {
            inspect_owned_binding_status(context, game_id, owner, &mut result)?;
        }
        return Ok(result);
    };

    if let Some(owner) = owned.as_ref()
        && renderpilot_domain::normalized_path_key(&owner.binding_path)
            != renderpilot_domain::normalized_path_key(path)
        && selected_executable_allows_owned_delete(Some(path), &owner.binding_path)?
    {
        inspect_owned_binding_status(context, game_id, owner, &mut result)?;
        return Ok(result);
    }

    let session = match open_drs_session() {
        Ok(session) => session,
        Err(error) => {
            result.state = NvapiProfileState::NvapiUnavailable;
            result.detail = Some(error.to_string());
            return Ok(result);
        }
    };
    match session.find_profile_by_exe(path) {
        Err(NvapiError::ExecutableNotFound) => {
            if let Some(pending) = matching_pending(context, None, path)? {
                result.state = NvapiProfileState::Pending;
                let detail = pending_operation_detail(pending.game_id.as_deref(), game_id);
                result.pending_operation = Some(pending.kind);
                result.pending_operation_game_id = pending.game_id;
                result.detail = Some(detail);
            } else if let Some(path_owner) = context
                .storage()
                .get_nvapi_owned_profile_by_binding_path(path)?
            {
                result.state = if path_owner.game_id == game_id {
                    NvapiProfileState::Conflict
                } else {
                    NvapiProfileState::OwnedByAnotherGame
                };
                result.detail = Some(format!(
                    "This executable path is already recorded for RenderPilot profile {} (game {}).",
                    path_owner.profile_name, path_owner.game_id
                ));
            } else {
                result.state = if owned.is_some() {
                    NvapiProfileState::Conflict
                } else {
                    NvapiProfileState::Missing
                };
                result.can_create = result.state == NvapiProfileState::Missing && owned.is_none();
                if !result.can_create {
                    result.detail = Some(if owned.is_some() {
                        "The owned profile receipt exists, but its executable binding no longer resolves. Select the recorded executable or move the profile explicitly.".to_owned()
                    } else {
                        "A pending NVIDIA operation must be reconciled before creating a profile."
                            .to_owned()
                    });
                }
            }
        }
        Err(NvapiError::ExecutableAmbiguous) => {
            result.state = NvapiProfileState::Ambiguous;
            result.detail = Some("NVIDIA reports more than one application for this executable path. No profile can be changed safely.".to_owned());
        }
        Err(error) => {
            result.state = NvapiProfileState::Error;
            result.detail = Some(error.to_string());
        }
        Ok(profile) => {
            let identity = profile.identity().map_err(map_drs)?;
            result.profile_name = Some(identity.name.clone());
            result.is_predefined = Some(identity.is_predefined);
            let witness = profile.matched_application().ok_or_else(|| {
                ServiceError::command_failed(
                    "full-path NVIDIA lookup returned no application witness",
                )
            })?;
            let witness_json = serde_json::to_string(&witness)
                .map_err(|error| ServiceError::command_failed(error.to_string()))?;
            let owner_by_name = context
                .storage()
                .get_nvapi_owned_profile_by_name(&identity.name)?;
            result.owned_by_this_game = owned.is_some();
            result.state = if identity.is_predefined {
                NvapiProfileState::Predefined
            } else {
                NvapiProfileState::External
            };
            if let Some(owner) = owner_by_name {
                result.binding_path = Some(owner.binding_path.clone());
                if owner.game_id != game_id {
                    result.state = NvapiProfileState::OwnedByAnotherGame;
                    result.detail = Some(format!(
                        "This profile is managed by another catalog game ({}).",
                        owner.game_id
                    ));
                } else {
                    let apps = profile.applications().map_err(map_drs)?;
                    let settings = profile.settings().map_err(map_drs)?;
                    let composition = composition_json(apps, settings)?;
                    if owner.state == "conflict"
                        || renderpilot_domain::normalized_path_key(&owner.binding_path)
                            != renderpilot_domain::normalized_path_key(path)
                        || owner.application_witness_json != witness_json
                        || owner.composition_json != composition
                    {
                        result.state = NvapiProfileState::Conflict;
                        result.detail = Some("The owned NVIDIA profile or its executable binding changed outside RenderPilot. Deletion is disabled until the observed state is reviewed.".to_owned());
                    } else {
                        result.state = NvapiProfileState::Owned;
                        result.can_delete = !identity.is_predefined
                            && !has_pending_for_path(context, Some(&identity.name), path)?
                            && !context
                                .storage()
                                .nvapi_target_has_other_game_claims(&identity.name, game_id)?;
                    }
                }
            } else {
                result.can_delete = false;
                if owned.as_ref().is_some_and(|owner| {
                    owner.profile_name != identity.name
                        || renderpilot_domain::normalized_path_key(&owner.binding_path)
                            != renderpilot_domain::normalized_path_key(path)
                }) {
                    result.state = NvapiProfileState::Conflict;
                    result.detail = Some("The selected executable no longer resolves to this game's recorded NVIDIA profile. Select the recorded executable or move the profile explicitly.".to_owned());
                }
            }
            if let Some(pending) = matching_pending(context, Some(&identity.name), path)? {
                result.state = NvapiProfileState::Pending;
                result.can_create = false;
                result.can_delete = false;
                let detail = pending_operation_detail(pending.game_id.as_deref(), game_id);
                result.pending_operation = Some(pending.kind);
                result.pending_operation_game_id = pending.game_id;
                result.detail = Some(detail);
            }
        }
    }
    Ok(result)
}

fn inspect_owned_binding_status(
    context: &Context,
    game_id: &str,
    owner: &renderpilot_storage_sqlite::NvapiOwnedProfileRow,
    result: &mut NvapiProfileStatusDto,
) -> Result<(), ServiceError> {
    result.binding_path = Some(owner.binding_path.clone());
    result.profile_name = Some(owner.profile_name.clone());
    if let Some(pending) =
        matching_pending(context, Some(&owner.profile_name), &owner.binding_path)?
    {
        result.state = NvapiProfileState::Pending;
        let detail = pending_operation_detail(pending.game_id.as_deref(), game_id);
        result.pending_operation = Some(pending.kind);
        result.pending_operation_game_id = pending.game_id;
        result.detail = Some(detail);
        return Ok(());
    }
    let session = match open_drs_session() {
        Ok(session) => session,
        Err(error) => {
            result.state = NvapiProfileState::NvapiUnavailable;
            result.detail = Some(error.to_string());
            return Ok(());
        }
    };
    let profile = match session.find_profile_by_exe(&owner.binding_path) {
        Ok(profile) => profile,
        Err(NvapiError::ExecutableNotFound) => {
            result.state = NvapiProfileState::Conflict;
            result.detail = Some(
                "The durable profile receipt no longer resolves to its recorded NVIDIA application binding. Deletion is blocked.".to_owned(),
            );
            return Ok(());
        }
        Err(NvapiError::ExecutableAmbiguous) => {
            result.state = NvapiProfileState::Ambiguous;
            result.detail = Some(
                "NVIDIA reports an ambiguous application for the owned profile binding. Deletion is blocked.".to_owned(),
            );
            return Ok(());
        }
        Err(error) => {
            result.state = NvapiProfileState::Error;
            result.detail = Some(error.to_string());
            return Ok(());
        }
    };
    let identity = match verify_owned_composition(owner, &profile, game_id) {
        Ok((identity, _)) => identity,
        Err(error) => {
            result.state = NvapiProfileState::Conflict;
            result.detail = Some(error.to_string());
            return Ok(());
        }
    };
    result.profile_name = Some(identity.name);
    result.is_predefined = Some(identity.is_predefined);
    result.state = NvapiProfileState::Owned;
    result.can_delete = !context
        .storage()
        .nvapi_target_has_other_game_claims(&owner.profile_name, game_id)?
        && !has_pending_for_path(context, Some(&owner.profile_name), &owner.binding_path)?;
    if !result.can_delete {
        result.detail = Some(
            "This owned profile is still referenced by another setting or has an unresolved NVIDIA operation.".to_owned(),
        );
    } else {
        result.detail = Some(format!(
            "The owned profile remains bound to {} and can be removed safely.",
            owner.binding_path
        ));
    }
    Ok(())
}
