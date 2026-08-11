use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

use super::{
    error::{PortableRuntimeError, Result},
    rpu::{VerifiedRpu, verify_rpu_expected},
    signature::sha256_hex,
};

/// Rejects a signed RPU whose canonical manifest version does not exactly
/// match the authenticated release selection before staging bytes.
pub fn stage_verified_rpu_expected(
    update_root: &Path,
    bytes: &[u8],
    signature: &str,
    expected_version: &str,
) -> Result<(PathBuf, VerifiedRpu)> {
    let verified = verify_rpu_expected(bytes, signature, expected_version)?;
    persist_verified_rpu(update_root, bytes, verified)
}

fn persist_verified_rpu(
    update_root: &Path,
    bytes: &[u8],
    verified: VerifiedRpu,
) -> Result<(PathBuf, VerifiedRpu)> {
    let root = update_root.join("staging");
    std::fs::create_dir_all(&root)?;
    let path = root.join(format!("{}.rpu", sha256_hex(bytes)));
    if !path.exists() {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    if std::fs::read(&path)? != bytes {
        return Err(PortableRuntimeError::new(
            "portable_stage_identity",
            "existing staged RPU differed from verified bytes",
        ));
    }
    Ok((path, verified))
}
