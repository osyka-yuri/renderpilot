//! Read/write the global NVIDIA "DLSS Indicator" debug overlay toggle.
//!
//! The indicator is a single machine-wide registry value the NGX runtime reads
//! for **every** DLSS title — it is not per-game or per-executable:
//!
//! ```text
//! HKEY_LOCAL_MACHINE\SOFTWARE\NVIDIA Corporation\Global\NGXCore
//!   ShowDlssIndicator : REG_DWORD = 0x400   (on; absent or 0 = off)
//! ```
//!
//! Reading the value needs no special rights; writing under `HKLM\SOFTWARE`
//! requires an elevated process (see the desktop elevation flow). NVIDIA stores
//! this in the 64-bit registry view, so we pin `KEY_WOW64_64KEY` for both read
//! and write to stay consistent regardless of the host process bitness.

use std::io;

use winreg::{
    enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY, KEY_WRITE},
    RegKey,
};

/// NGX core key holding the DLSS indicator toggle, relative to `HKLM`.
const NGX_CORE_KEY: &str = r"SOFTWARE\NVIDIA Corporation\Global\NGXCore";
/// DWORD value name the NGX runtime reads to decide whether to draw the overlay.
const SHOW_DLSS_INDICATOR_VALUE: &str = "ShowDlssIndicator";
/// Value that enables the on-screen indicator (the textual overlay in the corner).
const INDICATOR_ON: u32 = 0x400;

/// Returns `true` when the DLSS indicator overlay is currently enabled.
///
/// A missing key or missing value is reported as "off" — the pristine default.
pub fn read_dlss_indicator_enabled() -> io::Result<bool> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    match hklm.open_subkey_with_flags(NGX_CORE_KEY, KEY_READ | KEY_WOW64_64KEY) {
        Ok(key) => match key.get_value::<u32, _>(SHOW_DLSS_INDICATOR_VALUE) {
            Ok(value) => Ok(value != 0),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Enables or disables the DLSS indicator overlay machine-wide.
///
/// Enabling writes `ShowDlssIndicator = 0x400`; disabling deletes the value so
/// the key returns to its pristine default. Requires an elevated process —
/// without it the write fails with `ERROR_ACCESS_DENIED` (`raw_os_error()` 5).
pub fn set_dlss_indicator_enabled(enabled: bool) -> io::Result<()> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let (key, _disposition) =
        hklm.create_subkey_with_flags(NGX_CORE_KEY, KEY_WRITE | KEY_WOW64_64KEY)?;

    if enabled {
        key.set_value(SHOW_DLSS_INDICATOR_VALUE, &INDICATOR_ON)
    } else {
        match key.delete_value(SHOW_DLSS_INDICATOR_VALUE) {
            Ok(()) => Ok(()),
            // Already absent — nothing to turn off.
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}
