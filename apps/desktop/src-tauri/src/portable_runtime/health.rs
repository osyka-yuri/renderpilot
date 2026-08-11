use std::path::Path;

use super::{
    error::{PortableRuntimeError, Result},
    rpu::RpuManifest,
    signature::sha256_file,
};

/// Rechecks the exact selected App image before each activation or recovery.
pub fn validate_selected_app(path: &Path, manifest: &RpuManifest) -> Result<()> {
    let bytes = std::fs::read(path)?;
    if bytes.len() < 0x40 || &bytes[..2] != b"MZ" || sha256_file(path)? != manifest.app_sha256 {
        return Err(PortableRuntimeError::new(
            "portable_app_identity",
            "selected App image did not match its signed RPU manifest",
        ));
    }
    let offset = u32::from_le_bytes([bytes[0x3c], bytes[0x3d], bytes[0x3e], bytes[0x3f]]) as usize;
    if bytes.get(offset..offset + 6) != Some(b"PE\0\0\x64\x86") {
        return Err(PortableRuntimeError::new(
            "portable_app_identity",
            "selected App was not an x64 PE image",
        ));
    }
    Ok(())
}
