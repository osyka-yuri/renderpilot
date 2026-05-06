//! Tauri desktop entry point for RenderPilot.

mod commands;

/// Runs the desktop shell.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_system_appearance,
            commands::scan_manual_folder,
            commands::get_game_cards,
            commands::get_game_details,
            commands::build_swap_plan,
            commands::apply_operation_plan,
            commands::rollback_operation,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run RenderPilot desktop shell");
}
