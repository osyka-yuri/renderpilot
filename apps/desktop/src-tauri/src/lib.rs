//! Tauri desktop entry point for RenderPilot.

mod backend_diagnostics;
mod command_error_contract;
mod commands;
mod diagnostic_event;
#[cfg(all(windows, feature = "portable"))]
mod diagnostics;
#[cfg(all(windows, feature = "portable"))]
mod portable_runtime;
#[cfg(all(windows, feature = "portable"))]
mod updater_signature;
#[cfg(windows)]
mod webview_runtime;

use std::sync::Arc;

use renderpilot_orchestration::Context;
use tauri::{Builder, Manager, Wry};

const APP_NAME: &str = "RenderPilot";
const STARTUP_FAILURE_EXIT_CODE: i32 = 1;

type DesktopBuilder = Builder<Wry>;

/// Runs the desktop shell.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(all(windows, feature = "portable"))]
    let portable_startup = match portable_runtime::bootstrap::dispatch_before_desktop() {
        Ok(portable_runtime::bootstrap::EarlyDispatch::DirectLaunchExit) => return,
        Ok(portable_runtime::bootstrap::EarlyDispatch::App(startup)) => startup,
        Err(error) => exit_with_portable_startup_error("bootstrap", &error),
    };

    #[cfg(all(windows, feature = "portable"))]
    if let Err(error) = portable_runtime::runtime_paths::install_from_startup(&portable_startup) {
        exit_with_portable_startup_error("runtime-path installation", &error);
    }

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    #[cfg(all(windows, feature = "portable"))]
    portable_runtime::diagnostics_files::install_app(&portable_startup);

    let context = tauri::generate_context!();

    #[cfg(windows)]
    webview_runtime::enforce_minimum_version(&context);

    #[cfg(all(windows, feature = "portable"))]
    portable_runtime::diagnostics_files::app_milestone(
        diagnostics::PortableMilestone::WebviewRuntimeReady,
    );

    #[cfg(windows)]
    webview_runtime::configure_user_data_path();

    if let Err(error) = run_desktop_shell(context) {
        exit_with_startup_error(&error);
    }
}

/// Stable raw portable-supervisor entry point. It never initializes desktop
/// logging, storage, WebView2, Tauri, or GUI before supervisor authority exists.
#[cfg(all(windows, feature = "portable"))]
pub fn run_portable_supervisor() -> std::process::ExitCode {
    let args = std::env::args_os().collect::<Vec<_>>();
    match portable_runtime::supervisor::dispatch_raw_or_supervisor(&args) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{APP_NAME}: portable supervisor failed: {error}");
            portable_runtime::win32::show_portable_supervisor_failure(&error);
            std::process::ExitCode::FAILURE
        }
    }
}

/// Verifies one updater artifact with the exact effective build-time key.
///
/// This tooling API is excluded from both distributed application variants.
#[cfg(all(windows, feature = "updater-artifact-verify"))]
#[doc(hidden)]
pub fn verify_updater_artifact(
    artifact: &std::path::Path,
    signature: &std::path::Path,
) -> Result<(), String> {
    updater_signature::verify_files(artifact, signature)
}

/// Builds and runs the Tauri application.
fn run_desktop_shell(context: tauri::Context<Wry>) -> tauri::Result<()> {
    let app = create_desktop_builder().build(context)?;
    app.run(|_handle, _event| {
        #[cfg(all(windows, feature = "portable"))]
        match _event {
            tauri::RunEvent::Ready => {
                portable_runtime::diagnostics_files::app_milestone(
                    diagnostics::PortableMilestone::DesktopShellReady,
                );
            }
            tauri::RunEvent::Exit => {
                portable_runtime::diagnostics_files::app_milestone(
                    diagnostics::PortableMilestone::ControlledExit,
                );
                portable_runtime::diagnostics_files::shutdown_app();
            }
            _ => {}
        }
    });
    Ok(())
}

