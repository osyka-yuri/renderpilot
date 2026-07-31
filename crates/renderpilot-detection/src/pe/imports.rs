//! Strict, bounded PE import-directory inspection.

use std::{error::Error, fmt};

use renderpilot_domain::{PeImportProfile, PeImportSet};

use super::{
    binary::{checked_range, read_u32},
    header::{PeHeaders, rva_bounded_file_range, rva_range_to_offset},
};

const MAX_DIRECTORY_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
enum ImportDirectoryKind {
    Regular,
    Delay,
}

impl ImportDirectoryKind {
    const fn directory_index(self) -> usize {
        match self {
            Self::Regular => 1,
            Self::Delay => 13,
        }
    }

    const fn descriptor_len(self) -> usize {
        match self {
            Self::Regular => 20,
            Self::Delay => 32,
        }
    }

    const fn name_offset(self) -> usize {
        match self {
            Self::Regular => 12,
            Self::Delay => 4,
        }
    }

    fn name_rva(self, descriptor: &[u8], headers: &PeHeaders<'_>) -> Result<u32, PeImportError> {
        let raw_name = read_u32(descriptor, self.name_offset()).ok_or(PeImportError)?;
        if raw_name == 0 {
            return Err(PeImportError);
        }
        if matches!(self, Self::Regular) {
            return Ok(raw_name);
        }

        let attributes = read_u32(descriptor, 0).ok_or(PeImportError)?;
        if attributes & !1 != 0 {
            return Err(PeImportError);
        }
        if attributes & 1 == 1 {
            return Ok(raw_name);
        }
        u32::try_from(
            u64::from(raw_name)
                .checked_sub(headers.image_base())
                .ok_or(PeImportError)?,
        )
        .map_err(|_| PeImportError)
    }
}

/// A malformed or truncated PE import table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeImportError;

impl fmt::Display for PeImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("malformed or unsafe PE import directory")
    }
}

impl Error for PeImportError {}

pub(super) fn profile_from_bytes(bytes: &[u8]) -> Option<Result<PeImportProfile, PeImportError>> {
    let headers = PeHeaders::parse(bytes)?;
    Some(profile(&headers, bytes))
}

fn profile(headers: &PeHeaders<'_>, bytes: &[u8]) -> Result<PeImportProfile, PeImportError> {
    let regular = read_directory(headers, bytes, ImportDirectoryKind::Regular)?;
    let delay = read_directory(headers, bytes, ImportDirectoryKind::Delay)?;
    Ok(PeImportProfile { regular, delay })
}

fn read_directory(
    headers: &PeHeaders<'_>,
    bytes: &[u8],
    kind: ImportDirectoryKind,
) -> Result<PeImportSet, PeImportError> {
    let (directory_rva, directory_size) = headers
        .data_directory(kind.directory_index())
        .ok_or(PeImportError)?;
    if directory_rva == 0 && directory_size == 0 {
        return PeImportSet::from_canonical_names(Vec::new()).map_err(|_| PeImportError);
    }
    let directory_size = usize::try_from(directory_size).map_err(|_| PeImportError)?;
    if directory_rva == 0
        || directory_size < kind.descriptor_len()
        || directory_size > MAX_DIRECTORY_BYTES
    {
        return Err(PeImportError);
    }
    let directory_offset = rva_range_to_offset(
        headers.sections(),
        directory_rva,
        u32::try_from(directory_size).map_err(|_| PeImportError)?,
    )
    .ok_or(PeImportError)?;
    let directory = checked_range(bytes, directory_offset, directory_size).ok_or(PeImportError)?;

    let mut names = Vec::new();
    let mut terminated = false;
    for descriptor in directory
        .chunks_exact(kind.descriptor_len())
        .take(PeImportSet::MAX_NAMES.saturating_add(1))
    {
        if descriptor.iter().all(|byte| *byte == 0) {
            terminated = true;
            break;
        }
        if names.len() == PeImportSet::MAX_NAMES {
            return Err(PeImportError);
        }

        let name_rva = kind.name_rva(descriptor, headers)?;
        names.push(read_dll_name(headers, bytes, name_rva)?);
    }
    if !terminated {
        return Err(PeImportError);
    }
    PeImportSet::from_observed_names(names).map_err(|_| PeImportError)
}

