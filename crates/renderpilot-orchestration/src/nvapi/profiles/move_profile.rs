use renderpilot_domain::GameId;
use renderpilot_nvapi::NvapiError;
use renderpilot_storage_sqlite::{NvapiProfileMoveCompletion, NvapiVerifiedProfileReceipt};

use super::{
    Context, ProfileTransition, ServiceError, classify_profile_transition, composition_json,
    map_drs, open_drs_session, validate_game_executable, verify_owned_composition,
};
/// Moves the single profile binding after the caller has shown and confirmed both full paths.
pub(in crate::nvapi) fn move_owned_profile(
    context: &Context,
    game_id: &str,
    new_path: &str,
    select_automatically: bool,
) -> Result<(), ServiceError> {
    let parsed =
        GameId::new(game_id).map_err(|_| ServiceError::GameNotFound(game_id.to_owned()))?;
    let owner = context
        .storage()
        .get_nvapi_owned_profile(game_id)?
        .ok_or_else(|| {
            ServiceError::command_failed(
                "this game has no RenderPilot-owned NVIDIA profile to move",
            )
        })?;
    let (canonical_new, basename) = validate_game_executable(context, game_id, new_path)?;
    let new_path = canonical_new.to_string_lossy().replace('\\', "/");
    if renderpilot_domain::normalized_path_key(&owner.binding_path)
        == renderpilot_domain::normalized_path_key(&new_path)
    {
        return Ok(());
    }
    crate::nvapi::resolve::require_d3d12_executable_binding(context, &parsed, Some(&new_path))?;
    if context
        .storage()
        .get_nvapi_owned_profile_by_binding_path(&new_path)?
        .is_some_and(|other| other.game_id != game_id)
    {
        return Err(ServiceError::command_failed(
            "another RenderPilot-owned profile is already bound to the destination executable",
        ));
    }
    if context
        .storage()
        .nvapi_target_has_other_game_claims(&owner.profile_name, game_id)?
    {
        return Err(ServiceError::command_failed(
            "another game references this profile's settings; the profile cannot move safely",
        ));
    }
    let session = open_drs_session().map_err(map_drs)?;
    match session.find_profile_by_exe(&new_path) {
        Err(NvapiError::ExecutableNotFound) => {}
        Err(error) => return Err(map_drs(error)),
        Ok(_) => {
            return Err(ServiceError::command_failed(
                "the destination executable already resolves to an NVIDIA profile; choose a path without a DRS binding",
            ));
        }
    }
    let profile = session
        .find_profile_by_exe(&owner.binding_path)
        .map_err(map_drs)?;
    verify_owned_composition(&owner, &profile, game_id)?;
    let old_witness = profile.matched_application().ok_or_else(|| {
        ServiceError::command_failed("full-path lookup returned no application witness")
    })?;
    let before = serde_json::json!({
        "old_path": owner.binding_path,
        "new_path": new_path,
        "profile_name": owner.profile_name,
        "composition": owner.composition_json,
        "application_witness": owner.application_witness_json,
    })
    .to_string();
    let after = serde_json::json!({
        "old_path": owner.binding_path,
        "new_path": new_path,
        "new_basename": basename,
        "select_automatically": select_automatically,
        "profile_name": owner.profile_name,
    })
    .to_string();
    let op_id = ulid::Ulid::generate().to_string();
    context.storage().begin_nvapi_operation(
        &op_id,
        Some(game_id),
        Some(&owner.profile_name),
        "move_profile",
        &before,
        &after,
    )?;
    if let Err(error) = profile.delete_application_exact(&old_witness) {
        drop(session);
        if reconcile_move_operation(
            context,
            &op_id,
            game_id,
            &owner,
            &new_path,
            &basename,
            select_automatically,
        )? {
            return Ok(());
        }
        return Err(map_drs(error));
    }
    if let Err(error) = profile.create_application(&new_path) {
        drop(session);
        if reconcile_move_operation(
            context,
            &op_id,
            game_id,
            &owner,
            &new_path,
            &basename,
            select_automatically,
        )? {
            return Ok(());
        }
        return Err(map_drs(error));
    }
    let save_result = session.save();
    drop(session);
    if reconcile_move_operation(
        context,
        &op_id,
        game_id,
        &owner,
        &new_path,
        &basename,
        select_automatically,
    )? {
        return Ok(());
    }
    if let Err(error) = save_result {
        return Err(ServiceError::command_failed(format!(
            "NVIDIA executable move did not persist; the exact old binding was verified and the journal was canceled: {error}"
        )));
    }
    Err(ServiceError::command_failed(
        "NVIDIA executable move was not present in a fresh driver session; the journal was canceled",
    ))
}

