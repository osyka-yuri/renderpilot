//! Managed external dependencies (e.g. dgVoodoo2) declared on Luma titles.

use renderpilot_domain::GraphicsApi;
use serde::{Deserialize, Serialize};

/// Managed external dependency a Luma title needs before it can work.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
pub enum LumaExternalRequirement {
    /// dgVoodoo2 DirectX wrapper used by older Direct3D titles to expose a D3D11
    /// swapchain for Luma/ReShade.
    Dgvoodoo2 {
        /// Required dgVoodoo2 version.
        version: String,
        /// Detected render APIs accepted for this title because dgVoodoo2 will
        /// translate them to the API Luma actually hooks.
        accepted_detected_apis: Vec<GraphicsApi>,
        /// ReShade proxy DLL to install when this requirement applies.
        reshade_proxy_dll: String,
        /// dgVoodoo2 archive source RenderPilot downloads and verifies.
        source: ManagedArchiveSource,
        /// Files extracted from the archive into the game directory.
        install_map: Vec<ManagedInstallMapEntry>,
        /// Root-level archive file and game-directory target used as the
        /// dependency config base.
        config_file: String,
        /// Exact config keys RenderPilot merges into `config_file`.
        config: Vec<ExternalConfigSection>,
    },
}

impl LumaExternalRequirement {
    /// Detected graphics APIs this managed dependency can bridge.
    #[must_use]
    pub(crate) fn accepted_detected_apis(&self) -> &[GraphicsApi] {
        match self {
            Self::Dgvoodoo2 {
                accepted_detected_apis,
                ..
            } => accepted_detected_apis,
        }
    }

    /// ReShade proxy slot required by this managed dependency.
    #[must_use]
    pub(crate) fn reshade_proxy_dll(&self) -> &str {
        match self {
            Self::Dgvoodoo2 {
                reshade_proxy_dll, ..
            } => reshade_proxy_dll,
        }
    }
}

/// Managed archive source.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedArchiveSource {
    /// HTTPS URL of the archive.
    pub url: String,
    /// SHA-256 of the raw archive bytes.
    pub sha256: String,
    /// Exact archive size in bytes.
    pub size: u64,
}

/// One file extracted from a managed dependency archive into the game folder.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedInstallMapEntry {
    /// Path inside the archive.
    pub source: String,
    /// Bare filename placed in the game directory.
    pub dest: String,
    /// SHA-256 of the extracted file bytes.
    pub sha256: String,
    /// Exact extracted file size in bytes.
    pub size: u64,
}

/// One section in a managed dependency config file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalConfigSection {
    /// INI section name.
    pub section: String,
    /// Key/value entries in this section.
    pub entries: Vec<ExternalConfigEntry>,
}

/// One key/value line in a managed dependency config file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalConfigEntry {
    /// INI key.
    pub key: String,
    /// Required value.
    pub value: String,
}
