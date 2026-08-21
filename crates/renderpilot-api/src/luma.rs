//! Desktop UI facade for installing the Luma Framework add-on.
//!
//! All work (manifest fetch, matching, file-safety validation, download, install) lives
//! in `renderpilot-orchestration::addons::luma`. This module parses GUI string
//! ids, loads/caches the manifest, and wraps the typed results in
//! `serde_json::Value` for the command layer. Unlike RenoDX, there is no
//! channel parameter anywhere here — Luma always installs the nightly ReShade
//! host.
//!
//! Mutation helpers return install state by calling orchestration `status`
//! directly (with optional manifest for `launch_args`). CLI status / bulk
//! check-updates do not go through this crate — they call orchestration.

use renderpilot_orchestration::Context;
use renderpilot_orchestration::addons::luma;
use renderpilot_orchestration::addons::luma::dto::update::LumaUpdateReport;
use renderpilot_orchestration::addons::luma::use_cases::commands::install::InstallRequest;
use renderpilot_orchestration::addons::update::UpdateStatus;
use renderpilot_orchestration::net::ProgressObserver;

use crate::utils::{JsonResult, parse_game_id, to_json};

/// Previews whether Luma can be installed for a game, with match
/// explanation. Loads (and caches) the manifest as needed.
pub async fn luma_availability(context: &Context, game_id: impl Into<String>) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let bundle = luma::manifest_store::get_or_fetch_bundle().await?;
    to_json(
        luma::use_cases::queries::availability::load_availability(
            context,
            &bundle.tool,
            &bundle.reshade.sources,
            &game_id,
        )
        .await?,
    )
}

/// Installs Luma into a game, reporting download progress, and returns the
/// resulting install state. `game_context_token` must come from a fresh file
/// safety assessment.
pub async fn luma_install(
    context: &Context,
    game_id: impl Into<String>,
    game_context_token: Option<String>,
    progress: Option<&ProgressObserver<'_>>,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let safety = renderpilot_orchestration::FileSafetyAuthority::new()
        .game_permit(game_id.clone(), game_context_token.as_deref())?;
    let bundle = luma::manifest_store::get_or_fetch_bundle().await?;
    luma::use_cases::commands::install::install(InstallRequest {
        context,
        manifest: &bundle.tool,
        reshade_sources: &bundle.reshade.sources,
        game_id: &game_id,
        safety,
        progress,
    })
    .await?;
    to_json(luma::use_cases::queries::status::status(
        context,
        Some(&bundle.tool),
        &game_id,
    )?)
}

/// Uninstalls Luma from a game and returns the resulting install state.
///
/// Builds the resulting status without fetching the manifest — an uninstall
/// that already succeeded must not fail (or need network) just to report the
/// state it left behind.
pub fn luma_uninstall(context: &Context, game_id: impl Into<String>) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    luma::use_cases::commands::uninstall::uninstall(context, &game_id)?;
    to_json(luma::use_cases::queries::status::status(
        context, None, &game_id,
    )?)
}

/// Checks whether the installed Luma add-on for a game has an upstream update.
///
/// Luma ships a rolling release, so this compares the recorded release asset
/// against upstream (cheap `HEAD`/ETag first, digest fallback). Returns a
/// per-source update report.
///
/// `deep` (default `false`): full ZIP / host-archive identity when true; passive
/// HEAD/build + disk intactness when false, with a one-shot release-ZIP bind for
/// unbound advisory payloads after DB-loss adoption (host nightlies never
/// auto-download on passive).
///
/// Catalogue/cache and upstream failures soft-fail to overall `unknown`.
pub async fn luma_check_update(
    context: &Context,
    game_id: impl Into<String>,
    deep: bool,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let Ok(bundle) = luma::manifest_store::get_or_fetch_bundle().await else {
        return to_json(LumaUpdateReport::new(
            Some(UpdateStatus::Unknown),
            None,
            None,
        ));
    };
    to_json(
        luma::use_cases::queries::updates::check_update(
            context,
            &bundle.tool,
            &bundle.reshade.sources,
            &game_id,
            deep,
        )
        .await?,
    )
}

/// Applies an available Luma update for a game (re-fetch + set-diff apply),
/// reporting download progress, and returns the resulting install state.
///
/// When `force_full` is true (desktop Repair), the prepare path always
/// re-fetches the release ZIP and runs a full set-diff reconverge even if the
/// cheap ETag pre-check says the payload is current.
pub async fn luma_update(
    context: &Context,
    game_id: impl Into<String>,
    force_full: bool,
    game_context_token: Option<String>,
    progress: Option<&ProgressObserver<'_>>,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let safety = renderpilot_orchestration::FileSafetyAuthority::new()
        .game_permit(game_id.clone(), game_context_token.as_deref())?;
    let bundle = luma::manifest_store::get_or_fetch_bundle().await?;
    luma::use_cases::commands::update::update(luma::use_cases::commands::update::UpdateRequest {
        context,
        manifest: &bundle.tool,
        reshade_sources: &bundle.reshade.sources,
        game_id: &game_id,
        force_full,
        safety,
        progress,
    })
    .await?;
    to_json(luma::use_cases::queries::status::status(
        context,
        Some(&bundle.tool),
        &game_id,
    )?)
}
