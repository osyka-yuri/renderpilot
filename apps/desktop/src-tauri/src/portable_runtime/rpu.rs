use std::{
    collections::BTreeSet,
    io::{Cursor, Read},
};

use semver::Version;
use serde::Deserialize;

use super::{
    error::{PortableRuntimeError, Result},
    random::hex,
    signature::{sha256_hex, verify},
};

pub const RPU_PROTOCOL: &str = "renderpilot-portable-rpu-v1";
/// Oldest catalog schema shipped by a public RenderPilot 1.x release.
///
/// This is a stable compatibility floor, not the schema compiled into the
/// supervisor. Every portable App generation retains the complete migration
/// chain from this floor to its own current schema.
pub use renderpilot_storage_sqlite::{
    CURRENT_PORTABLE_SCHEMA_VERSION as MAXIMUM_SCHEMA,
    MINIMUM_PORTABLE_SCHEMA_VERSION as MINIMUM_SCHEMA, PORTABLE_APP_SESSION_PROTOCOL,
    PORTABLE_SUPERVISOR_CAPABILITY,
};
const FOOTER_MAGIC: &[u8; 5] = b"RPSX1";
const FOOTER_LEN: usize = 102;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpuManifest {
    pub protocol: String,
    pub platform: String,
    pub version: String,
    pub app_sha256: String,
    pub app_length: u64,
    pub minimum_supervisor_protocol: u16,
    pub app_session_protocol: String,
    pub minimum_schema: u32,
    pub maximum_schema: u32,
    pub portable_role: String,
}

pub struct VerifiedRpu {
    pub manifest: RpuManifest,
    pub app_bytes: Vec<u8>,
    pub rpu_sha256: String,
}

pub struct EmbeddedRpu<'a> {
    pub rpu: &'a [u8],
    pub signature: &'a str,
}

/// Extracts the fixed footer without attempting to execute, copy, or replace
/// the stable raw supervisor.
pub fn embedded_rpu(raw: &[u8]) -> Result<EmbeddedRpu<'_>> {
    if raw.len() < FOOTER_LEN {
        return Err(PortableRuntimeError::new(
            "portable_sfx_footer",
            "raw supervisor is smaller than RPSX1 footer",
        ));
    }
    let footer = &raw[raw.len() - FOOTER_LEN..];
    if &footer[..5] != FOOTER_MAGIC || footer[5] != 1 {
        return Err(PortableRuntimeError::new(
            "portable_sfx_footer",
            "RPSX1 footer magic or protocol was invalid",
        ));
    }
    let read_u64 = |offset: usize| -> Result<usize> {
        let end = offset.checked_add(8).ok_or_else(|| {
            PortableRuntimeError::new("portable_sfx_footer", "footer range overflow")
        })?;
        let bytes: [u8; 8] = footer
            .get(offset..end)
            .ok_or_else(|| {
                PortableRuntimeError::new("portable_sfx_footer", "footer range was invalid")
            })?
            .try_into()
            .map_err(|_| {
                PortableRuntimeError::new("portable_sfx_footer", "footer field width was invalid")
            })?;
        let value = u64::from_le_bytes(bytes);
        usize::try_from(value).map_err(|_| {
            PortableRuntimeError::new("portable_sfx_footer", "footer offset did not fit usize")
        })
    };
    let rpu_offset = read_u64(6)?;
    let rpu_length = read_u64(14)?;
    let signature_offset = read_u64(22)?;
    let signature_length = read_u64(30)?;
    let rpu_end = rpu_offset
        .checked_add(rpu_length)
        .ok_or_else(|| PortableRuntimeError::new("portable_sfx_footer", "RPU range overflow"))?;
    let signature_end = signature_offset
        .checked_add(signature_length)
        .ok_or_else(|| {
            PortableRuntimeError::new("portable_sfx_footer", "signature range overflow")
        })?;
    if rpu_end > raw.len() - FOOTER_LEN
        || signature_end > raw.len() - FOOTER_LEN
        || rpu_end != signature_offset
    {
        return Err(PortableRuntimeError::new(
            "portable_sfx_footer",
            "RPSX1 ranges were not adjacent embedded assets",
        ));
    }
    let rpu = &raw[rpu_offset..rpu_end];
    let signature_bytes = &raw[signature_offset..signature_end];
    let expected_rpu = hex(&footer[38..70]);
    let expected_signature = hex(&footer[70..102]);
    if sha256_hex(rpu) != expected_rpu || sha256_hex(signature_bytes) != expected_signature {
        return Err(PortableRuntimeError::new(
            "portable_sfx_footer",
            "embedded asset digest did not match RPSX1 footer",
        ));
    }
    let signature = std::str::from_utf8(signature_bytes).map_err(|_| {
        PortableRuntimeError::new("portable_sfx_footer", "embedded signature was not UTF-8")
    })?;
    Ok(EmbeddedRpu { rpu, signature })
}

