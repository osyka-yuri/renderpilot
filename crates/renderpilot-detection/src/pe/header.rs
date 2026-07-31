//! Shared PE header and section-table parsing.
//!
//! Both the version-resource reader ([`super::image`]) and the graphics-API
//! detector ([`super::graphics`]) need the same first steps: validate the
//! DOS/PE/COFF headers, locate the data-directory table, and read the section
//! table for RVA→file-offset resolution. [`PeHeaders`] performs that shared walk
//! once; each consumer then reads only the data directory it cares about (the
//! resource directory or the import directory).

use std::ops::Range;

use super::binary::{checked_range, read_u16, read_u32, read_u64};

// `pub(super)` so the synthetic-PE test builders in sibling modules can lay out
// a structurally valid image with the same offsets the parser reads.
pub(super) const DOS_PE_POINTER_OFFSET: usize = 0x3c;
pub(super) const COFF_HEADER_LEN: usize = 20;
pub(super) const SECTION_HEADER_LEN: usize = 40;
pub(super) const SECTION_VIRTUAL_ADDRESS_OFFSET: usize = 12;
pub(super) const SECTION_RAW_DATA_SIZE_OFFSET: usize = 16;
pub(super) const SECTION_RAW_DATA_POINTER_OFFSET: usize = 20;
pub(super) const SECTION_CHARACTERISTICS_OFFSET: usize = 36;
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
    characteristics: u32,
}

