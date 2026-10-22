use renderpilot_nvapi::{
    ApplicationIdentity, DrsSession, NvapiError, Profile, ProfileIdentity, SettingIdentity,
    setting::SettingContext,
};

use super::{
    Context, ProfileTransition, ServiceError, classify_profile_transition, composition_json,
    map_drs, open_drs_session, owned_profile_receipt_matches, read_profile_receipts,
    selected_executable_allows_owned_delete, verify_owned_composition,
};

/// Lookup result for an observed DRS profile entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Lookup<T> {
    Found(T),
    Missing,
    Ambiguous,
    Failed(String),
}

/// Classification of the evaluated profile delete preconditions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeletePlan {
    LiveDelete,
    ConfirmedAbsent,
}

/// Observed snapshot of a DRS profile's identity, applications, settings, and composition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservedProfileReceipt {
    pub identity: ProfileIdentity,
    pub witness: Option<ApplicationIdentity>,
    pub witness_json: Option<String>,
    pub applications: Vec<ApplicationIdentity>,
    pub settings: Vec<SettingIdentity>,
    pub composition: String,
}

/// Combined observation of an owned profile by name and by bound executable path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnedProfileObservation {
    pub by_name: Lookup<ObservedProfileReceipt>,
    pub by_path: Lookup<ObservedProfileReceipt>,
}

/// Internal trait abstracting DRS observation and live deletion of an owned profile.
pub(crate) trait OwnedProfileDeleteDrs {
    fn observe(
        &self,
        owner: &renderpilot_storage_sqlite::NvapiOwnedProfileRow,
    ) -> Result<OwnedProfileObservation, ServiceError>;

    fn execute_live_delete(
        &self,
        context: &Context,
        game_id: &str,
        owner: &renderpilot_storage_sqlite::NvapiOwnedProfileRow,
        observation: &OwnedProfileObservation,
    ) -> Result<(), ServiceError>;
}

/// Production implementation of `OwnedProfileDeleteDrs` using live NVAPI sessions.
pub(crate) struct LiveOwnedProfileDeleteDrs;

impl OwnedProfileDeleteDrs for LiveOwnedProfileDeleteDrs {
    fn observe(
        &self,
        owner: &renderpilot_storage_sqlite::NvapiOwnedProfileRow,
    ) -> Result<OwnedProfileObservation, ServiceError> {
        let session = open_drs_session().map_err(map_drs)?;
        Ok(observe_live_owned_profile(&session, owner))
    }

