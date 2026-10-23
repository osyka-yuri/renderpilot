//! External NVIDIA DRS setting claim observation, evaluation, and release.
//!
//! Owns the lifecycle of external (non-RenderPilot-owned) profile setting claims:
//! preflight classification against live driver state, authorization validation for
//! diverged settings, fail-closed handling for witness/profile mismatches, and safe
//! restoration or drop of tracked references upon game catalog removal.

use renderpilot_nvapi::NvapiError;
use renderpilot_storage_sqlite::{
    NvapiGameSettingPreparation, NvapiSettingOperationCompletion, NvapiSettingOperationScope,
    NvapiSettingState, NvapiTargetSettingClaimRow,
};

use super::session::{map_nvapi_write_error, open_drs_session, warning_to_service_error};
use super::write::{
    application_witness_matches, composition_matches_except_setting, profile_composition,
    setting_application_witness_json, stable_profile_identity_matches,
};
use crate::ServiceError;
use crate::error::NvapiClaimBlockedReason;

/// Explicit user authorization to abandon divergent NVAPI setting claims without restoring them.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NvapiClaimAbandonmentAuthorization {
    /// Game identity whose claims are authorized for abandonment.
    pub game_id: String,
    /// Explicit claim tokens with expected, original, and live driver state.
    pub claims: Vec<NvapiClaimAbandonmentToken>,
}

/// Token authorizing abandonment of a specific divergent setting claim.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NvapiClaimAbandonmentToken {
    /// NVIDIA profile target name or executable identifier.
    pub target_id: String,
    /// Setting identifier.
    pub setting_id: u32,
    /// Whether the setting was expected to be present in DRS.
    pub expected_present: bool,
    /// Expected DWORD value in DRS.
    pub expected_value: Option<u32>,
    /// Whether the setting was originally present before RenderPilot mutated it.
    pub original_present: bool,
    /// Original DWORD value before RenderPilot mutated it.
    pub original_value: Option<u32>,
    /// Live observed presence in DRS during user assessment.
    pub observed_live_present: Option<bool>,
    /// Live observed DWORD value in DRS during user assessment.
    pub observed_live_value: Option<u32>,
}

/// Status of an observed external claim in DRS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ObservedClaimDrs {
    ConfirmedAbsent,
    Verified {
        profile_name: String,
        is_predefined: bool,
        witness_json: String,
        live: (bool, Option<u32>),
    },
    WitnessMismatch {
        detail: String,
        live: Option<(bool, Option<u32>)>,
    },
    FailedOrAmbiguous(String),
}

/// Plan for releasing or restoring an external NVIDIA setting claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClaimReleasePlan {
    SharedReferenceDrop,
    RestoreLiveExpected {
        target_name: String,
        setting_id: u32,
        original_present: bool,
        original_value: Option<u32>,
    },
    AlreadyOriginal,
    ConfirmedAbsent,
    DivergedSetting {
        target_id: String,
        setting_id: u32,
        live: (bool, Option<u32>),
        expected: (bool, Option<u32>),
        original: (bool, Option<u32>),
    },
    WitnessMismatch {
        target_id: String,
        setting_id: u32,
        detail: String,
        live: Option<(bool, Option<u32>)>,
        expected: (bool, Option<u32>),
        original: (bool, Option<u32>),
    },
    FailedOrAmbiguous {
        target_id: String,
        setting_id: u32,
        reason: String,
    },
}

pub(crate) trait ExternalSettingClaimDrs: Send + Sync {
    fn observe_claim(
        &self,
        context: &crate::Context,
        claim: &NvapiTargetSettingClaimRow,
    ) -> Result<ObservedClaimDrs, ServiceError>;

    fn execute_restore(
        &self,
        context: &crate::Context,
        game_id: &str,
        claim: &NvapiTargetSettingClaimRow,
    ) -> Result<(), ServiceError>;
}

