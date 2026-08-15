use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

use semver::Version;
use serde::{Deserialize, Serialize};

use super::{
    error::{PortableRuntimeError, Result},
    health::validate_selected_app,
    provenance::{self, SealDomain},
    random::hex_32,
    rpu::{VerifiedRpu, canonical_version, schema_range_is_supported},
    signature::sha256_file,
    win32::file::{NoReplacePublication, publish_no_replace},
};

const APP_NAME: &str = "renderpilot-app.exe";

/// Exact historical metadata accepted only by the initial full-package
/// reducer. These are singular contracts, not an extensible compatibility
/// registry; their release versions remain authenticated data rather than
/// becoming part of type or function names.
struct ReleasedPredecessorContract {
    version: &'static str,
    minimum_schema: u32,
    maximum_schema: u32,
}

struct SessionBoundPredecessorContract {
    released: ReleasedPredecessorContract,
    supervisor_capability: u16,
    app_session_protocol: &'static str,
}

const RECEIPT_V2_PREDECESSOR: ReleasedPredecessorContract = ReleasedPredecessorContract {
    version: "1.9.0",
    minimum_schema: 4,
    maximum_schema: 16,
};
const RECEIPT_V3_PREDECESSOR: SessionBoundPredecessorContract = SessionBoundPredecessorContract {
    released: ReleasedPredecessorContract {
        version: "1.9.1",
        minimum_schema: 4,
        maximum_schema: 16,
    },
    supervisor_capability: 2,
    app_session_protocol: "renderpilot-portable-app-session-v1",
};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GenerationReceiptV3 {
    protocol: u16,
    rpu_sha256: String,
    version: String,
    app_sha256: String,
    minimum_supervisor_protocol: u16,
    app_session_protocol: String,
    minimum_schema: u32,
    maximum_schema: u32,
}

/// Only the initial-selection reducer may inspect this metadata. It is never a
/// launchable generation because it predates the private App-session binding.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyGenerationReceiptV2 {
    protocol: u16,
    rpu_sha256: String,
    version: String,
    app_sha256: String,
    minimum_schema: u32,
    maximum_schema: u32,
}

/// Immutable selection proof loaded from a previously published RPU object.
pub struct StoredGeneration {
    pub generation_root: PathBuf,
    pub app: PathBuf,
    pub rpu_sha256: String,
    /// Already signed and receipt-validated; propagated only so activation can
    /// compare retained App capability bytes without reopening its path.
    pub app_sha256: String,
    pub version: String,
    pub minimum_supervisor_protocol: u16,
    pub app_session_protocol: String,
    pub minimum_schema: u32,
    pub maximum_schema: u32,
}

/// A sealed predecessor proof that may authorize only publication of the
/// embedded native generation. Its App image is never launchable.
pub struct MetadataOnlyPredecessor {
    pub version: Version,
}

pub enum InitialSelectedGeneration {
    Current(StoredGeneration),
    MetadataOnly(MetadataOnlyPredecessor),
}

/// Publishes one immutable generation. Existing generations are revalidated;
/// they are never overwritten or garbage-collected by the updater.
pub fn publish(generation_store_root: &Path, rpu: &VerifiedRpu) -> Result<(PathBuf, PathBuf)> {
    let objects = generation_store_root.join("objects");
    std::fs::create_dir_all(&objects)?;
    let generation = objects.join(&rpu.rpu_sha256);
    let app = generation.join(APP_NAME);
    if generation.exists() {
        validate_existing(generation_store_root, rpu)?;
        return Ok((generation, app));
    }

    let pending_root = generation_store_root.join("generation-pending");
    std::fs::create_dir_all(&pending_root)?;
    // Every attempt owns a fresh, unguessable candidate directory.  A crash
    // or an occupied race leaves the losing attempt as non-authoritative
    // evidence; it must never block a later retry for the same RPU digest.
    let pending_generation = pending_root.join(format!("{}.{}", rpu.rpu_sha256, hex_32()?));
    std::fs::create_dir(&pending_generation)?;
    let pending_app = pending_generation.join(APP_NAME);
    let object_id = format!("object:{}", rpu.rpu_sha256);
    let result = (|| -> Result<()> {
        let mut image = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&pending_app)?;
        image.write_all(&rpu.app_bytes)?;
        image.sync_all()?;
        drop(image);
        let receipt = serde_json::to_vec(&GenerationReceiptV3 {
            protocol: 3,
            rpu_sha256: rpu.rpu_sha256.clone(),
            version: rpu.manifest.version.clone(),
            app_sha256: rpu.manifest.app_sha256.clone(),
            minimum_supervisor_protocol: rpu.manifest.minimum_supervisor_protocol,
            app_session_protocol: rpu.manifest.app_session_protocol.clone(),
            minimum_schema: rpu.manifest.minimum_schema,
            maximum_schema: rpu.manifest.maximum_schema,
        })
        .map_err(|error| {
            PortableRuntimeError::new("portable_generation_receipt", error.to_string())
        })?;
        let receipt = provenance::seal(SealDomain::Object, &object_id, &receipt)?;
        let mut receipt_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(pending_generation.join("generation.json"))?;
        receipt_file.write_all(&receipt)?;
        receipt_file.sync_all()?;
        drop(receipt_file);
        validate_selected_app(&pending_app, &rpu.manifest)
    })();
    result?;
    match publish_no_replace(&pending_generation, &generation)? {
        NoReplacePublication::Published => {
            let receipt = std::fs::read(generation.join("generation.json"))?;
            validate_existing(generation_store_root, rpu)?;
            provenance::observe(SealDomain::Object, &object_id, &receipt)?;
        }
        // Both outcomes must validate the canonical destination.  `Occupied`
        // is success only when the immutable winner proves exactly the signed
        // package we were attempting to publish.
        NoReplacePublication::Occupied => {}
    }
    validate_existing(generation_store_root, rpu)?;
    Ok((generation, app))
}

