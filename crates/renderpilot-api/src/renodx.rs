//! Desktop UI facade for installing the RenoDX HDR add-on.
//!
//! All work (manifest fetch, matching, file-safety validation, download, install) lives
//! in `renderpilot-orchestration::addons::renodx`. This module parses GUI string
//! ids, loads/caches the manifest, and wraps the typed results in
//! `serde_json::Value` for the command layer.

use renderpilot_orchestration::Context;
use renderpilot_orchestration::addons::renodx;
use renderpilot_orchestration::addons::renodx::dto::update::RenoDxUpdateReport;
use renderpilot_orchestration::addons::renodx::use_cases::commands::install::InstallRequest;
use renderpilot_orchestration::addons::update::UpdateStatus;
use renderpilot_orchestration::net::ProgressObserver;
use renodx::types::ReshadeChannel;

use crate::utils::{JsonResult, parse_game_id, to_json};

/// Previews whether RenoDX can be installed for a game, with match
/// explanation. Loads (and caches) the manifest as needed.
pub async fn renodx_availability(context: &Context, game_id: impl Into<String>) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let bundle = renodx::manifest_store::get_or_fetch_bundle().await?;
    to_json(
        renodx::use_cases::queries::availability::load_availability(
            context,
            &bundle.tool,
            &bundle.reshade.sources,
            &game_id,
        )
        .await?,
    )
}

/// Installs RenoDX into a game, reporting download progress, and returns the
/// resulting install state. `game_context_token` must come from a fresh file
/// safety assessment. The desktop flow transparently permits
/// installing the shared Vulkan layer when a Vulkan game needs it.
pub async fn renodx_install(
    context: &Context,
    game_id: impl Into<String>,
    reshade_channel: impl Into<String>,
    game_context_token: Option<String>,
    shared_vulkan_context_token: Option<String>,
    progress: Option<&ProgressObserver<'_>>,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let reshade_channel = parse_reshade_channel(reshade_channel)?;
    let safety = renderpilot_orchestration::FileSafetyAuthority::new().game_mutation_permits(
        game_id.clone(),
        game_context_token.as_deref(),
        shared_vulkan_context_token.as_deref(),
    )?;
    let bundle = renodx::manifest_store::get_or_fetch_bundle().await?;
    renodx::use_cases::commands::install::install(InstallRequest {
        context,
        manifest: &bundle.tool,
        reshade_sources: &bundle.reshade.sources,
        game_id: &game_id,
        requested_channel: reshade_channel,
        safety,
        allow_shared_vulkan_layer_install: true,
        progress,
    })
    .await?;
    to_json(renodx::use_cases::queries::status::status(
        context, &game_id,
    )?)
}

/// Installs RenoDX into a game from a user-downloaded add-on file (for external,
/// Discord/Nexus-distributed games), reporting ReShade-host download progress,
/// and returns the resulting install state. The game and optional shared-layer
/// safety contexts are validated again after download. Shared Vulkan layer
/// installation is permitted transparently
/// in the desktop flow.
pub async fn renodx_install_from_file(
    context: &Context,
    game_id: impl Into<String>,
    file_path: impl Into<String>,
    reshade_channel: impl Into<String>,
    game_context_token: Option<String>,
    shared_vulkan_context_token: Option<String>,
    progress: Option<&ProgressObserver<'_>>,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let file_path = file_path.into();
    let reshade_channel = parse_reshade_channel(reshade_channel)?;
    let safety = renderpilot_orchestration::FileSafetyAuthority::new().game_mutation_permits(
        game_id.clone(),
        game_context_token.as_deref(),
        shared_vulkan_context_token.as_deref(),
    )?;
    let bundle = renodx::manifest_store::get_or_fetch_bundle().await?;
    renodx::use_cases::commands::install::install_from_file(
        InstallRequest {
            context,
            manifest: &bundle.tool,
            reshade_sources: &bundle.reshade.sources,
            game_id: &game_id,
            requested_channel: reshade_channel,
            safety,
            allow_shared_vulkan_layer_install: true,
            progress,
        },
        &file_path,
    )
    .await?;
    to_json(renodx::use_cases::queries::status::status(
        context, &game_id,
    )?)
}

/// Uninstalls RenoDX from a game and returns the resulting install state.
pub fn renodx_uninstall(context: &Context, game_id: impl Into<String>) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    renodx::use_cases::commands::uninstall::uninstall(context, &game_id)?;
    to_json(renodx::use_cases::queries::status::status(
        context, &game_id,
    )?)
}