pub(crate) struct LiveExternalSettingClaimDrs;

impl ExternalSettingClaimDrs for LiveExternalSettingClaimDrs {
    fn observe_claim(
        &self,
        context: &crate::Context,
        claim: &NvapiTargetSettingClaimRow,
    ) -> Result<ObservedClaimDrs, ServiceError> {
        let session = match open_drs_session() {
            Ok(session) => session,
            Err(err) => return Ok(ObservedClaimDrs::FailedOrAmbiguous(format!("{err:?}"))),
        };

        let by_name = session.find_profile_by_name(&claim.target_id);
        let by_path = session.find_profile_by_exe(&claim.executable_path);

        match (by_name, by_path) {
            (Err(NvapiError::ProfileNotFound), Err(NvapiError::ExecutableNotFound)) => {
                Ok(ObservedClaimDrs::ConfirmedAbsent)
            }
            (Err(NvapiError::ExecutableAmbiguous), _)
            | (_, Err(NvapiError::ExecutableAmbiguous)) => Ok(ObservedClaimDrs::FailedOrAmbiguous(
                "ambiguous DRS profile or application lookup".to_owned(),
            )),
            (Ok(profile_by_name), Ok(profile_by_exe)) => {
                let identity = match profile_by_name.identity() {
                    Ok(id) => id,
                    Err(err) => return Ok(ObservedClaimDrs::FailedOrAmbiguous(err.to_string())),
                };
                let recorded_profile_status = match context
                    .storage()
                    .get_nvapi_target_profile_is_predefined(&claim.target_id)?
                {
                    Some(status) => status,
                    None => {
                        return Ok(ObservedClaimDrs::FailedOrAmbiguous(format!(
                            "NVIDIA claim profile identity receipt is missing for {}",
                            claim.executable_path
                        )));
                    }
                };
                if !stable_profile_identity_matches(
                    &claim.target_id,
                    Some(recorded_profile_status),
                    &identity.name,
                    identity.is_predefined,
                ) {
                    return Ok(ObservedClaimDrs::WitnessMismatch {
                        detail: format!(
                            "NVIDIA claim profile identity changed for {}",
                            claim.executable_path
                        ),
                        live: None,
                    });
                }
                let exe_identity = match profile_by_exe.identity() {
                    Ok(id) => id,
                    Err(err) => return Ok(ObservedClaimDrs::FailedOrAmbiguous(err.to_string())),
                };
                if exe_identity.name != identity.name {
                    return Ok(ObservedClaimDrs::WitnessMismatch {
                        detail: format!(
                            "executable is bound to a different profile: {}",
                            exe_identity.name
                        ),
                        live: None,
                    });
                }
                let witness = match profile_by_exe.matched_application() {
                    Some(w) => w,
                    None => {
                        return Ok(ObservedClaimDrs::WitnessMismatch {
                            detail: "NVIDIA profile lookup returned no application witness"
                                .to_owned(),
                            live: None,
                        });
                    }
                };
                let witness_json = match setting_application_witness_json(Some(&witness))? {
                    Some(w) => w,
                    None => {
                        return Ok(ObservedClaimDrs::WitnessMismatch {
                            detail: "NVIDIA profile lookup returned empty application witness"
                                .to_owned(),
                            live: None,
                        });
                    }
                };
                let recorded_witness = context
                    .storage()
                    .get_nvapi_application_witness(&claim.target_id, &claim.executable_path)?;
                if recorded_witness.as_deref() != Some(&witness_json) {
                    return Ok(ObservedClaimDrs::WitnessMismatch {
                        detail: format!("executable witness changed for {}", claim.executable_path),
                        live: None,
                    });
                }
                let live = match read_explicit_setting(&profile_by_name, claim.setting_id) {
                    Ok(l) => l,
                    Err(err) => return Ok(ObservedClaimDrs::FailedOrAmbiguous(err.to_string())),
                };
                Ok(ObservedClaimDrs::Verified {
                    profile_name: identity.name,
                    is_predefined: identity.is_predefined,
                    witness_json,
                    live,
                })
            }
            (Ok(_), Err(NvapiError::ExecutableNotFound)) => Ok(ObservedClaimDrs::WitnessMismatch {
                detail: format!(
                    "executable binding {} is missing from profile {}",
                    claim.executable_path, claim.target_id
                ),
                live: None,
            }),
            (Err(NvapiError::ProfileNotFound), Ok(profile_by_exe)) => {
                let exe_id = profile_by_exe
                    .identity()
                    .map(|id| id.name)
                    .unwrap_or_default();
                Ok(ObservedClaimDrs::WitnessMismatch {
                    detail: format!(
                        "profile {} missing by name, but executable bound to profile {}",
                        claim.target_id, exe_id
                    ),
                    live: None,
                })
            }
            (Err(err), _) => Ok(ObservedClaimDrs::FailedOrAmbiguous(format!(
                "DRS profile lookup failed: {err}"
            ))),
            (_, Err(err)) => Ok(ObservedClaimDrs::FailedOrAmbiguous(format!(
                "DRS application lookup failed: {err}"
            ))),
        }
    }

