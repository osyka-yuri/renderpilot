mod binary;
mod image;
#[cfg(test)]
mod tests;
mod version_info;

use std::{fs, path::Path};

use renderpilot_domain::Version;

use self::{image::PeResourceImage, version_info::VersionInfo};

pub(crate) fn read_windows_file_version(path: &Path) -> Option<Version> {
    let bytes = fs::read(path).ok()?;
    read_windows_file_version_from_bytes(&bytes)
}

pub(super) fn read_windows_file_version_from_bytes(bytes: &[u8]) -> Option<Version> {
    let image = PeResourceImage::parse(bytes)?;
    let resource = image.version_resource()?;

    VersionInfo::parse(resource).version()
}
