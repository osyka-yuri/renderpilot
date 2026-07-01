use std::path::PathBuf;

/// The shared ReShade install directory: `C:\ProgramData\ReShade\`.
/// Matches the official ReShade installer's `commonPath`.
#[must_use]
pub fn reshade_common_dir() -> Option<PathBuf> {
    std::env::var_os("PROGRAMDATA").map(|p| PathBuf::from(p).join("ReShade"))
}

// -----------------------------------------------------------------------------
// Registry abstraction
// -----------------------------------------------------------------------------
