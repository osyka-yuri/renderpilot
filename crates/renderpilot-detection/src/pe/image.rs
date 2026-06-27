//! Version-resource location in a PE image.
//!
//! The shared DOS/PE/COFF header and section-table walk lives in
//! [`super::header`]; this module reads only the resource data directory
//! (index 2) and traverses its directory tree to the `VS_VERSION_INFO` blob.

use super::binary::{checked_range, read_u16, read_u32};
use super::header::{PeHeaders, rva_to_offset};

const RESOURCE_DIRECTORY_HEADER_LEN: usize = 16;
const RESOURCE_DIRECTORY_NAMED_ENTRIES_OFFSET: usize = 12;
const RESOURCE_DIRECTORY_ID_ENTRIES_OFFSET: usize = 14;
const RESOURCE_DIRECTORY_ENTRY_LEN: usize = 8;
const RESOURCE_DATA_ENTRY_LEN: usize = 16;
const RESOURCE_DIRECTORY_INDEX: usize = 2;
const RESOURCE_TYPE_VERSION: u16 = 16;
const RESOURCE_DIRECTORY_FLAG: u32 = 0x8000_0000;

/// Cap on the total number of entries in any resource directory node. Real
/// executables have a small handful; this prevents malformed/crafted binaries
/// from forcing huge allocations or long loops.
const MAX_RESOURCE_DIRECTORY_ENTRIES: usize = 8192;

pub(super) struct PeResourceImage<'a> {
    headers: PeHeaders<'a>,
    resource_offset: usize,
    resource_size: u32,
}

impl<'a> PeResourceImage<'a> {
    pub(super) fn parse(bytes: &'a [u8]) -> Option<Self> {
        let headers = PeHeaders::parse(bytes)?;
        let (resource_rva, resource_size) = headers.data_directory(RESOURCE_DIRECTORY_INDEX)?;
        if resource_rva == 0 || resource_size == 0 {
            return None;
        }
        let resource_offset = rva_to_offset(headers.sections(), resource_rva)?;

        Some(Self {
            headers,
            resource_offset,
            resource_size,
        })
    }

    pub(super) fn version_resource(&self) -> Option<&'a [u8]> {
        // Find the first VS_VERSION_INFO data entry. PEs may legally contain
        // multiple version resources; "first wins" matches the Windows loader
        // behavior for the common case and keeps the API deterministic.
        let type_directory =
            self.find_child_directory(self.resource_offset, RESOURCE_TYPE_VERSION)?;
        let name_directory = self.first_child_directory(type_directory)?;
        let data_entry_offset = self.first_data_entry(name_directory)?;

        // Validate the data entry itself before reading its fields.
        checked_range(self.bytes(), data_entry_offset, RESOURCE_DATA_ENTRY_LEN)?;
        let data_rva = read_u32(self.bytes(), data_entry_offset)?;
        let data_size =
            usize::try_from(read_u32(self.bytes(), data_entry_offset.checked_add(4)?)?).ok()?;

        if data_size == 0 || u32::try_from(data_size).ok()? > self.resource_size {
            return None;
        }

        let data_offset = rva_to_offset(self.headers.sections(), data_rva)?;
        checked_range(self.bytes(), data_offset, data_size)
    }

    fn bytes(&self) -> &'a [u8] {
        self.headers.bytes()
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
        let bytes = self.bytes();
        checked_range(bytes, directory_offset, RESOURCE_DIRECTORY_HEADER_LEN)?;

        let named_count = usize::from(read_u16(
            bytes,
            directory_offset.checked_add(RESOURCE_DIRECTORY_NAMED_ENTRIES_OFFSET)?,
        )?);
        let id_count = usize::from(read_u16(
            bytes,
            directory_offset.checked_add(RESOURCE_DIRECTORY_ID_ENTRIES_OFFSET)?,
        )?);
        let entry_count = named_count.checked_add(id_count)?;
        if entry_count > MAX_RESOURCE_DIRECTORY_ENTRIES {
            return None;
        }
        let entries_offset = directory_offset.checked_add(RESOURCE_DIRECTORY_HEADER_LEN)?;
        let mut entries = Vec::with_capacity(entry_count);

        for entry_index in 0..entry_count {
            let entry_offset = entries_offset
                .checked_add(entry_index.checked_mul(RESOURCE_DIRECTORY_ENTRY_LEN)?)?;
            let name = read_u32(bytes, entry_offset)?;
            let target = read_u32(bytes, entry_offset.checked_add(4)?)?;
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
struct ResourceEntry {
    id: Option<u16>,
    is_directory: bool,
    target_offset: u32,
}
