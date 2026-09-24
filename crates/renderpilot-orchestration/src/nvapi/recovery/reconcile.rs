use renderpilot_application::{AppError, AppResult};
use renderpilot_storage_sqlite::{
    NvapiPendingOperationRow, NvapiProfileCreationCompletion, NvapiProfileMoveCompletion,
    NvapiSettingOperationCompletion, NvapiSettingOperationScope, NvapiSettingState,
    NvapiVerifiedProfileReceipt, SqliteStorage,
};
use serde_json::Value;

use super::receipts::{
    composition_json, composition_matches_except_setting, created_profile_matches, found_path,
    parse_snapshot, path_equal, path_lookup, profile_receipts, setting_state,
    settings_match_before, string, unsigned, witness_for_path, witness_json_for_path,
};
use super::{Lookup, Observation, ObservedProfile};
pub(super) fn reconcile_row(
    storage: &SqliteStorage,
    row: &NvapiPendingOperationRow,
    observation: &Observation,
) -> AppResult<()> {
    let before = match parse_snapshot(&row.before_json) {
        Ok(value) => value,
        Err(error) => {
            return mark_conflict(storage, row, &format!("invalid before receipt: {error}"));
        }
    };
    let after = match parse_snapshot(&row.after_json) {
        Ok(value) => value,
        Err(error) => {
            return mark_conflict(storage, row, &format!("invalid after receipt: {error}"));
        }
    };
    match row.kind.as_str() {
        "create_profile" => reconcile_create(storage, row, &before, &after, observation),
        "delete_profile" => reconcile_delete(storage, row, &before, observation),
        "move_profile" => reconcile_move(storage, row, &before, &after, observation),
        "setting" => reconcile_setting(storage, row, &before, &after, observation),
        other => mark_conflict(storage, row, &format!("unknown operation kind `{other}`")),
    }
}

fn reconcile_create(
    storage: &SqliteStorage,
    row: &NvapiPendingOperationRow,
    before: &Value,
    after: &Value,
    observation: &Observation,
) -> AppResult<()> {
    let (Some(game_id), Some(profile_name), Some(path)) = (
        row.game_id.as_deref(),
        row.target_id.as_deref(),
        string(after, "application_path").or_else(|| string(before, "binding_path")),
    ) else {
        return mark_conflict(
            storage,
            row,
            "create receipt is missing its game, profile, or path",
        );
    };
    match (&observation.named_profile, path_lookup(observation, path)) {
        (Lookup::Missing, Lookup::Missing) => storage.cancel_nvapi_operation(&row.op_id),
        (Lookup::Found(named), Lookup::Found(by_path))
            if created_profile_matches(named, by_path, profile_name, path) =>
        {
            let witness = witness_for_path(by_path, path);
            let Some(witness) = witness else {
                return mark_conflict(
                    storage,
                    row,
                    "created profile has no exact full-path application witness",
                );
            };
            let (identity_json, witness_json, composition) = profile_receipts(named, witness)?;
            storage.complete_nvapi_profile_creation(NvapiProfileCreationCompletion {
                op_id: &row.op_id,
                game_id,
                binding_path: path,
                profile: NvapiVerifiedProfileReceipt {
                    profile_name,
                    profile_identity_json: &identity_json,
                    application_witness_json: &witness_json,
                    composition_json: &composition,
                },
            })
        }
        _ => classify_unresolved(storage, row, observation, "create_profile"),
    }
}

fn reconcile_delete(
    storage: &SqliteStorage,
    row: &NvapiPendingOperationRow,
    before: &Value,
    observation: &Observation,
) -> AppResult<()> {
    let (
        Some(game_id),
        Some(profile_name),
        Some(path),
        Some(expected_composition),
        Some(expected_witness),
    ) = (
        row.game_id.as_deref(),
        row.target_id.as_deref(),
        string(before, "binding_path"),
        string(before, "composition"),
        string(before, "application_witness"),
    )
    else {
        return mark_conflict(storage, row, "delete receipt is incomplete");
    };
    if matches!(observation.named_profile, Lookup::Missing) {
        // The exact RenderPilot profile identity is absent. The path can now
        // resolve to a separate NVIDIA profile, so it is deliberately ignored.
        return storage.complete_nvapi_profile_deletion(&row.op_id, game_id, profile_name);
    }
    let (Lookup::Found(named), Lookup::Found(by_path)) =
        (&observation.named_profile, path_lookup(observation, path))
    else {
        return classify_unresolved(storage, row, observation, "delete_profile");
    };
    if named.identity.name == profile_name
        && !named.identity.is_predefined
        && by_path.identity.name == profile_name
        && witness_json_for_path(by_path, path).as_deref() == Some(expected_witness)
        && composition_json(named).as_deref() == Some(expected_composition)
    {
        storage.cancel_nvapi_operation(&row.op_id)
    } else {
        mark_conflict(
            storage,
            row,
            "observed profile differs from the exact owned delete before-state",
        )
    }
}