fn reconcile_move_operation(
    context: &Context,
    op_id: &str,
    game_id: &str,
    owner: &renderpilot_storage_sqlite::NvapiOwnedProfileRow,
    new_path: &str,
    basename: &str,
    select_automatically: bool,
) -> Result<bool, ServiceError> {
    let session = open_drs_session().map_err(map_drs)?;
    let named = match session.find_profile_by_name(&owner.profile_name) {
        Ok(profile) => profile,
        Err(NvapiError::ProfileNotFound) => {
            let transition = classify_profile_transition(Some(false), Some(false));
            let observed = serde_json::json!({"profile_lookup": "profile_not_found"}).to_string();
            return match transition {
                ProfileTransition::Conflict => {
                    context
                        .storage()
                        .mark_nvapi_operation_conflict(op_id, &observed)?;
                    Err(ServiceError::command_failed(
                        "the moved NVIDIA profile is absent; the operation is marked as a conflict",
                    ))
                }
                _ => unreachable!("absent move profile contradicts both recorded states"),
            };
        }
        Err(error) => return Err(map_drs(error)),
    };
    let current_identity = named.identity().map_err(map_drs)?;
    if current_identity.name != owner.profile_name || current_identity.is_predefined {
        let observed = serde_json::json!({"identity": current_identity}).to_string();
        context
            .storage()
            .mark_nvapi_operation_conflict(op_id, &observed)?;
        return Err(ServiceError::command_failed(
            "the NVIDIA profile identity changed during move; the operation is marked as a conflict",
        ));
    }
    let apps = named.applications().map_err(map_drs)?;
    let settings = named.settings().map_err(map_drs)?;
    let expected_composition = serde_json::from_str::<serde_json::Value>(&owner.composition_json)
        .map_err(|error| {
        ServiceError::command_failed(format!(
            "the recorded NVIDIA composition is invalid: {error}"
        ))
    })?;
    let expected_settings = expected_composition
        .get("settings")
        .filter(|value| value.is_array())
        .ok_or_else(|| {
            ServiceError::command_failed("the recorded NVIDIA composition has no settings array")
        })?;
    let observed_settings = serde_json::to_value(&settings).map_err(|error| {
        ServiceError::command_failed(format!(
            "could not encode observed NVIDIA settings: {error}"
        ))
    })?;
    let settings_unchanged = expected_settings == &observed_settings;

    let destination = match session.find_profile_by_exe(new_path) {
        Ok(profile) => Some(profile),
        Err(NvapiError::ExecutableNotFound) => None,
        Err(error) => return Err(map_drs(error)),
    };
    let source = match session.find_profile_by_exe(&owner.binding_path) {
        Ok(profile) => {
            let identity = profile.identity().map_err(map_drs)?;
            let witness = profile.matched_application().ok_or_else(|| {
                ServiceError::command_failed(
                    "fresh NVIDIA session returned no source application witness",
                )
            })?;
            Some((identity, witness))
        }
        Err(NvapiError::ExecutableNotFound) => None,
        Err(error) => return Err(map_drs(error)),
    };
    let destination = match destination {
        Some(profile) => {
            let identity = profile.identity().map_err(map_drs)?;
            let witness = profile.matched_application().ok_or_else(|| {
                ServiceError::command_failed(
                    "fresh NVIDIA session returned no destination application witness",
                )
            })?;
            Some((identity, witness))
        }
        None => None,
    };

    let observed_composition = composition_json(apps.clone(), settings.clone())?;
    let source_matches_owner = match source.as_ref() {
        Some((identity, witness)) => {
            let witness_json = serde_json::to_string(witness)
                .map_err(|error| ServiceError::command_failed(error.to_string()))?;
            identity.name == owner.profile_name
                && !identity.is_predefined
                && renderpilot_domain::normalized_path_key(&witness.app_name)
                    == renderpilot_domain::normalized_path_key(&owner.binding_path)
                && witness_json == owner.application_witness_json
        }
        None => false,
    };
    let before = apps.len() == 1
        && renderpilot_domain::normalized_path_key(&apps[0].app_name)
            == renderpilot_domain::normalized_path_key(&owner.binding_path)
        && observed_composition == owner.composition_json
        && source_matches_owner
        && destination.is_none();
    match classify_profile_transition(Some(before), Some(false)) {
        ProfileTransition::Before => {
            context.storage().cancel_nvapi_operation(op_id)?;
            return Ok(false);
        }
        ProfileTransition::After | ProfileTransition::Conflict => {}
        ProfileTransition::Pending => {
            return Err(ServiceError::command_failed(
                "the NVIDIA move observations do not uniquely prove a state; the operation remains pending",
            ));
        }
    }

    let destination_matches_after = destination.as_ref().is_some_and(|(identity, witness)| {
        identity.name == owner.profile_name
            && !identity.is_predefined
            && renderpilot_domain::normalized_path_key(&witness.app_name)
                == renderpilot_domain::normalized_path_key(new_path)
    });
    let source_still_points_to_owned = source
        .as_ref()
        .is_some_and(|(identity, _)| identity.name == owner.profile_name);
    let after = destination_matches_after
        && !source_still_points_to_owned
        && apps.len() == 1
        && renderpilot_domain::normalized_path_key(&apps[0].app_name)
            == renderpilot_domain::normalized_path_key(new_path)
        && settings_unchanged;
    if classify_profile_transition(Some(false), Some(after)) == ProfileTransition::After {
        let (identity, witness) = destination.expect("after-state requires destination witness");
        let identity_json = serde_json::to_string(&identity)
            .map_err(|error| ServiceError::command_failed(error.to_string()))?;
        let witness_json = serde_json::to_string(&witness)
            .map_err(|error| ServiceError::command_failed(error.to_string()))?;
        let composition = composition_json(apps, settings)?;
        context
            .storage()
            .complete_nvapi_profile_move(NvapiProfileMoveCompletion {
                op_id,
                game_id,
                binding_path: new_path,
                binding_basename: basename,
                select_automatically,
                profile: NvapiVerifiedProfileReceipt {
                    profile_name: &owner.profile_name,
                    profile_identity_json: &identity_json,
                    application_witness_json: &witness_json,
                    composition_json: &composition,
                },
            })?;
        return Ok(true);
    }
    let observed = serde_json::json!({
        "applications": apps,
        "settings": settings,
        "source_profile": source.as_ref().map(|(identity, witness)| {
            serde_json::json!({"identity": identity, "witness": witness})
        }),
        "destination_profile": destination.as_ref().map(|(identity, witness)| {
            serde_json::json!({"identity": identity, "witness": witness})
        }),
        "old_path": owner.binding_path,
        "new_path": new_path,
    })
    .to_string();
    context
        .storage()
        .mark_nvapi_operation_conflict(op_id, &observed)?;
    Err(ServiceError::command_failed(
        "the NVIDIA executable move matches neither its exact before-state nor expected after-state; the operation is marked as a conflict",
    ))
}
