//! Shared test fixtures for the addon subsystem: synthetic PE images and zip
//! archives used by the ReShade fetch tests and each tool's install/fetch tests.

use std::io::{Cursor, Write};

use crate::addons::reshade::types::{ReshadeNightly, ReshadeSourceCatalog, ReshadeStable};

/// Shared ReShade host source catalogue fixture (stable + nightly URLs).
///
/// Independent of any tool catalogue — both RenoDX and Luma tests re-export
/// this so fixtures stay byte-identical across tools.
#[must_use]
pub(crate) fn reshade_sources() -> ReshadeSourceCatalog {
    ReshadeSourceCatalog {
        stable: Some(ReshadeStable {
            url: "https://reshade.me/downloads/ReShade_Setup_6.7.3_Addon.exe".to_owned(),
        }),
        nightly: ReshadeNightly {
            url64:
                "https://nightly.link/crosire/reshade/workflows/build/main/ReShade%20(64-bit).zip"
                    .to_owned(),
            url32:
                "https://nightly.link/crosire/reshade/workflows/build/main/ReShade%20(32-bit).zip"
                    .to_owned(),
        },
    }
}

/// COFF machine type for 32-bit x86.
pub(crate) const MACHINE_I386: u16 = 0x014c;
/// COFF machine type for 64-bit x86-64.
pub(crate) const MACHINE_AMD64: u16 = 0x8664;
/// Optional-header magic for a PE32 (32-bit) image.
pub(crate) const PE32_MAGIC: u16 = 0x10b;
/// Optional-header magic for a PE32+ (64-bit) image.
pub(crate) const PE32_PLUS_MAGIC: u16 = 0x20b;

const DOS_PE_POINTER_OFFSET: usize = 0x3c;
const COFF_HEADER_LEN: usize = 20;
const SECTION_HEADER_LEN: usize = 40;
const DATA_DIRECTORY_ENTRY_LEN: usize = 8;
const PE32_DATA_DIRECTORY_OFFSET: usize = 96;
const PE32_PLUS_DATA_DIRECTORY_OFFSET: usize = 112;
const EXPORT_DIRECTORY_INDEX: usize = 0;
const EXPORT_DIRECTORY_LEN: usize = 40;
const EXPORT_NUMBER_OF_FUNCTIONS_OFFSET: usize = 20;
const EXPORT_NUMBER_OF_NAMES_OFFSET: usize = 24;
const EXPORT_ADDRESS_OF_FUNCTIONS_OFFSET: usize = 28;
const EXPORT_ADDRESS_OF_NAMES_OFFSET: usize = 32;
const EXPORT_ADDRESS_OF_NAME_ORDINALS_OFFSET: usize = 36;
const RESOURCE_DIRECTORY_INDEX: usize = 2;

