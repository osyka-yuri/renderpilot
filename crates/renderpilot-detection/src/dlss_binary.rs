//! Strict identification of NVIDIA DLSS super-resolution binaries.

use std::fmt;
use std::fs;
use std::path::Path;

use renderpilot_domain::{Architecture, Version};

use crate::pe::{
    read_pe_architecture_from_bytes, read_windows_file_version_from_bytes,
    read_windows_version_strings_from_bytes,
};

/// The canonical file name of the NVIDIA DLSS super-resolution binary.
pub const NVNGX_DLSS_FILE_NAME: &str = "nvngx_dlss.dll";

/// Verified facts used when deciding whether a DLSS DLL may be reused or replaced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlssBinaryInfo {
    architecture: Architecture,
    version: Version,
}

impl DlssBinaryInfo {
    /// Inspects a file on disk and fails closed when identity or version is unknown.
    pub fn from_path(path: &Path) -> Result<Self, DlssBinaryError> {
        let bytes = fs::read(path).map_err(|error| DlssBinaryError::Unreadable {
            path: path.display().to_string(),
            detail: error.to_string(),
        })?;
        Self::from_bytes(&bytes)
    }

    /// Inspects in-memory PE bytes using the same identity policy as [`Self::from_path`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DlssBinaryError> {
        let architecture = read_pe_architecture_from_bytes(bytes).ok_or(DlssBinaryError::NotPe)?;
        if architecture != Architecture::X64 {
            return Err(DlssBinaryError::UnsupportedArchitecture(architecture));
        }

        let identity = read_windows_version_strings_from_bytes(bytes)
            .ok_or(DlssBinaryError::MissingIdentity)?;
        if !is_nvidia_dlss_identity(&identity) {
            return Err(DlssBinaryError::WrongIdentity);
        }

        let version =
            read_windows_file_version_from_bytes(bytes).ok_or(DlssBinaryError::MissingVersion)?;

        Ok(Self {
            architecture,
            version,
        })
    }

    /// Returns the verified PE architecture.
    #[must_use]
    pub fn architecture(&self) -> Architecture {
        self.architecture
    }

    /// Returns the normalized, numerically comparable file version.
    #[must_use]
    pub fn version(&self) -> &Version {
        &self.version
    }
}

fn is_nvidia_dlss_identity(identity: &crate::VersionIdentityStrings) -> bool {
    let values = [
        identity.product_name.as_deref(),
        identity.file_description.as_deref(),
        identity.original_filename.as_deref(),
        identity.company_name.as_deref(),
    ];
    let nvidia = values
        .iter()
        .flatten()
        .any(|value| value.to_ascii_lowercase().contains("nvidia"));
    let dlss = identity
        .original_filename
        .as_deref()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case(NVNGX_DLSS_FILE_NAME))
        || values.iter().flatten().any(|value| {
            let value = value.to_ascii_lowercase();
            value.contains("dlss") || value.contains("deep learning super sampling")
        });

    nvidia && dlss
}

/// Why a candidate cannot be trusted as an NVIDIA DLSS binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DlssBinaryError {
    /// The file could not be read completely.
    Unreadable {
        /// Display path of the candidate.
        path: String,
        /// Underlying I/O failure.
        detail: String,
    },
    /// The bytes are not a supported PE image.
    NotPe,
    /// DLSS replacement is only supported for the x64 binary used by games.
    UnsupportedArchitecture(Architecture),
    /// The PE has no readable version identity strings.
    MissingIdentity,
    /// The PE identity does not prove NVIDIA DLSS super resolution.
    WrongIdentity,
    /// The PE identity is valid but its numeric version is unavailable.
    MissingVersion,
}

impl fmt::Display for DlssBinaryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable { path, detail } => {
                write!(
                    formatter,
                    "could not read DLSS candidate `{path}`: {detail}"
                )
            }
            Self::NotPe => formatter.write_str("DLSS candidate is not a readable PE image"),
            Self::UnsupportedArchitecture(architecture) => write!(
                formatter,
                "DLSS candidate has unsupported architecture {architecture:?}"
            ),
            Self::MissingIdentity => {
                formatter.write_str("DLSS candidate has no readable PE identity")
            }
            Self::WrongIdentity => {
                formatter.write_str("PE identity does not prove NVIDIA DLSS super resolution")
            }
            Self::MissingVersion => {
                formatter.write_str("NVIDIA DLSS candidate has no reliable numeric version")
            }
        }
    }
}