/// The shared header fields both PE consumers need: the COFF machine type, the
/// section table, and the location of the data-directory table.
pub(super) struct PeHeaders<'a> {
    bytes: &'a [u8],
    machine: u16,
    image_base: u64,
    optional_header_end: usize,
    data_directories_offset: usize,
    data_directory_count: usize,
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
        let (data_directories_offset, image_base) = match magic {
            PE32_MAGIC => (
                optional_header_offset.checked_add(PE32_DATA_DIRECTORY_OFFSET)?,
                u64::from(read_u32(bytes, optional_header_offset.checked_add(28)?)?),
            ),
            PE32_PLUS_MAGIC => (
                optional_header_offset.checked_add(PE32_PLUS_DATA_DIRECTORY_OFFSET)?,
                read_u64(bytes, optional_header_offset.checked_add(24)?)?,
            ),
            _ => return None,
        };
        let data_directory_count =
            usize::try_from(read_u32(bytes, data_directories_offset.checked_sub(4)?)?).ok()?;
        let data_directory_capacity =
            optional_header_end.checked_sub(data_directories_offset)? / DATA_DIRECTORY_ENTRY_LEN;
        if data_directory_count > data_directory_capacity {
            return None;
        }

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
                characteristics: read_u32(
                    bytes,
                    section_offset.checked_add(SECTION_CHARACTERISTICS_OFFSET)?,
                )?,
            });
        }

        Some(Self {
            bytes,
            machine,
            image_base,
            optional_header_end,
            data_directories_offset,
            data_directory_count,
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

    /// Preferred image base used by legacy VA-based delay-import descriptors.
    pub(super) fn image_base(&self) -> u64 {
        self.image_base
    }

    /// The parsed section table.
    pub(super) fn sections(&self) -> &[SectionHeader] {
        &self.sections
    }

    /// Reads data-directory entry `index` as `(rva, size)`, bounds-checked
    /// against both `NumberOfRvaAndSizes` and the optional header. A directory
    /// not declared by an otherwise valid image is represented as `(0, 0)`;
    /// `None` is reserved for arithmetic overflow or malformed/truncated data.
    pub(super) fn data_directory(&self, index: usize) -> Option<(u32, u32)> {
        if index >= self.data_directory_count {
            return Some((0, 0));
        }
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

/// Resolves an RVA-sized range only when it belongs unambiguously to a
/// file-backed, non-executable section. Named DATA exports must never be read
/// from code bytes.
pub(super) fn data_rva_to_offset(sections: &[SectionHeader], rva: u32, size: u32) -> Option<usize> {
    const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;
    unique_section_mapping(sections, rva, size, |section| {
        section.characteristics & IMAGE_SCN_MEM_EXECUTE == 0
    })
    .map(|mapping| mapping.file_offset)
}

/// Resolves a relative virtual address to a file offset using the section table.
///
/// For file-backed reads the bound is `raw_data_size`. `virtual_size` can be
/// larger than the bytes stored on disk (zero-initialized BSS), so an RVA that
/// falls in that tail has no file offset and returns `None`.
pub(super) fn rva_to_offset(sections: &[SectionHeader], rva: u32) -> Option<usize> {
    rva_range_to_offset(sections, rva, 1)
}

/// Resolves a bounded file range starting at `rva` without crossing the raw
/// boundary of its unique section.
///
/// This is the correct primitive for NUL-terminated PE strings: mapping only
/// the first byte and then reading through the remainder of the file could
/// otherwise accept a string that continues into another section or overlay.
/// Partially overlapping section mappings are rejected as malformed.
pub(super) fn rva_bounded_file_range(
    sections: &[SectionHeader],
    rva: u32,
    max_size: usize,
) -> Option<Range<usize>> {
    let requested_size = u32::try_from(max_size).ok()?;
    let point = unique_section_mapping(sections, rva, 1, |_| true)?;
    let size = requested_size.min(point.available_bytes);
    let mapping = unique_section_mapping(sections, rva, size, |_| true)?;
    let start = mapping.file_offset;
    let end = start.checked_add(usize::try_from(size).ok()?)?;
    Some(start..end)
}

/// Resolves an entire file-backed RVA range and rejects overlapping section
/// mappings. Ambiguous section tables are malformed and must never be resolved
/// by whichever header happens to appear first.
pub(super) fn rva_range_to_offset(
    sections: &[SectionHeader],
    rva: u32,
    size: u32,
) -> Option<usize> {
    unique_section_mapping(sections, rva, size, |_| true).map(|mapping| mapping.file_offset)
}

struct SectionMapping {
    file_offset: usize,
    available_bytes: u32,
}

fn unique_section_mapping(
    sections: &[SectionHeader],
    rva: u32,
    size: u32,
    accepts: impl Fn(&SectionHeader) -> bool,
) -> Option<SectionMapping> {
    if size == 0 {
        return None;
    }
    let requested_end = rva.checked_add(size)?;
    let mut resolved = None;
    for section in sections {
        if section.raw_data_size == 0 {
            continue;
        }
        let section_end = section.virtual_address.checked_add(section.raw_data_size)?;
        if rva >= section_end || section.virtual_address >= requested_end {
            continue;
        }

        // Any partial overlap makes the mapping ambiguous or crosses a raw
        // section boundary. Both are malformed for a file-backed PE read.
        if rva < section.virtual_address || requested_end > section_end {
            return None;
        }
        if !accepts(section) || resolved.is_some() {
            return None;
        }
        let offset_in_section = rva.checked_sub(section.virtual_address)?;
        let file_offset =
            usize::try_from(section.raw_data_pointer.checked_add(offset_in_section)?).ok()?;
        resolved = Some(SectionMapping {
            file_offset,
            available_bytes: section.raw_data_size.checked_sub(offset_in_section)?,
        });
    }
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undeclared_data_directory_is_structurally_absent() {
        let bytes = build_header_only_pe(2);
        let headers = PeHeaders::parse(&bytes).expect("valid PE headers");

        assert_eq!(headers.data_directory(1), Some((0, 0)));
        assert_eq!(headers.data_directory(13), Some((0, 0)));
    }

    #[test]
    fn declared_directory_count_must_fit_optional_header() {
        let bytes = build_header_only_pe(17);
        assert!(PeHeaders::parse(&bytes).is_none());
    }

    #[test]
    fn rva_range_rejects_partial_overlap_with_another_raw_section() {
        let sections = [
            test_section(0x1000, 0x100, 0x200),
            test_section(0x1080, 0x100, 0x400),
        ];

        assert_eq!(rva_range_to_offset(&sections, 0x1010, 0x20), Some(0x210));
        assert_eq!(rva_range_to_offset(&sections, 0x1040, 0x80), None);
    }

    #[test]
    fn bounded_rva_range_rejects_overlapping_raw_sections() {
        let sections = [
            test_section(0x1000, 0x100, 0x200),
            test_section(0x1080, 0x100, 0x400),
        ];

        assert_eq!(
            rva_bounded_file_range(&sections, 0x1010, 0x20),
            Some(0x210..0x230)
        );
        assert_eq!(rva_bounded_file_range(&sections, 0x1040, 0x80), None);
    }

    fn test_section(
        virtual_address: u32,
        raw_data_size: u32,
        raw_data_pointer: u32,
    ) -> SectionHeader {
        SectionHeader {
            virtual_address,
            raw_data_size,
            raw_data_pointer,
            characteristics: 0,
        }
    }

    fn build_header_only_pe(data_directory_count: u32) -> Vec<u8> {
        let pe_offset = 0x80usize;
        let coff_offset = pe_offset + 4;
        let optional_header_offset = coff_offset + COFF_HEADER_LEN;
        let optional_header_size = 0xf0usize;
        let mut bytes = vec![0u8; optional_header_offset + optional_header_size];

        bytes[0..2].copy_from_slice(b"MZ");
        bytes[DOS_PE_POINTER_OFFSET..DOS_PE_POINTER_OFFSET + 4]
            .copy_from_slice(&(pe_offset as u32).to_le_bytes());
        bytes[pe_offset..pe_offset + 4].copy_from_slice(b"PE\0\0");
        bytes[coff_offset..coff_offset + 2].copy_from_slice(&0x8664u16.to_le_bytes());
        bytes[coff_offset + 16..coff_offset + 18]
            .copy_from_slice(&(optional_header_size as u16).to_le_bytes());
        bytes[optional_header_offset..optional_header_offset + 2]
            .copy_from_slice(&PE32_PLUS_MAGIC.to_le_bytes());
        let count_offset = optional_header_offset + PE32_PLUS_DATA_DIRECTORY_OFFSET - 4;
        bytes[count_offset..count_offset + 4].copy_from_slice(&data_directory_count.to_le_bytes());
        bytes
    }
}