    fn execute_restore(
        &self,
        context: &crate::Context,
        game_id: &str,
        claim: &NvapiTargetSettingClaimRow,
    ) -> Result<(), ServiceError> {
        execute_live_restore(context, game_id, claim)
    }
}

#[cfg(test)]
thread_local! {
    static TEST_CLAIM_DRS: std::cell::RefCell<Option<std::sync::Arc<dyn ExternalSettingClaimDrs>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) struct ClaimDrsHookGuard;

#[cfg(test)]
impl Drop for ClaimDrsHookGuard {
    fn drop(&mut self) {
        TEST_CLAIM_DRS.with(|cell| {
            cell.borrow_mut().take();
        });
    }
}

#[cfg(test)]
pub(crate) fn set_test_external_setting_claim_drs(
    drs: std::sync::Arc<dyn ExternalSettingClaimDrs>,
) -> ClaimDrsHookGuard {
    TEST_CLAIM_DRS.with(|cell| {
        *cell.borrow_mut() = Some(drs);
    });
    ClaimDrsHookGuard
}

pub(crate) fn active_external_setting_claim_drs() -> std::sync::Arc<dyn ExternalSettingClaimDrs> {
    #[cfg(test)]
    {
        if let Some(drs) = TEST_CLAIM_DRS.with(|cell| cell.borrow().clone()) {
            return drs;
        }
        std::sync::Arc::new(LiveExternalSettingClaimDrs)
    }
    #[cfg(not(test))]
    {
        std::sync::Arc::new(LiveExternalSettingClaimDrs)
    }
}

pub(crate) fn evaluate_claim_release_plan(
    context: &crate::Context,
    claim: &NvapiTargetSettingClaimRow,
    drs: &dyn ExternalSettingClaimDrs,
) -> Result<ClaimReleasePlan, ServiceError> {
    let refs = context
        .storage()
        .count_nvapi_setting_claim_refs(&claim.target_id, claim.setting_id)?;
    if refs > 1 {
        return Ok(ClaimReleasePlan::SharedReferenceDrop);
    }

    let observed = drs.observe_claim(context, claim)?;
    match observed {
        ObservedClaimDrs::ConfirmedAbsent => Ok(ClaimReleasePlan::ConfirmedAbsent),
        ObservedClaimDrs::FailedOrAmbiguous(reason) => Ok(ClaimReleasePlan::FailedOrAmbiguous {
            target_id: claim.target_id.clone(),
            setting_id: claim.setting_id,
            reason,
        }),
        ObservedClaimDrs::WitnessMismatch { detail, live } => {
            Ok(ClaimReleasePlan::WitnessMismatch {
                target_id: claim.target_id.clone(),
                setting_id: claim.setting_id,
                detail,
                live,
                expected: (claim.expected_present, claim.expected_value),
                original: (claim.original_present, claim.original_value),
            })
        }
        ObservedClaimDrs::Verified { live, .. } => {
            if live == (claim.expected_present, claim.expected_value) {
                Ok(ClaimReleasePlan::RestoreLiveExpected {
                    target_name: claim.target_id.clone(),
                    setting_id: claim.setting_id,
                    original_present: claim.original_present,
                    original_value: claim.original_value,
                })
            } else if live == (claim.original_present, claim.original_value) {
                Ok(ClaimReleasePlan::AlreadyOriginal)
            } else {
                Ok(ClaimReleasePlan::DivergedSetting {
                    target_id: claim.target_id.clone(),
                    setting_id: claim.setting_id,
                    live,
                    expected: (claim.expected_present, claim.expected_value),
                    original: (claim.original_present, claim.original_value),
                })
            }
        }
    }
}

