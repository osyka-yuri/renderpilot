//! Tool-agnostic ReShade host detection and `ReShade.ini` reads.
//!
//! An add-on tool (RenoDX, Luma) is a ReShade add-on, so the question is not "did
//! RenderPilot install this host?" but "is the host the game will load a ReShade
//! build with full add-on support?". This module keeps that read-only host model
//! and the `ReShade.ini` reads it needs (path resolution, add-on config state, user
//! effect detection). The INI *write* transform lives in [`super::ini_schema`];
//! tool-specific add-on-state derivations live with the tool.
//!
//! Split by concern: [`host_model`] is the host data model and policy action,
//! [`hosts`] is the proxy-slot scan algorithm that produces it, [`identity`]
//! tells ReShade (and recognized non-ReShade forks) apart from PE strings and
//! file names, [`paths`] resolves `ReShade.ini` paths and generic `[ADDON]`
//! state, and [`effects`] detects an existing user effects/preset setup.

mod effects;
mod host_model;
mod hosts;
mod identity;
mod paths;

pub(crate) use effects::has_user_effect_assets;
pub use host_model::{
    ReshadeAddonSupport, ReshadeHost, ReshadeHostAction, ReshadeIdentity, ReshadeScan, SlotActivity,
};
pub use hosts::{host_action, scan_reshade_hosts};
pub use identity::is_known_custom_build;
pub(crate) use identity::{guess_advisory_channel, is_proxy_slot};
pub use paths::{
    RESHADE_INI_FILE_NAME, ReshadePaths, remove_reshade_logs_best_effort, reshade_ini_path,
    resolve_paths,
};
pub(crate) use paths::{load_ini, read_addon_config_state, same_path, split_ini_list};

/// Test fixtures construct host present-states by hand (`reshade::host_policy`,
/// `renodx` host_report tests). Production code only names these types inside
/// `host_model`; re-export under `cfg(test)` avoids a production `unused_imports`.
#[cfg(test)]
pub use host_model::{ActiveSlotReason, ActiveSlotState};