fn validate_existing(generation_store_root: &Path, rpu: &VerifiedRpu) -> Result<()> {
    let stored = load_selected(generation_store_root, &rpu.rpu_sha256)?;
    validate_selected_app(&stored.app, &rpu.manifest)?;
    if stored.version != rpu.manifest.version
        || stored.minimum_supervisor_protocol != rpu.manifest.minimum_supervisor_protocol
        || stored.app_session_protocol != rpu.manifest.app_session_protocol
        || stored.minimum_schema != rpu.manifest.minimum_schema
        || stored.maximum_schema != rpu.manifest.maximum_schema
    {
        return Err(PortableRuntimeError::new(
            "portable_generation_receipt",
            "existing generation did not match the signed RPU descriptor",
        ));
    }
    Ok(())
}

pub fn selected_app_path(generation_store_root: &Path, generation_sha256: &str) -> Result<PathBuf> {
    if generation_sha256.len() != 64 {
        return Err(PortableRuntimeError::new(
            "portable_generation_invalid",
            "generation hash length was invalid",
        ));
    }
    let app = generation_store_root
        .join("objects")
        .join(generation_sha256)
        .join(APP_NAME);
    if !app.is_file() {
        return Err(PortableRuntimeError::new(
            "portable_generation_missing",
            "selected immutable generation was absent",
        ));
    }
    Ok(app)
}

/// Validates the receipt and App hash of a selected immutable object without
/// treating an ambient path or a generation directory listing as authority.
pub fn load_selected(
    generation_store_root: &Path,
    generation_sha256: &str,
) -> Result<StoredGeneration> {
    let (generation_root, app, plaintext) =
        load_generation_parts(generation_store_root, generation_sha256)?;
    let receipt: GenerationReceiptV3 = serde_json::from_slice(&plaintext).map_err(|error| {
        PortableRuntimeError::new("portable_generation_receipt", error.to_string())
    })?;
    validate_current_receipt(&receipt, generation_sha256, &app)?;
    Ok(StoredGeneration {
        generation_root,
        app,
        rpu_sha256: receipt.rpu_sha256,
        app_sha256: receipt.app_sha256,
        version: receipt.version,
        minimum_supervisor_protocol: receipt.minimum_supervisor_protocol,
        app_session_protocol: receipt.app_session_protocol,
        minimum_schema: receipt.minimum_schema,
        maximum_schema: receipt.maximum_schema,
    })
}

