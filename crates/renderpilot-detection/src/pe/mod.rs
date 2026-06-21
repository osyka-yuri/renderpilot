//! Portable Executable (PE) parsing helpers.
//!
//! Read-only inspection of Windows PE binaries: the file version from the
//! resource table ([`read_windows_file_version`]) and the graphics API plus
//! architecture from the COFF header and import table ([`analyze_executable`]).

mod binary;
mod graphics;
mod header;
mod image;
#[cfg(test)]
mod tests;
mod version_info;

use std::{fs, path::Path};

use renderpilot_domain::Version;

pub use self::graphics::{analyze_executable, analyze_executable_bytes};
use self::{image::PeResourceImage, version_info::VersionInfo};

/// Reads the Windows file version from the PE resource table at the given path.
pub fn read_windows_file_version(path: &Path) -> Option<Version> {
    let bytes = fs::read(path).ok()?;
    read_windows_file_version_from_bytes(&bytes)
}

pub(crate) fn read_windows_file_version_from_bytes(bytes: &[u8]) -> Option<Version> {
    let image = PeResourceImage::parse(bytes)?;
    let resource = image.version_resource()?;

    VersionInfo::parse(resource)?.version()
}
