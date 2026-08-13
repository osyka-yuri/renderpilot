use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    provenance::{self, SealDomain},
    random::hex_32,
    win32::file::NoReplacePublication,
};

pub(in crate::portable_runtime::journal) const OPERATION_OUTBOX_PROTOCOL: u16 = 1;

pub(in crate::portable_runtime::journal) fn operation_root(
    generation_store_root: &Path,
) -> PathBuf {
    generation_store_root.join("journal-operations")
}

/// Publishes one authenticated immutable record and validates an occupied leaf
/// against the same plaintext; no outbox retry overwrites existing evidence.
pub(in crate::portable_runtime::journal) fn publish_outbox_record(
    destination: &Path,
    object_id: &str,
    payload: &[u8],
) -> Result<()> {
    match fs::read(destination) {
        Ok(existing) => {
            if provenance::open(SealDomain::Journal, object_id, &existing)? == payload {
                return Ok(());
            }
            return Err(PortableRuntimeError::new(
                "portable_journal_outbox",
                "existing operation record did not match its immutable payload",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let parent = destination.parent().ok_or_else(|| {
        PortableRuntimeError::new("portable_journal_outbox", "operation record had no parent")
    })?;
    let attempts = parent.join("attempts");
    fs::create_dir_all(&attempts)?;
    let sealed = provenance::seal(SealDomain::Journal, object_id, payload)?;
    let attempt = attempts.join(format!("{}.sealed", hex_32()?));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&attempt)?;
    file.write_all(&sealed)?;
    file.sync_all()?;
    drop(file);
    match super::super::super::win32::file::publish_no_replace(&attempt, destination)? {
        NoReplacePublication::Published | NoReplacePublication::Occupied => {}
    }
    let existing = fs::read(destination)?;
    if provenance::open(SealDomain::Journal, object_id, &existing)? != payload {
        return Err(PortableRuntimeError::new(
            "portable_journal_outbox",
            "published operation record did not match its immutable payload",
        ));
    }
    Ok(())
}
