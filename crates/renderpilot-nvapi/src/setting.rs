//! Defines the foundational architectural traits and polymorphic interfaces for NVIDIA API (NVAPI) configurations.
//!
//! Concrete setting definitions (e.g., DLSS Render Presets, DLAA toggles, Frame Generation) inherently implement 
//! the core [`NvapiSetting`] trait. The upstream CLI and application layers interoperate exclusively through 
//! this polymorphic abstraction, ensuring that the integration of novel settings necessitates solely a localized 
//! trait implementation and its corresponding frontend UI control representation.
//!
//! It is critical to note that the dynamic filtering of supported configuration variants against the DLSS preset 
//! manifest is strictly **NOT** executed within this crate boundary. That authority is delegated to the `renderpilot-libraries` 
//! sub-system and enforced by the central orchestration layer. This trait architecture restricts itself to merely 
//! declaring structural DLL family dependencies (via [`NvapiSetting::dll_kind`]), enabling the orchestrator to 
//! deterministically resolve the appropriate manifest mapping.

use std::{collections::HashMap, fmt, path::PathBuf};

use crate::api::DwordSettingState;

// -----------------------------------------------------------------------------
// DLSS DLL family
// -----------------------------------------------------------------------------

/// DLSS DLL family. Each ships in a different file with an independent
/// version stream, and each supports a different preset table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DlssDllKind {
    /// `nvngx_dlss.dll` — Super Resolution.
    Sr,
    /// `nvngx_dlssg.dll` — Frame Generation.
    FrameGen,
    /// `nvngx_dlssd.dll` — Ray Reconstruction.
    RayReconstruction,
}

impl DlssDllKind {
    /// Stable string tag matching the `dll_kind` field in DLSS preset manifests.
    pub const fn manifest_tag(self) -> &'static str {
        match self {
            Self::Sr => "sr",
            Self::FrameGen => "fg",
            Self::RayReconstruction => "rr",
        }
    }

    /// DLL file name on disk (case-insensitive comparison).
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Sr => "nvngx_dlss.dll",
            Self::FrameGen => "nvngx_dlssg.dll",
            Self::RayReconstruction => "nvngx_dlssd.dll",
        }
    }
}

// -----------------------------------------------------------------------------
// DlssVersion
// -----------------------------------------------------------------------------

/// Four-part DLSS DLL version: `major.minor.patch.build`.
///
/// Components are `u32` because some library families (AMD FidelityFX,
/// kept here only for the sort-key parser) use five-digit build numbers.
/// Ordering is lexicographic over components, which matches semver-style
/// ordering for DLSS releases (e.g. `3.10.2` < `310.1.0` because the
/// major component differs).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DlssVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub build: u32,
}

impl DlssVersion {
    pub const fn new(major: u32, minor: u32, patch: u32, build: u32) -> Self {
        Self {
            major,
            minor,
            patch,
            build,
        }
    }

    /// Parses a dotted-or-comma-separated version string like
    /// `"3.10.1.0"` or `"310,6,0,0"`. Returns `None` if there are not
    /// exactly four numeric components.
    pub fn parse(input: &str) -> Option<Self> {
        let parts: Vec<&str> = input.split(['.', ',']).filter(|s| !s.is_empty()).collect();
        if parts.len() != 4 {
            return None;
        }
        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        let patch = parts[2].parse().ok()?;
        let build = parts[3].parse().ok()?;
        Some(Self::new(major, minor, patch, build))
    }
}

impl fmt::Display for DlssVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}.{}.{}.{}",
            self.major, self.minor, self.patch, self.build
        )
    }
}

// -----------------------------------------------------------------------------
// Value types
// -----------------------------------------------------------------------------

/// NVAPI setting payload type. Only `Dword` is in active use; `WString`
/// is reserved for future settings that store strings (e.g. AppName).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NvapiValueType {
    Dword,
    WString,
}

/// One option a user can pick for a setting.
///
/// `supported_by_context` is `true` by default — the orchestration layer
/// flips it to `false` for options that the current DLSS DLL version
/// does not advertise, so the UI can grey them out instead of hiding
/// them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvapiValueOption {
    pub wire: String,
    pub label: String,
    pub dword: u32,
    pub supported_by_context: bool,
}

// -----------------------------------------------------------------------------
// Context + state
// -----------------------------------------------------------------------------

/// DLL discovered in a game install directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DllInfo {
    /// Absolute path to the DLL file.
    pub path: PathBuf,
    /// Version read from the PE `VS_VERSION_INFO` resource.
    pub version: DlssVersion,
}

/// Per-operation context built once at the start of a CLI call and
/// passed through to all setting reads/writes. Detecting DLLs is the
/// slow path (PE parsing) — doing it once avoids repeated work and
/// avoids the race where a DLL gets swapped mid-operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingContext {
    /// Root of the game's installation.
    pub game_install_dir: PathBuf,
    /// DLLs detected in the install directory, keyed by family.
    pub dlls: HashMap<DlssDllKind, DllInfo>,
    /// Executable file name (basename only) that NVAPI should be
    /// queried for. `None` if no executable could be resolved.
    pub effective_exe: Option<String>,
}

