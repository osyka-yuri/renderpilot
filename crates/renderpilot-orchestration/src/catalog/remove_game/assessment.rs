//! Read-only preflight assessment for catalog game card removal.
//!
//! Evaluates whether a game can be removed automatically without additional user
//! authorization, or whether divergent external NVIDIA setting claims or active
//! operations require explicit user confirmation or resolution first.

use renderpilot_domain::GameId;

use super::require_removable_game;
use crate::ServiceError;
use crate::nvapi::ops::{
    ClaimReleasePlan, active_external_setting_claim_drs, evaluate_claim_release_plan,
};

/// Read-only assessment of game removal feasibility and NVAPI claim release requirements.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRemovalAssessment {
    /// Game identity.
    pub game_id: String,
    /// Whether the game can be removed automatically without additional user authorization.
    pub can_remove_automatically: bool,
    /// Detailed assessment of NVAPI claim release feasibility.
    pub nvapi_assessment: NvapiRemovalAssessment,
}

/// NVAPI-specific claim release evaluation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NvapiRemovalAssessment {
    /// Whether removal requires user authorization to abandon divergent claims.
    pub requires_user_authorization: bool,
    /// Reason why removal is strictly blocked, if any.
    pub blocked_reason: Option<String>,
    /// Assessments for each tracked setting claim.
    pub claims: Vec<NvapiClaimAssessment>,
}

/// Detailed assessment for one setting claim.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NvapiClaimAssessment {
    /// Profile or executable target identifier.
    pub target_id: String,
    /// Setting identifier.
    pub setting_id: u32,
    /// Human-readable setting name.
    pub setting_name: String,
    /// Status description.
    pub status: String,
    /// Recorded expected driver value.
    pub recorded_expected: NvapiSettingValueDto,
    /// Recorded original driver value before mutation.
    pub recorded_original: NvapiSettingValueDto,
    /// Observed live driver value, if inspection succeeded.
    pub observed_live: Option<NvapiSettingValueDto>,
    /// Evaluated disposition for this claim.
    pub disposition: NvapiClaimDisposition,
}

/// Action disposition for a setting claim during game removal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NvapiClaimDisposition {
    /// Setting is referenced by other games; drop reference without driver mutation.
    SafeSharedRelease,
    /// Setting live value matches recorded expected value; restore original.
    SafeAutomaticRestore,
    /// Setting live value already matches original value; safe to release.
    SafeAlreadyOriginal,
    /// Profile or application target is confirmed absent from DRS; safe to release.
    SafeConfirmedAbsent,
    /// Setting live value has diverged; requires explicit user authorization to abandon.
    RequiresAbandonmentAuthorization,
    /// Setting or profile is blocked from removal due to pending operations or ambiguous state.
    Blocked,
}

/// Transport DTO representing presence and DWORD value of an NVAPI setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NvapiSettingValueDto {
    /// Whether the setting is present.
    pub present: bool,
    /// DWORD value if present.
    pub value: Option<u32>,
}

/// Performs a read-only assessment of game removal, evaluating whether automatic removal
/// is possible or whether divergent NVAPI claims require user authorization to abandon.
pub fn assess_game_removal(
    context: &crate::Context,
    game_id: &GameId,
) -> Result<GameRemovalAssessment, ServiceError> {
    let _catalog_guard = context.catalog_scan_guard();
    require_removable_game(context, game_id)?;
    let claims = context
        .storage()
        .list_nvapi_setting_claims_for_game(game_id.as_str())?;
    let drs = active_external_setting_claim_drs();

    let mut claim_assessments = Vec::new();
    let mut requires_user_authorization = false;
    let mut blocked_reason = None;

    let pending_nvapi = context.storage().list_pending_nvapi_operations()?;
    let has_pending_op = pending_nvapi.iter().any(|op| {
        op.game_id.as_deref() == Some(game_id.as_str())
            || claims
                .iter()
                .any(|c| op.target_id.as_deref() == Some(&c.target_id))
    });
    if has_pending_op {
        blocked_reason =
            Some("a durable DRS operation must be reconciled before removing this game".to_owned());
    }

    if context
        .storage()
        .get_nvapi_owned_profile(game_id.as_str())?
        .is_some()
    {
        blocked_reason = Some(
            "delete the owned NVIDIA profile from the game details page before removing this game"
                .to_owned(),
        );
    }

    for claim in &claims {
        let plan = evaluate_claim_release_plan(context, claim, drs.as_ref())?;
        let setting_name = crate::dlss::settings_catalog::catalog()
            .iter()
            .find(|s| s.nvapi_id == claim.setting_id)
            .map(|s| s.label.to_owned())
            .unwrap_or_else(|| format!("Setting 0x{:08X}", claim.setting_id));

        let (disposition, observed_live, status_str) = match &plan {
            ClaimReleasePlan::SharedReferenceDrop => (
                NvapiClaimDisposition::SafeSharedRelease,
                None,
                "shared_reference".to_owned(),
            ),
            ClaimReleasePlan::RestoreLiveExpected { .. } => (
                NvapiClaimDisposition::SafeAutomaticRestore,
                Some(NvapiSettingValueDto {
                    present: claim.expected_present,
                    value: claim.expected_value,
                }),
                "restorable".to_owned(),
            ),
            ClaimReleasePlan::AlreadyOriginal => (
                NvapiClaimDisposition::SafeAlreadyOriginal,
                Some(NvapiSettingValueDto {
                    present: claim.original_present,
                    value: claim.original_value,
                }),
                "already_original".to_owned(),
            ),
            ClaimReleasePlan::ConfirmedAbsent => (
                NvapiClaimDisposition::SafeConfirmedAbsent,
                None,
                "confirmed_absent".to_owned(),
            ),
            ClaimReleasePlan::DivergedSetting { live, .. } => {
                requires_user_authorization = true;
                (
                    NvapiClaimDisposition::RequiresAbandonmentAuthorization,
                    Some(NvapiSettingValueDto {
                        present: live.0,
                        value: live.1,
                    }),
                    "diverged".to_owned(),
                )
            }
            ClaimReleasePlan::WitnessMismatch { detail, .. } => {
                blocked_reason = Some(format!(
                    "NVIDIA profile identity or executable witness changed ({detail}); cannot safely release or mutate claim"
                ));
                (
                    NvapiClaimDisposition::Blocked,
                    None,
                    format!("witness_mismatch: {detail}"),
                )
            }
            ClaimReleasePlan::FailedOrAmbiguous { reason, .. } => {
                blocked_reason = Some(format!("NVIDIA observation failed: {reason}"));
                (
                    NvapiClaimDisposition::Blocked,
                    None,
                    format!("failed: {reason}"),
                )
            }
        };

        claim_assessments.push(NvapiClaimAssessment {
            target_id: claim.target_id.clone(),
            setting_id: claim.setting_id,
            setting_name,
            status: status_str,
            recorded_expected: NvapiSettingValueDto {
                present: claim.expected_present,
                value: claim.expected_value,
            },
            recorded_original: NvapiSettingValueDto {
                present: claim.original_present,
                value: claim.original_value,
            },
            observed_live,
            disposition,
        });
    }

    let can_remove_automatically = blocked_reason.is_none() && !requires_user_authorization;

    Ok(GameRemovalAssessment {
        game_id: game_id.as_str().to_owned(),
        can_remove_automatically,
        nvapi_assessment: NvapiRemovalAssessment {
            requires_user_authorization,
            blocked_reason,
            claims: claim_assessments,
        },
    })
}
