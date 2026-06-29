//! Global ReShade Vulkan implicit-layer management.
//!
//! Vulkan games cannot load a proxy DLL the way Direct3D games do, so ReShade
//! hooks them through a single **global implicit layer** registered with the
//! Vulkan loader (a `Software\Khronos\Vulkan\ImplicitLayers` registry value whose
//! name is the layer's JSON manifest path and whose `DWORD` data is `0` =
//! enabled). There can be only one ReShade layer: two would mean two ReShade
//! overlays in every Vulkan application. So RenderPilot **detects an existing
//! ReShade layer and reuses it**, installing its own only when none is present.
//!
//! This mirrors the proxy `detect_reshade` ownership model: a layer RenderPilot
//! installed carries an ownership marker next to it (so an uninstall may remove
//! it), while a layer anyone else installed is [`VulkanLayerState::Foreign`] and
//! is reused untouched. A registered manifest whose files are gone is treated as
//! absent (orphan-safe), so a half-removed install never blocks a clean reinstall.
//!
//! Two seams keep this testable without touching the real registry or the user's
//! AppData: the registry is abstracted behind [`LayerRegistry`] (a fake is used
//! in unit tests), and the install directory is passed in by the caller
//! ([`default_layer_dir`] resolves the real one via a known-folder API).

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Vulkan loader implicit-layer registry key (relative to a hive root). The same
/// path is consulted under both `HKEY_CURRENT_USER` and `HKEY_LOCAL_MACHINE`.
const IMPLICIT_LAYERS_KEY: &str = r"Software\Khronos\Vulkan\ImplicitLayers";

/// Layer name in our generated manifest. ReShade's own Vulkan layer uses this
/// name; the loader loads an implicit layer regardless of its name, and we only
/// ever register ours when no ReShade layer already exists, so there is no
/// same-name collision in practice.
const LAYER_NAME: &str = "VK_LAYER_reshade";
/// The Vulkan-capable ReShade host DLL we write into our layer directory.
pub const LAYER_DLL_NAME: &str = "ReShade64.dll";
/// Our generated Vulkan layer manifest file name.
const LAYER_JSON_NAME: &str = "renderpilot-reshade-layer.json";
/// Ownership sentinel written next to the layer so an uninstall can tell a layer
/// RenderPilot installed from a foreign one it must leave intact.
const MARKER_FILE_NAME: &str = "renderpilot-vulkan-layer.json";

/// Whether a working ReShade Vulkan layer is present, and who owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulkanLayerState {
    /// No ReShade Vulkan layer is registered; an install must provide one.
    Absent,
    /// A foreign ReShade layer (user- or tool-installed) is present; reuse it and
    /// never modify it.
    Foreign,
    /// A layer RenderPilot installed is present (its files and marker exist).
    Managed,
}

/// Ownership record written next to the layer when RenderPilot installs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VulkanLayerMarker {
    /// Marker schema version.
    pub schema_version: u32,
    /// ReShade host version installed, when known.
    pub reshade_version: Option<String>,
}

impl VulkanLayerMarker {
    /// Current marker schema version.
    pub const SCHEMA_VERSION: u32 = 1;
}

/// Read/write access to the Vulkan loader's implicit-layer registrations.
///
/// Abstracted so the detect/install/uninstall logic is unit-testable without
/// touching the real registry. The production implementation is
/// [`WindowsLayerRegistry`].
pub trait LayerRegistry {
    /// Every currently registered implicit-layer manifest path (the value names
    /// under `ImplicitLayers`, across the hives and views consulted).
    fn registered_manifests(&self) -> Vec<PathBuf>;
    /// Registers a manifest path (HKCU; value name = path, `DWORD` data `0`).
    /// Idempotent — registering an already-registered path is a no-op overwrite.
    fn register(&self, manifest_path: &Path) -> io::Result<()>;
    /// Removes a manifest-path registration. A path that is not registered is a
    /// success (nothing to do).
    fn unregister(&self, manifest_path: &Path) -> io::Result<()>;
}

/// The directory RenderPilot installs its Vulkan layer into, resolved via a
/// known-folder API (Local AppData). `None` if no local data directory exists.
#[must_use]
pub fn default_layer_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|base| base.join("RenderPilot").join("VulkanLayer"))
}

/// Detects the ReShade Vulkan layer state, given the registry and the directory a
/// RenderPilot-managed layer would live in.
///
/// A layer we own (its registered manifest path is ours **and** its files exist)
/// wins; otherwise any foreign ReShade layer makes the result [`Foreign`];
/// otherwise [`Absent`]. Non-ReShade implicit layers (overlays, capture tools)
/// are ignored — they coexist fine with ReShade.
///
/// [`Foreign`]: VulkanLayerState::Foreign
/// [`Absent`]: VulkanLayerState::Absent
#[must_use]
pub fn detect(registry: &impl LayerRegistry, layer_dir: &Path) -> VulkanLayerState {
    let mut foreign = false;
    for manifest in registry.registered_manifests() {
        match classify_manifest(&manifest, layer_dir) {
            ManifestKind::Managed => return VulkanLayerState::Managed,
            ManifestKind::ForeignReshade => foreign = true,
            ManifestKind::Other => {}
        }
    }
    if foreign {
        VulkanLayerState::Foreign
    } else {
        VulkanLayerState::Absent
    }
}

