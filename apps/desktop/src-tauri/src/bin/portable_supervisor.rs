#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
//! Stable manual-only portable supervisor binary.

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    renderpilot_desktop::run_portable_supervisor()
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    std::process::ExitCode::SUCCESS
}