pub(crate) fn validate_claim_abandonment_authorization(
    plan: &ClaimReleasePlan,
    authorization: Option<&NvapiClaimAbandonmentAuthorization>,
    game_id: &str,
) -> Result<(), ServiceError> {
    match plan {
        ClaimReleasePlan::SharedReferenceDrop
        | ClaimReleasePlan::RestoreLiveExpected { .. }
        | ClaimReleasePlan::AlreadyOriginal
        | ClaimReleasePlan::ConfirmedAbsent => Ok(()),
        ClaimReleasePlan::DivergedSetting {
            target_id,
            setting_id,
            live,
            expected,
            original,
        } => {
            let auth = authorization.ok_or_else(|| ServiceError::NvapiClaimReleaseBlocked {
                game_id: game_id.to_owned(),
                target_id: target_id.clone(),
                setting_id: *setting_id,
                reason: NvapiClaimBlockedReason::RequiredAuthorizationMissing,
                detail: format!(
                    "NVIDIA setting {setting_id} changed outside RenderPilot; explicit user confirmation is required to abandon this claim before removing this game"
                ),
            })?;
            if auth.game_id != game_id {
                return Err(ServiceError::NvapiClaimAuthorizationStale {
                    game_id: game_id.to_owned(),
                    target_id: target_id.clone(),
                    setting_id: *setting_id,
                    detail: "authorization game id does not match the game being removed"
                        .to_owned(),
                });
            }
            let token = auth
                .claims
                .iter()
                .find(|c| c.target_id == *target_id && c.setting_id == *setting_id)
                .ok_or_else(|| ServiceError::NvapiClaimAuthorizationStale {
                    game_id: game_id.to_owned(),
                    target_id: target_id.clone(),
                    setting_id: *setting_id,
                    detail: format!(
                        "authorization missing required token for setting claim {target_id}:{setting_id}"
                    ),
                })?;
            if token.expected_present != expected.0
                || token.expected_value != expected.1
                || token.original_present != original.0
                || token.original_value != original.1
                || token.observed_live_present != Some(live.0)
                || token.observed_live_value != live.1
            {
                return Err(ServiceError::NvapiClaimAuthorizationStale {
                    game_id: game_id.to_owned(),
                    target_id: target_id.clone(),
                    setting_id: *setting_id,
                    detail: format!(
                        "observed DRS state ({live:?}) diverged from authorization token ({token:?})"
                    ),
                });
            }
            Ok(())
        }
        ClaimReleasePlan::WitnessMismatch {
            target_id,
            setting_id,
            detail,
            ..
        } => Err(ServiceError::NvapiClaimReleaseBlocked {
            game_id: game_id.to_owned(),
            target_id: target_id.clone(),
            setting_id: *setting_id,
            reason: NvapiClaimBlockedReason::WitnessMismatch,
            detail: detail.clone(),
        }),
        ClaimReleasePlan::FailedOrAmbiguous {
            target_id,
            setting_id,
            reason,
        } => Err(ServiceError::NvapiClaimReleaseBlocked {
            game_id: game_id.to_owned(),
            target_id: target_id.clone(),
            setting_id: *setting_id,
            reason: NvapiClaimBlockedReason::UncertainObservation,
            detail: reason.clone(),
        }),
    }
}