/// Snapshot persisted on the **first** ever write through RenderPilot
/// for a given (game, setting). Represents what the setting looked
/// like before RenderPilot intervened, so "Revert to baseline" can
/// always undo our influence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaselineSnapshot {
    /// Raw DWORD value at capture time.
    pub dword: u32,
    /// Whether the captured value matched the driver's predefined
    /// value (`true`) or differed (`false`, meaning some other tool
    /// — likely NVIDIA Profile Inspector — had already overridden it).
    pub was_predefined_when_captured: bool,
    /// Unix epoch seconds.
    pub captured_at_unix_secs: i64,
}

/// Full state of a setting for a game, combining a live NVAPI read
/// with any baseline snapshot from the storage layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingState {
    pub live: DwordSettingState,
    pub baseline: Option<BaselineSnapshot>,
}

// -----------------------------------------------------------------------------
// The trait
// -----------------------------------------------------------------------------

/// One configurable NVAPI setting. One implementation per setting.
///
/// The orchestration layer keeps a registry (key → `Arc<dyn NvapiSetting>`)
/// and dispatches through this trait. Adding a new setting is:
/// implement the trait + register the new impl + add a UI control.
pub trait NvapiSetting: Send + Sync {
    /// Wire-stable identifier, e.g. `"dlss_sr_render_preset"`.
    fn key(&self) -> &'static str;

    /// Short label for the UI.
    fn label(&self) -> &'static str;

    /// NVIDIA DRS setting ID (e.g. `DLSS_FORCED_RENDER_PRESET = 0x10B3292C`).
    fn nvapi_id(&self) -> u32;

    /// Payload type.
    fn value_type(&self) -> NvapiValueType;

    /// DLL family whose version may constrain valid values, if any.
    /// Returning `None` means the setting is DLL-agnostic.
    fn dll_kind(&self) -> Option<DlssDllKind>;

    /// All values this setting can take, ignoring DLL version constraints.
    /// `supported_by_context` is set to `true` for every option here;
    /// the orchestration layer downgrades unsupported ones based on the
    /// DLSS preset manifest.
    fn enumerate_values(&self, ctx: &SettingContext) -> Vec<NvapiValueOption>;

    /// Parses a wire-string into the raw DWORD value sent to the driver.
    /// Returns `None` for unrecognised wire values.
    fn parse_wire(&self, wire: &str) -> Option<u32>;

    /// Formats a raw DWORD as the canonical wire-string for this setting.
    /// Returns `None` for values outside the setting's known range.
    fn format_wire(&self, dword: u32) -> Option<String>;

    /// Best-effort human-readable label for a DWORD (used when the
    /// orchestration layer needs to render `current.label` /
    /// `predefined.label` / `baseline.label` in a response).
    fn label_for_dword(&self, dword: u32) -> Option<String>;
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parse_dotted() {
        assert_eq!(
            DlssVersion::parse("3.10.1.0"),
            Some(DlssVersion::new(3, 10, 1, 0))
        );
    }

    #[test]
    fn version_parse_comma() {
        assert_eq!(
            DlssVersion::parse("310,6,0,0"),
            Some(DlssVersion::new(310, 6, 0, 0))
        );
    }

    #[test]
    fn version_parse_rejects_short() {
        assert!(DlssVersion::parse("3.10.1").is_none());
        assert!(DlssVersion::parse("").is_none());
    }

    #[test]
    fn version_parse_rejects_non_numeric() {
        assert!(DlssVersion::parse("3.10.x.0").is_none());
    }

    #[test]
    fn version_ordering_components_first() {
        // 3.10.2 sorts BELOW 310.1.0 because the major component
        // differs (3 < 310). This is the critical case the old
        // manifest code got wrong by sorting strings.
        let a = DlssVersion::new(3, 10, 2, 0);
        let b = DlssVersion::new(310, 1, 0, 0);
        assert!(a < b);
    }

    #[test]
    fn version_display_roundtrip() {
        let v = DlssVersion::new(3, 1, 1, 0);
        assert_eq!(v.to_string(), "3.1.1.0");
        assert_eq!(DlssVersion::parse(&v.to_string()), Some(v));
    }

    #[test]
    fn dll_kind_tags_are_stable() {
        // Manifest files reference these literally — never rename.
        assert_eq!(DlssDllKind::Sr.manifest_tag(), "sr");
        assert_eq!(DlssDllKind::FrameGen.manifest_tag(), "fg");
        assert_eq!(DlssDllKind::RayReconstruction.manifest_tag(), "rr");
    }

    #[test]
    fn dll_kind_file_names() {
        assert_eq!(DlssDllKind::Sr.file_name(), "nvngx_dlss.dll");
        assert_eq!(DlssDllKind::FrameGen.file_name(), "nvngx_dlssg.dll");
        assert_eq!(
            DlssDllKind::RayReconstruction.file_name(),
            "nvngx_dlssd.dll"
        );
    }
}
