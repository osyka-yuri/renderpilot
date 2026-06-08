//! Windows registry access helpers for launcher discovery.
//!
//! Reads string path values from a set of candidate keys, transparently trying
//! the 32- and 64-bit registry views, and enumerates per-game subkeys for
//! launchers (GOG, EA, Ubisoft) that store one install path per subkey.

use std::path::PathBuf;

use winreg::{
    enums::{KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY},
    RegKey, HKEY,
};

use super::paths::path_from_string;

const REGISTRY_READ_FLAGS: &[u32] = &[
    KEY_READ,
    KEY_READ | KEY_WOW64_64KEY,
    KEY_READ | KEY_WOW64_32KEY,
];

pub(super) fn discover_registry_paths(
    hive: HKEY,
    base_key_paths: &[&str],
    value_names: &[&str],
) -> Vec<PathBuf> {
    let mut paths = read_registry_values(hive, base_key_paths, value_names);

    for base_key_path in base_key_paths {
        for base_key in open_registry_keys(hive, base_key_path) {
            for subkey_name in base_key.enum_keys().filter_map(Result::ok) {
                let Ok(subkey) = base_key.open_subkey_with_flags(subkey_name, KEY_READ) else {
                    continue;
                };

                paths.extend(read_path_values(&subkey, value_names));
            }
        }
    }

    paths
}

pub(super) fn read_registry_values(
    hive: HKEY,
    key_paths: &[&str],
    value_names: &[&str],
) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    for key_path in key_paths {
        for key in open_registry_keys(hive, key_path) {
            paths.extend(read_path_values(&key, value_names));
        }
    }

    paths
}

fn open_registry_keys(hive: HKEY, key_path: &str) -> Vec<RegKey> {
    let root = RegKey::predef(hive);

    REGISTRY_READ_FLAGS
        .iter()
        .filter_map(|flags| root.open_subkey_with_flags(key_path, *flags).ok())
        .collect()
}

fn read_path_values(key: &RegKey, value_names: &[&str]) -> Vec<PathBuf> {
    value_names
        .iter()
        .filter_map(|value_name| key.get_value::<String, _>(*value_name).ok())
        .filter_map(|value| path_from_string(&value))
        .collect()
}