/// Switches the recorded ReShade host channel for a RenoDX install.
pub async fn renodx_switch_reshade_channel(
    context: &Context,
    game_id: impl Into<String>,
    reshade_channel: impl Into<String>,
    game_context_token: Option<String>,
    shared_vulkan_context_token: Option<String>,
    progress: Option<&ProgressObserver<'_>>,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let reshade_channel = parse_reshade_channel(reshade_channel)?;
    let safety = renderpilot_orchestration::FileSafetyAuthority::new().game_mutation_permits(
        game_id.clone(),
        game_context_token.as_deref(),
        shared_vulkan_context_token.as_deref(),
    )?;
    let bundle = renodx::manifest_store::get_or_fetch_bundle().await?;
    let state = renodx::use_cases::commands::switch_reshade_channel::switch_reshade_channel(
        renodx::use_cases::commands::switch_reshade_channel::SwitchChannelRequest {
            context,
            manifest: &bundle.tool,
            reshade_sources: &bundle.reshade.sources,
            game_id: &game_id,
            target_channel: reshade_channel,
            safety,
            progress,
        },
    )
    .await?;
    to_json(state)
}

/// Returns the shared ReShade Vulkan layer status (`not_installed` / `installed`
/// / `external_read_only` / `conflict` / `unsupported`), so the UI can decide
/// whether a Vulkan install can use or manage the system-wide layer.
pub fn renodx_vulkan_layer_status() -> JsonResult {
    to_json(renodx::use_cases::queries::vulkan_layer::status())
}

/// Returns the settings-facing shared Vulkan layer management report.
pub async fn renodx_vulkan_layer_management_status(context: &Context) -> JsonResult {
    let bundle = renodx::manifest_store::get_or_fetch_bundle().await?;
    to_json(
        renodx::use_cases::queries::vulkan_layer::management_status(
            context,
            &bundle.reshade.sources,
        )
        .await,
    )
}

/// Applies the shared ReShade Vulkan layer for the selected settings channel and
/// returns the refreshed management report.
pub async fn renodx_apply_vulkan_layer(
    context: &Context,
    reshade_channel: impl Into<String>,
    shared_vulkan_context_token: Option<String>,
    progress: Option<&ProgressObserver<'_>>,
) -> JsonResult {
    let reshade_channel = parse_reshade_channel(reshade_channel)?;
    let safety = renderpilot_orchestration::FileSafetyAuthority::new()
        .shared_vulkan_permit(shared_vulkan_context_token.as_deref())?;
    let bundle = renodx::manifest_store::get_or_fetch_bundle().await?;
    to_json(
        renodx::use_cases::commands::shared_vulkan_layer::apply_vulkan_layer(
            context,
            &bundle.reshade.sources,
            reshade_channel,
            safety,
            progress,
        )
        .await?,
    )
}

/// Removes RenderPilot's shared ReShade Vulkan layer (an external layer is left
/// untouched), returning the resulting layer status. A user maintenance action;
/// per-game installs are unaffected but stop loading until a layer is present again.
pub async fn renodx_remove_vulkan_layer(context: &Context) -> JsonResult {
    renodx::use_cases::commands::shared_vulkan_layer::remove_vulkan_layer(context).await?;
    to_json(renodx::use_cases::queries::vulkan_layer::status())
}

/// Checks whether the installed RenoDX add-on for a game has an upstream update.
///
/// RenoDX ships rolling snapshots, so this compares the recorded source against
/// upstream (cheap `HEAD`/ETag first, digest fallback). Returns a per-source
/// update report.
///
/// Catalogue/cache resolution and upstream HEAD/digest failures soft-fail to
/// overall `unknown` — check never hard-fails on network. Install/update still
/// hard-require a resolvable catalogue.
pub async fn renodx_check_update(context: &Context, game_id: impl Into<String>) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let Ok(bundle) = renodx::manifest_store::get_or_fetch_bundle().await else {
        return to_json(RenoDxUpdateReport::new(
            Some(UpdateStatus::Unknown),
            None,
            None,
        ));
    };
    to_json(
        renodx::use_cases::queries::updates::check_update(
            context,
            &bundle.tool,
            &bundle.reshade.sources,
            &game_id,
        )
        .await?,
    )
}

/// Applies an available RenoDX update for a game (re-fetch + atomic in-place
/// replace), reporting download progress, and returns the resulting install state.
/// This updates the add-on and applies the ReShade host policy for the active slot,
/// regardless of who originally placed that host.
pub async fn renodx_update(
    context: &Context,
    game_id: impl Into<String>,
    game_context_token: Option<String>,
    shared_vulkan_context_token: Option<String>,
    progress: Option<&ProgressObserver<'_>>,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let safety = renderpilot_orchestration::FileSafetyAuthority::new().game_mutation_permits(
        game_id.clone(),
        game_context_token.as_deref(),
        shared_vulkan_context_token.as_deref(),
    )?;
    let bundle = renodx::manifest_store::get_or_fetch_bundle().await?;
    renodx::use_cases::commands::update::update(
        renodx::use_cases::commands::update::UpdateRequest {
            context,
            manifest: &bundle.tool,
            reshade_sources: &bundle.reshade.sources,
            game_id: &game_id,
            safety,
            progress,
        },
    )
    .await?;
    to_json(renodx::use_cases::queries::status::status(
        context, &game_id,
    )?)
}