/// Installs RenderPilot's ReShade Vulkan layer into `layer_dir`: writes the host
/// DLL and the generated manifest, records the ownership marker, and registers
/// the manifest with the loader.
///
/// The caller is responsible for only invoking this when [`detect`] returned
/// [`VulkanLayerState::Absent`] (a foreign layer must be reused, not displaced).
///
/// # Errors
/// Propagates filesystem and registry errors.
pub fn install(
    registry: &impl LayerRegistry,
    layer_dir: &Path,
    dll_bytes: &[u8],
    reshade_version: Option<&str>,
) -> io::Result<()> {
    std::fs::create_dir_all(layer_dir)?;

    let dll_path = layer_dir.join(LAYER_DLL_NAME);
    std::fs::write(&dll_path, dll_bytes)?;

    let manifest_path = layer_dir.join(LAYER_JSON_NAME);
    std::fs::write(&manifest_path, layer_manifest_json(&dll_path))?;

    let marker = VulkanLayerMarker {
        schema_version: VulkanLayerMarker::SCHEMA_VERSION,
        reshade_version: reshade_version.map(str::to_owned),
    };
    let marker_bytes = serde_json::to_vec(&marker).map_err(io::Error::other)?;
    std::fs::write(layer_dir.join(MARKER_FILE_NAME), marker_bytes)?;

    registry.register(&manifest_path)
}

/// Removes a RenderPilot-managed ReShade Vulkan layer: unregisters the manifest
/// and deletes the layer directory. Safe to call when nothing is installed.
///
/// # Errors
/// Propagates registry and filesystem errors (a missing directory is not one).
pub fn uninstall(registry: &impl LayerRegistry, layer_dir: &Path) -> io::Result<()> {
    registry.unregister(&layer_dir.join(LAYER_JSON_NAME))?;
    match std::fs::remove_dir_all(layer_dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

// -----------------------------------------------------------------------------
// Manifest generation + classification (pure)
// -----------------------------------------------------------------------------

/// Serializable Vulkan implicit-layer manifest in the loader's schema.
#[derive(Serialize)]
struct LayerManifest<'a> {
    file_format_version: &'a str,
    layer: LayerEntry<'a>,
}

#[derive(Serialize)]
struct LayerEntry<'a> {
    name: &'a str,
    #[serde(rename = "type")]
    layer_type: &'a str,
    library_path: String,
    api_version: &'a str,
    implementation_version: &'a str,
    description: &'a str,
}

/// Builds the Vulkan layer manifest JSON pointing at `dll_path`. Serialized via
/// serde so the path's backslashes are escaped correctly.
#[must_use]
fn layer_manifest_json(dll_path: &Path) -> String {
    let manifest = LayerManifest {
        file_format_version: "1.0.0",
        layer: LayerEntry {
            name: LAYER_NAME,
            layer_type: "GLOBAL",
            library_path: dll_path.to_string_lossy().into_owned(),
            api_version: "1.3.0",
            implementation_version: "1",
            description: "ReShade (managed by RenderPilot)",
        },
    };
    // Infallible: the manifest is a fixed struct of owned strings.
    serde_json::to_string_pretty(&manifest).expect("layer manifest serializes")
}

/// A registered manifest, classified for [`detect`].
enum ManifestKind {
    /// Ours, with its files present.
    Managed,
    /// A foreign ReShade layer whose DLL exists.
    ForeignReshade,
    /// Anything else: a non-ReShade layer, or an orphaned/invalid entry.
    Other,
}

/// Minimal view of a foreign layer manifest for ReShade identification.
#[derive(Deserialize)]
struct ForeignManifest {
    #[serde(default)]
    layer: ForeignLayer,
}

#[derive(Deserialize, Default)]
struct ForeignLayer {
    #[serde(default)]
    name: String,
    #[serde(default)]
    library_path: String,
}

fn classify_manifest(manifest_path: &Path, layer_dir: &Path) -> ManifestKind {
    if same_path(manifest_path, &layer_dir.join(LAYER_JSON_NAME)) {
        // Ours by path — but only "Managed" if the files are actually there, so a
        // stale registry entry left by a half-removed install reads as absent and
        // a clean reinstall is unblocked.
        if layer_dir.join(MARKER_FILE_NAME).is_file() && layer_dir.join(LAYER_DLL_NAME).is_file() {
            return ManifestKind::Managed;
        }
        return ManifestKind::Other;
    }
    if is_foreign_reshade(manifest_path) {
        return ManifestKind::ForeignReshade;
    }
    ManifestKind::Other
}