fn reconcile_move(
    storage: &SqliteStorage,
    row: &NvapiPendingOperationRow,
    before: &Value,
    after: &Value,
    observation: &Observation,
) -> AppResult<()> {
    let (
        Some(game_id),
        Some(profile_name),
        Some(old_path),
        Some(new_path),
        Some(old_composition),
        Some(old_witness),
    ) = (
        row.game_id.as_deref(),
        row.target_id.as_deref(),
        string(before, "old_path"),
        string(after, "new_path"),
        string(before, "composition"),
        string(before, "application_witness"),
    )
    else {
        return mark_conflict(storage, row, "move receipt is incomplete");
    };
    let Some(select_automatically) = after.get("select_automatically").and_then(Value::as_bool)
    else {
        return mark_conflict(
            storage,
            row,
            "move receipt is missing executable-selection intent",
        );
    };
    let basename = string(after, "new_basename").or_else(|| {
        std::path::Path::new(new_path)
            .file_name()
            .and_then(|name| name.to_str())
    });
    let Some(basename) = basename else {
        return mark_conflict(
            storage,
            row,
            "move receipt has no destination executable basename",
        );
    };
    let named = match &observation.named_profile {
        Lookup::Found(profile)
            if profile.identity.name == profile_name && !profile.identity.is_predefined =>
        {
            profile
        }
        _ => return classify_unresolved(storage, row, observation, "move_profile"),
    };
    let old_path_is_exact = matches!(
        path_lookup(observation, old_path),
        Lookup::Found(by_old_path)
            if by_old_path.identity == named.identity
                && witness_json_for_path(by_old_path, old_path).as_deref() == Some(old_witness)
    );
    let old_is_exact = named.applications.len() == 1
        && path_equal(&named.applications[0].app_name, old_path)
        && composition_json(named).as_deref() == Some(old_composition)
        && matches!(path_lookup(observation, new_path), Lookup::Missing)
        && old_path_is_exact;
    if old_is_exact {
        return storage.cancel_nvapi_operation(&row.op_id);
    }
    let new_is_exact = named.applications.len() == 1
        && path_equal(&named.applications[0].app_name, new_path)
        && settings_match_before(named, before)
        && matches!(path_lookup(observation, new_path), Lookup::Found(profile)
            if profile.identity.name == profile_name
                && witness_for_path(profile, new_path).is_some());
    if new_is_exact {
        match path_lookup(observation, old_path) {
            Lookup::Missing => {}
            Lookup::Found(profile) if profile.identity.name == profile_name => {
                return mark_conflict(
                    storage,
                    row,
                    "moved profile still resolves from its old executable path",
                );
            }
            Lookup::Found(_) => {
                return mark_conflict(
                    storage,
                    row,
                    "old executable path unexpectedly resolves to another profile",
                );
            }
            Lookup::Ambiguous | Lookup::Failed(_) => {
                // The destination is proven, but the old-path after-state is
                // not. Keep the intent so a later fresh session can decide.
                return Ok(());
            }
        }
        let Some(by_path) = found_path(observation, new_path) else {
            return mark_conflict(
                storage,
                row,
                "moved profile lost its exact destination application witness",
            );
        };
        let Some(witness) = witness_for_path(by_path, new_path) else {
            return mark_conflict(
                storage,
                row,
                "moved profile has no exact destination application witness",
            );
        };
        let (identity_json, witness_json, composition) = profile_receipts(named, witness)?;
        return storage.complete_nvapi_profile_move(NvapiProfileMoveCompletion {
            op_id: &row.op_id,
            game_id,
            binding_path: new_path,
            binding_basename: basename,
            select_automatically,
            profile: NvapiVerifiedProfileReceipt {
                profile_name,
                profile_identity_json: &identity_json,
                application_witness_json: &witness_json,
                composition_json: &composition,
            },
        });
    }
    classify_unresolved(storage, row, observation, "move_profile")
}

