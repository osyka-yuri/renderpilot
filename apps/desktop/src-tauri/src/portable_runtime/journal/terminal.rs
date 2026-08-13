use std::path::Path;

use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    provenance::{self, SealDomain},
    publication::publish_bytes_no_replace,
    signature::sha256_hex,
    supervisor::authority::SupervisorSessionAuthority,
    win32::file::NoReplacePublication,
};

use super::{
    paths::is_digest,
    protocol::{JOURNAL_PROTOCOL, JournalAppendKind, JournalPhase, TerminalReceiptV3},
    reader::read_valid_prefix,
};

/// Creates one immutable, hash-bound terminal receipt. Existing receipts are
/// revalidated byte-for-byte; recovery never overwrites terminal evidence.
pub(in crate::portable_runtime) fn write_terminal_receipt(
    path: &Path,
    authority: &SupervisorSessionAuthority,
) -> Result<()> {
    let prefix = read_valid_prefix(path)?;
    let last = prefix.entries.last().ok_or_else(|| {
        PortableRuntimeError::new("portable_terminal_receipt", "terminal journal was empty")
    })?;
    if !matches!(
        last.phase,
        JournalPhase::CommitObserved | JournalPhase::RolledBack
    ) {
        return Err(PortableRuntimeError::new(
            "portable_terminal_receipt",
            "journal was not terminal",
        ));
    }
    let journal_head_sha256 = prefix.head_sha256.clone().ok_or_else(|| {
        PortableRuntimeError::new(
            "portable_terminal_receipt",
            "terminal journal lacked a sealed head",
        )
    })?;
    let mut receipt = TerminalReceiptV3 {
        protocol: JOURNAL_PROTOCOL,
        phase: last.phase,
        transaction_id: last.transaction_id.clone(),
        selected_generation_sha256: last.selected_generation_sha256.clone(),
        selection_record_sha256: last.selection_record_sha256.clone(),
        journal_head_sha256,
        origin_session_sha256: last.origin_session_sha256.clone(),
        finalizer_session_sha256: last.writer_session_sha256.clone(),
        predecessor_writer_session_sha256: last.predecessor_writer_session_sha256.clone(),
        terminal_journal_sequence: last.sequence,
        terminal_journal_transcript_sha256: last.transcript_sha256.clone(),
        recovery_action: match &last.append_kind {
            JournalAppendKind::Recovery { action, .. } => Some(*action),
            JournalAppendKind::Origin | JournalAppendKind::Normal => None,
        },
        receipt_sha256: String::new(),
    };
    receipt.receipt_sha256 = sha256_hex(&serde_json::to_vec(&receipt).map_err(|error| {
        PortableRuntimeError::new("portable_terminal_receipt", error.to_string())
    })?);
    let plaintext = serde_json::to_vec(&receipt).map_err(|error| {
        PortableRuntimeError::new("portable_terminal_receipt", error.to_string())
    })?;
    let object_id = format!("terminal:{}", last.transaction_id);
    let receipt_path = path
        .parent()
        .ok_or_else(|| {
            PortableRuntimeError::new("portable_terminal_receipt", "journal had no parent")
        })?
        .join("terminal-receipt.json");
    match std::fs::read(&receipt_path) {
        Ok(existing) => {
            validate_existing_terminal_receipt(&existing, &object_id, &receipt)?;
            return provenance::observe(SealDomain::Terminal, &object_id, &existing);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    if last.writer_session_sha256 != authority.transcript_sha256() {
        return Err(PortableRuntimeError::new(
            "portable_terminal_receipt",
            "terminal receipt finalizer did not match the journal tail writer",
        ));
    }
    let bytes = provenance::seal(SealDomain::Terminal, &object_id, &plaintext)?;
    let pending_root = receipt_path
        .parent()
        .ok_or_else(|| {
            PortableRuntimeError::new("portable_terminal_receipt", "receipt had no parent")
        })?
        .join("publication-pending");
    let observed = match publish_bytes_no_replace(&receipt_path, &pending_root, &bytes)? {
        NoReplacePublication::Published => bytes,
        NoReplacePublication::Occupied => {
            let existing = std::fs::read(&receipt_path)?;
            validate_existing_terminal_receipt(&existing, &object_id, &receipt)?;
            existing
        }
    };
    provenance::observe(SealDomain::Terminal, &object_id, &observed)
}

pub(in crate::portable_runtime) fn terminal_receipt_exists(path: &Path) -> Result<bool> {
    Ok(path
        .parent()
        .ok_or_else(|| {
            PortableRuntimeError::new("portable_terminal_receipt", "journal had no parent")
        })?
        .join("terminal-receipt.json")
        .exists())
}

fn validate_existing_terminal_receipt(
    bytes: &[u8],
    object_id: &str,
    expected: &TerminalReceiptV3,
) -> Result<()> {
    let existing: TerminalReceiptV3 =
        serde_json::from_slice(&provenance::open(SealDomain::Terminal, object_id, bytes)?)
            .map_err(|error| {
                PortableRuntimeError::new("portable_terminal_receipt", error.to_string())
            })?;
    let digest = existing.receipt_sha256.clone();
    let mut unsigned = existing.clone();
    unsigned.receipt_sha256.clear();
    let valid_self_hash = digest
        == sha256_hex(&serde_json::to_vec(&unsigned).map_err(|error| {
            PortableRuntimeError::new("portable_terminal_receipt", error.to_string())
        })?);
    if !valid_self_hash || !valid_terminal_receipt_domains(&existing) || existing != *expected {
        return Err(PortableRuntimeError::new(
            "portable_terminal_receipt",
            "terminal receipt did not match immutable journal head",
        ));
    }
    Ok(())
}

fn valid_terminal_receipt_domains(receipt: &TerminalReceiptV3) -> bool {
    receipt.protocol == JOURNAL_PROTOCOL
        && matches!(
            receipt.phase,
            JournalPhase::CommitObserved | JournalPhase::RolledBack
        )
        && is_digest(&receipt.transaction_id)
        && is_digest(&receipt.selected_generation_sha256)
        && receipt
            .selection_record_sha256
            .as_deref()
            .is_none_or(is_digest)
        && is_digest(&receipt.journal_head_sha256)
        && is_digest(&receipt.origin_session_sha256)
        && is_digest(&receipt.finalizer_session_sha256)
        && receipt
            .predecessor_writer_session_sha256
            .as_deref()
            .is_none_or(is_digest)
        && is_digest(&receipt.terminal_journal_transcript_sha256)
        && is_digest(&receipt.receipt_sha256)
}