/// Installs the DLSS-Fix companion add-on for a game that already has RenoDX,
/// reporting download progress, and returns the resulting install state.
pub async fn renodx_install_dlss_fix(
    context: &Context,
    game_id: impl Into<String>,
    game_context_token: Option<String>,
    progress: Option<&ProgressObserver<'_>>,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let safety = renderpilot_orchestration::FileSafetyAuthority::new()
        .game_permit(game_id.clone(), game_context_token.as_deref())?;
    let state = renodx::use_cases::commands::dlss_fix::install_dlss_fix(
        context, &game_id, safety, progress,
    )
    .await?;
    to_json(state)
}

/// Updates or payload-repairs only the DLSS-Fix companion. This intentionally
/// bypasses generic RenoDX add-on, host, and shared-Vulkan policy.
pub async fn renodx_update_dlss_fix(
    context: &Context,
    game_id: impl Into<String>,
    game_context_token: Option<String>,
    progress: Option<&ProgressObserver<'_>>,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let safety = renderpilot_orchestration::FileSafetyAuthority::new()
        .game_permit(game_id.clone(), game_context_token.as_deref())?;
    let state =
        renodx::use_cases::commands::dlss_fix::update_dlss_fix(context, &game_id, safety, progress)
            .await?;
    to_json(state)
}

/// Retries pending DLSS-Fix transaction recovery without downloading or
/// evaluating generic RenoDX/ReShade update policy.
pub fn renodx_retry_dlss_fix_recovery(context: &Context, game_id: impl Into<String>) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let state = renodx::use_cases::commands::dlss_fix::retry_dlss_fix_recovery(context, &game_id)?;
    to_json(state)
}

/// Removes the DLSS-Fix companion add-on, leaving the main RenoDX install intact,
/// and returns the resulting install state.
pub fn renodx_uninstall_dlss_fix(context: &Context, game_id: impl Into<String>) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let state = renodx::use_cases::commands::dlss_fix::uninstall_dlss_fix(context, &game_id)?;
    to_json(state)
}

/// Returns the explicit DLSS-Fix ownership and action projection.
pub fn renodx_dlss_fix_availability(context: &Context, game_id: impl Into<String>) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    to_json(renodx::use_cases::queries::dlss_fix::availability(
        context, &game_id,
    )?)
}

fn parse_reshade_channel(value: impl Into<String>) -> Result<ReshadeChannel, crate::ApiError> {
    let value = value.into();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(ReshadeChannel::Stable);
    }
    trimmed
        .parse()
        .map_err(|error: renodx::types::ReshadeChannelParseError| {
            renderpilot_orchestration::ServiceError::invalid_input(error.to_string()).into()
        })
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::task::{Context as TaskContext, Poll, Waker};

    use renderpilot_orchestration::{SafetyScope, ServiceError};

    use super::*;

    fn poll_ready<F: Future>(future: F) -> F::Output {
        let mut task_context = TaskContext::from_waker(Waker::noop());
        let mut future = Box::pin(future);
        match Future::poll(future.as_mut(), &mut task_context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("missing safety context reached an async prepare boundary"),
        }
    }

    fn assert_missing_game_context(error: &crate::ApiError) {
        assert!(matches!(
            error,
            crate::ApiError::Service(ServiceError::SafetyContextMissing {
                scope: SafetyScope::Game(_)
            })
        ));
    }

    #[test]
    fn risky_renodx_commands_reject_missing_game_context_before_async_prepare() {
        let dir = tempfile::tempdir().expect("tempdir");
        let context = Context::open_at(dir.path().join("catalog.sqlite")).expect("context");
        let game_id = "manual:missing-safety";

        assert_missing_game_context(
            &poll_ready(renodx_install(
                &context, game_id, "stable", None, None, None,
            ))
            .expect_err("install must require game context"),
        );
        assert_missing_game_context(
            &poll_ready(renodx_install_from_file(
                &context,
                game_id,
                "unused.addon64",
                "stable",
                None,
                None,
                None,
            ))
            .expect_err("manual install must require game context"),
        );
        assert_missing_game_context(
            &poll_ready(renodx_switch_reshade_channel(
                &context, game_id, "stable", None, None, None,
            ))
            .expect_err("channel switch must require game context"),
        );
        assert_missing_game_context(
            &poll_ready(renodx_update(&context, game_id, None, None, None))
                .expect_err("update must require game context"),
        );
        assert_missing_game_context(
            &poll_ready(renodx_install_dlss_fix(&context, game_id, None, None))
                .expect_err("DLSS-Fix install must require game context"),
        );
        assert_missing_game_context(
            &poll_ready(renodx_update_dlss_fix(&context, game_id, None, None))
                .expect_err("DLSS-Fix update must require game context"),
        );
    }

    #[test]
    fn shared_vulkan_apply_rejects_missing_global_context_before_async_prepare() {
        let dir = tempfile::tempdir().expect("tempdir");
        let context = Context::open_at(dir.path().join("catalog.sqlite")).expect("context");

        let error = poll_ready(renodx_apply_vulkan_layer(&context, "stable", None, None))
            .expect_err("shared apply must require global context");

        assert!(matches!(
            error,
            crate::ApiError::Service(ServiceError::SafetyContextMissing {
                scope: SafetyScope::SharedVulkan
            })
        ));
    }
}
