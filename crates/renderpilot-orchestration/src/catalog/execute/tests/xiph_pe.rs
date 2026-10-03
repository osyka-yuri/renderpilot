//! Minimal PE32+ fixtures used by Xiph transition tests.

/// Builds a PE32+ fixture with imports and one public export for boundary checks.
pub(super) fn synthetic_xiph_pe(public_export: &str, static_imports: &[&str]) -> Vec<u8> {
    build(public_export, static_imports, None, None)
}

pub(super) fn synthetic_xiph_pe_with_delay(
    public_export: &str,
    static_imports: &[&str],
    delay_imports: &[&str],
    marker: u8,
) -> Vec<u8> {
    build(
        public_export,
        static_imports,
        Some(delay_imports),
        Some(marker),
    )
}

fn build(
    public_export: &str,
    static_imports: &[&str],
    delay_imports: Option<&[&str]>,
    marker: Option<u8>,
) -> Vec<u8> {
    const PE_OFFSET: usize = 0x80;
    const COFF_HEADER_LEN: usize = 20;
    const OPTIONAL_HEADER_SIZE: usize = 0xf0;
    const SECTION_RVA: u32 = 0x1000;
    const SECTION_RAW_POINTER: usize = 0x200;
    const DATA_DIRECTORIES_OFFSET: usize = 112;
    const DATA_DIRECTORY_ENTRY_LEN: usize = 8;
    const EXPORT_DIRECTORY_LEN: usize = 40;
    const IMPORT_DESCRIPTOR_LEN: usize = 20;
    const DELAY_DESCRIPTOR_LEN: usize = 32;
    const REGULAR_DIRECTORY_INDEX: usize = 1;
    const DELAY_DIRECTORY_INDEX: usize = 13;

    let coff_offset = PE_OFFSET + 4;
    let optional_offset = coff_offset + COFF_HEADER_LEN;
    let section_offset = optional_offset + OPTIONAL_HEADER_SIZE;
    let export_functions_offset = EXPORT_DIRECTORY_LEN;
    let export_names_offset = export_functions_offset + 4;
    let export_ordinals_offset = export_names_offset + 4;
    let regular_offset = export_ordinals_offset + 2;
    let regular_len = (static_imports.len() + 1) * IMPORT_DESCRIPTOR_LEN;
    let delay_offset = regular_offset + regular_len;
    let delay_len = delay_imports.map_or(0, |imports| (imports.len() + 1) * DELAY_DESCRIPTOR_LEN);
    let delay_imports = delay_imports.unwrap_or_default();
    let names_offset = delay_offset + delay_len;

    let mut section = vec![0; names_offset];
    let export_name_offset = section.len();
    section.extend_from_slice(public_export.as_bytes());
    section.push(0);
    let mut regular_name_rvas = Vec::with_capacity(static_imports.len());
    for import in static_imports {
        regular_name_rvas.push(SECTION_RVA + u32::try_from(section.len()).expect("fixture size"));
        section.extend_from_slice(import.as_bytes());
        section.push(0);
    }
    let mut delay_name_rvas = Vec::with_capacity(delay_imports.len());
    for import in delay_imports {
        delay_name_rvas.push(SECTION_RVA + u32::try_from(section.len()).expect("fixture size"));
        section.extend_from_slice(import.as_bytes());
        section.push(0);
    }
    let function_rva = SECTION_RVA + u32::try_from(section.len()).expect("fixture size");
    section.push(0xc3); // mapped byte: the export parser only needs a valid target.
    if let Some(marker) = marker {
        section.push(marker);
    }

    section[20..24].copy_from_slice(&1u32.to_le_bytes());
    section[24..28].copy_from_slice(&1u32.to_le_bytes());
    section[28..32].copy_from_slice(
        &(SECTION_RVA + u32::try_from(export_functions_offset).expect("export offset"))
            .to_le_bytes(),
    );
    section[32..36].copy_from_slice(
        &(SECTION_RVA + u32::try_from(export_names_offset).expect("export offset")).to_le_bytes(),
    );
    section[36..40].copy_from_slice(
        &(SECTION_RVA + u32::try_from(export_ordinals_offset).expect("export offset"))
            .to_le_bytes(),
    );
    section[export_functions_offset..export_functions_offset + 4]
        .copy_from_slice(&function_rva.to_le_bytes());
    section[export_names_offset..export_names_offset + 4].copy_from_slice(
        &(SECTION_RVA + u32::try_from(export_name_offset).expect("export name offset"))
            .to_le_bytes(),
    );
    section[export_ordinals_offset..export_ordinals_offset + 2]
        .copy_from_slice(&0u16.to_le_bytes());
    for (index, name_rva) in regular_name_rvas.iter().enumerate() {
        let descriptor = regular_offset + index * IMPORT_DESCRIPTOR_LEN;
        section[descriptor..descriptor + 4].copy_from_slice(&SECTION_RVA.to_le_bytes());
        section[descriptor + 12..descriptor + 16].copy_from_slice(&name_rva.to_le_bytes());
    }
    for (index, name_rva) in delay_name_rvas.iter().enumerate() {
        let descriptor = delay_offset + index * DELAY_DESCRIPTOR_LEN;
        section[descriptor..descriptor + 4].copy_from_slice(&1u32.to_le_bytes());
        section[descriptor + 4..descriptor + 8].copy_from_slice(&name_rva.to_le_bytes());
    }

    let mut bytes = vec![0; SECTION_RAW_POINTER + section.len()];
    bytes[0..2].copy_from_slice(b"MZ");
    bytes[0x3c..0x40].copy_from_slice(&(PE_OFFSET as u32).to_le_bytes());
    bytes[PE_OFFSET..PE_OFFSET + 4].copy_from_slice(b"PE\0\0");
    bytes[coff_offset..coff_offset + 2].copy_from_slice(&0x8664u16.to_le_bytes());
    bytes[coff_offset + 2..coff_offset + 4].copy_from_slice(&1u16.to_le_bytes());
    bytes[coff_offset + 16..coff_offset + 18]
        .copy_from_slice(&(OPTIONAL_HEADER_SIZE as u16).to_le_bytes());
    bytes[optional_offset..optional_offset + 2].copy_from_slice(&0x20bu16.to_le_bytes());
    let data_directories = optional_offset + DATA_DIRECTORIES_OFFSET;
    bytes[data_directories - 4..data_directories].copy_from_slice(&16u32.to_le_bytes());
    bytes[data_directories..data_directories + 4].copy_from_slice(&SECTION_RVA.to_le_bytes());
    bytes[data_directories + 4..data_directories + 8]
        .copy_from_slice(&(EXPORT_DIRECTORY_LEN as u32).to_le_bytes());
    let regular_directory = data_directories + REGULAR_DIRECTORY_INDEX * DATA_DIRECTORY_ENTRY_LEN;
    bytes[regular_directory..regular_directory + 4].copy_from_slice(
        &(SECTION_RVA + u32::try_from(regular_offset).expect("regular import offset"))
            .to_le_bytes(),
    );
    bytes[regular_directory + 4..regular_directory + 8]
        .copy_from_slice(&(regular_len as u32).to_le_bytes());
    if !delay_imports.is_empty() {
        let delay_directory = data_directories + DELAY_DIRECTORY_INDEX * DATA_DIRECTORY_ENTRY_LEN;
        bytes[delay_directory..delay_directory + 4].copy_from_slice(
            &(SECTION_RVA + u32::try_from(delay_offset).expect("delay import offset"))
                .to_le_bytes(),
        );
        bytes[delay_directory + 4..delay_directory + 8]
            .copy_from_slice(&(delay_len as u32).to_le_bytes());
    }
    bytes[section_offset..section_offset + 8].copy_from_slice(b".rdata\0\0");
    bytes[section_offset + 8..section_offset + 12]
        .copy_from_slice(&(section.len() as u32).to_le_bytes());
    bytes[section_offset + 12..section_offset + 16].copy_from_slice(&SECTION_RVA.to_le_bytes());
    bytes[section_offset + 16..section_offset + 20]
        .copy_from_slice(&(section.len() as u32).to_le_bytes());
    bytes[section_offset + 20..section_offset + 24]
        .copy_from_slice(&(SECTION_RAW_POINTER as u32).to_le_bytes());
    bytes[SECTION_RAW_POINTER..].copy_from_slice(&section);
    bytes
}
