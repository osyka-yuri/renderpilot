//! Export-table inspection for PE images.
//!
//! ReShade exposes stable symbols such as `ReShadeVersion` from the host DLL.
//! Reading the export name table gives the orchestration layer a much stronger
//! identity signal than file names or neighbouring config files.

use super::binary::{checked_range, read_u16, read_u32};
use super::header::{PeHeaders, data_rva_to_offset, rva_range_to_offset};
use renderpilot_domain::PeExportSet;

const EXPORT_DIRECTORY_INDEX: usize = 0;
const EXPORT_DIRECTORY_LEN: usize = 40;
const EXPORT_NUMBER_OF_NAMES_OFFSET: usize = 24;
const EXPORT_NUMBER_OF_FUNCTIONS_OFFSET: usize = 20;
const EXPORT_ADDRESS_OF_FUNCTIONS_OFFSET: usize = 28;
const EXPORT_ADDRESS_OF_NAMES_OFFSET: usize = 32;
const EXPORT_ADDRESS_OF_NAME_ORDINALS_OFFSET: usize = 36;

/// Bounds-checked view over one PE export directory.
///
/// Header parsing and directory validation live here so name enumeration and
/// typed DATA-export lookup cannot drift apart.
struct ExportTable<'a> {
    bytes: &'a [u8],
    headers: PeHeaders<'a>,
    directory_rva: u32,
    directory_size: u32,
    directory_offset: usize,
    function_count: usize,
    name_count: usize,
}

impl<'a> ExportTable<'a> {
    /// Returns `Some(None)` for a valid image without exports.
    fn parse(bytes: &'a [u8]) -> Option<Option<Self>> {
        let headers = PeHeaders::parse(bytes)?;
        let Some((directory_rva, directory_size)) = headers.data_directory(EXPORT_DIRECTORY_INDEX)
        else {
            return Some(None);
        };
        if directory_rva == 0 && directory_size == 0 {
            return Some(None);
        }
        if directory_rva == 0 || directory_size < EXPORT_DIRECTORY_LEN as u32 {
            return None;
        }

        let directory_offset = rva_range_to_offset(
            headers.sections(),
            directory_rva,
            EXPORT_DIRECTORY_LEN as u32,
        )?;
        checked_range(bytes, directory_offset, EXPORT_DIRECTORY_LEN)?;

        let function_count =
            Self::read_count(bytes, directory_offset, EXPORT_NUMBER_OF_FUNCTIONS_OFFSET)?;
        let name_count = Self::read_count(bytes, directory_offset, EXPORT_NUMBER_OF_NAMES_OFFSET)?;
        if function_count > PeExportSet::MAX_NAMES || name_count > PeExportSet::MAX_NAMES {
            return None;
        }

        Some(Some(Self {
            bytes,
            headers,
            directory_rva,
            directory_size,
            directory_offset,
            function_count,
            name_count,
        }))
    }

    fn read_count(bytes: &[u8], directory_offset: usize, field: usize) -> Option<usize> {
        usize::try_from(read_u32(bytes, directory_offset.checked_add(field)?)?).ok()
    }

    fn array_offset(&self, field: usize, count: usize, element_size: usize) -> Option<usize> {
        if count == 0 {
            return Some(0);
        }
        let rva = read_u32(self.bytes, self.directory_offset.checked_add(field)?)?;
        if rva == 0 {
            return None;
        }
        let size = u32::try_from(count.checked_mul(element_size)?).ok()?;
        rva_range_to_offset(self.headers.sections(), rva, size)
    }

    fn named_exports(&self) -> Option<Vec<(usize, String)>> {
        let names_offset = self.array_offset(EXPORT_ADDRESS_OF_NAMES_OFFSET, self.name_count, 4)?;
        let ordinals_offset =
            self.array_offset(EXPORT_ADDRESS_OF_NAME_ORDINALS_OFFSET, self.name_count, 2)?;

        let mut names = Vec::with_capacity(self.name_count);
        for index in 0..self.name_count {
            let name_rva = read_u32(self.bytes, names_offset.checked_add(index.checked_mul(4)?)?)?;
            let name =
                read_ascii_null_terminated_rva(self.bytes, self.headers.sections(), name_rva)?;
            let ordinal = usize::from(read_u16(
                self.bytes,
                ordinals_offset.checked_add(index.checked_mul(2)?)?,
            )?);
            if ordinal >= self.function_count
                || !self.valid_function_target(self.function_rva(ordinal)?)?
            {
                return None;
            }
            names.push((ordinal, name));
        }
        Some(names)
    }

