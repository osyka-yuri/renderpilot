//! DLSS DLL detection and version reading.
//!
//! Pure-Rust replacement for the previous PowerShell-based approach.
//! Reads the `VS_VERSION_INFO` PE resource of `nvngx_dlss.dll`,
//! `nvngx_dlssg.dll`, or `nvngx_dlssd.dll` directly via [`pelite`],
//! which uses a read-only memory map and therefore works even when
//! the running game holds an exclusive write lock on the DLL.
//!
//! All public types depend on [`renderpilot_nvapi::DlssVersion`] and
//! [`renderpilot_nvapi::DlssDllKind`] so the rest of the codebase has
//! one set of vocabulary for DLSS concepts.

mod dll_search;
mod indicator;
mod version;

pub use dll_search::{find_dlss_dlls, DllSearchResult};
pub use indicator::{read_dlss_indicator_enabled, set_dlss_indicator_enabled};
pub use version::{read_dll_version, DllVersionError};
