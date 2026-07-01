use std::io;
use std::path::Path;

use super::types::VulkanLayerArchitecture;

/// Reads a PE file and returns its machine architecture, or the I/O error
/// encountered while reading the file. Callers distinguish a missing DLL
/// (`NotFound`) from an unreadable one (permission denied, etc.).
pub(crate) fn read_pe_architecture(path: &Path) -> io::Result<VulkanLayerArchitecture> {
    let bytes = std::fs::read(path)?;
    if bytes.len() < 0x40 || &bytes[..2] != b"MZ" {
        return Ok(VulkanLayerArchitecture::Unknown);
    }
    let pe_offset =
        u32::from_le_bytes([bytes[0x3c], bytes[0x3d], bytes[0x3e], bytes[0x3f]]) as usize;
    if bytes.len() < pe_offset + 6 || &bytes[pe_offset..pe_offset + 4] != b"PE\0\0" {
        return Ok(VulkanLayerArchitecture::Unknown);
    }
    Ok(
        match u16::from_le_bytes([bytes[pe_offset + 4], bytes[pe_offset + 5]]) {
            0x8664 => VulkanLayerArchitecture::X64,
            0x014c => VulkanLayerArchitecture::X86,
            _ => VulkanLayerArchitecture::Unknown,
        },
    )
}
