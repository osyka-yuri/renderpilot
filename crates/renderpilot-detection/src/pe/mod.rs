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
mod source;
#[cfg(test)]
mod tests;
mod version_info;

use std::{fs, path::Path};

use renderpilot_domain::{Architecture, PeCompatibilityProfile, PeExportSet, Version};

pub use self::graphics::{analyze_executable, analyze_executable_bytes};
pub use self::version_info::VersionIdentityStrings;
use self::{
    exports::{
        export_names_from_bytes, exported_u32_location_from_bytes, exported_u32_location_from_path,
    },
    image::PeResourceImage,
    version_info::VersionInfo,
};

/// A unique named PE DATA export containing an inline little-endian `u32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeExportedU32 {
    /// Value observed in the image.
    pub value: u32,
    /// Byte offset of the four-byte value in the PE file.
    pub file_offset: usize,
}

/// PE facts observed from one in-memory image, allowing callers to derive file
/// version and compatibility data without reading the DLL once per field.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeInspection {
    /// COFF architecture of the image, when supported.
    pub architecture: Option<Architecture>,
    /// File version from the version resource, when present.
    pub version: Option<Version>,
    /// Identity strings from the version resource.
    pub identity: VersionIdentityStrings,
    /// Exported symbol names (`None` when the export table is unreadable).
    pub export_names: Option<Vec<String>>,
}

impl PeInspection {
    /// Builds a complete export-surface compatibility profile.
    ///
    /// A readable architecture with missing, empty, duplicate, or malformed
    /// named exports is intentionally treated as no profile.
    #[must_use]
    pub fn compatibility_profile(&self) -> Option<PeCompatibilityProfile> {
        let architecture = self.architecture?;
        let exports = PeExportSet::from_observed_names(self.export_names.clone()?).ok()?;
        Some(PeCompatibilityProfile::new(architecture, exports))
    }
}

/// Reads [`PeInspection`] from a PE on disk in a single read. Returns `None` only
/// when the file cannot be read; a readable non-PE yields a default inspection.
#[must_use]
pub fn inspect_pe(path: &Path) -> Option<PeInspection> {
    let bytes = fs::read(path).ok()?;
    Some(inspect_pe_bytes(&bytes))
}

/// Inspects one immutable in-memory PE image.
///
/// A readable non-PE image yields a default inspection. Keeping this operation
/// byte-based lets mutation boundaries hash and inspect the exact same snapshot.
#[must_use]
pub fn inspect_pe_bytes(bytes: &[u8]) -> PeInspection {
    PeInspection {
        architecture: read_pe_architecture_from_bytes(bytes),
        version: read_windows_file_version_from_bytes(bytes),
        identity: read_windows_version_strings_from_bytes(bytes).unwrap_or_default(),
        export_names: read_pe_export_names_from_bytes(bytes),
    }
}

/// Reads a unique named PE DATA export containing an inline `u32`.
pub fn read_pe_exported_u32(path: &Path, name: &str) -> Option<u32> {
    exported_u32_location_from_path(path, name).map(|export| export.value)
}

/// Locates a unique named PE DATA export containing an inline `u32`.
///
/// Function/forwarder exports, duplicate names, and malformed PE images return
/// `None`. The returned file offset is suitable for a narrowly scoped byte
/// patch after the caller has independently validated the source image.
#[must_use]
pub fn pe_exported_u32_from_bytes(bytes: &[u8], name: &str) -> Option<PeExportedU32> {
    exported_u32_location_from_bytes(bytes, name)
}

/// Replaces one unique inline `u32` PE DATA export in memory.
///
/// The mutation is performed only when the currently observed value equals
/// `expected`. The returned location describes the value before replacement.
/// No filesystem writes are performed by this helper.
pub fn replace_pe_exported_u32_in_bytes(
    bytes: &mut [u8],
    name: &str,
    expected: u32,
    replacement: u32,
) -> Option<PeExportedU32> {
    let export = pe_exported_u32_from_bytes(bytes, name)?;
    if export.value != expected {
        return None;
    }
    let end = export.file_offset.checked_add(std::mem::size_of::<u32>())?;
    bytes
        .get_mut(export.file_offset..end)?
        .copy_from_slice(&replacement.to_le_bytes());
    (pe_exported_u32_from_bytes(bytes, name)?.value == replacement).then_some(export)
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
