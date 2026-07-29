//! Read/write operations for NVAPI driver profile settings.
//!
//! ## Modules
//!
//! - `target` -- [`crate::nvapi::ops::SettingTarget`] / [`crate::nvapi::ops::WriteOp`]
//! - `session` -- DRS session open + error mapping
//! - `live` -- live DWORD reads
//! - `assemble` -- DTO assembly from a live read
//! - `read` -- single/batch read entry points
//! - `write` -- write, revert, value validation
//!
//! External callers use the flat `nvapi::ops::{...}` surface re-exported below.

mod assemble;
mod live;
mod read;
mod session;
mod target;
mod write;

#[cfg(test)]
mod tests;

pub use read::{read_all_setting_states, read_setting_state};
pub use target::{SettingTarget, WriteOp};
pub(crate) use write::restore_game_baselines;
pub use write::{resolve_revert_op, validate_value_supported, write_setting_value};
