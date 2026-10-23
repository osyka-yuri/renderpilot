//! Read/write operations for NVAPI driver profile settings.
//!
//! ## Modules
//!
//! - `target` -- internal `SettingTarget` / `WriteOp` routing types
//! - `session` -- DRS session open + error mapping
//! - `live` -- live DWORD reads
//! - `assemble` -- DTO assembly from a live read
//! - `read` -- single/batch read entry points
//! - `write` -- write, revert, value validation
//!
//! The flat operation surface is crate-private. Public per-game callers must
//! use [`crate::nvapi::game_session::GameNvapiSession`] so the mutation guard
//! remains held through projection, driver work, and response serialization.

mod assemble;
pub(crate) mod claim_release;
mod live;
mod read;
mod session;
mod target;
pub(crate) mod write;

#[cfg(test)]
mod tests;

pub(crate) use claim_release::{
    ClaimReleasePlan, NvapiClaimAbandonmentAuthorization, active_external_setting_claim_drs,
    evaluate_claim_release_plan, restore_game_setting_claims,
    validate_claim_abandonment_authorization,
};
pub(crate) use read::{read_all_setting_states, read_setting_state};
pub(crate) use target::{SettingTarget, WriteOp};
pub(crate) use write::{
    ensure_dll_setting_catalog_ready, resolve_revert_op, validate_value_supported,
    write_setting_value,
};

#[cfg(test)]
pub(crate) use claim_release::{
    ExternalSettingClaimDrs, ObservedClaimDrs, set_test_external_setting_claim_drs,
};