pub(crate) fn execute_claim_release_plan(
    context: &crate::Context,
    game_id: &str,
    claim: &NvapiTargetSettingClaimRow,
    plan: &ClaimReleasePlan,
    drs: &dyn ExternalSettingClaimDrs,
) -> Result<(), ServiceError> {
    match plan {
        ClaimReleasePlan::SharedReferenceDrop => {
            context.storage().release_nvapi_setting_claim(
                game_id,
                &claim.target_id,
                claim.setting_id,
            )?;
            Ok(())
        }
        ClaimReleasePlan::RestoreLiveExpected { .. } => {
            drs.execute_restore(context, game_id, claim)
        }
        ClaimReleasePlan::AlreadyOriginal | ClaimReleasePlan::ConfirmedAbsent => {
            context.storage().release_nvapi_setting_claim(
                game_id,
                &claim.target_id,
                claim.setting_id,
            )?;
            Ok(())
        }
        ClaimReleasePlan::DivergedSetting { .. } => {
            context.storage().release_nvapi_setting_claim(
                game_id,
                &claim.target_id,
                claim.setting_id,
            )?;
            Ok(())
        }
        ClaimReleasePlan::WitnessMismatch {
            target_id,
            setting_id,
            detail,
            ..
        } => Err(ServiceError::NvapiClaimReleaseBlocked {
            game_id: game_id.to_owned(),
            target_id: target_id.clone(),
            setting_id: *setting_id,
            reason: NvapiClaimBlockedReason::WitnessMismatch,
            detail: detail.clone(),
        }),
        ClaimReleasePlan::FailedOrAmbiguous {
            target_id,
            setting_id,
            reason,
        } => Err(ServiceError::NvapiClaimReleaseBlocked {
            game_id: game_id.to_owned(),
            target_id: target_id.clone(),
            setting_id: *setting_id,
            reason: NvapiClaimBlockedReason::UncertainObservation,
            detail: reason.clone(),
        }),
    }
}

pub(crate) fn restore_game_setting_claims(
    context: &crate::Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    game_id: &str,
    authorization: Option<&NvapiClaimAbandonmentAuthorization>,
) -> Result<(), ServiceError> {
    let drs = active_external_setting_claim_drs();
    restore_game_setting_claims_with_drs(context, guard, game_id, authorization, drs.as_ref())
}

pub(crate) fn restore_game_setting_claims_with_drs(
    context: &crate::Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    game_id: &str,
    authorization: Option<&NvapiClaimAbandonmentAuthorization>,
    drs: &dyn ExternalSettingClaimDrs,
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

    let mut plans = Vec::with_capacity(claims.len());
    for claim in &claims {
        let plan = evaluate_claim_release_plan(context, claim, drs)?;
        validate_claim_abandonment_authorization(&plan, authorization, game_id)?;
        plans.push(plan);
    }

    for (claim, plan) in claims.iter().zip(plans.iter()) {
        execute_claim_release_plan(context, game_id, claim, plan, drs)?;
    }

    Ok(())
}

