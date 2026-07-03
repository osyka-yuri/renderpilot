//! Shared fixtures for RenoDX unit tests.

use renderpilot_domain::Architecture;

use super::types::{
    AnticheatEngine, AssessmentConfidence, Category, Channel, Compatibility, MatchKind, MatchRule,
    OnlineKind, RenoDxManifest, ReshadeConfig, ReshadeNightly, ReshadeStable, Risk, RiskSeverity,
    Status, Title, manifest_defaults,
};

/// Builds a single match rule.
pub(crate) fn rule(kind: MatchKind, value: &str, tier: u32) -> MatchRule {
    MatchRule {
        kind,
        value: value.to_owned(),
        tier,
    }
}

/// Builds a single-line risk, used by both the matcher and anti-cheat fixtures.
pub(crate) fn manifest_risk(
    severity: RiskSeverity,
    anticheat_engine: AnticheatEngine,
    message_key: &str,
) -> Risk {
    Risk {
        anticheat_engine,
        online: OnlineKind::Singleplayer,
        severity,
        message_key: message_key.to_owned(),
        confidence: AssessmentConfidence::Medium,
        source: None,
    }
}

/// Builds a standard title with the given match rules.
pub(crate) fn title(
    id: &str,
    slug: &str,
    arch: Architecture,
    status: Status,
    match_rules: Vec<MatchRule>,
) -> Title {
    Title {
        id: id.to_owned(),
        name: format!("Game {id}"),
        category: Category::default(),
        slug: slug.to_owned(),
        arch,
        status,
        channel: Channel::default(),
        min_app_version: "1.0.0".to_owned(),
        match_rules,
        compatibility: Compatibility::default(),
        risk: manifest_risk(
            RiskSeverity::Info,
            AnticheatEngine::None,
            "renodx.risk.sp_safe",
        ),
        proxy_dll_override: None,
        notes_keys: Vec::new(),
        download_url: None,
    }
}

/// Builds a manifest over the given titles with a default ReShade config.
pub(crate) fn manifest(titles: Vec<Title>) -> RenoDxManifest {
    RenoDxManifest {
        schema_version: 3,
        generated_at: "2026-06-15T00:00:00Z".to_owned(),
        reshade: ReshadeConfig {
            stable: Some(ReshadeStable {
                url: "https://reshade.me/downloads/ReShade_Setup_6.7.3_Addon.exe".to_owned(),
            }),
            nightly: ReshadeNightly {
                url64: "https://nightly.link/crosire/reshade/workflows/build/main/ReShade%20(64-bit).zip".to_owned(),
                url32: "https://nightly.link/crosire/reshade/workflows/build/main/ReShade%20(32-bit).zip".to_owned(),
            },
        },
        generics: Vec::new(),
        defaults: manifest_defaults(),
        titles,
    }
}

// ---------------------------------------------------------------------------
// Synthetic PE images for the install/fetch tests.
// ---------------------------------------------------------------------------

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
const EXPORT_NUMBER_OF_NAMES_OFFSET: usize = 24;
const EXPORT_ADDRESS_OF_NAMES_OFFSET: usize = 32;

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
    let names_table_offset = section_body.len();
    section_body.resize(names_table_offset + exports.len() * 4, 0);

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
    section_body[EXPORT_NUMBER_OF_NAMES_OFFSET..EXPORT_NUMBER_OF_NAMES_OFFSET + 4]
        .copy_from_slice(&(exports.len() as u32).to_le_bytes());
    section_body[EXPORT_ADDRESS_OF_NAMES_OFFSET..EXPORT_ADDRESS_OF_NAMES_OFFSET + 4]
        .copy_from_slice(&(section_rva + names_table_offset as u32).to_le_bytes());

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
        .copy_from_slice(&(section_body.len() as u32).to_le_bytes());

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

fn align_up(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}

#[cfg(windows)]
use renderpilot_application::{AppResult, SharedArtifactRepository};
#[cfg(windows)]
use renderpilot_domain::{SharedArtifactKind, SharedArtifactRecord};
#[cfg(windows)]
use std::sync::{Arc, Mutex};

#[cfg(windows)]
#[derive(Default, Clone)]
pub(crate) struct InMemorySharedArtifactRepository {
    pub artifacts: Arc<Mutex<Vec<SharedArtifactRecord>>>,
}

#[cfg(windows)]
impl SharedArtifactRepository for InMemorySharedArtifactRepository {
    fn upsert_shared_artifact(&self, record: &SharedArtifactRecord) -> AppResult<()> {
        let mut artifacts = self.artifacts.lock().unwrap();
        artifacts.retain(|r| r.kind() != record.kind());
        artifacts.push(record.clone());
        Ok(())
    }

    fn get_shared_artifact(
        &self,
        kind: SharedArtifactKind,
    ) -> AppResult<Option<SharedArtifactRecord>> {
        let artifacts = self.artifacts.lock().unwrap();
        Ok(artifacts.iter().find(|r| r.kind() == kind).cloned())
    }

    fn delete_shared_artifact(&self, kind: SharedArtifactKind) -> AppResult<()> {
        let mut artifacts = self.artifacts.lock().unwrap();
        artifacts.retain(|r| r.kind() != kind);
        Ok(())
    }
}
