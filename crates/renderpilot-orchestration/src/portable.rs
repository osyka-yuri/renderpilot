//! Constants shared between path-resolution modules for portable-mode support.
//!
//! When `RENDERPILOT_APP_DIR` is set (done at process start by the portable
//! desktop launcher), both the catalog database and the library storage use
//! that directory as their root instead of `%LOCALAPPDATA%\RenderPilot`.

/// Environment variable that overrides the application data root directory.
///
/// When set, all persistent data is stored under this path:
/// - catalog database  → `$RENDERPILOT_APP_DIR/catalog.db`
/// - library manifests → `$RENDERPILOT_APP_DIR/libraries_manifest.json`
/// - library archives  → `$RENDERPILOT_APP_DIR/libraries/…`
/// - cover images      → `$RENDERPILOT_APP_DIR/covers/…` (relative to catalog)
pub const APP_DIR_ENV: &str = "RENDERPILOT_APP_DIR";
