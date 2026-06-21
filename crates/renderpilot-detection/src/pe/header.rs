//! Shared PE header and section-table parsing.
//!
//! Both the version-resource reader ([`super::image`]) and the graphics-API
//! detector ([`super::graphics`]) need the same first steps: validate the
//! DOS/PE/COFF headers, locate the data-directory table, and read the section
//! table for RVA→file-offset resolution. [`PeHeaders`] performs that shared walk
//! once; each consumer then reads only the data directory it cares about (the
//! resource directory or the import directory).

use super::binary::{checked_range, read_u16, read_u32};

// `pub(super)` so the synthetic-PE test builders in sibling modules can lay out
// a structurally valid image with the same offsets the parser reads.
pub(super) const DOS_PE_POINTER_OFFSET: usize = 0x3c;
pub(super) const COFF_HEADER_LEN: usize = 20;
pub(super) const SECTION_HEADER_LEN: usize = 40;
pub(super) const SECTION_VIRTUAL_ADDRESS_OFFSET: usize = 12;
pub(super) const SECTION_RAW_DATA_SIZE_OFFSET: usize = 16;
pub(super) const SECTION_RAW_DATA_POINTER_OFFSET: usize = 20;
pub(super) const DATA_DIRECTORY_ENTRY_LEN: usize = 8;
pub(super) const PE32_MAGIC: u16 = 0x10b;
pub(super) const PE32_PLUS_MAGIC: u16 = 0x20b;
pub(super) const PE32_DATA_DIRECTORY_OFFSET: usize = 96;
pub(super) const PE32_PLUS_DATA_DIRECTORY_OFFSET: usize = 112;

/// Reasonable upper bound on the number of sections in a legitimate PE image.
/// The Windows loader caps this at 96 for normal images; 256 keeps headroom for
/// drivers/EFI images while still rejecting pathological values.
pub(super) const MAX_SECTIONS: usize = 256;

/// A parsed PE section-table entry, used to map an RVA to a file offset.
#[derive(Debug, Clone, Copy)]
pub(super) struct SectionHeader {
    virtual_address: u32,
    raw_data_size: u32,
    raw_data_pointer: u32,
}

/// The shared header fields both PE consumers need: the COFF machine type, the
/// section table, and the location of the data-directory table.
pub(super) struct PeHeaders<'a> {
    bytes: &'a [u8],
    machine: u16,
    optional_header_end: usize,
    data_directories_offset: usize,
    sections: Vec<SectionHeader>,
}

impl<'a> PeHeaders<'a> {
    /// Validates the DOS/PE/COFF/optional headers and reads the section table.
    /// Returns `None` for anything that is not a well-formed PE image.
    pub(super) fn parse(bytes: &'a [u8]) -> Option<Self> {
        if checked_range(bytes, 0, 2)? != b"MZ" {
            return None;
        }

        let pe_offset = usize::try_from(read_u32(bytes, DOS_PE_POINTER_OFFSET)?).ok()?;
        if checked_range(bytes, pe_offset, 4)? != b"PE\0\0" {
            return None;
        }

        let coff_offset = pe_offset.checked_add(4)?;
        let machine = read_u16(bytes, coff_offset)?;
        let section_count = usize::from(read_u16(bytes, coff_offset.checked_add(2)?)?);
        if section_count > MAX_SECTIONS {
            return None;
        }
        let optional_header_size = usize::from(read_u16(bytes, coff_offset.checked_add(16)?)?);
        let optional_header_offset = coff_offset.checked_add(COFF_HEADER_LEN)?;
        let optional_header_end = optional_header_offset.checked_add(optional_header_size)?;
        checked_range(bytes, optional_header_offset, optional_header_size)?;

        let magic = read_u16(bytes, optional_header_offset)?;
        let data_directories_offset = match magic {
            PE32_MAGIC => optional_header_offset.checked_add(PE32_DATA_DIRECTORY_OFFSET)?,
            PE32_PLUS_MAGIC => {
                optional_header_offset.checked_add(PE32_PLUS_DATA_DIRECTORY_OFFSET)?
            }
            _ => return None,
        };

        let section_table_offset = optional_header_end;
        let mut sections = Vec::with_capacity(section_count);
        for section_index in 0..section_count {
            let section_offset =
                section_table_offset.checked_add(section_index.checked_mul(SECTION_HEADER_LEN)?)?;
            checked_range(bytes, section_offset, SECTION_HEADER_LEN)?;

            sections.push(SectionHeader {
                virtual_address: read_u32(
                    bytes,
                    section_offset.checked_add(SECTION_VIRTUAL_ADDRESS_OFFSET)?,
                )?,
                raw_data_size: read_u32(
                    bytes,
                    section_offset.checked_add(SECTION_RAW_DATA_SIZE_OFFSET)?,
                )?,
                raw_data_pointer: read_u32(
                    bytes,
                    section_offset.checked_add(SECTION_RAW_DATA_POINTER_OFFSET)?,
                )?,
            });
        }

        Some(Self {
            bytes,
            machine,
            optional_header_end,
            data_directories_offset,
            sections,
        })
    }

    /// The raw image bytes these headers were parsed from.
    pub(super) fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// The COFF machine type (used to derive the architecture).
    pub(super) fn machine(&self) -> u16 {
        self.machine
    }

    /// The parsed section table.
    pub(super) fn sections(&self) -> &[SectionHeader] {
        &self.sections
    }

    /// Reads data-directory entry `index` as `(rva, size)`, bounds-checked
    /// against the optional header. Returns `None` if the entry lies past the
    /// header (a truncated or malformed image).
    pub(super) fn data_directory(&self, index: usize) -> Option<(u32, u32)> {
        let entry_offset = self
            .data_directories_offset
            .checked_add(index.checked_mul(DATA_DIRECTORY_ENTRY_LEN)?)?;
        if entry_offset.checked_add(DATA_DIRECTORY_ENTRY_LEN)? > self.optional_header_end {
            return None;
        }
        let rva = read_u32(self.bytes, entry_offset)?;
        let size = read_u32(self.bytes, entry_offset.checked_add(4)?)?;
        Some((rva, size))
    }
}

/// Resolves a relative virtual address to a file offset using the section table.
///
/// For file-backed reads the bound is `raw_data_size`. `virtual_size` can be
/// larger than the bytes stored on disk (zero-initialized BSS), so an RVA that
/// falls in that tail has no file offset and returns `None`.
pub(super) fn rva_to_offset(sections: &[SectionHeader], rva: u32) -> Option<usize> {
    for section in sections {
        let Some(offset_in_section) = rva.checked_sub(section.virtual_address) else {
            continue;
        };
        if offset_in_section >= section.raw_data_size {
            continue;
        }
        let file_offset = section.raw_data_pointer.checked_add(offset_in_section)?;
        return usize::try_from(file_offset).ok();
    }

    None
}
