use std::io;
use std::path::{Path, PathBuf};

use super::types::LayerRegistryEntry;
#[cfg(windows)]
use super::types::RegistryHive;
#[cfg(windows)]
use super::util::same_path;
#[cfg(windows)]
use super::{IMPLICIT_LAYERS_KEY, IMPLICIT_LAYERS_KEY_WOW64, LAYER_JSON_NAME};

/// Exact raw state of the one HKLM/64-bit registration value owned by the
/// standard shared ReShade layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryValueState {
    /// The exact manifest value is absent. The key and all other values may
    /// still exist and are not owned by this state.
    Absent,
    /// The exact manifest value exists with its original registry type/data.
    Present {
        /// Native Windows registry value type identifier.
        value_type: u32,
        /// Raw value data, byte-for-byte.
        raw_bytes: Vec<u8>,
    },
}

/// Read/write access to the Vulkan loader's implicit-layer registrations.
///
/// Abstracted so the detect/install/uninstall logic is unit-testable without
/// touching the real registry. The production Windows implementation is
/// available only on Windows; tests use a `FakeRegistry` double.
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
    /// Test-fixture compatibility for the removed direct installer.
    #[cfg(test)]
    fn register(&self, manifest_path: &Path) -> io::Result<()>;
    /// Test-fixture compatibility for the removed direct uninstaller.
    #[cfg(test)]
    fn unregister(&self, manifest_path: &Path) -> io::Result<()>;
    /// Whether the registry scope used by the official ReShade layout (HKLM)
    /// can be written by this process. Used by `detect_report` to surface a
    /// `RegistryScopeNotWritable` caveat when the user might want to install
    /// or re-register the layer but the process is not elevated. Default is
    /// `true` (test fakes override to simulate a non-elevated process).
    fn can_write_scope(&self) -> bool {
        true
    }

    /// Observes only the exact standard `ReShade64.json` value in the
    /// canonical HKLM 64-bit implicit-layer key. Detection intentionally uses
    /// [`registered_layers`](Self::registered_layers) instead.
    fn observe_canonical_registration(
        &self,
        manifest_path: &Path,
    ) -> io::Result<RegistryValueState> {
        let _ = manifest_path;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "canonical registry participant is not implemented by this registry",
        ))
    }

    /// Test-fixture compatibility for the removed direct installer.
    #[cfg(test)]
    fn activate_canonical_registration(&self, manifest_path: &Path) -> io::Result<()> {
        let _ = manifest_path;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "canonical registry participant is not implemented by this registry",
        ))
    }

    /// Restores only the exact standard registration to a previously observed
    /// raw state. Restoring [`RegistryValueState::Absent`] deletes the value,
    /// never the containing key.
    fn restore_canonical_registration(
        &self,
        manifest_path: &Path,
        state: &RegistryValueState,
    ) -> io::Result<()> {
        let _ = (manifest_path, state);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "canonical registry participant is not implemented by this registry",
        ))
    }
}

/// Production [`LayerRegistry`] backed by the Windows registry.
///
/// Reads from both HKLM and HKCU (all views) for detection. Writes to HKLM
/// only — the official ReShade installer registers under HKLM, and matching
/// that ensures the layer is visible to all processes, including elevated
/// ones. Writing to HKLM requires the process to be elevated.
#[cfg(windows)]
pub struct WindowsLayerRegistry;

#[cfg(windows)]
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

    #[cfg(test)]
    fn register(&self, manifest_path: &Path) -> io::Result<()> {
        self.activate_canonical_registration(manifest_path)
    }

    #[cfg(test)]
    fn unregister(&self, manifest_path: &Path) -> io::Result<()> {
        self.restore_canonical_registration(manifest_path, &RegistryValueState::Absent)
    }

    fn can_write_scope(&self) -> bool {
        use winreg::RegKey;
        use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY, KEY_WRITE};

        // Probe whether the process can write to the HKLM implicit-layers key.
        // If the key exists, opening it with KEY_WRITE succeeds when the
        // process has write access. If it doesn't exist yet, check the parent
        // — if the parent is writable, `create_subkey` in `register` will
        // succeed.
        match RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(IMPLICIT_LAYERS_KEY, KEY_WRITE | KEY_WOW64_64KEY)
        {
            Ok(_) => true,
            Err(_) => RegKey::predef(HKEY_LOCAL_MACHINE)
                .open_subkey_with_flags("Software\\Khronos\\Vulkan", KEY_WRITE | KEY_WOW64_64KEY)
                .is_ok(),
        }
    }

    fn observe_canonical_registration(
        &self,
        manifest_path: &Path,
    ) -> io::Result<RegistryValueState> {
        let value_name = validate_canonical_manifest(manifest_path)?;
        let key = match open_canonical_key_read() {
            Ok(key) => key,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(RegistryValueState::Absent);
            }
            Err(error) => return Err(error),
        };
        match key.get_raw_value(value_name) {
            Ok(value) => Ok(RegistryValueState::Present {
                value_type: value.vtype as u32,
                raw_bytes: value.bytes.into_owned(),
            }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(RegistryValueState::Absent),
            Err(error) => Err(error),
        }
    }

    #[cfg(test)]
    fn activate_canonical_registration(&self, manifest_path: &Path) -> io::Result<()> {
        let value_name = validate_canonical_manifest(manifest_path)?;
        let key = open_canonical_key_write()?;
        key.set_raw_value(
            value_name,
            &winreg::RegValue {
                vtype: winreg::enums::REG_DWORD,
                bytes: vec![0; 4].into(),
            },
        )
    }

    fn restore_canonical_registration(
        &self,
        manifest_path: &Path,
        state: &RegistryValueState,
    ) -> io::Result<()> {
        let value_name = validate_canonical_manifest(manifest_path)?;
        match state {
            RegistryValueState::Absent => {
                let Some(key) = open_canonical_key_set_value_if_present()? else {
                    return Ok(());
                };
                match key.delete_value(value_name) {
                    Ok(()) => Ok(()),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                    Err(error) => Err(error),
                }
            }
            RegistryValueState::Present {
                value_type,
                raw_bytes,
            } => {
                let value_type = registry_type(*value_type)?;
                let key = open_canonical_key_write()?;
                key.set_raw_value(
                    value_name,
                    &winreg::RegValue {
                        vtype: value_type,
                        bytes: raw_bytes.clone().into(),
                    },
                )
            }
        }
    }
}

