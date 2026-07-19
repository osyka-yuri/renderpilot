//! Tauri desktop entry point for RenderPilot.

mod commands;
#[cfg(windows)]
mod elevation;

use std::sync::Arc;

use renderpilot_orchestration::Context;
use serde::Serialize;
use tauri::{Builder, Manager, Wry};

const APP_NAME: &str = "RenderPilot";
const STARTUP_FAILURE_EXIT_CODE: i32 = 1;

type DesktopBuilder = Builder<Wry>;

/// Initialization snapshot computed once at process start.
///
/// Exposed to the UI via the `get_app_initialization_state` Tauri command.
/// Only the boolean projection is part of the IPC contract — everything
/// else is internal to the startup flow.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInitializationState {
    /// `true` if the current process is running with administrator rights.
    pub is_elevated: bool,
    /// `false` on non-Windows platforms; UI hides the elevation banner.
    pub elevation_supported: bool,
    /// Degraded unelevated mode after cancel, policy block, or live handoff skip.
    /// IPC name kept for compatibility; not only a literal UAC cancel.
    /// Banner visibility still keys off `!is_elevated && elevation_supported`.
    pub elevation_user_declined: bool,
    /// Startup auto-elevation was attempted or skipped due to a live handoff
    /// marker (file-based anti-loop). Does not mean UserRequest was shown.
    pub elevation_attempted: bool,
    /// Internal-only: `true` if an elevated relaunch is starting and the
    /// current (un-elevated) process should return from `run` immediately.
    /// Never serialized.
    #[serde(skip)]
    pub relaunch_in_progress: bool,
}

impl AppInitializationState {
    /// Running elevated — no further action needed.
    #[cfg(windows)]
    fn elevated() -> Self {
        Self {
            is_elevated: true,
            elevation_supported: true,
            elevation_user_declined: false,
            elevation_attempted: false,
            relaunch_in_progress: false,
        }
    }

    /// Degraded unelevated mode after cancel, policy block, or live handoff skip.
    /// Field `elevation_user_declined` remains the IPC name for the banner flag.
    #[cfg(windows)]
    fn declined() -> Self {
        Self {
            is_elevated: false,
            elevation_supported: true,
            elevation_user_declined: true,
            elevation_attempted: true,
            relaunch_in_progress: false,
        }
    }

    /// Elevated relaunch is starting; current (un-elevated) process should exit.
    #[cfg(windows)]
    fn relaunching() -> Self {
        Self {
            is_elevated: false,
            elevation_supported: true,
            elevation_user_declined: false,
            elevation_attempted: true,
            relaunch_in_progress: true,
        }
    }

    /// Non-Windows platform — elevation concept does not apply.
    #[cfg(not(windows))]
    fn unsupported() -> Self {
        Self {
            is_elevated: true,
            elevation_supported: false,
            elevation_user_declined: false,
            elevation_attempted: false,
            relaunch_in_progress: false,
        }
    }
}

/// Runs the desktop shell.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    #[cfg(feature = "portable")]
    apply_portable_mode();

    #[cfg(windows)]
    apply_webview2_elevation_workaround();

    let init_state = compute_initialization_state();
    if init_state.relaunch_in_progress {
        // Elevated copy is starting; exit the un-elevated process cleanly.
        return;
    }

    if let Err(error) = run_desktop_shell(init_state) {
        exit_with_startup_error(error);
    }
}

/// Redirects all persistent data to `<exe_dir>/data` by setting
/// `RENDERPILOT_APP_DIR` and `WEBVIEW2_USER_DATA_FOLDER` before any other
/// subsystem initialises.  Both env vars are idempotent — they are only set
/// when not already present, so the user can still override them manually.
#[cfg(feature = "portable")]
#[expect(
    unsafe_code,
    reason = "std::env::set_var is unsafe in edition 2024; only called single-threaded at process start"
)]
fn apply_portable_mode() {
    use renderpilot_orchestration::portable::APP_DIR_ENV;

    if std::env::var_os(APP_DIR_ENV).is_some() {
        return; // already set (e.g. by the user)
    }

    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            log::warn!(
                "Portable mode: could not resolve exe path, falling back to standard data directory: {error}"
            );
            return;
        }
    };
    let Some(exe_dir) = exe.parent() else {
        log::warn!(
            "Portable mode: exe has no parent directory, falling back to standard data directory"
        );
        return;
    };

    let data_dir = exe_dir.join("data");

    // SAFETY: single-threaded during startup, before any plugin or thread init.
    unsafe {
        std::env::set_var(APP_DIR_ENV, &data_dir);
    }

    if std::env::var_os("WEBVIEW2_USER_DATA_FOLDER").is_none() {
        // SAFETY: same as above.
        unsafe {
            std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", data_dir.join("WebView2"));
        }
    }
}

/// Pins the WebView2 user data folder to `%LOCALAPPDATA%\RenderPilot\WebView2`
/// so elevated and non-elevated sessions of the app share a cache and don't
/// fight over default per-user state directories (which has caused blank-window
/// regressions in elevated processes). Idempotent: only sets the env var if
/// the user has not provided one.
#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "std::env::set_var is unsafe in edition 2024; only called single-threaded at process start"
)]
fn apply_webview2_elevation_workaround() {
    if std::env::var_os("WEBVIEW2_USER_DATA_FOLDER").is_none()
        && std::env::var_os("LOCALAPPDATA").is_some()
    {
        let path = elevation::renderpilot_local_data_dir().join("WebView2");
        // SAFETY: single-threaded during startup, before any plugin init.
        unsafe {
            std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", path);
        }
    }
}

