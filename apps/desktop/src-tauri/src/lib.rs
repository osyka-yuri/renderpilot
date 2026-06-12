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
    /// `true` if we attempted elevation and the user (or group policy)
    /// said no — used to surface a "Relaunch as administrator" banner.
    pub elevation_user_declined: bool,
    /// `true` if we already invoked `ShellExecuteExW` once in this session,
    /// regardless of outcome. Stops infinite UAC prompt loops.
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

    /// User cancelled or group policy blocked the UAC prompt.
    #[cfg(all(windows, not(debug_assertions)))]
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
    #[cfg(all(windows, not(debug_assertions)))]
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
fn apply_portable_mode() {
    use renderpilot_orchestration::portable::APP_DIR_ENV;

    if std::env::var_os(APP_DIR_ENV).is_some() {
        return; // already set (e.g. by the user)
    }

    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            log::warn!("Portable mode: could not resolve exe path, falling back to standard data directory: {error}");
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
fn apply_webview2_elevation_workaround() {
    use std::path::PathBuf;
    if std::env::var_os("WEBVIEW2_USER_DATA_FOLDER").is_none() {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let path = PathBuf::from(local).join("RenderPilot").join("WebView2");
            // SAFETY: single-threaded during startup, before any plugin init.
            unsafe {
                std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", path);
            }
        }
    }
}

#[cfg(windows)]
fn compute_initialization_state() -> AppInitializationState {
    use elevation::{current_elevation, ElevationState};

    // Already running elevated — nothing more to do.
    if matches!(current_elevation(), ElevationState::Elevated) {
        return AppInitializationState::elevated();
    }

    resolve_unelevated_startup()
}

/// Debug builds skip the startup auto-relaunch.
///
/// `cargo tauri dev` spawns both the Vite dev server and the app binary as
/// child processes. When the initial (un-elevated) binary exits to hand off
/// to the elevated copy, the Tauri CLI detects the child death and tears
/// down the Vite server — the elevated copy then starts to a blank window
/// because `http://localhost:1420` no longer exists.
///
/// Skipping auto-relaunch lets the dev session keep running normally.
/// The in-app `ElevationBanner` still appears and its "Relaunch as
/// administrator" button still works; developers who need NVAPI write access
/// can also run the compiled binary directly as administrator.
#[cfg(windows)]
#[cfg(debug_assertions)]
fn resolve_unelevated_startup() -> AppInitializationState {
    AppInitializationState {
        is_elevated: false,
        elevation_supported: true,
        elevation_user_declined: false,
        elevation_attempted: false,
        relaunch_in_progress: false,
    }
}

/// Release builds attempt a UAC auto-relaunch on first startup.
#[cfg(windows)]
#[cfg(not(debug_assertions))]
fn resolve_unelevated_startup() -> AppInitializationState {
    use elevation::{
        attempt_self_relaunch_elevated, has_elevation_attempted_marker, ElevationStartupDecision,
    };

    // This session already tried — don't loop on the relaunch.
    if has_elevation_attempted_marker() {
        return AppInitializationState::declined();
    }

    // First attempt this session.
    match attempt_self_relaunch_elevated() {
        ElevationStartupDecision::Relaunched => AppInitializationState::relaunching(),
        ElevationStartupDecision::UserCancelled | ElevationStartupDecision::PolicyBlocked(_) => {
            AppInitializationState::declined()
        }
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
        commands::set_game_executable_override,
        commands::clear_game_executable_override,
        commands::get_nvapi_setting_state,
        commands::set_nvapi_setting_value,
        commands::revert_nvapi_setting,
        // DLSS indicator (system-wide)
        commands::get_dlss_indicator_state,
        commands::set_dlss_indicator_enabled,
        // App initialization / elevation
        commands::get_app_initialization_state,
        commands::request_admin_relaunch,
    ])
}

/// Reports a startup failure and terminates the process.
fn exit_with_startup_error(error: tauri::Error) -> ! {
    eprintln!("{APP_NAME}: failed to run desktop shell: {error}");
    std::process::exit(STARTUP_FAILURE_EXIT_CODE);
}