#[cfg(windows)]
fn validate_canonical_manifest(manifest_path: &Path) -> io::Result<PathBuf> {
    let layer_dir = super::reshade_common_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "canonical Vulkan registration requires the ProgramData directory",
        )
    })?;
    let expected = layer_dir.join(LAYER_JSON_NAME);
    if !same_path(manifest_path, &expected) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "canonical Vulkan registration requires `{}`",
                expected.display()
            ),
        ));
    }
    Ok(expected)
}

#[cfg(windows)]
fn open_canonical_key_read() -> io::Result<winreg::RegKey> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};

    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(IMPLICIT_LAYERS_KEY, KEY_READ | KEY_WOW64_64KEY)
}

#[cfg(windows)]
fn open_canonical_key_write() -> io::Result<winreg::RegKey> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_CREATE_SUB_KEY, KEY_SET_VALUE, KEY_WOW64_64KEY};

    RegKey::predef(HKEY_LOCAL_MACHINE)
        .create_subkey_with_flags(
            IMPLICIT_LAYERS_KEY,
            KEY_CREATE_SUB_KEY | KEY_SET_VALUE | KEY_WOW64_64KEY,
        )
        .map(|(key, _)| key)
}

#[cfg(windows)]
fn open_canonical_key_set_value_if_present() -> io::Result<Option<winreg::RegKey>> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_SET_VALUE, KEY_WOW64_64KEY};

    match RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(IMPLICIT_LAYERS_KEY, KEY_SET_VALUE | KEY_WOW64_64KEY)
    {
        Ok(key) => Ok(Some(key)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
fn registry_type(value_type: u32) -> io::Result<winreg::enums::RegType> {
    use winreg::enums::*;

    match value_type {
        value if value == REG_NONE as u32 => Ok(REG_NONE),
        value if value == REG_SZ as u32 => Ok(REG_SZ),
        value if value == REG_EXPAND_SZ as u32 => Ok(REG_EXPAND_SZ),
        value if value == REG_BINARY as u32 => Ok(REG_BINARY),
        value if value == REG_DWORD as u32 => Ok(REG_DWORD),
        value if value == REG_DWORD_BIG_ENDIAN as u32 => Ok(REG_DWORD_BIG_ENDIAN),
        value if value == REG_LINK as u32 => Ok(REG_LINK),
        value if value == REG_MULTI_SZ as u32 => Ok(REG_MULTI_SZ),
        value if value == REG_RESOURCE_LIST as u32 => Ok(REG_RESOURCE_LIST),
        value if value == REG_FULL_RESOURCE_DESCRIPTOR as u32 => Ok(REG_FULL_RESOURCE_DESCRIPTOR),
        value if value == REG_RESOURCE_REQUIREMENTS_LIST as u32 => {
            Ok(REG_RESOURCE_REQUIREMENTS_LIST)
        }
        value if value == REG_QWORD as u32 => Ok(REG_QWORD),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "registry value uses an unknown Windows type",
        )),
    }
}

#[cfg(all(test, windows))]
mod canonical_registration_tests {
    use super::*;

    #[test]
    fn mutation_key_is_the_full_standard_manifest_path() {
        let Some(layer_dir) = super::super::reshade_common_dir() else {
            return;
        };
        let manifest = layer_dir.join(LAYER_JSON_NAME);

        assert_eq!(
            validate_canonical_manifest(&manifest).expect("canonical manifest"),
            manifest
        );
        assert!(
            validate_canonical_manifest(&layer_dir.join("foreign").join(LAYER_JSON_NAME)).is_err(),
            "a matching basename outside the standard layer directory is not canonical"
        );
    }

    #[test]
    fn validate_canonical_manifest_accepts_verbatim_dos_path() {
        let Some(layer_dir) = super::super::reshade_common_dir() else {
            return;
        };
        let manifest = layer_dir.join(LAYER_JSON_NAME);
        let verbatim = PathBuf::from(format!(r"\\?\{}", manifest.display()));

        assert_eq!(
            validate_canonical_manifest(&verbatim).expect("canonical verbatim manifest"),
            manifest
        );
    }
}

// -----------------------------------------------------------------------------
// Detection
// -----------------------------------------------------------------------------
