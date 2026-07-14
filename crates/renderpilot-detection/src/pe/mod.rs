//! Portable Executable (PE) parsing helpers.
//!
//! Read-only inspection of Windows PE binaries: the file version from the
//! resource table ([`read_windows_file_version`]) and the graphics API plus
//! architecture from the COFF header and import table ([`analyze_executable`]).

mod binary;
mod exports;
mod graphics;
mod header;
mod image;
#[cfg(test)]
mod tests;
mod version_info;

use std::{fs, path::Path};

use renderpilot_domain::{Architecture, Version};

pub use self::graphics::{analyze_executable, analyze_executable_bytes};
pub use self::version_info::VersionIdentityStrings;
use self::{exports::export_names_from_bytes, image::PeResourceImage, version_info::VersionInfo};

/// All PE facts RenoDX host detection needs, read from a single in-memory buffer
/// so a candidate DLL is read from disk only once (rather than once per field).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeInspection {
    /// File version from the version resource, when present.
    pub version: Option<Version>,
    /// Identity strings from the version resource.
    pub identity: VersionIdentityStrings,
    /// Exported symbol names (`None` when the export table is unreadable).
    pub export_names: Option<Vec<String>>,
}

/// Reads [`PeInspection`] from a PE on disk in a single read. Returns `None` only
/// when the file cannot be read; a readable non-PE yields a default inspection.
#[must_use]
pub fn inspect_pe(path: &Path) -> Option<PeInspection> {
    let bytes = fs::read(path).ok()?;
    Some(PeInspection {
        version: read_windows_file_version_from_bytes(&bytes),
        identity: read_windows_version_strings_from_bytes(&bytes).unwrap_or_default(),
        export_names: read_pe_export_names_from_bytes(&bytes),
    })
}

/// Reads the Windows file version from the PE resource table at the given path.
pub fn read_windows_file_version(path: &Path) -> Option<Version> {
    let bytes = fs::read(path).ok()?;
    read_windows_file_version_from_bytes(&bytes)
}

/// Parses a version string as it appears in a Windows version resource.
///
/// Product-version strings are often decorated (for example with commas or a
/// surrounding label), so consumers that specifically require `ProductVersion`
/// should use this instead of treating the generic file-version fallback as an
/// identity signal.
#[must_use]
pub fn parse_windows_version_text(value: &str) -> Option<Version> {
    version_info::parse_version_text(value)
}

pub(crate) fn read_windows_file_version_from_bytes(bytes: &[u8]) -> Option<Version> {
    let image = PeResourceImage::parse(bytes)?;
    let resource = image.version_resource()?;

    VersionInfo::parse(resource)?.version()
}

/// Reads version-resource identity strings (`ProductName`, `FileDescription`,
/// `OriginalFilename`, `CompanyName`) from a PE file.
pub fn read_windows_version_strings(path: &Path) -> Option<VersionIdentityStrings> {
    let bytes = fs::read(path).ok()?;
    read_windows_version_strings_from_bytes(&bytes)
}

pub(crate) fn read_windows_version_strings_from_bytes(
    bytes: &[u8],
) -> Option<VersionIdentityStrings> {
    let image = PeResourceImage::parse(bytes)?;
    let resource = image.version_resource()?;

    Some(VersionInfo::parse(resource)?.identity_strings())
}

/// Reads exported symbol names from a PE file.
///
/// Returns `Some(vec![])` for a valid PE without exports and `None` for an
/// unreadable or malformed image.
pub fn read_pe_export_names(path: &Path) -> Option<Vec<String>> {
    let bytes = fs::read(path).ok()?;
    read_pe_export_names_from_bytes(&bytes)
}

pub(crate) fn read_pe_export_names_from_bytes(bytes: &[u8]) -> Option<Vec<String>> {
    export_names_from_bytes(bytes)
}

/// Reads the PE COFF machine type from bytes and maps it to RenderPilot's
/// architecture enum, reusing the same machine-type mapping as executable analysis.
pub fn read_pe_architecture_from_bytes(bytes: &[u8]) -> Option<Architecture> {
    let headers = header::PeHeaders::parse(bytes)?;
    graphics::architecture_from_machine(headers.machine())
}
