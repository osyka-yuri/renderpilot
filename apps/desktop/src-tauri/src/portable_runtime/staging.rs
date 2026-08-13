use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

#[cfg(test)]
use std::sync::{Arc, Mutex};

use super::{
    error::{PortableRuntimeError, Result},
    random::hex_32,
    rpu::{VerifiedRpu, verify_rpu_expected},
    signature::sha256_hex,
    win32::file::publish_no_replace,
};

/// Private proof that the supervisor, not a later filesystem scan, selected
/// one fully verified immutable RPU object for Apply.  It deliberately has no
/// Clone implementation: an accepted Apply consumes this capability.
pub(super) struct StagedVerifiedRpu {
    canonical_path: PathBuf,
    rpu_sha256: String,
    signature: String,
    expected_version: String,
    verified: VerifiedRpu,
    #[cfg(test)]
    reread_trace: Option<Arc<Mutex<Vec<&'static str>>>>,
    #[cfg(test)]
    skip_signature_check_for_test: bool,
}

impl StagedVerifiedRpu {
    /// Reopens the canonical object at the moment Apply consumes the staged
    /// capability.  The digest, complete signature and version binding are
    /// all rechecked; an ambient replacement can never inherit this proof.
    pub(super) fn into_verified(self) -> Result<VerifiedRpu> {
        let Self {
            canonical_path,
            rpu_sha256,
            signature,
            expected_version,
            verified,
            #[cfg(test)]
            reread_trace,
            #[cfg(test)]
            skip_signature_check_for_test,
        } = self;
        #[cfg(test)]
        if let Some(trace) = reread_trace {
            trace
                .lock()
                .expect("test reread trace lock")
                .push("staged_reread");
        }
        let bytes = std::fs::read(canonical_path)?;
        if sha256_hex(&bytes) != rpu_sha256 {
            return Err(PortableRuntimeError::new(
                "portable_stage_identity",
                "canonical staged RPU changed after supervisor verification",
            ));
        }
        #[cfg(test)]
        if skip_signature_check_for_test {
            return Ok(verified);
        }
        let reread = verify_rpu_expected(&bytes, &signature, &expected_version)?;
        if reread.rpu_sha256 != verified.rpu_sha256
            || reread.manifest.version != verified.manifest.version
        {
            return Err(PortableRuntimeError::new(
                "portable_stage_identity",
                "canonical staged RPU no longer matched its accepted capability",
            ));
        }
        Ok(reread)
    }
}

#[cfg(test)]
pub(super) fn staged_for_apply_test(
    canonical_path: PathBuf,
    staged_bytes: &[u8],
    verified: VerifiedRpu,
    reread_trace: Arc<Mutex<Vec<&'static str>>>,
) -> StagedVerifiedRpu {
    StagedVerifiedRpu {
        canonical_path,
        rpu_sha256: sha256_hex(staged_bytes),
        signature: String::new(),
        expected_version: verified.manifest.version.clone(),
        verified,
        reread_trace: Some(reread_trace),
        // Scripted transport tests cannot mint an artifact signature for the
        // production updater key. The production-only path above always
        // reopens, digests, and fully verifies the signed RPU; this fixture
        // isolates the surrounding single-use ordering contract.
        skip_signature_check_for_test: true,
    }
}

/// Rejects a signed RPU whose canonical manifest version does not exactly
/// match the authenticated release selection before staging bytes.  Every
/// retry writes a new retained attempt then races it into one hash-named
/// immutable object; neither a losing attempt nor a previous crash is reused.
pub(super) fn stage_verified_rpu_expected(
    update_root: &Path,
    bytes: &[u8],
    signature: &str,
    expected_version: &str,
) -> Result<StagedVerifiedRpu> {
    let verified = verify_rpu_expected(bytes, signature, expected_version)?;
    persist_verified_rpu(update_root, bytes, signature, expected_version, &verified)
}

fn persist_verified_rpu(
    update_root: &Path,
    bytes: &[u8],
    signature: &str,
    expected_version: &str,
    verified: &VerifiedRpu,
) -> Result<StagedVerifiedRpu> {
    let staging_root = update_root.join("staging");
    let objects = staging_root.join("objects");
    let attempts = staging_root.join("attempts");
    std::fs::create_dir_all(&objects)?;
    std::fs::create_dir_all(&attempts)?;

    let hash = verified.rpu_sha256.as_str();
    let attempt = attempts.join(format!("{hash}.{}.rpu", hex_32()?));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&attempt)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);

    let canonical_path = objects.join(format!("{hash}.rpu"));
    let _publication = publish_no_replace(&attempt, &canonical_path)?;
    // `Occupied` is a successful publication result only after the immutable
    // winner is reread and independently signature/version verified.
    let stored = std::fs::read(&canonical_path)?;
    if sha256_hex(&stored) != hash {
        return Err(PortableRuntimeError::new(
            "portable_stage_identity",
            "canonical staged RPU did not match the verified digest",
        ));
    }
    let verified_stored = verify_rpu_expected(&stored, signature, expected_version)?;
    if verified_stored.rpu_sha256 != hash {
        return Err(PortableRuntimeError::new(
            "portable_stage_identity",
            "canonical staged RPU did not match the verified object identity",
        ));
    }
    Ok(StagedVerifiedRpu {
        canonical_path,
        rpu_sha256: verified_stored.rpu_sha256.clone(),
        signature: signature.to_owned(),
        expected_version: expected_version.to_owned(),
        verified: verified_stored,
        #[cfg(test)]
        reread_trace: None,
        #[cfg(test)]
        skip_signature_check_for_test: false,
    })
}
