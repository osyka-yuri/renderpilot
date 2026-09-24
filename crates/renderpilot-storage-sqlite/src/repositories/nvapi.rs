//! Durable storage for NVAPI executable overrides, profile ownership, and setting claims.

mod executable_overrides;
mod owned_profiles;
mod pending;
mod setting_claims;
#[cfg(test)]
mod tests;
/// One row from `nvapi_executable_overrides`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvapiExecutableOverrideRow {
    /// Game this override applies to.
    pub game_id: String,
    /// Absolute path to the chosen executable on disk.
    pub selected_path: String,
    /// Basename (filename only) — what gets passed to NVAPI.
    pub selected_basename: String,
    /// Unix milliseconds of the last write.
    pub updated_at: i64,
}

// -----------------------------------------------------------------------------
// Profile ownership
// -----------------------------------------------------------------------------

/// Durable ownership receipt for a RenderPilot-created NVIDIA profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvapiOwnedProfileRow {
    /// Owning game id.
    pub game_id: String,
    /// Exact profile name in the NVIDIA driver database.
    pub profile_name: String,
    /// The one full-path executable binding confirmed by RenderPilot.
    pub binding_path: String,
    /// Serialized NVDRS_APPLICATION identity witness.
    pub application_witness_json: String,
    /// Serialized confirmed profile application and setting composition.
    pub composition_json: String,
    /// Ownership state: active, pending, or conflict.
    pub state: String,
    /// Last durable update time in Unix milliseconds.
    pub updated_at: i64,
}

/// Durable pre-mutation and last-confirmed state for a DRS setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvapiTargetSettingClaimRow {
    /// Game currently holding the setting claim.
    pub game_id: String,
    /// Verified DRS target identity.
    pub target_id: String,
    /// NVIDIA DRS setting identifier.
    pub setting_id: u32,
    /// Exact full-path application scope this game confirmed for this claim.
    pub executable_path: String,
    /// Whether the setting had an explicit profile value before RenderPilot.
    pub original_present: bool,
    /// Original explicit DWORD value, when present.
    pub original_value: Option<u32>,
    /// Whether the last confirmed RenderPilot state is explicit.
    pub expected_present: bool,
    /// Last confirmed RenderPilot DWORD value, when present.
    pub expected_value: Option<u32>,
}

/// Unresolved DRS operation receipt. Rows fence overlapping target/path mutations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvapiPendingOperationRow {
    /// Stable operation receipt id.
    pub op_id: String,
    /// Affected game, or `None` for a global operation.
    pub game_id: Option<String>,
    /// Verified DRS profile identity, when available.
    pub target_id: Option<String>,
    /// Operation kind such as setting, create_profile, move_profile, or delete_profile.
    pub kind: String,
    /// Durable state observed before the driver mutation.
    pub before_json: String,
    /// Expected durable state after the driver mutation.
    pub after_json: String,
    /// Third-state observation for an operation that could not be reconciled.
    pub observed_json: Option<String>,
    /// Current reconciliation phase.
    pub phase: String,
    /// Receipt creation time in Unix milliseconds.
    pub created_at: i64,
    /// Last receipt update time in Unix milliseconds.
    pub updated_at: i64,
}

/// Presence flag and optional DWORD as recorded for a DRS setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvapiSettingState {
    /// Whether the profile has an explicit value for this setting.
    pub present: bool,
    /// Recorded DWORD value, if any.
    pub value: Option<u32>,
}

/// Input for preparing a durable setting write scoped to one game executable.
#[derive(Debug, Clone, Copy)]
pub struct NvapiGameSettingPreparation<'a> {
    /// Stable operation receipt id.
    pub op_id: &'a str,
    /// Game whose executable participates in the setting claim.
    pub game_id: &'a str,
    /// Exact NVIDIA profile name.
    pub profile_name: &'a str,
    /// RenderPilot ownership kind for the target profile.
    pub target_kind: &'a str,
    /// Whether the NVIDIA profile is predefined.
    pub profile_is_predefined: bool,
    /// Serialized profile identity receipt.
    pub profile_identity_json: &'a str,
    /// Full path of the participating game executable.
    pub executable_path: &'a str,
    /// Serialized full-path application witness.
    pub application_witness_json: &'a str,
    /// NVIDIA setting identifier.
    pub setting_id: u32,
    /// State before RenderPilot first claimed the setting.
    pub original: NvapiSettingState,
    /// Live state observed immediately before this write.
    pub before: NvapiSettingState,
    /// State requested by this write.
    pub after: NvapiSettingState,
}