/// Whether a registered manifest is a ReShade layer whose DLL is present. A layer
/// counts as ReShade if its name matches ReShade's or its `library_path` resolves
/// to a `reshade*.dll`; the DLL must exist on disk (so a dangling entry is not
/// mistaken for a working foreign install).
fn is_foreign_reshade(manifest_path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(manifest_path) else {
        return false;
    };
    let Ok(parsed) = serde_json::from_str::<ForeignManifest>(&content) else {
        return false;
    };
    let dll = resolve_library_path(manifest_path, &parsed.layer.library_path);
    let looks_reshade = parsed.layer.name.eq_ignore_ascii_case(LAYER_NAME)
        || dll
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.to_ascii_lowercase().starts_with("reshade"));
    looks_reshade && dll.is_file()
}

/// Resolves a manifest's `library_path` (absolute, or relative to the manifest's
/// own directory per the Vulkan loader spec).
fn resolve_library_path(manifest_path: &Path, library_path: &str) -> PathBuf {
    let candidate = PathBuf::from(library_path);
    if candidate.is_absolute() {
        return candidate;
    }
    manifest_path
        .parent()
        .map_or_else(|| candidate.clone(), |dir| dir.join(&candidate))
}

/// Case- and separator-insensitive path equality (Windows registry values mix
/// separators and casing).
fn same_path(a: &Path, b: &Path) -> bool {
    normalize(a) == normalize(b)
}