fn execute_live_restore(
    context: &crate::Context,
    game_id: &str,
    claim: &NvapiTargetSettingClaimRow,
) -> Result<(), ServiceError> {
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
    let recorded_profile_status = context
        .storage()
        .get_nvapi_target_profile_is_predefined(&claim.target_id)?
        .ok_or_else(|| {
            ServiceError::command_failed(format!(
                "NVIDIA claim profile identity receipt is missing for {}; the claim was kept for review",
                claim.executable_path
            ))
        })?;
    if !stable_profile_identity_matches(
        &claim.target_id,
        Some(recorded_profile_status),
        &identity.name,
        identity.is_predefined,
    ) {
        return Err(ServiceError::command_failed(format!(
            "NVIDIA claim profile identity changed for {}; the claim was kept for review",
            claim.executable_path
        )));
    }
    let witness = profile.matched_application().ok_or_else(|| {
        ServiceError::command_failed(
            "NVIDIA full-path profile lookup returned no application witness",
        )
    })?;
    let witness_json = setting_application_witness_json(Some(&witness))?.ok_or_else(|| {
        ServiceError::command_failed(
            "NVIDIA full-path profile lookup returned an empty application witness",
        )
    })?;
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
    session.save().map_err(|error| {
        ServiceError::command_failed(format!(
            "NVIDIA claim restore has an uncertain save result; the journal remains pending: {error}"
        ))
    })?;
    drop(session);

    let verify_session = open_drs_session().map_err(warning_to_service_error)?;
    let verified = verify_session
        .find_profile_by_exe(&claim.executable_path)
        .map_err(|error| {
            ServiceError::command_failed(format!("could not verify restored NVIDIA claim: {error}"))
        })?;
    let actual = read_explicit_setting(&verified, claim.setting_id)?;
    let verified_identity = verified
        .identity()
        .map_err(|error| ServiceError::command_failed(error.to_string()))?;
    let verified_witness =
        setting_application_witness_json(verified.matched_application().as_ref())?;
    let verified_composition = if owner.is_some() {
        Some(profile_composition(&verified)?)
    } else {
        None
    };
    let observation_error = validate_claim_restore_observation(
        &ClaimRestoreExpectation {
            target_id: &claim.target_id,
            profile_is_predefined: recorded_profile_status,
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
    context
        .storage()
        .release_nvapi_setting_claim(game_id, &claim.target_id, claim.setting_id)?;
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

pub(crate) struct ClaimRestoreExpectation<'a> {
    pub(crate) target_id: &'a str,
    pub(crate) profile_is_predefined: bool,
    pub(crate) application_witness_json: &'a str,
    pub(crate) state: NvapiSettingState,
    pub(crate) setting_id: u32,
}

pub(crate) struct ClaimRestoreCompositionObservation<'a> {
    pub(crate) owner_receipt: Option<&'a str>,
    pub(crate) before_restore: Option<&'a str>,
    pub(crate) after_restore: Option<&'a serde_json::Value>,
}

pub(crate) struct ClaimRestoreObservation<'a> {
    pub(crate) profile_name: &'a str,
    pub(crate) is_predefined: bool,
    pub(crate) application_witness_json: Option<&'a str>,
    pub(crate) state: NvapiSettingState,
    pub(crate) composition: ClaimRestoreCompositionObservation<'a>,
}

pub(crate) fn validate_claim_restore_observation(
    expected: &ClaimRestoreExpectation<'_>,
    observed: &ClaimRestoreObservation<'_>,
) -> Result<(), &'static str> {
    if !stable_profile_identity_matches(
        expected.target_id,
        Some(expected.profile_is_predefined),
        observed.profile_name,
        observed.is_predefined,
    ) || (observed.composition.owner_receipt.is_some() && observed.is_predefined)
    {
        return Err("profile identity changed");
    }
    let observed_witness_json = observed
        .application_witness_json
        .ok_or("effective-profile application witness is missing")?;
    if !application_witness_matches(
        Some(expected.application_witness_json),
        Some(observed_witness_json),
    ) {
        return Err("effective-profile application witness changed");
    }
    let witness: renderpilot_nvapi::ApplicationIdentity =
        serde_json::from_str(observed_witness_json)
            .map_err(|_| "effective-profile application witness is invalid")?;
    if witness.app_name.is_empty() {
        return Err("effective-profile application witness is invalid");
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
