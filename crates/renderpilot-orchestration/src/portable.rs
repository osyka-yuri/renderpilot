//! Portable runtime path authority.
//!
//! Portable children receive one supervisor-derived path object before any
//! durable consumer is initialized. The object is deliberately process-wide
//! and single-assignment: an ambient environment variable is a compatibility
//! input for installed launches, never portable path authority.

use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

/// Environment variable that overrides the application data root directory.
///
/// Outside an authenticated portable child, this compatibility override stores
/// persistent data under the supplied path:
/// - catalog database  → `$RENDERPILOT_APP_DIR/catalog.db`
/// - active library catalog → `$RENDERPILOT_APP_DIR/libraries/v1/catalog.json`
/// - library archives  → `$RENDERPILOT_APP_DIR/libraries/…`
/// - cover images      → `$RENDERPILOT_APP_DIR/covers/…` (relative to catalog)
pub const APP_DIR_ENV: &str = "RENDERPILOT_APP_DIR";

/// Immutable durable locations for one authenticated portable App start.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RuntimePathsV1 {
    /// Stable raw-supervisor directory.
    pub portable_root: PathBuf,
    /// Stable portable data directory.
    pub data_root: PathBuf,
    /// Exact catalog database location.
    pub catalog_db_path: PathBuf,
    /// Exact transaction scratch location.
    pub file_transactions_root: PathBuf,
    /// Exact library cache root.
    pub libraries_root: PathBuf,
    /// Exact CDN cache root.
    pub cdn_cache_root: PathBuf,
    /// Exact cover cache root.
    pub covers_root: PathBuf,
    /// Exact WebView2 profile root.
    pub webview2_root: PathBuf,
    /// Admission and epoch namespace root.
    pub authority_root: PathBuf,
    /// Immutable generation and selection root.
    pub generation_store_root: PathBuf,
    /// Selected immutable generation directory.
    pub selected_generation_root: PathBuf,
    /// Selected immutable App executable.
    pub selected_app_executable: PathBuf,
    /// Supervisor-owned update scratch root.
    pub update_root: PathBuf,
}

static RUNTIME_PATHS: OnceLock<RuntimePathsV1> = OnceLock::new();

impl RuntimePathsV1 {
    /// Derives every stable portable location from a selected generation.
    pub fn from_portable_root(
        portable_root: PathBuf,
        selected_generation_root: &Path,
        selected_app_executable: &Path,
    ) -> Result<Self, String> {
        if portable_root.as_os_str().is_empty()
            || !selected_generation_root.starts_with(&portable_root)
            || !selected_app_executable.starts_with(selected_generation_root)
        {
            return Err("portable runtime paths are not rooted in the selected generation".into());
        }
        let data_root = portable_root.join("data");
        Ok(Self {
            catalog_db_path: data_root.join("catalog.db"),
            file_transactions_root: data_root.join("file-transactions"),
            libraries_root: data_root.join("libraries"),
            cdn_cache_root: data_root.clone(),
            covers_root: data_root.join("covers"),
            webview2_root: data_root.join("WebView2"),
            authority_root: portable_root
                .join(".renderpilot-runtime-authority")
                .join("v1"),
            generation_store_root: portable_root.join(".renderpilot-generations").join("v1"),
            update_root: portable_root.join(".renderpilot-update").join("v2"),
            portable_root,
            data_root,
            selected_generation_root: selected_generation_root.to_owned(),
            selected_app_executable: selected_app_executable.to_owned(),
        })
    }

    /// Validates all fixed containment relations before installation.
    pub fn validate(&self) -> Result<(), String> {
        let under_data = [
            &self.catalog_db_path,
            &self.file_transactions_root,
            &self.libraries_root,
            &self.cdn_cache_root,
            &self.covers_root,
            &self.webview2_root,
        ];
        if self.portable_root.as_os_str().is_empty()
            || under_data
                .iter()
                .any(|path| !path.starts_with(&self.data_root))
            || !self.authority_root.starts_with(&self.portable_root)
            || !self.generation_store_root.starts_with(&self.portable_root)
            || !self.update_root.starts_with(&self.portable_root)
            || !self
                .selected_generation_root
                .starts_with(&self.generation_store_root)
            || !self
                .selected_app_executable
                .starts_with(&self.selected_generation_root)
        {
            return Err("portable runtime path relationship was invalid".into());
        }
        Ok(())
    }
}

/// Installs the authenticated portable startup paths exactly once.
pub fn install_runtime_paths(paths: RuntimePathsV1) -> Result<(), String> {
    paths.validate()?;
    match RUNTIME_PATHS.set(paths) {
        Ok(()) => Ok(()),
        Err(next) if RUNTIME_PATHS.get() == Some(&next) => Ok(()),
        Err(_) => Err("portable runtime paths were already installed for another startup".into()),
    }
}

/// Returns the process-authenticated portable paths, when this is a portable App.
pub fn runtime_paths() -> Option<&'static RuntimePathsV1> {
    RUNTIME_PATHS.get()
}

/// True only after authenticated portable startup installed its path authority.
pub fn has_runtime_paths() -> bool {
    runtime_paths().is_some()
}

pub(crate) fn portable_data_root() -> Option<&'static Path> {
    runtime_paths().map(|paths| paths.data_root.as_path())
}
