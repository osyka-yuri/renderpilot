//! DLSS on-screen debug indicator toggle.
//!
//! DLSS DLL discovery and version reading no longer live here: the global
//! catalog (`renderpilot-detection`) already scans the install directory, reads
//! each `nvngx_dlss*.dll` version, and persists it, and the NVAPI layer consumes
//! that catalog. What remains is the registry-backed DLSS indicator override.

mod indicator;

pub use indicator::{read_dlss_indicator_enabled, set_dlss_indicator_enabled};
