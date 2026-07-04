//! Graphics APIs and architecture inferred from a game executable.
//!
//! Produced by the detection layer from the executable's PE import table: the
//! `apis` set lists every graphics API the binary imports (a detection fact,
//! with no product-specific ranking applied), and `architecture` is the CPU
//! bitness read from the COFF machine type. The orchestration layer applies
//! any tool-specific policy (e.g. "pick the most capable DirectX API for
//! RenoDX") on top of these facts.

use serde::{Deserialize, Serialize};

use crate::{Architecture, GraphicsApi};

/// Graphics APIs and architecture inferred from a game executable.
///
/// Produced by the detection layer from the executable's PE import table: the
/// `apis` set lists every graphics API the binary imports (a detection fact,
/// with no product-specific ranking applied), and `architecture` is the CPU
/// bitness read from the COFF machine type. The orchestration layer applies
/// any tool-specific policy (e.g. "pick the most capable DirectX API for
/// RenoDX") on top of these facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExeGraphicsInfo {
    /// The set of graphics APIs the executable imports, deduplicated and without
    /// ranking. Empty when no known graphics import was found.
    apis: Vec<GraphicsApi>,
    architecture: Option<Architecture>,
    /// The actual graphics DLL basenames the executable imports (lowercased, e.g.
    /// `dxgi.dll`, `d3d12.dll`, `d3d9.dll`), in first-seen order. Unlike `apis`
    /// (which collapses `dxgi.dll` into `D3D11`), this preserves the exact DLL so
    /// the orchestration layer can pick the precise ReShade proxy the game loads
    /// instead of guessing. Empty when no known graphics import was found.
    #[serde(default)]
    graphics_dlls: Vec<String>,
}

impl ExeGraphicsInfo {
    /// Creates a new graphics-info record from the imported API set and
    /// architecture. The imported-DLL list is empty; use
    /// [`Self::with_graphics_dlls`] to attach it.
    #[must_use]
    pub fn new(apis: Vec<GraphicsApi>, architecture: Option<Architecture>) -> Self {
        Self {
            apis,
            architecture,
            graphics_dlls: Vec::new(),
        }
    }

    /// Attaches the exact imported graphics DLL basenames (lowercased).
    #[must_use]
    pub fn with_graphics_dlls(mut self, graphics_dlls: Vec<String>) -> Self {
        self.graphics_dlls = graphics_dlls;
        self
    }

    /// Returns the detected graphics API set, without ranking.
    #[must_use]
    pub fn apis(&self) -> &[GraphicsApi] {
        &self.apis
    }

    /// Returns the detected architecture, if it could be determined.
    #[must_use]
    pub const fn architecture(&self) -> Option<Architecture> {
        self.architecture
    }

    /// Returns the exact imported graphics DLL basenames (lowercased), in
    /// first-seen order. Empty when none were found or detection was inconclusive.
    #[must_use]
    pub fn graphics_dlls(&self) -> &[String] {
        &self.graphics_dlls
    }
}
