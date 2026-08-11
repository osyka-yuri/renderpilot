use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{
    error::{PortableRuntimeError, Result},
    health::validate_selected_app,
    provenance::{self, SealDomain},
    rpu::{VerifiedRpu, canonical_version},
    signature::sha256_file,
    win32::file::{NoReplacePublication, publish_no_replace},
};

const APP_NAME: &str = "renderpilot-app.exe";

#[derive(Deserialize, Serialize)]
struct GenerationReceipt {
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
    pub version: String,
    pub minimum_schema: u32,
    pub maximum_schema: u32,
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
    let pending_generation = pending_root.join(&rpu.rpu_sha256);
    require_absent_pending_generation(&pending_generation)?;
    std::fs::create_dir(&pending_generation)?;
    let pending_app = pending_generation.join(APP_NAME);
    let object_id = format!("object:{}", rpu.rpu_sha256);
    provenance::intent(
        SealDomain::Object,
        &object_id,
        b"publish-immutable-generation",
    )?;
    let result = (|| -> Result<()> {
        let mut image = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&pending_app)?;
        image.write_all(&rpu.app_bytes)?;
        image.sync_all()?;
        drop(image);
        let receipt = serde_json::to_vec(&GenerationReceipt {
            protocol: 2,
            rpu_sha256: rpu.rpu_sha256.clone(),
            version: rpu.manifest.version.clone(),
            app_sha256: rpu.manifest.app_sha256.clone(),
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
            provenance::observe(
                SealDomain::Object,
                &object_id,
                &std::fs::read(generation.join("generation.json"))?,
            )?;
        }
        NoReplacePublication::Occupied => {
            return Err(PortableRuntimeError::new(
                "portable_generation_pending",
                "occupied immutable generation left its pending nonce tree retained",
            ));
        }
    }
    validate_existing(generation_store_root, rpu)?;
    Ok((generation, app))
}

fn validate_existing(generation_store_root: &Path, rpu: &VerifiedRpu) -> Result<()> {
    let stored = load_selected(generation_store_root, &rpu.rpu_sha256)?;
    validate_selected_app(&stored.app, &rpu.manifest)?;
    if stored.version != rpu.manifest.version
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

fn require_absent_pending_generation(path: &Path) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let _ = metadata;
    Err(PortableRuntimeError::new(
        "portable_generation_pending",
        "existing pending generation was retained; no raw-path cleanup is authorized",
    ))
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
    let receipt: GenerationReceipt = serde_json::from_slice(&provenance::open(
        SealDomain::Object,
        &object_id,
        &std::fs::read(generation_root.join("generation.json"))?,
    )?)
    .map_err(|error| PortableRuntimeError::new("portable_generation_receipt", error.to_string()))?;
    canonical_version(&receipt.version)?;
    if receipt.protocol != 2
        || receipt.rpu_sha256 != generation_sha256
        || receipt.app_sha256 != sha256_file(&app)?
        || receipt.minimum_schema != super::rpu::MINIMUM_SCHEMA
        || receipt.maximum_schema != super::rpu::MAXIMUM_SCHEMA
    {
        return Err(PortableRuntimeError::new(
            "portable_generation_receipt",
            "selected immutable generation receipt was invalid",
        ));
    }
    Ok(StoredGeneration {
        generation_root,
        app,
        rpu_sha256: receipt.rpu_sha256,
        version: receipt.version,
        minimum_schema: receipt.minimum_schema,
        maximum_schema: receipt.maximum_schema,
    })
}