pub fn verify_rpu(bytes: &[u8], encoded_signature: &str) -> Result<VerifiedRpu> {
    verify(bytes, encoded_signature)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| PortableRuntimeError::new("portable_rpu_layout", error.to_string()))?;
    if archive.len() != 2 {
        return Err(PortableRuntimeError::new(
            "portable_rpu_layout",
            "RPU must contain exactly manifest and app entries",
        ));
    }
    let mut names = BTreeSet::new();
    let mut manifest_bytes = None;
    let mut app_bytes = None;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| PortableRuntimeError::new("portable_rpu_layout", error.to_string()))?;
        let name = entry.name().to_owned();
        if name.contains("..")
            || name.starts_with('/')
            || name.contains('\\')
            || !names.insert(name.clone())
            || entry.is_dir()
            || entry.compression() != zip::CompressionMethod::Stored
        {
            return Err(PortableRuntimeError::new(
                "portable_rpu_layout",
                "RPU contained an unsafe, duplicate, or directory entry",
            ));
        }
        let mut contents = Vec::new();
        entry.read_to_end(&mut contents)?;
        match name.as_str() {
            "rpu-manifest.json" => manifest_bytes = Some(contents),
            "app/renderpilot-app.exe" => app_bytes = Some(contents),
            _ => {
                return Err(PortableRuntimeError::new(
                    "portable_rpu_layout",
                    "RPU contained an unrecognized entry",
                ));
            }
        }
    }
    let manifest: RpuManifest = serde_json::from_slice(&manifest_bytes.ok_or_else(|| {
        PortableRuntimeError::new("portable_rpu_layout", "RPU manifest was missing")
    })?)
    .map_err(|error| PortableRuntimeError::new("portable_rpu_manifest", error.to_string()))?;
    let app_bytes = app_bytes.ok_or_else(|| {
        PortableRuntimeError::new("portable_rpu_layout", "RPU App image was missing")
    })?;
    canonical_version(&manifest.version)?;
    if manifest.protocol != RPU_PROTOCOL
        || manifest.platform != "windows-x86_64-portable"
        || manifest.portable_role != "app"
        || manifest.minimum_supervisor_protocol != PORTABLE_SUPERVISOR_CAPABILITY
        || manifest.app_session_protocol != PORTABLE_APP_SESSION_PROTOCOL
        || !schema_range_is_supported(manifest.minimum_schema, manifest.maximum_schema)
        || manifest.app_length != app_bytes.len() as u64
        || manifest.app_sha256 != sha256_hex(&app_bytes)
    {
        return Err(PortableRuntimeError::new(
            "portable_rpu_manifest",
            "RPU manifest did not authenticate its App image",
        ));
    }
    Ok(VerifiedRpu {
        rpu_sha256: sha256_hex(bytes),
        manifest,
        app_bytes,
    })
}

/// Validates a signed App generation's schema capability without binding the
/// stable supervisor to that generation's current schema.
///
/// `maximum_schema` is bounded by `i32::MAX` because SQLite's `PRAGMA user_version`
/// and catalog migration step transitions represent schema versions as signed 32-bit integers.
pub fn schema_range_is_supported(minimum_schema: u32, maximum_schema: u32) -> bool {
    minimum_schema == MINIMUM_SCHEMA
        && maximum_schema >= minimum_schema
        && i32::try_from(maximum_schema).is_ok()
}

/// Verifies a signed RPU and binds its canonical manifest version to the
/// release context before the caller may publish or stage any generation.
pub fn verify_rpu_expected(
    bytes: &[u8],
    encoded_signature: &str,
    expected_version: &str,
) -> Result<VerifiedRpu> {
    canonical_version(expected_version)?;
    let verified = verify_rpu(bytes, encoded_signature)?;
    if verified.manifest.version != expected_version {
        return Err(PortableRuntimeError::new(
            "portable_rpu_version",
            "signed RPU version did not match its expected release context",
        ));
    }
    Ok(verified)
}

/// Accepts only canonical SemVer text. Parsing alone is insufficient because
/// a context comparison must not admit a differently-spelled normalized value.
pub fn canonical_version(version: &str) -> Result<Version> {
    let parsed = Version::parse(version)
        .map_err(|error| PortableRuntimeError::new("portable_rpu_version", error.to_string()))?;
    if parsed.to_string() != version {
        return Err(PortableRuntimeError::new(
            "portable_rpu_version",
            "RPU version was valid SemVer but not canonical text",
        ));
    }
    Ok(parsed)
}
