use std::path::Path;

use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    provenance::{self, SealDomain},
    signature::sha256_hex,
};

use super::{paths::journal_identity, protocol::JournalEntry, transition::entry_is_valid_after};

#[derive(Clone, Debug)]
pub(in crate::portable_runtime::journal) struct ValidJournalPrefix {
    pub(in crate::portable_runtime::journal) entries: Vec<JournalEntry>,
    pub(in crate::portable_runtime::journal) valid_len: u64,
    pub(in crate::portable_runtime::journal) head_sha256: Option<String>,
}

pub(in crate::portable_runtime) fn read_entries(path: &Path) -> Result<Vec<JournalEntry>> {
    read_valid_prefix(path).map(|journal| journal.entries)
}

pub(in crate::portable_runtime::journal) fn read_valid_prefix(
    path: &Path,
) -> Result<ValidJournalPrefix> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ValidJournalPrefix {
                entries: Vec::new(),
                valid_len: 0,
                head_sha256: None,
            });
        }
        Err(error) => return Err(error.into()),
    };
    read_valid_prefix_bytes(path, &bytes)
}

pub(in crate::portable_runtime::journal) fn read_valid_prefix_bytes(
    path: &Path,
    bytes: &[u8],
) -> Result<ValidJournalPrefix> {
    let (transaction_id, object_id) = journal_identity(path)?;
    if bytes.last().is_some_and(|byte| *byte != b'\n') {
        return Err(PortableRuntimeError::new(
            "portable_journal_invalid",
            "torn journal tail was retained without mutation",
        ));
    }
    if bytes.is_empty() {
        return Ok(ValidJournalPrefix {
            entries: Vec::new(),
            valid_len: 0,
            head_sha256: None,
        });
    }
    let mut entries = Vec::new();
    let mut previous = None;
    let mut start = 0;
    for end in bytes
        .iter()
        .enumerate()
        .filter_map(|(index, byte)| (*byte == b'\n').then_some(index))
    {
        let line = bytes.get(start..end).ok_or_else(|| {
            PortableRuntimeError::new("portable_journal_invalid", "journal line was invalid")
        })?;
        let plaintext = provenance::open(SealDomain::Journal, &object_id, line)?;
        let entry: JournalEntry = serde_json::from_slice(&plaintext).map_err(|error| {
            PortableRuntimeError::new("portable_journal_invalid", error.to_string())
        })?;
        if !entry_is_valid_after(&entry, &entries, previous.as_deref(), &transaction_id)? {
            return Err(PortableRuntimeError::new(
                "portable_journal_invalid",
                "journal sequence or hash chain was invalid",
            ));
        }
        previous = Some(sha256_hex(line));
        entries.push(entry);
        start = end + 1;
    }
    Ok(ValidJournalPrefix {
        entries,
        valid_len: bytes.len() as u64,
        head_sha256: previous,
    })
}