/// Input for preparing a durable setting write on a global profile.
#[derive(Debug, Clone, Copy)]
pub struct NvapiGlobalSettingPreparation<'a> {
    /// Stable operation receipt id.
    pub op_id: &'a str,
    /// Exact NVIDIA profile name.
    pub profile_name: &'a str,
    /// Whether the NVIDIA profile is predefined.
    pub profile_is_predefined: bool,
    /// Serialized profile identity receipt.
    pub profile_identity_json: &'a str,
    /// NVIDIA setting identifier.
    pub setting_id: u32,
    /// State before RenderPilot first claimed the setting.
    pub original: NvapiSettingState,
    /// Live state observed immediately before this write.
    pub before: NvapiSettingState,
    /// State requested by this write.
    pub after: NvapiSettingState,
}

/// Scope of a verified setting operation receipt.
#[derive(Debug, Clone, Copy)]
pub enum NvapiSettingOperationScope<'a> {
    /// A game-scoped claim tied to a confirmed executable.
    Game {
        /// Game whose executable participates in the setting claim.
        game_id: &'a str,
    },
    /// A global write on a base profile.
    Global,
}

/// Input for atomically publishing a setting write verified by a fresh DRS read.
#[derive(Debug, Clone, Copy)]
pub struct NvapiSettingOperationCompletion<'a> {
    /// Stable operation receipt id.
    pub op_id: &'a str,
    /// Whether the setting write is game scoped or global.
    pub scope: NvapiSettingOperationScope<'a>,
    /// Exact NVIDIA profile identity key.
    pub target_id: &'a str,
    /// NVIDIA setting identifier.
    pub setting_id: u32,
    /// Verified setting state after the operation.
    pub expected: NvapiSettingState,
    /// Serialized profile identity receipt after verification.
    pub profile_identity_json: &'a str,
    /// Serialized owned-profile composition, when this setting belongs to an owned profile.
    pub composition_json: Option<&'a str>,
}

/// Profile identity, application witness, and composition verified together in DRS.
#[derive(Debug, Clone, Copy)]
pub struct NvapiVerifiedProfileReceipt<'a> {
    /// Exact NVIDIA profile name.
    pub profile_name: &'a str,
    /// Serialized profile identity receipt.
    pub profile_identity_json: &'a str,
    /// Serialized full-path application witness.
    pub application_witness_json: &'a str,
    /// Serialized profile composition.
    pub composition_json: &'a str,
}

/// Input for publishing a newly created and verified owned profile.
#[derive(Debug, Clone, Copy)]
pub struct NvapiProfileCreationCompletion<'a> {
    /// Stable operation receipt id.
    pub op_id: &'a str,
    /// Owning game id.
    pub game_id: &'a str,
    /// Confirmed full-path executable binding.
    pub binding_path: &'a str,
    /// Verified DRS profile receipt.
    pub profile: NvapiVerifiedProfileReceipt<'a>,
}

/// Input for publishing a verified executable move of an owned profile.
#[derive(Debug, Clone, Copy)]
pub struct NvapiProfileMoveCompletion<'a> {
    /// Stable operation receipt id.
    pub op_id: &'a str,
    /// Owning game id.
    pub game_id: &'a str,
    /// Confirmed destination executable path.
    pub binding_path: &'a str,
    /// Executable basename used by the global override.
    pub binding_basename: &'a str,
    /// Whether the executable selector should return to automatic selection.
    pub select_automatically: bool,
    /// Verified DRS profile receipt.
    pub profile: NvapiVerifiedProfileReceipt<'a>,
}
