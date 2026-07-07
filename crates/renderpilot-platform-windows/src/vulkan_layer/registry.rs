use std::io;
use std::path::{Path, PathBuf};

use super::types::{LayerRegistryEntry, RegistryHive};
use super::{IMPLICIT_LAYERS_KEY, IMPLICIT_LAYERS_KEY_WOW64};

/// Read/write access to the Vulkan loader's implicit-layer registrations.
///
/// Abstracted so the detect/install/uninstall logic is unit-testable without
/// touching the real registry. The production implementation is
/// [`WindowsLayerRegistry`] (HKLM); tests use a `FakeRegistry` double.
pub trait LayerRegistry {
    /// Every currently registered implicit-layer entry (across the hives and
    /// views consulted).
    fn registered_layers(&self) -> Vec<LayerRegistryEntry>;
    /// Every currently registered implicit-layer manifest path.
    fn registered_manifests(&self) -> Vec<PathBuf> {
        self.registered_layers()
            .into_iter()
            .map(|entry| entry.manifest_path)
            .collect()
    }
    /// Registers a manifest path. Idempotent.
    fn register(&self, manifest_path: &Path) -> io::Result<()>;
    /// Removes a manifest-path registration. Missing = success.
    fn unregister(&self, manifest_path: &Path) -> io::Result<()>;
    /// Whether the registry scope used by the official ReShade layout (HKLM)
    /// can be written by this process. Used by `detect_report` to surface a
    /// `RegistryScopeNotWritable` caveat when the user might want to install
    /// or re-register the layer but the process is not elevated. Default is
    /// `true` (test fakes override to simulate a non-elevated process).
    fn can_write_scope(&self) -> bool {
        true
    }
}

/// Production [`LayerRegistry`] backed by the Windows registry.
///
/// Reads from both HKLM and HKCU (all views) for detection. Writes to HKLM
/// only — the official ReShade installer registers under HKLM, and matching
/// that ensures the layer is visible to all processes, including elevated
/// ones. Writing to HKLM requires the process to be elevated.
pub struct WindowsLayerRegistry;

impl LayerRegistry for WindowsLayerRegistry {
    fn registered_layers(&self) -> Vec<LayerRegistryEntry> {
        use winreg::RegKey;
        use winreg::RegValue;
        use winreg::enums::{
            HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
            REG_DWORD,
        };

        fn is_active(value: &RegValue<'_>) -> bool {
            let bytes = value.bytes.as_ref();
            value.vtype == REG_DWORD
                && bytes.len() == 4
                && u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) == 0
        }

        let mut entries = Vec::new();
        for (hive_const, hive) in [
            (HKEY_LOCAL_MACHINE, RegistryHive::Hklm),
            (HKEY_CURRENT_USER, RegistryHive::Hkcu),
        ] {
            let root = RegKey::predef(hive_const);
            // Windows' WOW64 redirection maps `IMPLICIT_LAYERS_KEY` to
            // `IMPLICIT_LAYERS_KEY_WOW64` transparently for a 32-bit calling
            // process unless a `KEY_WOW64_*` flag overrides it — and that
            // mapping direction depends on both this process's bitness and
            // which bitness the registering installer used. Rather than
            // predict the combination, read both key paths under all three
            // access modes (default/forced-64/forced-32): the real
            // registration is reachable through at least one of the six, and
            // duplicate reads of the same manifest across the others are
            // harmless — `logical_registry_entries` (in detection.rs)
            // dedupes by resolved manifest path.
            let registry_views = [
                (IMPLICIT_LAYERS_KEY, KEY_READ),
                (IMPLICIT_LAYERS_KEY, KEY_READ | KEY_WOW64_64KEY),
                (IMPLICIT_LAYERS_KEY, KEY_READ | KEY_WOW64_32KEY),
                (IMPLICIT_LAYERS_KEY_WOW64, KEY_READ),
                (IMPLICIT_LAYERS_KEY_WOW64, KEY_READ | KEY_WOW64_64KEY),
                (IMPLICIT_LAYERS_KEY_WOW64, KEY_READ | KEY_WOW64_32KEY),
            ];
            for (key_path, flags) in registry_views {
                let Ok(key) = root.open_subkey_with_flags(key_path, flags) else {
                    continue;
                };
                for (name, data) in key.enum_values().flatten() {
                    let trimmed = name.trim_matches('"').trim();
                    if !trimmed.is_empty() {
                        entries.push(LayerRegistryEntry {
                            manifest_path: PathBuf::from(trimmed),
                            active: is_active(&data),
                            hive,
                        });
                    }
                }
            }
        }
        entries
    }

    fn register(&self, manifest_path: &Path) -> io::Result<()> {
        use winreg::RegKey;
        use winreg::enums::HKEY_LOCAL_MACHINE;

        let (key, _disposition) =
            RegKey::predef(HKEY_LOCAL_MACHINE).create_subkey(IMPLICIT_LAYERS_KEY)?;
        key.set_value(manifest_path.as_os_str(), &0u32)
    }

    fn unregister(&self, manifest_path: &Path) -> io::Result<()> {
        use winreg::RegKey;
        use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE};

        let key = match RegKey::predef(HKEY_LOCAL_MACHINE)
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

    fn can_write_scope(&self) -> bool {
        use winreg::RegKey;
        use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_WRITE};

        // Probe whether the process can write to the HKLM implicit-layers key.
        // If the key exists, opening it with KEY_WRITE succeeds when the
        // process has write access. If it doesn't exist yet, check the parent
        // — if the parent is writable, `create_subkey` in `register` will
        // succeed.
        match RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(IMPLICIT_LAYERS_KEY, KEY_WRITE)
        {
            Ok(_) => true,
            Err(_) => RegKey::predef(HKEY_LOCAL_MACHINE)
                .open_subkey_with_flags("Software\\Khronos\\Vulkan", KEY_WRITE)
                .is_ok(),
        }
    }
}

// -----------------------------------------------------------------------------
// Detection
// -----------------------------------------------------------------------------
