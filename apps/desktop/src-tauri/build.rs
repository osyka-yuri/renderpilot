//! Build script for the RenderPilot Tauri desktop shell.
//!
//! Tauri 2's `tauri-build` already embeds a Windows manifest with
//! `requestedExecutionLevel = asInvoker` and PerMonitorV2 DPI awareness,
//! which is exactly what we need. The EXE therefore does not auto-prompt
//! UAC at launch — elevation is requested at runtime in `src/elevation.rs`
//! (ShellExecuteExW with verb=runas) when needed; if the user cancels the
//! UAC dialog the app keeps running with NVAPI writes disabled.
//!
//! If we ever need to override the default Tauri manifest, do NOT use
//! `embed-manifest` / `embed-resource` here — those collide with the
//! manifest baked into `resource.lib` by `tauri-build` (CVT1100). Use a
//! custom `app.manifest` referenced via `tauri.conf.json` instead.

fn main() {
    tauri_build::build();
}