/// Creates the Tauri builder used by the desktop shell.
fn create_desktop_builder() -> DesktopBuilder {
    configure_cover_protocol(configure_commands(configure_plugins(Builder::default()))).setup(
        move |app| {
            app.manage(commands::AppUpdateState::default());
            #[cfg(not(all(windows, feature = "portable")))]
            {
                // Propagate (don't panic) so a catalog-open failure routes through the
                // graceful `exit_with_startup_error` path like any other startup error.
                let context = Arc::new(Context::open()?);
                app.manage(context);
            }
            Ok(())
        },
    )
}

/// Completes a portable trial only after the visible compiled UI has called
/// the request-only readiness command. Context creation is deliberately after
/// the supervisor's durable CommitPermit, never during TrialReadOnly.
#[cfg(all(windows, feature = "portable"))]
pub(crate) fn complete_portable_activation(app: &tauri::AppHandle) -> Result<(), String> {
    let result = portable_runtime::activation::prove_visible_and_commit(app, |catalog| {
        let context = Arc::new(match catalog {
            portable_runtime::app_catalog_migration::CatalogClassification::Fresh => {
                Context::open_fresh_portable_after_commit().map_err(|error| {
                    portable_runtime::error::PortableRuntimeError::new(
                        "portable_fresh_catalog_commit",
                        error.to_string(),
                    )
                })?
            }
            portable_runtime::app_catalog_migration::CatalogClassification::Existing { .. } => {
                Context::open_current_portable_after_commit().map_err(|error| {
                    portable_runtime::error::PortableRuntimeError::new(
                        "portable_context",
                        error.to_string(),
                    )
                })?
            }
        });
        if !app.manage(context) {
            return Err(portable_runtime::error::PortableRuntimeError::new(
                "portable_context",
                "portable Context was already installed",
            ));
        }
        Ok(())
    });
    if let Err(error) = result {
        portable_runtime::diagnostics_files::app_failure(
            crate::diagnostics::PortableFailureSite::ActivationCommit,
            &error,
        );
        app.exit(STARTUP_FAILURE_EXIT_CODE);
        return Err(error.to_string());
    }
    Ok(())
}

fn configure_cover_protocol(builder: DesktopBuilder) -> DesktopBuilder {
    builder.register_asynchronous_uri_scheme_protocol("rp-cover", |ctx, request, responder| {
        // Resolve the shared context on the webview thread (cheap), then hand the
        // blocking SQLite + filesystem lookup to a worker so the UI stays responsive.
        let context = ctx
            .app_handle()
            .try_state::<Arc<Context>>()
            .map(|state| state.inner().clone());
        let path = request.uri().path().to_owned();

        tauri::async_runtime::spawn_blocking(move || {
            // Always answer, never panic: a missing context degrades to NOT_FOUND.
            let response = match context {
                Some(context) => renderpilot_api::cover_asset_protocol_response(&context, &path),
                None => renderpilot_api::cover_unavailable_response(),
            };

            responder.respond(response);
        });
    })
}

/// Registers Tauri plugins.
///
/// Keep this function focused on plugin registration only.
fn configure_plugins(builder: DesktopBuilder) -> DesktopBuilder {
    let builder = builder
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init());

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    builder
}

