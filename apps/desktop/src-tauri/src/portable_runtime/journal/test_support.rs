use std::{fs, path::Path};

use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    provenance::{self, SealDomain},
    supervisor::authority::SupervisorSessionAuthority,
};

use super::{
    image,
    mutation::ExactJournalMutation,
    outbox,
    paths::{is_digest, journal_identity},
    protocol::{JOURNAL_PROTOCOL, JournalAppendKind, JournalEntry, JournalPhase},
    transition::{RecoveryTransition, phase_receipt},
};

/// Ordinary test appends use the same durable facade as production, with an
/// explicit isolated generation-store root supplied by the caller.
pub(in crate::portable_runtime) fn append_normal(
    path: &Path,
    generation_store_root: &Path,
    next: JournalEntry,
    authority: &SupervisorSessionAuthority,
) -> Result<JournalEntry> {
    super::append_normal_with_outbox(path, generation_store_root, next, authority)
}

/// Builds the one intentionally malformed authenticated source fixture needed
/// by protocol validation tests. It is confined to cfg(test) support and does
/// not relax production capture-B semantic proof or expose an application hook.
pub(in crate::portable_runtime) fn append_malformed_selection_digest_fixture(
    path: &Path,
    mut next: JournalEntry,
    authority: &SupervisorSessionAuthority,
) -> Result<JournalEntry> {
    if path.exists()
        || next.phase != JournalPhase::Prepared
        || !matches!(next.append_kind, JournalAppendKind::Origin)
        || next
            .selection_record_sha256
            .as_deref()
            .is_none_or(is_digest)
    {
        return Err(PortableRuntimeError::new(
            "portable_journal_invalid",
            "malformed-selection fixture requires absent Prepared Origin evidence",
        ));
    }
    let (transaction_id, object_id) = journal_identity(path)?;
    if next.transaction_id != transaction_id {
        return Err(PortableRuntimeError::new(
            "portable_journal_path",
            "invalid-selection fixture transaction did not match its journal",
        ));
    }
    let writer = authority.transcript_sha256().to_owned();
    next.protocol = JOURNAL_PROTOCOL;
    next.sequence = 1;
    next.origin_session_sha256 = writer.clone();
    next.writer_session_sha256 = writer;
    next.predecessor_writer_session_sha256 = None;
    next.previous_entry_sha256 = None;
    next.phase_receipt_sha256 = phase_receipt(&next)?;
    let plaintext = serde_json::to_vec(&next)
        .map_err(|error| PortableRuntimeError::new("portable_journal_encode", error.to_string()))?;
    let sealed = provenance::seal(SealDomain::Journal, &object_id, &plaintext)?;
    let parent = path.parent().ok_or_else(|| {
        PortableRuntimeError::new("portable_journal_path", "fixture journal had no parent")
    })?;
    fs::create_dir_all(parent)?;
    let captured = image::capture_journal_image(path)?;
    let _ = ExactJournalMutation::append(path, &captured, &sealed)?;
    Ok(next)
}

pub(in crate::portable_runtime) fn append_recovery(
    path: &Path,
    generation_store_root: &Path,
    next: JournalEntry,
    authority: &SupervisorSessionAuthority,
    transition: RecoveryTransition,
) -> Result<JournalEntry> {
    super::append_recovery_with_outbox(path, generation_store_root, next, authority, transition)
}

/// Narrow test seam for the exact pure suffix classifier that production
/// reconciliation uses before it records a repair intent.
pub(in crate::portable_runtime) fn intended_prefix_len(
    base: &[u8],
    target_line: &[u8],
    candidate: &[u8],
) -> Option<usize> {
    outbox::target_prefix_len_for_test(base, target_line, candidate)
}

/// Records a second authenticated origin intent for an ambiguity fixture. The
/// caller supplies a sealed line for the same canonical journal identity.
pub(in crate::portable_runtime) fn record_origin_append_intent(
    generation_store_root: &Path,
    journal: &Path,
    target_line: Vec<u8>,
) -> Result<()> {
    let intent = outbox::new_append_intent(
        journal,
        image::absent_image(),
        JournalPhase::Prepared,
        target_line,
    )?;
    outbox::record_append_intent(generation_store_root, &intent)
}