    fn function_rva(&self, ordinal: usize) -> Option<u32> {
        if ordinal >= self.function_count {
            return None;
        }
        let functions_offset =
            self.array_offset(EXPORT_ADDRESS_OF_FUNCTIONS_OFFSET, self.function_count, 4)?;
        read_u32(
            self.bytes,
            functions_offset.checked_add(ordinal.checked_mul(4)?)?,
        )
    }

    fn is_forwarder(&self, rva: u32) -> Option<bool> {
        let end = self.directory_rva.checked_add(self.directory_size)?;
        Some((self.directory_rva..end).contains(&rva))
    }

    fn valid_function_target(&self, rva: u32) -> Option<bool> {
        if rva == 0 {
            return Some(false);
        }
        if self.is_forwarder(rva)? {
            let forwarder =
                read_ascii_null_terminated_rva(self.bytes, self.headers.sections(), rva)?;
            let encoded_len = u32::try_from(forwarder.len().checked_add(1)?).ok()?;
            let forwarder_end = rva.checked_add(encoded_len)?;
            let directory_end = self.directory_rva.checked_add(self.directory_size)?;
            return Some(forwarder_end <= directory_end);
        }

        Some(
            rva_range_to_offset(self.headers.sections(), rva, 1)
                .and_then(|offset| self.bytes.get(offset))
                .is_some(),
        )
    }
}

/// Reads the exported symbol names from `bytes`.
///
/// Returns `Some(vec![])` for a valid PE without an export directory and `None`
/// for bytes that are not a parseable PE image.
pub(crate) fn export_names_from_bytes(bytes: &[u8]) -> Option<Vec<String>> {
    let Some(table) = ExportTable::parse(bytes)? else {
        return Some(Vec::new());
    };
    table
        .named_exports()
        .map(|exports| exports.into_iter().map(|(_, name)| name).collect())
}

/// Reads a unique named DATA export whose value is an inline little-endian `u32`.
///
/// Function/forwarder exports, duplicate names, missing symbols, and malformed
/// tables all return `None`. The routine deliberately resolves the name ordinal
/// through `AddressOfFunctions`; a name-table index is not a function index.
pub(crate) fn exported_u32_from_bytes(bytes: &[u8], target: &str) -> Option<u32> {
    if target.is_empty() || !target.is_ascii() {
        return None;
    }

    let table = ExportTable::parse(bytes)??;

    let mut matching_rva = None;
    for (ordinal, name) in table.named_exports()? {
        if name != target {
            continue;
        }
        if matching_rva.is_some() {
            return None;
        }
        matching_rva = Some(table.function_rva(ordinal)?);
    }

    let value_rva = matching_rva?;
    if table.is_forwarder(value_rva)? {
        return None;
    }
    let value_offset = data_rva_to_offset(table.headers.sections(), value_rva, 4)?;
    read_u32(bytes, value_offset)
}

