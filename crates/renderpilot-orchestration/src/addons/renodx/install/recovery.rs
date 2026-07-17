use std::path::Path;

use renderpilot_domain::AddonKind;

use crate::addons::engine;

/// Recovers from a torn RenoDX proxy install (abandoned debris with no DB record).
pub fn recover_torn_install(scan_dirs: &[&Path]) {
    use std::fs;

    for dir in scan_dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if !entry.file_type().is_ok_and(|ft| ft.is_file()) {
                    continue;
                }
                let lower = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if crate::addons::renodx::tool::is_renodx_addon_file_name(&lower) {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }

    if let Some(game_dir) = scan_dirs.first()
        && !crate::addons::tool::unmanaged_files_present_in_dirs(scan_dirs, AddonKind::RenoDx)
    {
        engine::clear_torn_install_marker(game_dir, AddonKind::RenoDx);
    }
}