fn normalize(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

// -----------------------------------------------------------------------------
// Real registry (Windows)
// -----------------------------------------------------------------------------

/// Production [`LayerRegistry`] backed by the Windows registry. Reads HKCU and
/// HKLM (all views) for detection; writes to HKCU (no admin required).
pub struct WindowsLayerRegistry;

impl LayerRegistry for WindowsLayerRegistry {
    fn registered_manifests(&self) -> Vec<PathBuf> {
        use winreg::RegKey;
        use winreg::enums::{
            HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
        };

        let mut manifests = Vec::new();
        for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
            let root = RegKey::predef(hive);
            for flags in [
                KEY_READ,
                KEY_READ | KEY_WOW64_64KEY,
                KEY_READ | KEY_WOW64_32KEY,
            ] {
                let Ok(key) = root.open_subkey_with_flags(IMPLICIT_LAYERS_KEY, flags) else {
                    continue;
                };
                for (name, _data) in key.enum_values().flatten() {
                    let trimmed = name.trim_matches('"').trim();
                    if !trimmed.is_empty() {
                        manifests.push(PathBuf::from(trimmed));
                    }
                }
            }
        }
        manifests
    }

    fn register(&self, manifest_path: &Path) -> io::Result<()> {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;

        let (key, _disposition) =
            RegKey::predef(HKEY_CURRENT_USER).create_subkey(IMPLICIT_LAYERS_KEY)?;
        // Value name = manifest path; data = DWORD 0 (enabled, per the loader).
        key.set_value(manifest_path.as_os_str(), &0u32)
    }

    fn unregister(&self, manifest_path: &Path) -> io::Result<()> {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};

        let key = match RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(IMPLICIT_LAYERS_KEY, KEY_READ | KEY_WRITE)
        {
            Ok(key) => key,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        match key.delete_value(manifest_path.as_os_str()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use tempfile::tempdir;

    /// In-memory [`LayerRegistry`] for hermetic tests.
    #[derive(Default)]
    struct FakeRegistry {
        manifests: RefCell<Vec<PathBuf>>,
    }

    impl LayerRegistry for FakeRegistry {
        fn registered_manifests(&self) -> Vec<PathBuf> {
            self.manifests.borrow().clone()
        }
        fn register(&self, manifest_path: &Path) -> io::Result<()> {
            let mut manifests = self.manifests.borrow_mut();
            if !manifests
                .iter()
                .any(|existing| same_path(existing, manifest_path))
            {
                manifests.push(manifest_path.to_path_buf());
            }
            Ok(())
        }
        fn unregister(&self, manifest_path: &Path) -> io::Result<()> {
            self.manifests
                .borrow_mut()
                .retain(|existing| !same_path(existing, manifest_path));
            Ok(())
        }
    }

    /// Writes a foreign ReShade layer (manifest + DLL) into `dir` and returns the
    /// manifest path, ready to register.
    fn write_foreign_reshade(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let dll = dir.join("ReShade64.dll");
        std::fs::write(&dll, b"dll").unwrap();
        let manifest = dir.join("reshade_layer.json");
        std::fs::write(
            &manifest,
            format!(
                r#"{{"file_format_version":"1.0.0","layer":{{"name":"VK_LAYER_reshade","type":"GLOBAL","library_path":"{}"}}}}"#,
                dll.to_string_lossy().replace('\\', "\\\\")
            ),
        )
        .unwrap();
        manifest
    }

    #[test]
    fn manifest_json_is_valid_and_carries_the_expected_fields() {
        let dll = Path::new(r"C:\Users\me\AppData\Local\RenderPilot\VulkanLayer\ReShade64.dll");
        let json = layer_manifest_json(dll);
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(value["file_format_version"], "1.0.0");
        assert_eq!(value["layer"]["name"], "VK_LAYER_reshade");
        assert_eq!(value["layer"]["type"], "GLOBAL");
        assert_eq!(
            value["layer"]["library_path"],
            dll.to_string_lossy().as_ref()
        );
    }

    #[test]
    fn detect_is_absent_for_empty_registry() {
        let registry = FakeRegistry::default();
        let dir = tempdir().unwrap();
        assert_eq!(detect(&registry, dir.path()), VulkanLayerState::Absent);
    }

    #[test]
    fn detect_is_foreign_for_a_registered_reshade_layer() {
        let foreign = tempdir().unwrap();
        let manifest = write_foreign_reshade(foreign.path());
        let registry = FakeRegistry::default();
        registry.register(&manifest).unwrap();

        let our_dir = tempdir().unwrap();
        assert_eq!(detect(&registry, our_dir.path()), VulkanLayerState::Foreign);
    }

    #[test]
    fn detect_ignores_a_non_reshade_implicit_layer() {
        // A capture/overlay layer pointing at a non-ReShade DLL must not be
        // mistaken for ReShade — they coexist, so we still need to install ours.
        let other = tempdir().unwrap();
        let dll = other.path().join("mangohud.dll");
        std::fs::write(&dll, b"x").unwrap();
        let manifest = other.path().join("mangohud.json");
        std::fs::write(
            &manifest,
            format!(
                r#"{{"layer":{{"name":"VK_LAYER_MANGOHUD_overlay","library_path":"{}"}}}}"#,
                dll.to_string_lossy().replace('\\', "\\\\")
            ),
        )
        .unwrap();
        let registry = FakeRegistry::default();
        registry.register(&manifest).unwrap();

        let our_dir = tempdir().unwrap();
        assert_eq!(detect(&registry, our_dir.path()), VulkanLayerState::Absent);
    }

    #[test]
    fn install_then_detect_is_managed_and_writes_the_files() {
        let registry = FakeRegistry::default();
        let dir = tempdir().unwrap();
        install(&registry, dir.path(), b"reshade-dll-bytes", Some("6.7.3")).unwrap();

        assert!(dir.path().join(LAYER_DLL_NAME).is_file());
        assert!(dir.path().join(LAYER_JSON_NAME).is_file());
        assert!(dir.path().join(MARKER_FILE_NAME).is_file());
        assert_eq!(registry.registered_manifests().len(), 1);
        assert_eq!(detect(&registry, dir.path()), VulkanLayerState::Managed);
    }

    #[test]
    fn a_registered_manifest_without_files_is_absent_not_managed() {
        // Orphaned registry entry pointing at our (now-deleted) manifest.
        let registry = FakeRegistry::default();
        let dir = tempdir().unwrap();
        registry
            .register(&dir.path().join(LAYER_JSON_NAME))
            .unwrap();
        assert_eq!(detect(&registry, dir.path()), VulkanLayerState::Absent);
    }

    #[test]
    fn uninstall_removes_files_and_registration() {
        let registry = FakeRegistry::default();
        let dir = tempdir().unwrap();
        install(&registry, dir.path(), b"x", None).unwrap();

        uninstall(&registry, dir.path()).unwrap();
        assert!(!dir.path().exists());
        assert!(registry.registered_manifests().is_empty());
        assert_eq!(detect(&registry, dir.path()), VulkanLayerState::Absent);
    }

    #[test]
    fn uninstall_is_ok_when_nothing_is_installed() {
        let registry = FakeRegistry::default();
        let dir = tempdir().unwrap();
        // Never installed: no files, no registration.
        uninstall(&registry, &dir.path().join("missing")).unwrap();
    }

    #[test]
    fn our_layer_wins_over_a_coexisting_foreign_entry() {
        // If both ours and a foreign ReShade layer are registered, we report
        // Managed (we own one and can manage it).
        let registry = FakeRegistry::default();
        let our_dir = tempdir().unwrap();
        install(&registry, our_dir.path(), b"x", None).unwrap();

        let foreign = tempdir().unwrap();
        let foreign_manifest = write_foreign_reshade(foreign.path());
        registry.register(&foreign_manifest).unwrap();

        assert_eq!(detect(&registry, our_dir.path()), VulkanLayerState::Managed);
    }
}
