//! Tauri desktop entry point for RenderPilot.

mod commands;

use tauri::{Builder, Wry};

const APP_NAME: &str = "RenderPilot";
const STARTUP_FAILURE_EXIT_CODE: i32 = 1;

type DesktopBuilder = Builder<Wry>;

/// Runs the desktop shell.
///
/// This function is intentionally small:
/// - startup remains fallible and testable through `run_desktop_shell`;
/// - process termination happens in exactly one place;
/// - startup errors are reported consistently.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if let Err(error) = run_desktop_shell() {
        exit_with_startup_error(error);
    }
}

/// Builds and runs the Tauri application.
fn run_desktop_shell() -> tauri::Result<()> {
    create_desktop_builder().run(tauri::generate_context!())
}

/// Creates the Tauri builder used by the desktop shell.
fn create_desktop_builder() -> DesktopBuilder {
    configure_commands(configure_plugins(Builder::default()))
}

/// Registers Tauri plugins.
///
/// Keep this function focused on plugin registration only.
fn configure_plugins(builder: DesktopBuilder) -> DesktopBuilder {
    builder.plugin(tauri_plugin_dialog::init())
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
        commands::get_game_cards,
        commands::get_game_details,
        // Operations
        commands::build_swap_plan,
        commands::apply_operation_plan,
        commands::rollback_operation,
    ])
}

/// Reports a startup failure and terminates the process.
///
/// This is the only place where startup failure is converted into process exit.
fn exit_with_startup_error(error: tauri::Error) -> ! {
    eprintln!("{APP_NAME}: failed to run desktop shell: {error}");
    std::process::exit(STARTUP_FAILURE_EXIT_CODE);
}