/// Registers commands exposed to the frontend.
///
/// Commands are grouped by domain to keep the invoke surface easy to audit.
fn configure_commands(builder: DesktopBuilder) -> DesktopBuilder {
    builder.invoke_handler(tauri::generate_handler![
        // Library scanning
        commands::inspect_game_install,
        commands::add_game,
        commands::remove_game_from_catalog,
        commands::scan_auto_libraries,
        commands::start_background_refresh,
        // Remote CDN manifests (shell Refresh force path)
        commands::refresh_remote_manifests,
        commands::refresh_catalog_capabilities,
        // Game data
        commands::query_game_cards,
        commands::bootstrap_games_catalog,
        commands::get_game_details,
        commands::fetch_game_cover,
        commands::clear_game_cover,
        commands::set_game_cover,
        commands::set_game_favorite,
        commands::set_game_hidden,
        commands::get_catalog_setting,
        commands::set_catalog_setting,
        // Operations
        commands::apply_swap,
        commands::plan_swap,
        commands::rollback_component,
        commands::plan_rollback,
        // Libraries
        commands::list_library_packages,
        commands::download_library_package,
        commands::download_artifact,
        commands::delete_library_package,
        // NVAPI / DLSS presets
        commands::list_nvapi_supported_settings,
        commands::list_nvapi_setting_states,
        commands::list_game_executable_candidates,
        commands::resolve_game_executable,
        commands::set_game_executable_override,
        commands::clear_game_executable_override,
        commands::get_nvapi_setting_state,
        commands::set_nvapi_setting_value,
        commands::revert_nvapi_setting,
        // Global (base profile) NVAPI settings
        commands::list_global_nvapi_setting_states,
        commands::set_global_nvapi_setting_value,
        commands::revert_global_nvapi_setting,
        // DLSS indicator (system-wide)
        commands::get_dlss_indicator_state,
        commands::set_dlss_indicator_enabled,
        // RenoDX HDR add-on (card/settings surface; CLI status/bulk stay off IPC)
        commands::renodx_availability,
        commands::renodx_install,
        commands::renodx_install_from_file,
        commands::renodx_switch_reshade_channel,
        commands::renodx_uninstall,
        commands::renodx_vulkan_layer_status,
        commands::renodx_vulkan_layer_management_status,
        commands::renodx_apply_vulkan_layer,
        commands::renodx_remove_vulkan_layer,
        commands::renodx_check_update,
        commands::renodx_update,
        commands::renodx_install_dlss_fix,
        commands::renodx_update_dlss_fix,
        commands::renodx_retry_dlss_fix_recovery,
        commands::renodx_uninstall_dlss_fix,
        commands::renodx_dlss_fix_availability,
        // Luma Framework add-on (card surface; CLI status/bulk stay off IPC)
        commands::luma_availability,
        commands::luma_install,
        commands::luma_uninstall,
        commands::luma_check_update,
        commands::luma_update,
        // Application updater (Rust-owned trust boundary)
        commands::app_update_check,
        commands::app_update_download,
        commands::app_update_apply,
        commands::app_update_close,
        commands::portable_trial_ready,
    ])
}

#[cfg(all(windows, feature = "portable"))]
mod updater_contract {
    include!(concat!(env!("OUT_DIR"), "/updater_contract.rs"));
}

/// Reports a startup failure and terminates the process.
fn exit_with_startup_error(error: &tauri::Error) -> ! {
    #[cfg(all(windows, feature = "portable"))]
    {
        let diagnostic = portable_runtime::error::PortableRuntimeError::new(
            "portable_desktop_shell",
            error.to_string(),
        );
        portable_runtime::diagnostics_files::app_failure(
            crate::diagnostics::PortableFailureSite::DesktopShell,
            &diagnostic,
        );
        portable_runtime::diagnostics_files::app_milestone(
            crate::diagnostics::PortableMilestone::ControlledExit,
        );
        portable_runtime::diagnostics_files::shutdown_app();
    }
    eprintln!("{APP_NAME}: failed to run desktop shell: {error}");
    std::process::exit(STARTUP_FAILURE_EXIT_CODE);
}

/// Stops a managed portable App before logger, Tauri, WebView2, or any ordinary
/// desktop runtime has been initialized.
#[cfg(all(windows, feature = "portable"))]
fn exit_with_portable_startup_error(
    stage: &str,
    error: &portable_runtime::error::PortableRuntimeError,
) -> ! {
    eprintln!("{APP_NAME}: portable App {stage} failed: {error}");
    std::process::exit(STARTUP_FAILURE_EXIT_CODE);
}