#[cfg(windows)]
fn compute_initialization_state() -> AppInitializationState {
    use elevation::{ElevationState, clear_elevation_handoff_marker, current_elevation};

    // Already running elevated — clear any handoff marker from the unelevated
    // parent so a later NSIS `/ARGS` restart is not mistaken for a failed
    // elevation attempt, then proceed normally.
    if matches!(current_elevation(), ElevationState::Elevated) {
        clear_elevation_handoff_marker();
        return AppInitializationState::elevated();
    }

    resolve_unelevated_startup()
}

/// Unelevated process startup policy.
///
/// **Debug:** skips auto-relaunch. `cargo tauri dev` owns the Vite server as a
/// sibling of this process; exiting to hand off elevation would tear Vite down
/// and leave the elevated copy on a blank `localhost` window. The in-app
/// elevation banner and "Relaunch as administrator" still work.
///
/// **Release:** attempts UAC auto-relaunch (handoff anti-loop is inside
/// [`elevation::attempt_self_relaunch_elevated`]).
#[cfg(windows)]
fn resolve_unelevated_startup() -> AppInitializationState {
    // Keep both arms in one function so release-only triggers stay type-checked
    // and variant-used in debug builds (avoids cfg-split dead_code noise).
    if cfg!(debug_assertions) {
        return AppInitializationState {
            is_elevated: false,
            elevation_supported: true,
            elevation_user_declined: false,
            elevation_attempted: false,
            relaunch_in_progress: false,
        };
    }

    use elevation::{
        ElevationRelaunchTrigger, ElevationStartupDecision, attempt_self_relaunch_elevated,
    };

    match attempt_self_relaunch_elevated(ElevationRelaunchTrigger::StartupAuto) {
        ElevationStartupDecision::Relaunched => AppInitializationState::relaunching(),
        ElevationStartupDecision::UserCancelled
        | ElevationStartupDecision::PolicyBlocked(_)
        | ElevationStartupDecision::SkippedRecentHandoff => AppInitializationState::declined(),
    }
}

#[cfg(not(windows))]
fn compute_initialization_state() -> AppInitializationState {
    AppInitializationState::unsupported()
}

/// Builds and runs the Tauri application.
fn run_desktop_shell(init_state: AppInitializationState) -> tauri::Result<()> {
    create_desktop_builder(init_state).run(tauri::generate_context!())
}

/// Creates the Tauri builder used by the desktop shell.
fn create_desktop_builder(init_state: AppInitializationState) -> DesktopBuilder {
    configure_cover_protocol(configure_commands(configure_plugins(Builder::default()))).setup(
        move |app| {
            app.manage(init_state);
            // Propagate (don't panic) so a catalog-open failure routes through the
            // graceful `exit_with_startup_error` path like any other startup error.
            let context = Arc::new(Context::open()?);
            app.manage(context.clone());
            log::info!(
                "Started with is_elevated={}, user_declined={}, attempted={}",
                init_state.is_elevated,
                init_state.elevation_user_declined,
                init_state.elevation_attempted
            );
            renderpilot_api::gc_cover_orphans_on_startup(&context);
            refresh_libraries_manifest_in_background();
            refresh_catalog_addon_capabilities_in_background(context);
            Ok(())
        },
    )
}

fn refresh_libraries_manifest_in_background() {
    tauri::async_runtime::spawn(async {
        if let Err(error) = renderpilot_api::fetch_libraries_manifest().await {
            log::warn!("Failed to refresh libraries manifest on startup: {error}");
        }
    });
}

/// Warms profile-derived add-on capability flags so catalog badges/filters work
/// on cold start without requiring a library rescan.
fn refresh_catalog_addon_capabilities_in_background(context: Arc<Context>) {
    tauri::async_runtime::spawn(async move {
        commands::addon_catalog::refresh_catalog_addon_capabilities(context).await;
    });
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
        commands::scan_manual_folder,
        commands::scan_auto_libraries,
        // Remote CDN manifests (shell Refresh force path)
        commands::refresh_remote_manifests,
        // Game data
        commands::query_game_cards,
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
        commands::rollback_component,
        // Libraries
        commands::fetch_libraries_manifest,
        commands::get_libraries_manifest,
        commands::download_library,
        commands::download_artifact,
        commands::delete_library,
        commands::get_library_states,
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
        // RenoDX HDR add-on
        commands::renodx_status,
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
        commands::luma_availability,
        commands::luma_install,
        commands::luma_uninstall,
        commands::luma_check_update,
        commands::luma_update,
        commands::renodx_check_updates,
        commands::renodx_install_dlss_fix,
        commands::renodx_uninstall_dlss_fix,
        commands::renodx_dlss_fix_availability,
        // App initialization / elevation
        commands::get_app_initialization_state,
        commands::request_admin_relaunch,
    ])
}

/// Reports a startup failure and terminates the process.
// Diverging sink: consuming the error by value is the point.
#[expect(
    clippy::needless_pass_by_value,
    reason = "startup failure sink consumes the error by value before process exit"
)]
fn exit_with_startup_error(error: tauri::Error) -> ! {
    eprintln!("{APP_NAME}: failed to run desktop shell: {error}");
    std::process::exit(STARTUP_FAILURE_EXIT_CODE);
}