    fn execute_live_delete(
        &self,
        context: &Context,
        game_id: &str,
        owner: &renderpilot_storage_sqlite::NvapiOwnedProfileRow,
        _observation: &OwnedProfileObservation,
    ) -> Result<(), ServiceError> {
        let session = open_drs_session().map_err(map_drs)?;
        let profile = session
            .find_profile_by_exe(&owner.binding_path)
            .map_err(map_drs)?;
        verify_owned_composition(owner, &profile, game_id)?;
        let before = serde_json::json!({
            "binding_path": owner.binding_path,
            "profile_name": owner.profile_name,
            "composition": owner.composition_json,
            "application_witness": owner.application_witness_json,
        })
        .to_string();
        let after = serde_json::json!({
            "binding_path": owner.binding_path,
            "profile_name": owner.profile_name,
            "profile_absent": true
        })
        .to_string();
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
            if reconcile_delete_operation(context, &op_id, game_id, owner)? {
                return Ok(());
            }
            return Err(map_drs(error));
        }
        let save_result = session.save();
        drop(session);
        if reconcile_delete_operation(context, &op_id, game_id, owner)? {
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
}

fn observed_receipt_from_profile(
    profile: &Profile<'_>,
    require_witness: bool,
) -> Result<ObservedProfileReceipt, ServiceError> {
    let identity = profile.identity().map_err(map_drs)?;
    let (witness, witness_json) = if let Some(witness) = profile.matched_application() {
        let json = serde_json::to_string(&witness)
            .map_err(|error| ServiceError::command_failed(error.to_string()))?;
        (Some(witness), Some(json))
    } else if require_witness {
        return Err(ServiceError::command_failed(
            "full-path lookup returned no application witness",
        ));
    } else {
        (None, None)
    };
    let applications = profile.applications().map_err(map_drs)?;
    let settings = profile.settings().map_err(map_drs)?;
    let composition = composition_json(applications.clone(), settings.clone())?;
    Ok(ObservedProfileReceipt {
        identity,
        witness,
        witness_json,
        applications,
        settings,
        composition,
    })
}

fn observe_live_owned_profile(
    session: &DrsSession<'_>,
    owner: &renderpilot_storage_sqlite::NvapiOwnedProfileRow,
) -> OwnedProfileObservation {
    let by_name = match session.find_profile_by_name(&owner.profile_name) {
        Ok(profile) => match observed_receipt_from_profile(&profile, false) {
            Ok(receipt) => Lookup::Found(receipt),
            Err(err) => Lookup::Failed(err.to_string()),
        },
        Err(NvapiError::ProfileNotFound) => Lookup::Missing,
        Err(err) => Lookup::Failed(err.to_string()),
    };

    let by_path = match session.find_profile_by_exe(&owner.binding_path) {
        Ok(profile) => match observed_receipt_from_profile(&profile, true) {
            Ok(receipt) => Lookup::Found(receipt),
            Err(err) => Lookup::Failed(err.to_string()),
        },
        Err(NvapiError::ExecutableNotFound) => Lookup::Missing,
        Err(NvapiError::ExecutableAmbiguous) => Lookup::Ambiguous,
        Err(err) => Lookup::Failed(err.to_string()),
    };

    OwnedProfileObservation { by_name, by_path }
}

/// Evaluates whether preconditions for deleting an owned profile are met and classifies the delete plan.
pub(crate) fn evaluate_delete_preconditions(
    context: &Context,
    game_id: &str,
    setting_context: &SettingContext,
    owner: &renderpilot_storage_sqlite::NvapiOwnedProfileRow,
    observation: &OwnedProfileObservation,
) -> Result<DeletePlan, ServiceError> {
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
    match context
        .storage()
        .get_nvapi_target_profile_is_predefined(&owner.profile_name)?
    {
        Some(false) => {}
        Some(true) => {
            return Err(ServiceError::command_failed(format!(
                "NVIDIA profile target '{}' is a predefined driver profile; cannot delete as an owned custom profile",
                owner.profile_name
            )));
        }
        None => {
            return Err(ServiceError::command_failed(format!(
                "NVIDIA profile target '{}' is not registered in storage; deletion is blocked",
                owner.profile_name
            )));
        }
    }

    match (&observation.by_name, &observation.by_path) {
        (Lookup::Missing, Lookup::Missing) => Ok(DeletePlan::ConfirmedAbsent),

        (Lookup::Missing, Lookup::Found(receipt)) => {
            if receipt.identity.name == owner.profile_name {
                Err(ServiceError::command_failed(format!(
                    "RenderPilot-owned NVIDIA profile lookup is contradictory: missing by name but found by executable path for game {game_id}; deletion is blocked"
                )))
            } else {
                Err(ServiceError::command_failed(format!(
                    "RenderPilot-owned NVIDIA profile changed since its last confirmation for game {game_id}; deletion is blocked"
                )))
            }
        }

        (Lookup::Found(_), Lookup::Missing) => Err(map_drs(NvapiError::ExecutableNotFound)),

        (Lookup::Failed(msg), _) => Err(ServiceError::command_failed(format!(
            "NVIDIA DRS operation failed: {msg}"
        ))),
        (_, Lookup::Failed(msg)) => Err(ServiceError::command_failed(format!(
            "NVIDIA DRS operation failed: {msg}"
        ))),

        (Lookup::Ambiguous, _) | (_, Lookup::Ambiguous) => {
            Err(map_drs(NvapiError::ExecutableAmbiguous))
        }

        (Lookup::Found(name_receipt), Lookup::Found(path_receipt)) => {
            if name_receipt.identity.name != owner.profile_name {
                return Err(ServiceError::command_failed(format!(
                    "RenderPilot-owned NVIDIA profile changed since its last confirmation for game {game_id}; deletion is blocked"
                )));
            }
            let Some(witness) = &path_receipt.witness else {
                return Err(ServiceError::command_failed(format!(
                    "RenderPilot-owned NVIDIA profile changed since its last confirmation for game {game_id}; deletion is blocked"
                )));
            };
            let Some(witness_json) = &path_receipt.witness_json else {
                return Err(ServiceError::command_failed(format!(
                    "RenderPilot-owned NVIDIA profile changed since its last confirmation for game {game_id}; deletion is blocked"
                )));
            };
            if !owned_profile_receipt_matches(
                owner,
                &path_receipt.identity,
                witness,
                witness_json,
                &path_receipt.composition,
            ) {
                return Err(ServiceError::command_failed(format!(
                    "RenderPilot-owned NVIDIA profile changed since its last confirmation for game {game_id}; deletion is blocked"
                )));
            }
            Ok(DeletePlan::LiveDelete)
        }
    }
}

/// Parameterized deletion flow supporting decoupled DRS observations.
pub(crate) fn delete_owned_profile_with_drs(
    context: &Context,
    game_id: &str,
    setting_context: &SettingContext,
    drs: &impl OwnedProfileDeleteDrs,
) -> Result<(), ServiceError> {
    let owner = context
        .storage()
        .get_nvapi_owned_profile(game_id)?
        .ok_or_else(|| {
            ServiceError::command_failed("this game has no RenderPilot-owned NVIDIA profile")
        })?;
    if owner.state != "active" {
        return Err(ServiceError::command_failed(format!(
            "RenderPilot-owned NVIDIA profile for game {game_id} is in state '{}'; deletion is blocked",
            owner.state
        )));
    }
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
    let observation = drs.observe(&owner)?;
    let plan =
        evaluate_delete_preconditions(context, game_id, setting_context, &owner, &observation)?;
    match plan {
        DeletePlan::LiveDelete => drs.execute_live_delete(context, game_id, &owner, &observation),
        DeletePlan::ConfirmedAbsent => {
            let before = serde_json::json!({
                "binding_path": owner.binding_path,
                "profile_name": owner.profile_name,
                "composition": owner.composition_json,
                "application_witness": owner.application_witness_json,
            })
            .to_string();
            let after = serde_json::json!({
                "binding_path": owner.binding_path,
                "profile_name": owner.profile_name,
                "profile_absent": true,
                "deletion_provenance": "confirmed_absent_v1",
            })
            .to_string();
            let op_id = ulid::Ulid::generate().to_string();
            context.storage().begin_nvapi_operation(
                &op_id,
                Some(game_id),
                Some(&owner.profile_name),
                "delete_profile",
                &before,
                &after,
            )?;

            let fresh = drs.observe(&owner)?;
            match (&fresh.by_name, &fresh.by_path) {
                (Lookup::Missing, Lookup::Missing) => {
                    context.storage().complete_nvapi_profile_deletion(
                        &op_id,
                        game_id,
                        &owner.profile_name,
                    )?;
                    Ok(())
                }
                (Lookup::Found(name_rec), Lookup::Found(path_rec)) => {
                    let matches_recorded = name_rec.identity.name == owner.profile_name
                        && path_rec.witness.as_ref().is_some_and(|w| {
                            path_rec.witness_json.as_ref().is_some_and(|wj| {
                                owned_profile_receipt_matches(
                                    &owner,
                                    &path_rec.identity,
                                    w,
                                    wj,
                                    &path_rec.composition,
                                )
                            })
                        });
                    if matches_recorded {
                        context.storage().cancel_nvapi_operation(&op_id)?;
                        Err(ServiceError::command_failed(
                            "NVIDIA profile reappeared during deletion; operation was canceled and ownership preserved",
                        ))
                    } else {
                        let conflict_json = serde_json::json!({
                            "name_identity": name_rec.identity,
                            "path_identity": path_rec.identity,
                        })
                        .to_string();
                        context
                            .storage()
                            .mark_nvapi_operation_conflict(&op_id, &conflict_json)?;
                        Err(ServiceError::command_failed(
                            "NVIDIA profile changed during deletion; operation was marked as a conflict",
                        ))
                    }
                }
                _ => Err(ServiceError::command_failed(
                    "second DRS observation is uncertain; pending delete_profile operation was preserved for recovery",
                )),
            }
        }
    }
}

/// Deletes only a profile whose complete current composition equals its local ownership receipt.
pub(in crate::nvapi) fn delete_owned_profile(
    context: &Context,
    game_id: &str,
    setting_context: &SettingContext,
) -> Result<(), ServiceError> {
    delete_owned_profile_with_drs(
        context,
        game_id,
        setting_context,
        &LiveOwnedProfileDeleteDrs,
    )
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
