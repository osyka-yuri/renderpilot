//! Read the file version from a PE DLL.
//!
//! Uses `pelite` to parse the `VS_VERSION_INFO` resource. Memory-maps
//! the file read-only so a running game's exclusive lock does not
//! block us.

use std::{fmt, io, path::Path};

use pelite::{FileMap, PeFile};
use renderpilot_nvapi::DlssVersion;

/// Errors that can occur while reading a DLL version.
#[derive(Debug)]
pub enum DllVersionError {
    /// Failed to map the file (file missing, permission denied, etc.).
    Io(io::Error),
    /// File does not parse as a valid PE image, or required PE resources
    /// were absent / malformed.
    InvalidPe(String),
    /// The PE has no `VS_VERSION_INFO` resource at all.
    NoVersionInfo,
}

impl fmt::Display for DllVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "failed to read DLL: {error}"),
            Self::InvalidPe(detail) => write!(formatter, "invalid PE image: {detail}"),
            Self::NoVersionInfo => write!(formatter, "PE has no VS_VERSION_INFO resource"),
        }
    }
}

impl std::error::Error for DllVersionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for DllVersionError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Reads the four-part file version from a PE DLL's `VS_VERSION_INFO`
/// resource.
///
/// Works regardless of PE architecture (32-bit or 64-bit) and regardless
/// of whether another process holds a write lock on the file — the
/// underlying memory map is opened read-only.
pub fn read_dll_version(path: &Path) -> Result<DlssVersion, DllVersionError> {
    let map = FileMap::open(path)?;

    // `PeFile` is `Wrap<pe32::PeFile, pe64::PeFile>`. The two variants
    // each implement `Pe<'_>`, but the trait isn't object-safe and the
    // `Wrap` itself doesn't implement it, so we dispatch by match and
    // extract just the raw u64 file-version word from each branch.
    let pe =
        PeFile::from_bytes(&map).map_err(|error| DllVersionError::InvalidPe(error.to_string()))?;

    // `pe32::Pe` and `pe64::Pe` are *different* traits, so we can't
    // dispatch through a generic helper. But both `.resources()` calls
    // return the same `pelite::resources::Resources`, so we only need
    // a match for that one call and the rest can stay linear.
    let resources = match pe {
        pelite::Wrap::T32(p) => {
            use pelite::pe32::Pe as _;
            p.resources()
        }
        pelite::Wrap::T64(p) => {
            use pelite::pe64::Pe as _;
            p.resources()
        }
    }
    .map_err(|error| DllVersionError::InvalidPe(format!("no resource section: {error}")))?;

    let version_info = resources
        .version_info()
        .map_err(|_| DllVersionError::NoVersionInfo)?;
    let fixed = version_info.fixed().ok_or(DllVersionError::NoVersionInfo)?;
    let v = fixed.dwFileVersion;
    Ok(DlssVersion::new(
        u32::from(v.Major),
        u32::from(v.Minor),
        u32::from(v.Patch),
        u32::from(v.Build),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads `C:\Windows\System32\kernel32.dll` — a well-known PE file
    /// with a standard `VS_VERSION_INFO` resource — and asserts we get a
    /// plausible Windows version. Skipped if the file is missing
    /// (e.g. when these tests run on a non-Windows CI machine).
    #[test]
    fn read_kernel32_returns_plausible_windows_version() {
        let path = Path::new(r"C:\Windows\System32\kernel32.dll");
        if !path.exists() {
            eprintln!("kernel32.dll not found, skipping smoke test");
            return;
        }
        let version = read_dll_version(path).expect("kernel32 should have a version resource");
        // Windows 10 and 11 both report major=10 in VS_FIXEDFILEINFO.
        // Anything from 6 (Vista) onward is a sane lower bound.
        assert!(
            version.major >= 6 && version.major <= 1000,
            "kernel32 reported implausible version: {version}"
        );
    }

    #[test]
    fn read_missing_file_returns_io_error() {
        let result = read_dll_version(Path::new("definitely_not_real.dll"));
        assert!(matches!(result, Err(DllVersionError::Io(_))));
    }

    #[test]
    fn read_non_pe_file_returns_error_not_panic() {
        // Either Io (mapping fails for tiny files on some systems) or
        // InvalidPe (mapping succeeds but PE parse rejects it).
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            tmp.path(),
            b"not a PE file, just plain text padding padding padding padding padding",
        )
        .unwrap();
        let result = read_dll_version(tmp.path());
        assert!(
            result.is_err(),
            "expected an error for non-PE file, got: {result:?}"
        );
    }
}