impl std::error::Error for DlssBinaryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arbitrary_bytes_fail_closed() {
        assert_eq!(
            DlssBinaryInfo::from_bytes(b"not a PE"),
            Err(DlssBinaryError::NotPe)
        );
    }

    #[test]
    fn unreadable_path_fail_closed() {
        let missing = std::env::temp_dir().join(format!(
            "renderpilot-missing-dlss-{}.dll",
            std::process::id()
        ));
        let error = DlssBinaryInfo::from_path(&missing).expect_err("missing path");
        assert!(matches!(error, DlssBinaryError::Unreadable { .. }));
    }

    #[test]
    fn x86_pe_fails_closed_as_unsupported_architecture() {
        let pe = minimal_pe(0x014c);
        assert_eq!(
            DlssBinaryInfo::from_bytes(&pe),
            Err(DlssBinaryError::UnsupportedArchitecture(Architecture::X86))
        );
    }

    #[test]
    fn x64_pe_without_identity_fails_closed() {
        // Valid x64 PE shape with no version resource must not be trusted as DLSS.
        let pe = minimal_pe(0x8664);
        assert_eq!(
            DlssBinaryInfo::from_bytes(&pe),
            Err(DlssBinaryError::MissingIdentity)
        );
    }

    #[test]
    fn nvidia_dlss_identity_pe_is_accepted() {
        // Happy-path identity/arch/version acceptance (fixture mirrors production
        // version-resource layout used by orchestration policy tests).
        let pe = nvidia_dlss_pe([3, 7, 10, 0]);
        let info = DlssBinaryInfo::from_bytes(&pe).expect("valid DLSS PE");
        assert_eq!(info.architecture(), Architecture::X64);
        assert_eq!(info.version().as_str(), "3.7.10.0");
    }

    fn nvidia_dlss_pe(version: [u16; 4]) -> Vec<u8> {
        // Minimal PE32+ x64 with RT_VERSION resource and NVIDIA DLSS strings.
        const COFF_HEADER_LEN: usize = 20;
        const SECTION_HEADER_LEN: usize = 40;
        const DOS_PE_POINTER_OFFSET: usize = 0x3c;
        const PE32_PLUS_MAGIC: u16 = 0x20b;
        const PE32_PLUS_DATA_DIRECTORY_OFFSET: usize = 112;
        const RESOURCE_DIRECTORY_INDEX: usize = 2;
        const DATA_DIRECTORY_ENTRY_LEN: usize = 8;
        const MACHINE_AMD64: u16 = 0x8664;

        let pe_offset: usize = 0x80;
        let optional_header_size: usize = 0xF0;
        let coff_offset = pe_offset + 4;
        let optional_header_offset = coff_offset + COFF_HEADER_LEN;
        let optional_header_end = optional_header_offset + optional_header_size;
        let section_table_offset = optional_header_end;
        let headers_end = section_table_offset + SECTION_HEADER_LEN;
        let section_rva: u32 = 0x1000;
        let section_raw_ptr = ((headers_end as u32) + 0x1ff) & !0x1ff;

        let mut version_blob = Vec::new();
        // VS_VERSIONINFO skeleton with FileVersion and StringFileInfo keys.
        let file_version = ((version[0] as u32) << 16) | version[1] as u32;
        let product_version = ((version[2] as u32) << 16) | version[3] as u32;
        // Use the shared PE version-info builder path by writing a compact blob
        // that `read_windows_*_from_bytes` already accepts in orchestration tests.
        // Fallback: if the compact blob is rejected, the fail-closed tests above
        // still cover security-relevant rejects; this path uses a full resource tree.
        let _ = (file_version, product_version);

        // Delegate to a complete resource tree identical in shape to orchestration fixtures.
        version_blob.extend_from_slice(&full_version_info_blob(version));

        let data_offset = 88usize;
        let mut section_body = vec![0u8; data_offset];
        section_body.extend_from_slice(&version_blob);
        section_body[14..16].copy_from_slice(&1u16.to_le_bytes());
        section_body[16..20].copy_from_slice(&16u32.to_le_bytes());
        section_body[20..24].copy_from_slice(&(0x8000_0000u32 | 24).to_le_bytes());
        section_body[24 + 14..24 + 16].copy_from_slice(&1u16.to_le_bytes());
        section_body[40..44].copy_from_slice(&1u32.to_le_bytes());
        section_body[44..48].copy_from_slice(&(0x8000_0000u32 | 48).to_le_bytes());
        section_body[48 + 14..48 + 16].copy_from_slice(&1u16.to_le_bytes());
        section_body[64..68].copy_from_slice(&1033u32.to_le_bytes());
        section_body[68..72].copy_from_slice(&72u32.to_le_bytes());
        section_body[72..76].copy_from_slice(&(section_rva + data_offset as u32).to_le_bytes());
        section_body[76..80].copy_from_slice(&(version_blob.len() as u32).to_le_bytes());

        let total_len = section_raw_ptr as usize + section_body.len();
        let mut bytes = vec![0u8; total_len];
        bytes[0..2].copy_from_slice(b"MZ");
        bytes[DOS_PE_POINTER_OFFSET..DOS_PE_POINTER_OFFSET + 4]
            .copy_from_slice(&(pe_offset as u32).to_le_bytes());
        bytes[pe_offset..pe_offset + 4].copy_from_slice(b"PE\0\0");
        bytes[coff_offset..coff_offset + 2].copy_from_slice(&MACHINE_AMD64.to_le_bytes());
        bytes[coff_offset + 2..coff_offset + 4].copy_from_slice(&1u16.to_le_bytes());
        bytes[coff_offset + 16..coff_offset + 18]
            .copy_from_slice(&(optional_header_size as u16).to_le_bytes());
        bytes[optional_header_offset..optional_header_offset + 2]
            .copy_from_slice(&PE32_PLUS_MAGIC.to_le_bytes());
        let resource_entry = optional_header_offset
            + PE32_PLUS_DATA_DIRECTORY_OFFSET
            + RESOURCE_DIRECTORY_INDEX * DATA_DIRECTORY_ENTRY_LEN;
        bytes[resource_entry..resource_entry + 4].copy_from_slice(&section_rva.to_le_bytes());
        bytes[resource_entry + 4..resource_entry + 8]
            .copy_from_slice(&(section_body.len() as u32).to_le_bytes());
        bytes[section_table_offset..section_table_offset + 8].copy_from_slice(b".rsrc\0\0\0");
        bytes[section_table_offset + 8..section_table_offset + 12]
            .copy_from_slice(&(section_body.len() as u32).to_le_bytes());
        bytes[section_table_offset + 12..section_table_offset + 16]
            .copy_from_slice(&section_rva.to_le_bytes());
        bytes[section_table_offset + 16..section_table_offset + 20]
            .copy_from_slice(&(section_body.len() as u32).to_le_bytes());
        bytes[section_table_offset + 20..section_table_offset + 24]
            .copy_from_slice(&section_raw_ptr.to_le_bytes());
        bytes[section_raw_ptr as usize..].copy_from_slice(&section_body);
        bytes
    }

    fn full_version_info_blob(version: [u16; 4]) -> Vec<u8> {
        fn utf16_bytes(value: &str) -> Vec<u8> {
            value.encode_utf16().flat_map(u16::to_le_bytes).collect()
        }
        fn align4(buf: &mut Vec<u8>) {
            while !buf.len().is_multiple_of(4) {
                buf.push(0);
            }
        }
        fn string_entry(key: &str, value: &str) -> Vec<u8> {
            let key_bytes = utf16_bytes(key);
            let value_bytes = utf16_bytes(&(value.to_owned() + "\0"));
            let mut body = Vec::new();
            body.extend_from_slice(&0u16.to_le_bytes()); // wLength placeholder
            body.extend_from_slice(&((value_bytes.len() / 2) as u16).to_le_bytes());
            body.extend_from_slice(&1u16.to_le_bytes()); // text
            body.extend_from_slice(&key_bytes);
            body.extend_from_slice(&0u16.to_le_bytes());
            align4(&mut body);
            body.extend_from_slice(&value_bytes);
            align4(&mut body);
            let len = body.len() as u16;
            body[0..2].copy_from_slice(&len.to_le_bytes());
            body
        }
        fn string_table(entries: &[(&str, &str)]) -> Vec<u8> {
            let mut children = Vec::new();
            for (k, v) in entries {
                children.extend_from_slice(&string_entry(k, v));
            }
            let key = utf16_bytes("040904B0");
            let mut body = Vec::new();
            body.extend_from_slice(&0u16.to_le_bytes());
            body.extend_from_slice(&0u16.to_le_bytes());
            body.extend_from_slice(&1u16.to_le_bytes());
            body.extend_from_slice(&key);
            body.extend_from_slice(&0u16.to_le_bytes());
            align4(&mut body);
            body.extend_from_slice(&children);
            let len = body.len() as u16;
            body[0..2].copy_from_slice(&len.to_le_bytes());
            body
        }
        fn string_file_info(entries: &[(&str, &str)]) -> Vec<u8> {
            let table = string_table(entries);
            let key = utf16_bytes("StringFileInfo");
            let mut body = Vec::new();
            body.extend_from_slice(&0u16.to_le_bytes());
            body.extend_from_slice(&0u16.to_le_bytes());
            body.extend_from_slice(&1u16.to_le_bytes());
            body.extend_from_slice(&key);
            body.extend_from_slice(&0u16.to_le_bytes());
            align4(&mut body);
            body.extend_from_slice(&table);
            let len = body.len() as u16;
            body[0..2].copy_from_slice(&len.to_le_bytes());
            body
        }

        let strings = string_file_info(&[
            ("CompanyName", "NVIDIA"),
            ("FileDescription", "NVIDIA DLSS"),
            ("OriginalFilename", "nvngx_dlss.dll"),
            ("ProductName", "NVIDIA DLSS"),
        ]);
        let mut fixed = vec![0u8; 52];
        fixed[0..4].copy_from_slice(&0xFEEF04BDu32.to_le_bytes()); // signature
        fixed[8..12]
            .copy_from_slice(&(((version[0] as u32) << 16) | version[1] as u32).to_le_bytes());
        fixed[12..16]
            .copy_from_slice(&(((version[2] as u32) << 16) | version[3] as u32).to_le_bytes());

        let key = utf16_bytes("VS_VERSION_INFO");
        let mut body = Vec::new();
        body.extend_from_slice(&0u16.to_le_bytes());
        body.extend_from_slice(&(fixed.len() as u16).to_le_bytes());
        body.extend_from_slice(&0u16.to_le_bytes()); // binary
        body.extend_from_slice(&key);
        body.extend_from_slice(&0u16.to_le_bytes());
        align4(&mut body);
        body.extend_from_slice(&fixed);
        align4(&mut body);
        body.extend_from_slice(&strings);
        let len = body.len() as u16;
        body[0..2].copy_from_slice(&len.to_le_bytes());
        body
    }

    fn minimal_pe(machine: u16) -> Vec<u8> {
        let pe_offset: usize = 0x80;
        let optional_header_size: usize = 0xF0;
        let coff_offset = pe_offset + 4;
        let optional_header_offset = coff_offset + 20;
        let total_len = optional_header_offset + optional_header_size;
        let mut bytes = vec![0u8; total_len];
        bytes[0..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&(pe_offset as u32).to_le_bytes());
        bytes[pe_offset..pe_offset + 4].copy_from_slice(b"PE\0\0");
        bytes[coff_offset..coff_offset + 2].copy_from_slice(&machine.to_le_bytes());
        bytes[coff_offset + 16..coff_offset + 18]
            .copy_from_slice(&(optional_header_size as u16).to_le_bytes());
        bytes[optional_header_offset..optional_header_offset + 2]
            .copy_from_slice(&0x20bu16.to_le_bytes()); // PE32+
        bytes
    }
}