fn read_dll_name(
    headers: &PeHeaders<'_>,
    bytes: &[u8],
    name_rva: u32,
) -> Result<String, PeImportError> {
    let range = rva_bounded_file_range(
        headers.sections(),
        name_rva,
        PeImportSet::MAX_NAME_BYTES.saturating_add(1),
    )
    .ok_or(PeImportError)?;
    let bounded = bytes.get(range).ok_or(PeImportError)?;
    let nul = bounded
        .iter()
        .position(|byte| *byte == 0)
        .ok_or(PeImportError)?;
    if nul == 0 || nul > PeImportSet::MAX_NAME_BYTES {
        return Err(PeImportError);
    }
    let raw = &bounded[..nul];
    if !raw.is_ascii() {
        return Err(PeImportError);
    }
    String::from_utf8(raw.to_vec()).map_err(|_| PeImportError)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pe::header::{
        COFF_HEADER_LEN, DOS_PE_POINTER_OFFSET, PE32_PLUS_DATA_DIRECTORY_OFFSET, PE32_PLUS_MAGIC,
        SECTION_HEADER_LEN, SECTION_RAW_DATA_POINTER_OFFSET, SECTION_RAW_DATA_SIZE_OFFSET,
        SECTION_VIRTUAL_ADDRESS_OFFSET,
    };

    const RAW_DATA_POINTER: usize = 0x200;
    const SECTION_RVA: u32 = 0x1000;

    #[test]
    fn import_name_must_terminate_inside_its_file_backed_section() {
        let mut pe = pe_with_one_section(5, 8);
        pe[RAW_DATA_POINTER..RAW_DATA_POINTER + 5].copy_from_slice(b"x.dll");
        pe[RAW_DATA_POINTER + 5] = 0;
        let headers = PeHeaders::parse(&pe).expect("headers");

        assert_eq!(
            read_dll_name(&headers, &pe, SECTION_RVA),
            Err(PeImportError),
            "an overlay NUL must not complete a section-truncated import name"
        );
    }

    #[test]
    fn import_name_inside_one_section_is_accepted() {
        let mut pe = pe_with_one_section(6, 6);
        pe[RAW_DATA_POINTER..RAW_DATA_POINTER + 6].copy_from_slice(b"x.dll\0");
        let headers = PeHeaders::parse(&pe).expect("headers");

        assert_eq!(
            read_dll_name(&headers, &pe, SECTION_RVA),
            Ok("x.dll".to_owned())
        );
    }

    fn pe_with_one_section(raw_size: u32, trailing_bytes: usize) -> Vec<u8> {
        let pe_offset = 0x80usize;
        let coff_offset = pe_offset + 4;
        let optional_header_offset = coff_offset + COFF_HEADER_LEN;
        let optional_header_size = 0xf0usize;
        let section_offset = optional_header_offset + optional_header_size;
        let file_len =
            RAW_DATA_POINTER + usize::try_from(raw_size).expect("raw size") + trailing_bytes;
        let mut bytes = vec![0u8; file_len.max(section_offset + SECTION_HEADER_LEN)];

        bytes[0..2].copy_from_slice(b"MZ");
        bytes[DOS_PE_POINTER_OFFSET..DOS_PE_POINTER_OFFSET + 4]
            .copy_from_slice(&(pe_offset as u32).to_le_bytes());
        bytes[pe_offset..pe_offset + 4].copy_from_slice(b"PE\0\0");
        bytes[coff_offset..coff_offset + 2].copy_from_slice(&0x8664u16.to_le_bytes());
        bytes[coff_offset + 2..coff_offset + 4].copy_from_slice(&1u16.to_le_bytes());
        bytes[coff_offset + 16..coff_offset + 18]
            .copy_from_slice(&(optional_header_size as u16).to_le_bytes());
        bytes[optional_header_offset..optional_header_offset + 2]
            .copy_from_slice(&PE32_PLUS_MAGIC.to_le_bytes());
        let directory_count_offset = optional_header_offset + PE32_PLUS_DATA_DIRECTORY_OFFSET - 4;
        bytes[directory_count_offset..directory_count_offset + 4]
            .copy_from_slice(&0u32.to_le_bytes());
        bytes[section_offset + SECTION_VIRTUAL_ADDRESS_OFFSET
            ..section_offset + SECTION_VIRTUAL_ADDRESS_OFFSET + 4]
            .copy_from_slice(&SECTION_RVA.to_le_bytes());
        bytes[section_offset + SECTION_RAW_DATA_SIZE_OFFSET
            ..section_offset + SECTION_RAW_DATA_SIZE_OFFSET + 4]
            .copy_from_slice(&raw_size.to_le_bytes());
        bytes[section_offset + SECTION_RAW_DATA_POINTER_OFFSET
            ..section_offset + SECTION_RAW_DATA_POINTER_OFFSET + 4]
            .copy_from_slice(&(RAW_DATA_POINTER as u32).to_le_bytes());
        bytes
    }
}