fn reconcile_setting(
    storage: &SqliteStorage,
    row: &NvapiPendingOperationRow,
    before: &Value,
    after: &Value,
    observation: &Observation,
) -> AppResult<()> {
    let (Some(target_id), Some(setting_id), Some(before_state), Some(after_state)) = (
        row.target_id.as_deref(),
        unsigned(before, "setting_id"),
        setting_state(before),
        setting_state(after),
    ) else {
        return mark_conflict(storage, row, "setting receipt is incomplete");
    };
    let (profile, observed) = if row.game_id.is_none() {
        let Lookup::Found(profile) = &observation.base_profile else {
            return classify_unresolved(storage, row, observation, "global setting");
        };
        let Some((observed_id, present, value)) = profile.explicit_setting else {
            return mark_conflict(
                storage,
                row,
                "global setting observation omitted its explicit value",
            );
        };
        if observed_id != setting_id || profile.identity.name != target_id {
            return mark_conflict(
                storage,
                row,
                "global DRS identity changed during pending setting recovery",
            );
        }
        (profile, (present, value))
    } else {
        let Some(path) = string(before, "executable_path") else {
            return mark_conflict(
                storage,
                row,
                "game setting receipt has no selected full-path executable",
            );
        };
        let Lookup::Found(profile) = path_lookup(observation, path) else {
            return classify_unresolved(storage, row, observation, "game setting");
        };
        let Some((observed_id, present, value)) = profile.explicit_setting else {
            return mark_conflict(
                storage,
                row,
                "game setting observation omitted its explicit value",
            );
        };
        let recorded_witness = storage.get_nvapi_application_witness(target_id, path)?;
        if observed_id != setting_id
            || profile.identity.name != target_id
            || recorded_witness.as_deref().is_none_or(|recorded| {
                witness_json_for_path(profile, path).as_deref() != Some(recorded)
            })
        {
            return mark_conflict(
                storage,
                row,
                "game profile or full-path witness changed during pending setting recovery",
            );
        }
        (profile, (present, value))
    };
    if let Some(game_id) = row.game_id.as_deref()
        && storage
            .get_nvapi_owned_profile_by_name(target_id)?
            .is_some_and(|owner| owner.game_id != game_id)
    {
        return mark_conflict(storage, row, "a different game owns this NVIDIA profile");
    }
    if observed == before_state {
        if let Some(owner) = storage.get_nvapi_owned_profile_by_name(target_id)?
            && composition_json(profile).as_deref() != Some(owner.composition_json.as_str())
        {
            return mark_conflict(
                storage,
                row,
                "owned profile changed outside RenderPilot before the pending write",
            );
        }
        return storage.cancel_nvapi_setting_operation(&row.op_id);
    }
    if observed != after_state {
        return mark_conflict(
            storage,
            row,
            "setting value matches neither its durable before-state nor after-state",
        );
    }
    let identity_json = match serde_json::to_string(&profile.identity) {
        Ok(json) => json,
        Err(error) => {
            return mark_conflict(
                storage,
                row,
                &format!("could not encode recovered profile identity: {error}"),
            );
        }
    };
    let composition = if let Some(owner) = storage.get_nvapi_owned_profile_by_name(target_id)? {
        if Some(owner.game_id.as_str()) != row.game_id.as_deref() {
            return mark_conflict(storage, row, "a different game owns this NVIDIA profile");
        }
        if !composition_matches_except_setting(&owner.composition_json, profile, setting_id)? {
            return mark_conflict(
                storage,
                row,
                "owned profile composition changed beyond the pending setting",
            );
        }
        Some(composition_json(profile).ok_or_else(|| {
            AppError::storage_failed("could not encode recovered NVIDIA profile composition")
        })?)
    } else {
        None
    };
    storage.finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
        op_id: &row.op_id,
        scope: match row.game_id.as_deref() {
            Some(game_id) => NvapiSettingOperationScope::Game { game_id },
            None => NvapiSettingOperationScope::Global,
        },
        target_id,
        setting_id,
        expected: NvapiSettingState {
            present: after_state.0,
            value: after_state.1,
        },
        profile_identity_json: &identity_json,
        composition_json: composition.as_deref(),
    })
}

fn classify_unresolved(
    storage: &SqliteStorage,
    row: &NvapiPendingOperationRow,
    observation: &Observation,
    label: &str,
) -> AppResult<()> {
    if observation_has_error(observation) {
        // Driver errors and missing NVAPI must not turn uncertainty into a
        // destructive resolution. Keep the original intent for the next entry.
        return Ok(());
    }
    mark_conflict(
        storage,
        row,
        &format!("{label} observation matches neither durable state"),
    )
}

fn observation_has_error(observation: &Observation) -> bool {
    let lookup_error =
        |lookup: &Lookup<ObservedProfile>| matches!(lookup, Lookup::Ambiguous | Lookup::Failed(_));
    lookup_error(&observation.named_profile)
        || lookup_error(&observation.base_profile)
        || observation.path_profiles.values().any(lookup_error)
}

pub(super) fn mark_conflict(
    storage: &SqliteStorage,
    row: &NvapiPendingOperationRow,
    detail: &str,
) -> AppResult<()> {
    let observed = serde_json::json!({"recovery": detail}).to_string();
    storage.mark_nvapi_operation_conflict(&row.op_id, &observed)
}
