//! Export-table inspection for PE images.
//!
//! ReShade exposes stable symbols such as `ReShadeVersion` from the host DLL.
//! Reading the export name table gives the orchestration layer a much stronger
//! identity signal than file names or neighbouring config files.

use super::binary::{checked_range, read_u32};
use super::header::{PeHeaders, rva_to_offset};

const EXPORT_DIRECTORY_INDEX: usize = 0;
const EXPORT_DIRECTORY_LEN: usize = 40;
const EXPORT_NUMBER_OF_NAMES_OFFSET: usize = 24;
const EXPORT_ADDRESS_OF_NAMES_OFFSET: usize = 32;
const MAX_EXPORT_NAMES: usize = 16_384;
const MAX_EXPORT_NAME_LEN: usize = 256;

/// Reads the exported symbol names from `bytes`.
///
/// Returns `Some(vec![])` for a valid PE without an export directory and `None`
/// for bytes that are not a parseable PE image.
pub(crate) fn export_names_from_bytes(bytes: &[u8]) -> Option<Vec<String>> {
    let headers = PeHeaders::parse(bytes)?;
    let Some((directory_rva, directory_size)) = headers.data_directory(EXPORT_DIRECTORY_INDEX)
    else {
        return Some(Vec::new());
    };
    if directory_rva == 0 || directory_size == 0 {
        return Some(Vec::new());
    }

    let directory_offset = rva_to_offset(headers.sections(), directory_rva)?;
    checked_range(bytes, directory_offset, EXPORT_DIRECTORY_LEN)?;

    let name_count = usize::try_from(read_u32(
        bytes,
        directory_offset + EXPORT_NUMBER_OF_NAMES_OFFSET,
    )?)
    .ok()?;
    if name_count > MAX_EXPORT_NAMES {
        return None;
    }
    let names_rva = read_u32(bytes, directory_offset + EXPORT_ADDRESS_OF_NAMES_OFFSET)?;
    if names_rva == 0 {
        return Some(Vec::new());
    }
    let names_offset = rva_to_offset(headers.sections(), names_rva)?;

    let mut names = Vec::with_capacity(name_count);
    for index in 0..name_count {
        let name_rva_offset = names_offset.checked_add(index.checked_mul(4)?)?;
        let name_rva = read_u32(bytes, name_rva_offset)?;
        let name_offset = rva_to_offset(headers.sections(), name_rva)?;
        if let Some(name) = read_ascii_null_terminated(bytes, name_offset) {
            names.push(name);
        }
    }

    Some(names)
}

fn read_ascii_null_terminated(bytes: &[u8], offset: usize) -> Option<String> {
    let mut name = String::new();
    for index in 0..MAX_EXPORT_NAME_LEN {
        let byte = *bytes.get(offset.checked_add(index)?)?;
        if byte == 0 {
            return (!name.is_empty()).then_some(name);
        }
        if !byte.is_ascii() {
            return None;
        }
        name.push(byte as char);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pe::header::{
        COFF_HEADER_LEN, DATA_DIRECTORY_ENTRY_LEN, DOS_PE_POINTER_OFFSET,
        PE32_PLUS_DATA_DIRECTORY_OFFSET, PE32_PLUS_MAGIC, SECTION_HEADER_LEN,
    };

    const MACHINE_AMD64: u16 = 0x8664;

    #[test]
    fn non_pe_has_no_exports_result() {
        assert_eq!(export_names_from_bytes(b"not a pe"), None);
    }

    #[test]
    fn pe_without_export_directory_returns_empty_names() {
        let pe = build_export_pe(&[]);
        assert_eq!(export_names_from_bytes(&pe), Some(Vec::new()));
    }

    #[test]
    fn reads_synthetic_export_names() {
        let pe = build_export_pe(&[
            "ReShadeVersion",
            "ReShadeRegisterAddon",
            "ReShadeUnregisterAddon",
        ]);

        let names = export_names_from_bytes(&pe).expect("parse exports");
        assert!(names.contains(&"ReShadeVersion".to_owned()));
        assert!(names.contains(&"ReShadeRegisterAddon".to_owned()));
        assert!(names.contains(&"ReShadeUnregisterAddon".to_owned()));
    }

    fn build_export_pe(export_names: &[&str]) -> Vec<u8> {
        let pe_offset: usize = 0x80;
        let optional_header_size: usize = 0xF0;
        let coff_offset = pe_offset + 4;
        let optional_header_offset = coff_offset + COFF_HEADER_LEN;
        let optional_header_end = optional_header_offset + optional_header_size;
        let section_table_offset = optional_header_end;
        let headers_end = section_table_offset + SECTION_HEADER_LEN;

        let section_rva: u32 = 0x1000;
        let section_raw_ptr = align_up(headers_end as u32, 0x200);
        let mut section_body = Vec::new();

        if !export_names.is_empty() {
            section_body.resize(EXPORT_DIRECTORY_LEN, 0);
            let names_table_offset = section_body.len();
            section_body.resize(names_table_offset + export_names.len() * 4, 0);

            let mut name_rvas = Vec::new();
            for name in export_names {
                let name_rva = section_rva + section_body.len() as u32;
                name_rvas.push(name_rva);
                section_body.extend_from_slice(name.as_bytes());
                section_body.push(0);
            }

            for (index, name_rva) in name_rvas.iter().enumerate() {
                let offset = names_table_offset + index * 4;
                section_body[offset..offset + 4].copy_from_slice(&name_rva.to_le_bytes());
            }

            section_body[EXPORT_NUMBER_OF_NAMES_OFFSET..EXPORT_NUMBER_OF_NAMES_OFFSET + 4]
                .copy_from_slice(&(export_names.len() as u32).to_le_bytes());
            section_body[EXPORT_ADDRESS_OF_NAMES_OFFSET..EXPORT_ADDRESS_OF_NAMES_OFFSET + 4]
                .copy_from_slice(&(section_rva + names_table_offset as u32).to_le_bytes());
        }

        let total_len = section_raw_ptr as usize + section_body.len().max(1);
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
        if !export_names.is_empty() {
            let data_dir_offset = optional_header_offset + PE32_PLUS_DATA_DIRECTORY_OFFSET;
            let export_entry = data_dir_offset + EXPORT_DIRECTORY_INDEX * DATA_DIRECTORY_ENTRY_LEN;
            bytes[export_entry..export_entry + 4].copy_from_slice(&section_rva.to_le_bytes());
            bytes[export_entry + 4..export_entry + 8]
                .copy_from_slice(&(section_body.len() as u32).to_le_bytes());
        }

        let section_offset = section_table_offset;
        bytes[section_offset..section_offset + 8].copy_from_slice(b".edata\0\0");
        bytes[section_offset + 8..section_offset + 12]
            .copy_from_slice(&(section_body.len().max(1) as u32).to_le_bytes());
        bytes[section_offset + 12..section_offset + 16].copy_from_slice(&section_rva.to_le_bytes());
        bytes[section_offset + 16..section_offset + 20]
            .copy_from_slice(&(section_body.len().max(1) as u32).to_le_bytes());
        bytes[section_offset + 20..section_offset + 24]
            .copy_from_slice(&section_raw_ptr.to_le_bytes());

        if !section_body.is_empty() {
            bytes[section_raw_ptr as usize..section_raw_ptr as usize + section_body.len()]
                .copy_from_slice(&section_body);
        }
        bytes
    }

    fn align_up(value: u32, alignment: u32) -> u32 {
        value.div_ceil(alignment) * alignment
    }
}
