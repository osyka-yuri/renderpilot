//! Read-only Windows Developer Mode detection.
//!
//! DX12Core preview builds require Developer Mode. The effective value can be
//! controlled by Group Policy, so the policy key is evaluated before the
//! ordinary Windows setting. Registry access is deliberately kept inside the
//! platform adapter; callers receive only a stable three-state result.

/// Effective Windows Developer Mode state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeveloperModeStatus {
    /// Developer Mode is enabled and DX12Core preview packages may be applied.
    Enabled,
    /// Developer Mode is explicitly disabled or has never been enabled.
    Disabled,
    /// The effective value could not be determined safely.
    Unknown,
}

/// Reads the effective Windows Developer Mode state.
///
/// Non-Windows builds return [`DeveloperModeStatus::Unknown`]. This keeps the
/// adapter callable from cross-platform orchestration tests without pretending
/// that another operating system satisfies the Windows prerequisite.
pub fn developer_mode_status() -> DeveloperModeStatus {
    platform_developer_mode_status()
}

#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegistryValue {
    Missing,
    Dword(u32),
    Unavailable,
}

#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PolicyResolution {
    Resolved(DeveloperModeStatus),
    UseUserSetting,
}

#[cfg(any(windows, test))]
fn resolve_policy(policy: RegistryValue) -> PolicyResolution {
    match policy {
        RegistryValue::Dword(0) => PolicyResolution::Resolved(DeveloperModeStatus::Disabled),
        RegistryValue::Dword(1) => PolicyResolution::Resolved(DeveloperModeStatus::Enabled),
        // ADMX-backed policy uses 65535 for "not configured".
        RegistryValue::Dword(65_535) | RegistryValue::Missing => PolicyResolution::UseUserSetting,
        RegistryValue::Dword(_) | RegistryValue::Unavailable => {
            PolicyResolution::Resolved(DeveloperModeStatus::Unknown)
        }
    }
}

#[cfg(any(windows, test))]
fn resolve_setting(setting: RegistryValue) -> DeveloperModeStatus {
    match setting {
        RegistryValue::Dword(1) => DeveloperModeStatus::Enabled,
        RegistryValue::Dword(0) | RegistryValue::Missing => DeveloperModeStatus::Disabled,
        RegistryValue::Dword(_) | RegistryValue::Unavailable => DeveloperModeStatus::Unknown,
    }
}

#[cfg(test)]
fn resolve_status(policy: RegistryValue, setting: RegistryValue) -> DeveloperModeStatus {
    resolve_status_with(policy, || setting)
}

#[cfg(any(windows, test))]
fn resolve_status_with(
    policy: RegistryValue,
    read_setting: impl FnOnce() -> RegistryValue,
) -> DeveloperModeStatus {
    match resolve_policy(policy) {
        PolicyResolution::Resolved(status) => status,
        PolicyResolution::UseUserSetting => resolve_setting(read_setting()),
    }
}

#[cfg(windows)]
fn platform_developer_mode_status() -> DeveloperModeStatus {
    use winreg::enums::HKEY_LOCAL_MACHINE;

    const POLICY_KEY: &str = r"SOFTWARE\Policies\Microsoft\Windows\Appx";
    const SETTING_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock";
    const VALUE_NAME: &str = "AllowDevelopmentWithoutDevLicense";

    resolve_status_with(
        read_hklm_dword(HKEY_LOCAL_MACHINE, POLICY_KEY, VALUE_NAME),
        || read_hklm_dword(HKEY_LOCAL_MACHINE, SETTING_KEY, VALUE_NAME),
    )
}

#[cfg(windows)]
fn read_hklm_dword(hive: winreg::HKEY, key_path: &str, value_name: &str) -> RegistryValue {
    use std::io;

    use winreg::{
        RegKey,
        enums::{KEY_READ, KEY_WOW64_64KEY},
    };

    let root = RegKey::predef(hive);
    let key = match root.open_subkey_with_flags(key_path, KEY_READ | KEY_WOW64_64KEY) {
        Ok(key) => key,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return RegistryValue::Missing,
        Err(_) => return RegistryValue::Unavailable,
    };

    match key.get_value::<u32, _>(value_name) {
        Ok(value) => RegistryValue::Dword(value),
        Err(error) if error.kind() == io::ErrorKind::NotFound => RegistryValue::Missing,
        Err(_) => RegistryValue::Unavailable,
    }
}

#[cfg(not(windows))]
const fn platform_developer_mode_status() -> DeveloperModeStatus {
    DeveloperModeStatus::Unknown
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::{DeveloperModeStatus, RegistryValue, resolve_status, resolve_status_with};

    #[test]
    fn registry_values_resolve_with_policy_precedence() {
        for (policy, setting, expected) in [
            (
                RegistryValue::Dword(0),
                RegistryValue::Dword(1),
                DeveloperModeStatus::Disabled,
            ),
            (
                RegistryValue::Dword(1),
                RegistryValue::Dword(0),
                DeveloperModeStatus::Enabled,
            ),
            (
                RegistryValue::Missing,
                RegistryValue::Dword(1),
                DeveloperModeStatus::Enabled,
            ),
            (
                RegistryValue::Missing,
                RegistryValue::Dword(0),
                DeveloperModeStatus::Disabled,
            ),
            (
                RegistryValue::Missing,
                RegistryValue::Missing,
                DeveloperModeStatus::Disabled,
            ),
            (
                RegistryValue::Dword(65_535),
                RegistryValue::Dword(1),
                DeveloperModeStatus::Enabled,
            ),
            (
                RegistryValue::Dword(65_535),
                RegistryValue::Dword(0),
                DeveloperModeStatus::Disabled,
            ),
            (
                RegistryValue::Dword(2),
                RegistryValue::Dword(1),
                DeveloperModeStatus::Unknown,
            ),
            (
                RegistryValue::Unavailable,
                RegistryValue::Dword(1),
                DeveloperModeStatus::Unknown,
            ),
            (
                RegistryValue::Missing,
                RegistryValue::Dword(2),
                DeveloperModeStatus::Unknown,
            ),
            (
                RegistryValue::Missing,
                RegistryValue::Unavailable,
                DeveloperModeStatus::Unknown,
            ),
        ] {
            assert_eq!(resolve_status(policy, setting), expected);
        }
    }

    #[test]
    fn user_setting_is_read_only_when_policy_is_not_configured() {
        for (policy, expected_reads) in [
            (RegistryValue::Dword(0), 0),
            (RegistryValue::Dword(1), 0),
            (RegistryValue::Dword(2), 0),
            (RegistryValue::Unavailable, 0),
            (RegistryValue::Missing, 1),
            (RegistryValue::Dword(65_535), 1),
        ] {
            let reads = Cell::new(0);
            let _ = resolve_status_with(policy, || {
                reads.set(reads.get() + 1);
                RegistryValue::Dword(1)
            });
            assert_eq!(reads.get(), expected_reads, "policy: {policy:?}");
        }
    }
}