fn read_ascii_null_terminated_rva(
    bytes: &[u8],
    sections: &[super::header::SectionHeader],
    rva: u32,
) -> Option<String> {
    let mut name = String::new();
    for index in 0..=PeExportSet::MAX_NAME_BYTES {
        let current_rva = rva.checked_add(u32::try_from(index).ok()?)?;
        let offset = rva_range_to_offset(sections, current_rva, 1)?;
        let byte = *bytes.get(offset)?;
        if byte == 0 {
            return (!name.is_empty()).then_some(name);
        }
        if !(0x20..=0x7e).contains(&byte) {
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
    const MACHINE_I386: u16 = 0x014c;

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

    #[test]
    fn named_export_surface_is_strict_and_honors_the_256_byte_limit() {
        let maximum = "A".repeat(PeExportSet::MAX_NAME_BYTES);
        let pe = build_export_pe(&[maximum.as_str()]);
        assert_eq!(export_names_from_bytes(&pe), Some(vec![maximum]));

        let too_long = "B".repeat(PeExportSet::MAX_NAME_BYTES + 1);
        assert_eq!(
            export_names_from_bytes(&build_export_pe(&[&too_long])),
            None
        );

        let malformed = build_export_pe(&["Valid", "Invalid\u{1}Name"]);
        assert_eq!(
            export_names_from_bytes(&malformed),
            None,
            "one malformed entry must reject the complete compatibility surface"
        );
    }

    #[test]
    fn reads_named_u32_data_export_from_pe32_and_pe32_plus() {
        for (machine, magic) in [
            (MACHINE_I386, super::super::header::PE32_MAGIC),
            (MACHINE_AMD64, PE32_PLUS_MAGIC),
        ] {
            let pe = build_u32_export_pe(machine, magic, 618, false, false);
            assert_eq!(exported_u32_from_bytes(&pe, "D3D12SDKVersion"), Some(618));
        }
    }

    #[test]
    fn rejects_forwarded_duplicate_and_out_of_bounds_data_exports() {
        let forwarded = build_u32_export_pe(MACHINE_AMD64, PE32_PLUS_MAGIC, 618, true, false);
        assert_eq!(exported_u32_from_bytes(&forwarded, "D3D12SDKVersion"), None);

        let duplicate = build_u32_export_pe(MACHINE_AMD64, PE32_PLUS_MAGIC, 618, false, true);
        assert_eq!(exported_u32_from_bytes(&duplicate, "D3D12SDKVersion"), None);

        let mut truncated = build_u32_export_pe(MACHINE_AMD64, PE32_PLUS_MAGIC, 618, false, false);
        truncated.truncate(truncated.len() - 2);
        assert_eq!(exported_u32_from_bytes(&truncated, "D3D12SDKVersion"), None);

        let mut invalid_ordinal =
            build_u32_export_pe(MACHINE_AMD64, PE32_PLUS_MAGIC, 618, false, false);
        let section_raw_ptr = u32::from_le_bytes(
            invalid_ordinal
                [0x80 + 4 + COFF_HEADER_LEN + 0xF0 + 20..0x80 + 4 + COFF_HEADER_LEN + 0xF0 + 24]
                .try_into()
                .expect("raw pointer"),
        ) as usize;
        let ordinal_offset = section_raw_ptr + EXPORT_DIRECTORY_LEN + 4 + 4;
        invalid_ordinal[ordinal_offset..ordinal_offset + 2].copy_from_slice(&1u16.to_le_bytes());
        assert_eq!(
            exported_u32_from_bytes(&invalid_ordinal, "D3D12SDKVersion"),
            None
        );

        let mut function = build_u32_export_pe(MACHINE_AMD64, PE32_PLUS_MAGIC, 618, false, false);
        let section_offset = 0x80 + 4 + COFF_HEADER_LEN + 0xF0;
        function[section_offset + super::super::header::SECTION_CHARACTERISTICS_OFFSET
            ..section_offset + super::super::header::SECTION_CHARACTERISTICS_OFFSET + 4]
            .copy_from_slice(&0x2000_0000u32.to_le_bytes());
        assert_eq!(exported_u32_from_bytes(&function, "D3D12SDKVersion"), None);
    }

    #[test]
    fn rejects_short_export_directory_and_overlapping_sections() {
        let mut short = build_u32_export_pe(MACHINE_AMD64, PE32_PLUS_MAGIC, 618, false, false);
        let optional_header_offset = 0x80 + 4 + COFF_HEADER_LEN;
        let export_size_offset = optional_header_offset + PE32_PLUS_DATA_DIRECTORY_OFFSET + 4;
        short[export_size_offset..export_size_offset + 4]
            .copy_from_slice(&((EXPORT_DIRECTORY_LEN - 1) as u32).to_le_bytes());
        assert_eq!(exported_u32_from_bytes(&short, "D3D12SDKVersion"), None);

        let mut overlapping =
            build_u32_export_pe(MACHINE_AMD64, PE32_PLUS_MAGIC, 618, false, false);
        let coff_offset = 0x80 + 4;
        overlapping[coff_offset + 2..coff_offset + 4].copy_from_slice(&2u16.to_le_bytes());
        let first_section = coff_offset + COFF_HEADER_LEN + 0xF0;
        let second_section = first_section + SECTION_HEADER_LEN;
        let first_header = overlapping[first_section..first_section + SECTION_HEADER_LEN].to_vec();
        overlapping[second_section..second_section + SECTION_HEADER_LEN]
            .copy_from_slice(&first_header);
        assert_eq!(
            exported_u32_from_bytes(&overlapping, "D3D12SDKVersion"),
            None
        );
    }

    #[test]
    fn readable_architecture_with_a_damaged_export_table_has_no_profile() {
        let mut damaged = build_export_pe(&["VR_InitInternal"]);
        let optional_header_offset = 0x80 + 4 + COFF_HEADER_LEN;
        let export_size_offset = optional_header_offset + PE32_PLUS_DATA_DIRECTORY_OFFSET + 4;
        damaged[export_size_offset..export_size_offset + 4]
            .copy_from_slice(&((EXPORT_DIRECTORY_LEN - 1) as u32).to_le_bytes());

        let architecture = super::super::read_pe_architecture_from_bytes(&damaged);
        assert_eq!(architecture, Some(renderpilot_domain::Architecture::X64));
        let inspection = super::super::PeInspection {
            architecture,
            export_names: export_names_from_bytes(&damaged),
            ..Default::default()
        };
        assert!(
            inspection.compatibility_profile().is_none(),
            "a partial architecture-only profile must never escape inspection"
        );
    }

    #[test]
    fn malformed_name_ordinals_reject_the_complete_export_surface() {
        let mut invalid_ordinal = build_export_pe(&["VR_InitInternal"]);
        let section_raw_ptr = export_section_raw_pointer(&invalid_ordinal);
        let ordinals_offset = section_raw_ptr + EXPORT_DIRECTORY_LEN + 4 + 4;
        invalid_ordinal[ordinals_offset..ordinals_offset + 2].copy_from_slice(&1u16.to_le_bytes());
        assert_eq!(export_names_from_bytes(&invalid_ordinal), None);

        let architecture = super::super::read_pe_architecture_from_bytes(&invalid_ordinal);
        let inspection = super::super::PeInspection {
            architecture,
            export_names: export_names_from_bytes(&invalid_ordinal),
            ..Default::default()
        };
        assert!(
            inspection.compatibility_profile().is_none(),
            "an out-of-range name ordinal must invalidate the complete profile"
        );

        let mut names_without_functions = build_export_pe(&["VR_InitInternal"]);
        let directory = export_section_raw_pointer(&names_without_functions);
        names_without_functions[directory + EXPORT_NUMBER_OF_FUNCTIONS_OFFSET
            ..directory + EXPORT_NUMBER_OF_FUNCTIONS_OFFSET + 4]
            .copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(export_names_from_bytes(&names_without_functions), None);
    }

    #[test]
    fn invalid_function_rvas_reject_the_complete_export_surface() {
        let mut unmapped_target = build_export_pe(&["VR_InitInternal"]);
        let functions = export_section_raw_pointer(&unmapped_target) + EXPORT_DIRECTORY_LEN;
        unmapped_target[functions..functions + 4].copy_from_slice(&0xffff_0000u32.to_le_bytes());
        assert_eq!(export_names_from_bytes(&unmapped_target), None);
        let inspection = super::super::inspect_pe_bytes(&unmapped_target);
        assert_eq!(
            inspection.architecture,
            Some(renderpilot_domain::Architecture::X64)
        );
        assert!(
            inspection.compatibility_profile().is_none(),
            "an unmapped EAT target must invalidate the atomic profile"
        );

        let mut malformed_forwarder = build_export_pe(&["VR_InitInternal"]);
        let functions = export_section_raw_pointer(&malformed_forwarder) + EXPORT_DIRECTORY_LEN;
        malformed_forwarder[functions..functions + 4].copy_from_slice(&0x1000u32.to_le_bytes());
        assert_eq!(
            export_names_from_bytes(&malformed_forwarder),
            None,
            "a forwarder RVA must name a valid printable ASCII string"
        );
    }

    fn build_u32_export_pe(
        machine: u16,
        magic: u16,
        value: u32,
        forwarded: bool,
        duplicate_name: bool,
    ) -> Vec<u8> {
        let pe_offset: usize = 0x80;
        let optional_header_size: usize = 0xF0;
        let coff_offset = pe_offset + 4;
        let optional_header_offset = coff_offset + COFF_HEADER_LEN;
        let optional_header_end = optional_header_offset + optional_header_size;
        let section_table_offset = optional_header_end;
        let headers_end = section_table_offset + SECTION_HEADER_LEN;
        let section_rva: u32 = 0x1000;
        let section_raw_ptr = align_up(headers_end as u32, 0x200);
        let name_count = if duplicate_name { 2usize } else { 1usize };

        let functions_offset = EXPORT_DIRECTORY_LEN;
        let names_offset = functions_offset + 4;
        let ordinals_offset = names_offset + name_count * 4;
        let strings_offset = ordinals_offset + name_count * 2;
        let name = b"D3D12SDKVersion\0";
        let value_offset = strings_offset + name.len() * name_count;
        let mut body = vec![0u8; value_offset + 4];

        body[EXPORT_NUMBER_OF_FUNCTIONS_OFFSET..EXPORT_NUMBER_OF_FUNCTIONS_OFFSET + 4]
            .copy_from_slice(&1u32.to_le_bytes());
        body[EXPORT_NUMBER_OF_NAMES_OFFSET..EXPORT_NUMBER_OF_NAMES_OFFSET + 4]
            .copy_from_slice(&(name_count as u32).to_le_bytes());
        body[EXPORT_ADDRESS_OF_FUNCTIONS_OFFSET..EXPORT_ADDRESS_OF_FUNCTIONS_OFFSET + 4]
            .copy_from_slice(&(section_rva + functions_offset as u32).to_le_bytes());
        body[EXPORT_ADDRESS_OF_NAMES_OFFSET..EXPORT_ADDRESS_OF_NAMES_OFFSET + 4]
            .copy_from_slice(&(section_rva + names_offset as u32).to_le_bytes());
        body[EXPORT_ADDRESS_OF_NAME_ORDINALS_OFFSET..EXPORT_ADDRESS_OF_NAME_ORDINALS_OFFSET + 4]
            .copy_from_slice(&(section_rva + ordinals_offset as u32).to_le_bytes());

        let function_rva = if forwarded {
            section_rva + 4
        } else {
            section_rva + value_offset as u32
        };
        body[functions_offset..functions_offset + 4].copy_from_slice(&function_rva.to_le_bytes());
        for index in 0..name_count {
            let string_offset = strings_offset + index * name.len();
            body[names_offset + index * 4..names_offset + index * 4 + 4]
                .copy_from_slice(&(section_rva + string_offset as u32).to_le_bytes());
            body[ordinals_offset + index * 2..ordinals_offset + index * 2 + 2]
                .copy_from_slice(&0u16.to_le_bytes());
            body[string_offset..string_offset + name.len()].copy_from_slice(name);
        }
        body[value_offset..value_offset + 4].copy_from_slice(&value.to_le_bytes());

        let total_len = section_raw_ptr as usize + body.len();
        let mut bytes = vec![0u8; total_len];
        bytes[0..2].copy_from_slice(b"MZ");
        bytes[DOS_PE_POINTER_OFFSET..DOS_PE_POINTER_OFFSET + 4]
            .copy_from_slice(&(pe_offset as u32).to_le_bytes());
        bytes[pe_offset..pe_offset + 4].copy_from_slice(b"PE\0\0");
        bytes[coff_offset..coff_offset + 2].copy_from_slice(&machine.to_le_bytes());
        bytes[coff_offset + 2..coff_offset + 4].copy_from_slice(&1u16.to_le_bytes());
        bytes[coff_offset + 16..coff_offset + 18]
            .copy_from_slice(&(optional_header_size as u16).to_le_bytes());
        bytes[optional_header_offset..optional_header_offset + 2]
            .copy_from_slice(&magic.to_le_bytes());
        let data_directory_offset = optional_header_offset
            + if magic == PE32_PLUS_MAGIC {
                PE32_PLUS_DATA_DIRECTORY_OFFSET
            } else {
                super::super::header::PE32_DATA_DIRECTORY_OFFSET
            };
        bytes[data_directory_offset..data_directory_offset + 4]
            .copy_from_slice(&section_rva.to_le_bytes());
        bytes[data_directory_offset + 4..data_directory_offset + 8]
            .copy_from_slice(&(EXPORT_DIRECTORY_LEN as u32).to_le_bytes());
        bytes[section_table_offset..section_table_offset + 8].copy_from_slice(b".edata\0\0");
        bytes[section_table_offset + 8..section_table_offset + 12]
            .copy_from_slice(&(body.len() as u32).to_le_bytes());
        bytes[section_table_offset + 12..section_table_offset + 16]
            .copy_from_slice(&section_rva.to_le_bytes());
        bytes[section_table_offset + 16..section_table_offset + 20]
            .copy_from_slice(&(body.len() as u32).to_le_bytes());
        bytes[section_table_offset + 20..section_table_offset + 24]
            .copy_from_slice(&section_raw_ptr.to_le_bytes());
        bytes[section_raw_ptr as usize..section_raw_ptr as usize + body.len()]
            .copy_from_slice(&body);
        bytes
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
            let functions_table_offset = section_body.len();
            section_body.resize(functions_table_offset + export_names.len() * 4, 0);
            let names_table_offset = section_body.len();
            section_body.resize(names_table_offset + export_names.len() * 4, 0);
            let ordinals_table_offset = section_body.len();
            section_body.resize(ordinals_table_offset + export_names.len() * 2, 0);

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

            let function_stub_rva = section_rva + section_body.len() as u32;
            section_body.push(0xc3);
            for index in 0..export_names.len() {
                let function_offset = functions_table_offset + index * 4;
                section_body[function_offset..function_offset + 4]
                    .copy_from_slice(&function_stub_rva.to_le_bytes());
                let ordinal_offset = ordinals_table_offset + index * 2;
                section_body[ordinal_offset..ordinal_offset + 2]
                    .copy_from_slice(&(index as u16).to_le_bytes());
            }

            section_body[EXPORT_NUMBER_OF_FUNCTIONS_OFFSET..EXPORT_NUMBER_OF_FUNCTIONS_OFFSET + 4]
                .copy_from_slice(&(export_names.len() as u32).to_le_bytes());
            section_body[EXPORT_NUMBER_OF_NAMES_OFFSET..EXPORT_NUMBER_OF_NAMES_OFFSET + 4]
                .copy_from_slice(&(export_names.len() as u32).to_le_bytes());
            section_body
                [EXPORT_ADDRESS_OF_FUNCTIONS_OFFSET..EXPORT_ADDRESS_OF_FUNCTIONS_OFFSET + 4]
                .copy_from_slice(&(section_rva + functions_table_offset as u32).to_le_bytes());
            section_body[EXPORT_ADDRESS_OF_NAMES_OFFSET..EXPORT_ADDRESS_OF_NAMES_OFFSET + 4]
                .copy_from_slice(&(section_rva + names_table_offset as u32).to_le_bytes());
            section_body[EXPORT_ADDRESS_OF_NAME_ORDINALS_OFFSET
                ..EXPORT_ADDRESS_OF_NAME_ORDINALS_OFFSET + 4]
                .copy_from_slice(&(section_rva + ordinals_table_offset as u32).to_le_bytes());
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
                .copy_from_slice(&(EXPORT_DIRECTORY_LEN as u32).to_le_bytes());
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

    fn export_section_raw_pointer(bytes: &[u8]) -> usize {
        let section_offset = 0x80 + 4 + COFF_HEADER_LEN + 0xF0;
        u32::from_le_bytes(
            bytes[section_offset + 20..section_offset + 24]
                .try_into()
                .expect("section raw pointer"),
        ) as usize
    }

    fn align_up(value: u32, alignment: u32) -> u32 {
        value.div_ceil(alignment) * alignment
    }
}
