use super::binary::{checked_range, read_u16, read_u32};

const DOS_PE_POINTER_OFFSET: usize = 0x3c;
const COFF_HEADER_LEN: usize = 20;
const SECTION_HEADER_LEN: usize = 40;
const RESOURCE_DIRECTORY_HEADER_LEN: usize = 16;
const RESOURCE_DIRECTORY_ENTRY_LEN: usize = 8;
const RESOURCE_DATA_ENTRY_LEN: usize = 16;
const DATA_DIRECTORY_ENTRY_LEN: usize = 8;
const PE32_MAGIC: u16 = 0x10b;
const PE32_PLUS_MAGIC: u16 = 0x20b;
const PE32_DATA_DIRECTORY_OFFSET: usize = 96;
const PE32_PLUS_DATA_DIRECTORY_OFFSET: usize = 112;
const RESOURCE_DIRECTORY_INDEX: usize = 2;
const RESOURCE_TYPE_VERSION: u16 = 16;
const RESOURCE_DIRECTORY_FLAG: u32 = 0x8000_0000;

#[derive(Debug, Clone)]
pub(super) struct PeResourceImage<'a> {
    bytes: &'a [u8],
    resource_offset: usize,
    resource_size: u32,
    sections: Vec<SectionHeader>,
}

impl<'a> PeResourceImage<'a> {
    pub(super) fn parse(bytes: &'a [u8]) -> Option<Self> {
        if checked_range(bytes, 0, 2)? != b"MZ" {
            return None;
        }

        let pe_offset = usize::try_from(read_u32(bytes, DOS_PE_POINTER_OFFSET)?).ok()?;

        if checked_range(bytes, pe_offset, 4)? != b"PE\0\0" {
            return None;
        }

        let coff_offset = pe_offset.checked_add(4)?;
        let section_count = usize::from(read_u16(bytes, coff_offset.checked_add(2)?)?);
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
        let resource_directory_offset = data_directories_offset
            .checked_add(RESOURCE_DIRECTORY_INDEX.checked_mul(DATA_DIRECTORY_ENTRY_LEN)?)?;

        if resource_directory_offset.checked_add(DATA_DIRECTORY_ENTRY_LEN)? > optional_header_end {
            return None;
        }

        let resource_rva = read_u32(bytes, resource_directory_offset)?;
        let resource_size = read_u32(bytes, resource_directory_offset.checked_add(4)?)?;

        if resource_rva == 0 || resource_size == 0 {
            return None;
        }

        let section_table_offset = optional_header_end;
        let mut sections = Vec::with_capacity(section_count);

        for section_index in 0..section_count {
            let section_offset =
                section_table_offset.checked_add(section_index.checked_mul(SECTION_HEADER_LEN)?)?;
            checked_range(bytes, section_offset, SECTION_HEADER_LEN)?;

            sections.push(SectionHeader {
                virtual_size: read_u32(bytes, section_offset.checked_add(8)?)?,
                virtual_address: read_u32(bytes, section_offset.checked_add(12)?)?,
                raw_data_size: read_u32(bytes, section_offset.checked_add(16)?)?,
                raw_data_pointer: read_u32(bytes, section_offset.checked_add(20)?)?,
            });
        }

        let resource_offset = rva_to_offset(&sections, resource_rva)?;

        Some(Self {
            bytes,
            resource_offset,
            resource_size,
            sections,
        })
    }

    pub(super) fn version_resource(&self) -> Option<&'a [u8]> {
        let type_directory =
            self.find_child_directory(self.resource_offset, RESOURCE_TYPE_VERSION)?;
        let name_directory = self.first_child_directory(type_directory)?;
        let data_entry_offset = self.first_data_entry(name_directory)?;
        let data_rva = read_u32(self.bytes, data_entry_offset)?;
        let data_size =
            usize::try_from(read_u32(self.bytes, data_entry_offset.checked_add(4)?)?).ok()?;
        let data_offset = rva_to_offset(&self.sections, data_rva)?;

        checked_range(self.bytes, data_entry_offset, RESOURCE_DATA_ENTRY_LEN)?;
        checked_range(self.bytes, data_offset, data_size)
    }

    fn find_child_directory(&self, directory_offset: usize, id: u16) -> Option<usize> {
        self.resource_entries(directory_offset)?
            .into_iter()
            .find(|entry| entry.id == Some(id) && entry.is_directory)
            .and_then(|entry| self.resource_relative_offset(entry.target_offset))
    }

    fn first_child_directory(&self, directory_offset: usize) -> Option<usize> {
        self.resource_entries(directory_offset)?
            .into_iter()
            .find(|entry| entry.is_directory)
            .and_then(|entry| self.resource_relative_offset(entry.target_offset))
    }

    fn first_data_entry(&self, directory_offset: usize) -> Option<usize> {
        self.resource_entries(directory_offset)?
            .into_iter()
            .find(|entry| !entry.is_directory)
            .and_then(|entry| self.resource_relative_offset(entry.target_offset))
    }

    fn resource_entries(&self, directory_offset: usize) -> Option<Vec<ResourceEntry>> {
        checked_range(self.bytes, directory_offset, RESOURCE_DIRECTORY_HEADER_LEN)?;

        let named_count = usize::from(read_u16(self.bytes, directory_offset.checked_add(12)?)?);
        let id_count = usize::from(read_u16(self.bytes, directory_offset.checked_add(14)?)?);
        let entry_count = named_count.checked_add(id_count)?;
        let entries_offset = directory_offset.checked_add(RESOURCE_DIRECTORY_HEADER_LEN)?;
        let mut entries = Vec::with_capacity(entry_count);

        for entry_index in 0..entry_count {
            let entry_offset = entries_offset
                .checked_add(entry_index.checked_mul(RESOURCE_DIRECTORY_ENTRY_LEN)?)?;
            let name = read_u32(self.bytes, entry_offset)?;
            let target = read_u32(self.bytes, entry_offset.checked_add(4)?)?;
            let is_directory = target & RESOURCE_DIRECTORY_FLAG != 0;
            let target_offset = target & !RESOURCE_DIRECTORY_FLAG;
            let id = if name & RESOURCE_DIRECTORY_FLAG == 0 {
                Some((name & 0xffff) as u16)
            } else {
                None
            };

            entries.push(ResourceEntry {
                id,
                is_directory,
                target_offset,
            });
        }

        Some(entries)
    }

    fn resource_relative_offset(&self, relative_offset: u32) -> Option<usize> {
        if relative_offset >= self.resource_size {
            return None;
        }

        self.resource_offset
            .checked_add(usize::try_from(relative_offset).ok()?)
    }
}

#[derive(Debug, Clone, Copy)]
struct SectionHeader {
    virtual_size: u32,
    virtual_address: u32,
    raw_data_size: u32,
    raw_data_pointer: u32,
}

#[derive(Debug, Clone, Copy)]
struct ResourceEntry {
    id: Option<u16>,
    is_directory: bool,
    target_offset: u32,
}

fn rva_to_offset(sections: &[SectionHeader], rva: u32) -> Option<usize> {
    for section in sections {
        let Some(offset_in_section) = rva.checked_sub(section.virtual_address) else {
            continue;
        };
        let section_size = section.virtual_size.max(section.raw_data_size);

        if offset_in_section >= section_size {
            continue;
        }

        let file_offset = section.raw_data_pointer.checked_add(offset_in_section)?;
        return usize::try_from(file_offset).ok();
    }

    None
}