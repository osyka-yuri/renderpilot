//! Desktop UI facade for installing the RenoDX HDR add-on.
//!
//! All work (manifest fetch, matching, risk assessment, download, install) lives
//! in `renderpilot-orchestration::addons::renodx`. This module parses GUI string
//! ids, loads/caches the manifest, and wraps the typed results in
//! `serde_json::Value` for the command layer.

use renderpilot_orchestration::Context;
use renderpilot_orchestration::addons::renodx;
use renderpilot_orchestration::net::ProgressObserver;

use crate::utils::{JsonResult, parse_game_id, to_json};

/// Returns the current RenoDX install state for a game.
pub fn renodx_status(context: &Context, game_id: impl Into<String>) -> JsonResult {
    to_json(renodx::service::status(context, &parse_game_id(game_id)?)?)
}

/// Previews whether RenoDX can be installed for a game, with risk and a match
/// explanation. Loads (and caches) the manifest as needed.
pub async fn renodx_availability(context: &Context, game_id: impl Into<String>) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let manifest = renodx::manifest_store::get_or_fetch_manifest().await?;
    to_json(renodx::service::availability(context, &manifest, &game_id)?)
}

/// Installs RenoDX into a game, reporting download progress, and returns the
/// resulting install state. `confirm_anticheat` must be `true` to proceed when
/// the risk assessment requires confirmation.
pub async fn renodx_install(
    context: &Context,
    game_id: impl Into<String>,
    confirm_anticheat: bool,
    progress: Option<&ProgressObserver<'_>>,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let manifest = renodx::manifest_store::get_or_fetch_manifest().await?;
    renodx::service::install(context, &manifest, &game_id, confirm_anticheat, progress).await?;
    to_json(renodx::service::status(context, &game_id)?)
}

/// Installs RenoDX into a game from a user-downloaded add-on file (for external,
/// Discord/Nexus-distributed games), reporting ReShade-host download progress, and
/// returns the resulting install state. `confirm_anticheat` must be `true` to
/// proceed when the risk assessment requires confirmation.
pub async fn renodx_install_from_file(
    context: &Context,
    game_id: impl Into<String>,
    file_path: impl Into<String>,
    confirm_anticheat: bool,
    progress: Option<&ProgressObserver<'_>>,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let file_path = file_path.into();
    let manifest = renodx::manifest_store::get_or_fetch_manifest().await?;
    renodx::service::install_from_file(
        context,
        &manifest,
        &game_id,
        &file_path,
        confirm_anticheat,
        progress,
    )
    .await?;
    to_json(renodx::service::status(context, &game_id)?)
}

/// Uninstalls RenoDX from a game and returns the resulting install state.
pub fn renodx_uninstall(context: &Context, game_id: impl Into<String>) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    renodx::service::uninstall(context, &game_id)?;
    to_json(renodx::service::status(context, &game_id)?)
}

/// Checks whether the installed RenoDX add-on for a game has an upstream update.
///
/// RenoDX ships rolling snapshots, so this compares the recorded source against
/// upstream (cheap `HEAD`/ETag first, digest fallback). Returns a per-source
/// update report. Never errors on a network failure — it reports `unknown`.
pub async fn renodx_check_update(context: &Context, game_id: impl Into<String>) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    to_json(renodx::update::check_update(context, &game_id).await?)
}

/// Applies an available RenoDX update for a game (re-fetch + atomic in-place
/// replace), reporting download progress, and returns the resulting install state.
/// This updates both the add-on and the managed ReShade host if they have changed.
/// Foreign hosts are left untouched.
pub async fn renodx_update(
    context: &Context,
    game_id: impl Into<String>,
    progress: Option<&ProgressObserver<'_>>,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    renodx::update::update(context, &game_id, progress).await?;
    to_json(renodx::service::status(context, &game_id)?)
}

/// Bulk-checks every installed RenoDX add-on for upstream updates, returning a map
/// of game id to update status — the data behind an "Update all RenoDX" action.
pub async fn renodx_check_updates(context: &Context) -> JsonResult {
    let statuses = renodx::update::check_updates(context).await?;
    let mut map = serde_json::Map::with_capacity(statuses.len());
    for (game_id, status) in statuses {
        map.insert(game_id.as_str().to_owned(), to_json(status)?);
    }
    Ok(serde_json::Value::Object(map))
}

/// Installs the DLSS-Fix companion add-on for a game that already has RenoDX,
/// reporting download progress, and returns the resulting install state.
pub async fn renodx_install_dlss_fix(
    context: &Context,
    game_id: impl Into<String>,
    progress: Option<&ProgressObserver<'_>>,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let state = renodx::service::install_dlss_fix(context, &game_id, progress).await?;
    to_json(state)
}

/// Removes the DLSS-Fix companion add-on, leaving the main RenoDX install intact,
/// and returns the resulting install state.
pub fn renodx_uninstall_dlss_fix(context: &Context, game_id: impl Into<String>) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let state = renodx::service::uninstall_dlss_fix(context, &game_id)?;
    to_json(state)
}

/// Returns whether a DLSS-Fix can be installed for this game (RenoDX installed +
/// NVIDIA Frame Generation + DLSS + Streamline detected, and DLSS-Fix not already
/// installed).
pub fn renodx_dlss_fix_availability(context: &Context, game_id: impl Into<String>) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    to_json(renodx::service::dlss_fix_availability(context, &game_id)?)
}