/// Builds a minimal but well-formed PE image for tests: the given COFF `machine`
/// and optional-header `magic`, plus a `.edata` export name table when `exports`
/// is non-empty (otherwise a header-only image with no export directory). The one
/// home for the PE-layout offsets the install and fetch unit tests share.
pub(crate) fn build_pe_with_exports(machine: u16, magic: u16, exports: &[&str]) -> Vec<u8> {
    let pe_offset: usize = 0x80;
    let optional_header_size: usize = 0xF0;
    let coff_offset = pe_offset + 4;
    let optional_header_offset = coff_offset + COFF_HEADER_LEN;
    let optional_header_end = optional_header_offset + optional_header_size;

    let write_headers = |bytes: &mut [u8], section_count: u16| {
        bytes[0..2].copy_from_slice(b"MZ");
        bytes[DOS_PE_POINTER_OFFSET..DOS_PE_POINTER_OFFSET + 4]
            .copy_from_slice(&(pe_offset as u32).to_le_bytes());
        bytes[pe_offset..pe_offset + 4].copy_from_slice(b"PE\0\0");
        bytes[coff_offset..coff_offset + 2].copy_from_slice(&machine.to_le_bytes());
        bytes[coff_offset + 2..coff_offset + 4].copy_from_slice(&section_count.to_le_bytes());
        bytes[coff_offset + 16..coff_offset + 18]
            .copy_from_slice(&(optional_header_size as u16).to_le_bytes());
        bytes[optional_header_offset..optional_header_offset + 2]
            .copy_from_slice(&magic.to_le_bytes());
    };

    // No exports: a header-only image is enough for MZ + architecture checks.
    if exports.is_empty() {
        let mut bytes = vec![0u8; optional_header_end];
        write_headers(&mut bytes, 0);
        return bytes;
    }

    // With exports: append a single `.edata` section carrying the name table.
    let section_table_offset = optional_header_end;
    let headers_end = section_table_offset + SECTION_HEADER_LEN;
    let section_rva: u32 = 0x1000;
    let section_raw_ptr = align_up(headers_end as u32, 0x200);

    let mut section_body = vec![0u8; EXPORT_DIRECTORY_LEN];
    let functions_table_offset = section_body.len();
    section_body.resize(functions_table_offset + exports.len() * 4, 0);
    let names_table_offset = section_body.len();
    section_body.resize(names_table_offset + exports.len() * 4, 0);
    let ordinals_table_offset = section_body.len();
    section_body.resize(ordinals_table_offset + exports.len() * 2, 0);

    let mut name_rvas = Vec::new();
    for name in exports {
        let name_rva = section_rva + section_body.len() as u32;
        name_rvas.push(name_rva);
        section_body.extend_from_slice(name.as_bytes());
        section_body.push(0);
    }
    for (index, name_rva) in name_rvas.iter().enumerate() {
        let offset = names_table_offset + index * 4;
        section_body[offset..offset + 4].copy_from_slice(&name_rva.to_le_bytes());
    }
    let function_stub_rva = section_rva + section_body.len() as u32;
    section_body.push(0xc3);
    for index in 0..exports.len() {
        let function_offset = functions_table_offset + index * 4;
        section_body[function_offset..function_offset + 4]
            .copy_from_slice(&function_stub_rva.to_le_bytes());
        let ordinal_offset = ordinals_table_offset + index * 2;
        section_body[ordinal_offset..ordinal_offset + 2]
            .copy_from_slice(&(index as u16).to_le_bytes());
    }
    section_body[EXPORT_NUMBER_OF_FUNCTIONS_OFFSET..EXPORT_NUMBER_OF_FUNCTIONS_OFFSET + 4]
        .copy_from_slice(&(exports.len() as u32).to_le_bytes());
    section_body[EXPORT_NUMBER_OF_NAMES_OFFSET..EXPORT_NUMBER_OF_NAMES_OFFSET + 4]
        .copy_from_slice(&(exports.len() as u32).to_le_bytes());
    section_body[EXPORT_ADDRESS_OF_FUNCTIONS_OFFSET..EXPORT_ADDRESS_OF_FUNCTIONS_OFFSET + 4]
        .copy_from_slice(&(section_rva + functions_table_offset as u32).to_le_bytes());
    section_body[EXPORT_ADDRESS_OF_NAMES_OFFSET..EXPORT_ADDRESS_OF_NAMES_OFFSET + 4]
        .copy_from_slice(&(section_rva + names_table_offset as u32).to_le_bytes());
    section_body
        [EXPORT_ADDRESS_OF_NAME_ORDINALS_OFFSET..EXPORT_ADDRESS_OF_NAME_ORDINALS_OFFSET + 4]
        .copy_from_slice(&(section_rva + ordinals_table_offset as u32).to_le_bytes());

    let total_len = section_raw_ptr as usize + section_body.len();
    let mut bytes = vec![0u8; total_len];
    write_headers(&mut bytes, 1);

    let data_directory_offset = match magic {
        PE32_PLUS_MAGIC => PE32_PLUS_DATA_DIRECTORY_OFFSET,
        _ => PE32_DATA_DIRECTORY_OFFSET,
    };
    let export_entry = optional_header_offset
        + data_directory_offset
        + EXPORT_DIRECTORY_INDEX * DATA_DIRECTORY_ENTRY_LEN;
    bytes[export_entry..export_entry + 4].copy_from_slice(&section_rva.to_le_bytes());
    bytes[export_entry + 4..export_entry + 8]
        .copy_from_slice(&(EXPORT_DIRECTORY_LEN as u32).to_le_bytes());

    bytes[section_table_offset..section_table_offset + 8].copy_from_slice(b".edata\0\0");
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

/// Builds a minimal PE with a real `VS_VERSION_INFO` resource proving NVIDIA
/// DLSS identity. This exercises the production resource parser in policy tests.
pub(crate) fn build_nvidia_dlss_pe(version: [u16; 4]) -> Vec<u8> {
    let pe_offset: usize = 0x80;
    let optional_header_size: usize = 0xF0;
    let coff_offset = pe_offset + 4;
    let optional_header_offset = coff_offset + COFF_HEADER_LEN;
    let optional_header_end = optional_header_offset + optional_header_size;
    let section_table_offset = optional_header_end;
    let headers_end = section_table_offset + SECTION_HEADER_LEN;
    let section_rva: u32 = 0x1000;
    let section_raw_ptr = align_up(headers_end as u32, 0x200);

    let version_blob = version_info_blob(version);
    let data_offset = 88usize;
    let mut section_body = vec![0u8; data_offset];
    section_body.extend_from_slice(&version_blob);

    // root -> RT_VERSION(16) directory -> name directory -> language data entry
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

fn version_info_blob(version: [u16; 4]) -> Vec<u8> {
    let mut children = Vec::new();
    for (key, value) in [
        ("ProductName", "NVIDIA DLSS".to_owned()),
        ("FileDescription", "NVIDIA DLSS".to_owned()),
        ("OriginalFilename", "nvngx_dlss.dll".to_owned()),
        ("CompanyName", "NVIDIA Corporation".to_owned()),
        (
            "FileVersion",
            format!(
                "{}.{}.{}.{}",
                version[0], version[1], version[2], version[3]
            ),
        ),
    ] {
        children.extend_from_slice(&version_string_block(key, &value));
    }

    let mut blob = vec![0u8; 6];
    push_utf16_nul(&mut blob, "VS_VERSION_INFO");
    pad4(&mut blob);
    let value_offset = blob.len();
    blob.resize(value_offset + 52, 0);
    blob[value_offset..value_offset + 4].copy_from_slice(&0xfeef_04bdu32.to_le_bytes());
    let version_ms = (u32::from(version[0]) << 16) | u32::from(version[1]);
    let version_ls = (u32::from(version[2]) << 16) | u32::from(version[3]);
    blob[value_offset + 8..value_offset + 12].copy_from_slice(&version_ms.to_le_bytes());
    blob[value_offset + 12..value_offset + 16].copy_from_slice(&version_ls.to_le_bytes());
    pad4(&mut blob);
    blob.extend_from_slice(&children);
    let length = u16::try_from(blob.len()).expect("test version blob fits u16");
    blob[0..2].copy_from_slice(&length.to_le_bytes());
    blob[2..4].copy_from_slice(&52u16.to_le_bytes());
    blob
}

fn version_string_block(key: &str, value: &str) -> Vec<u8> {
    let mut block = vec![0u8; 6];
    push_utf16_nul(&mut block, key);
    pad4(&mut block);
    let mut value_utf16: Vec<u16> = value.encode_utf16().collect();
    value_utf16.push(0);
    for unit in &value_utf16 {
        block.extend_from_slice(&unit.to_le_bytes());
    }
    pad4(&mut block);
    let length = u16::try_from(block.len()).expect("test version string fits u16");
    block[0..2].copy_from_slice(&length.to_le_bytes());
    block[2..4].copy_from_slice(
        &u16::try_from(value_utf16.len())
            .expect("value fits")
            .to_le_bytes(),
    );
    block[4..6].copy_from_slice(&1u16.to_le_bytes());
    block
}

fn push_utf16_nul(bytes: &mut Vec<u8>, value: &str) {
    for unit in value.encode_utf16().chain(std::iter::once(0)) {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
}

fn pad4(bytes: &mut Vec<u8>) {
    while !bytes.len().is_multiple_of(4) {
        bytes.push(0);
    }
}

fn align_up(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}

/// Builds a zip archive from `(path, bytes)` entries, for extraction tests.
pub(crate) fn zip_with_entries(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    for (name, bytes) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .expect("start file");
        zip.write_all(bytes).expect("write entry");
    }
    zip.finish().expect("finish zip").into_inner()
}