/// Reads a selected generation only while the initial reducer decides whether
/// an authenticated v2 record must be superseded by this full package.
pub fn inspect_initial_selection(
    generation_store_root: &Path,
    generation_sha256: &str,
) -> Result<InitialSelectedGeneration> {
    let (generation_root, app, plaintext) =
        load_generation_parts(generation_store_root, generation_sha256)?;
    let protocol: serde_json::Value = serde_json::from_slice(&plaintext).map_err(|error| {
        PortableRuntimeError::new("portable_generation_receipt", error.to_string())
    })?;
    let protocol = protocol
        .get("protocol")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_generation_receipt",
                "selected immutable generation receipt did not declare a protocol",
            )
        })?;
    if protocol == 3 {
        let receipt: GenerationReceiptV3 = serde_json::from_slice(&plaintext).map_err(|error| {
            PortableRuntimeError::new("portable_generation_receipt", error.to_string())
        })?;
        if receipt.minimum_supervisor_protocol == RECEIPT_V3_PREDECESSOR.supervisor_capability
            && receipt.app_session_protocol == RECEIPT_V3_PREDECESSOR.app_session_protocol
        {
            let version = validate_receipt_v3_predecessor(&receipt, generation_sha256, &app)?;
            return Ok(InitialSelectedGeneration::MetadataOnly(
                MetadataOnlyPredecessor { version },
            ));
        }
        validate_current_receipt(&receipt, generation_sha256, &app)?;
        return Ok(InitialSelectedGeneration::Current(StoredGeneration {
            generation_root,
            app,
            rpu_sha256: receipt.rpu_sha256,
            app_sha256: receipt.app_sha256,
            version: receipt.version,
            minimum_supervisor_protocol: receipt.minimum_supervisor_protocol,
            app_session_protocol: receipt.app_session_protocol,
            minimum_schema: receipt.minimum_schema,
            maximum_schema: receipt.maximum_schema,
        }));
    }
    if protocol == 2 {
        let receipt: LegacyGenerationReceiptV2 =
            serde_json::from_slice(&plaintext).map_err(|error| {
                PortableRuntimeError::new("portable_generation_receipt", error.to_string())
            })?;
        let version = validate_released_predecessor(
            &RECEIPT_V2_PREDECESSOR,
            &receipt.version,
            receipt.minimum_schema,
            receipt.maximum_schema,
        )?;
        if receipt.protocol != 2
            || receipt.rpu_sha256 != generation_sha256
            || receipt.app_sha256 != sha256_file(&app)?
        {
            return Err(PortableRuntimeError::new(
                "portable_generation_receipt",
                "metadata-only predecessor receipt was invalid",
            ));
        }
        return Ok(InitialSelectedGeneration::MetadataOnly(
            MetadataOnlyPredecessor { version },
        ));
    }
    Err(PortableRuntimeError::new(
        "portable_generation_receipt",
        "selected immutable generation receipt protocol was not launchable",
    ))
}

fn load_generation_parts(
    generation_store_root: &Path,
    generation_sha256: &str,
) -> Result<(PathBuf, PathBuf, Vec<u8>)> {
    if generation_sha256.len() != 64
        || !generation_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(PortableRuntimeError::new(
            "portable_generation_invalid",
            "selection generation hash was invalid",
        ));
    }
    let app = selected_app_path(generation_store_root, generation_sha256)?;
    let generation_root = app
        .parent()
        .ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_generation_invalid",
                "selected App had no generation root",
            )
        })?
        .to_owned();
    let object_id = format!("object:{generation_sha256}");
    let plaintext = provenance::open(
        SealDomain::Object,
        &object_id,
        &std::fs::read(generation_root.join("generation.json"))?,
    )?;
    Ok((generation_root, app, plaintext))
}

fn validate_current_receipt(
    receipt: &GenerationReceiptV3,
    generation_sha256: &str,
    app: &Path,
) -> Result<()> {
    canonical_version(&receipt.version)?;
    if receipt.protocol != 3
        || receipt.rpu_sha256 != generation_sha256
        || receipt.app_sha256 != sha256_file(app)?
        || receipt.minimum_supervisor_protocol != super::rpu::PORTABLE_SUPERVISOR_CAPABILITY
        || receipt.app_session_protocol != super::rpu::PORTABLE_APP_SESSION_PROTOCOL
        || !schema_range_is_supported(receipt.minimum_schema, receipt.maximum_schema)
    {
        return Err(PortableRuntimeError::new(
            "portable_generation_receipt",
            "selected immutable generation receipt was invalid",
        ));
    }
    Ok(())
}

fn validate_receipt_v3_predecessor(
    receipt: &GenerationReceiptV3,
    generation_sha256: &str,
    app: &Path,
) -> Result<Version> {
    let version = validate_released_predecessor(
        &RECEIPT_V3_PREDECESSOR.released,
        &receipt.version,
        receipt.minimum_schema,
        receipt.maximum_schema,
    )?;
    if receipt.protocol != 3
        || receipt.rpu_sha256 != generation_sha256
        || receipt.app_sha256 != sha256_file(app)?
        || receipt.minimum_supervisor_protocol != RECEIPT_V3_PREDECESSOR.supervisor_capability
        || receipt.app_session_protocol != RECEIPT_V3_PREDECESSOR.app_session_protocol
    {
        return Err(PortableRuntimeError::new(
            "portable_generation_receipt",
            "metadata-only predecessor receipt was invalid",
        ));
    }
    Ok(version)
}

fn validate_released_predecessor(
    contract: &ReleasedPredecessorContract,
    version: &str,
    minimum_schema: u32,
    maximum_schema: u32,
) -> Result<Version> {
    let parsed = canonical_version(version)?;
    if version != contract.version
        || minimum_schema != contract.minimum_schema
        || maximum_schema != contract.maximum_schema
    {
        return Err(PortableRuntimeError::new(
            "portable_generation_receipt",
            "metadata-only predecessor contract did not match a released identity",
        ));
    }
    Ok(parsed)
}
