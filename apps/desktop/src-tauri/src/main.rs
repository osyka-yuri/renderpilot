#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Binary entry point for the RenderPilot desktop shell.

fn main() {
    renderpilot_desktop::run();
}
