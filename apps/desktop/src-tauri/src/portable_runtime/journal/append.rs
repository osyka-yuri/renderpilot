use std::{fs, path::Path};

use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    provenance::{self, SealDomain},
    supervisor::authority::SupervisorSessionAuthority,
};

use super::{
    image::{CapturedJournalImage, capture_journal_image},
    mutation::ExactJournalMutation,
    outbox,
    paths::journal_identity,
    protocol::{JOURNAL_PROTOCOL, JournalAppendKind, JournalEntry},
    reader::ValidJournalPrefix,
    transition::{
        RecoveryTransition, append_transition_is_legal, phase_receipt, valid_recovery_transition,
    },
};

pub(super) fn append_normal(
    path: &Path,
    mut next: JournalEntry,
    authority: &SupervisorSessionAuthority,
    generation_store_root: &Path,
) -> Result<JournalEntry> {
    let captured = capture_journal_image(path)?;
    let prefix = captured.valid_prefix()?;
    let writer = authority.transcript_sha256();
    if let Some(last) = prefix.entries.last() {
        if last.writer_session_sha256 != writer {
            return Err(PortableRuntimeError::new(
                "portable_journal_authority",
                "normal append writer did not match the journal tail writer",
            ));
        }
        next.origin_session_sha256 = last.origin_session_sha256.clone();
        next.writer_session_sha256 = writer.to_owned();
        next.predecessor_writer_session_sha256 = last.predecessor_writer_session_sha256.clone();
        next.append_kind = JournalAppendKind::Normal;
    } else {
        next.origin_session_sha256 = writer.to_owned();
        next.writer_session_sha256 = writer.to_owned();
        next.predecessor_writer_session_sha256 = None;
        next.append_kind = JournalAppendKind::Origin;
    }
    append_with_capture(path, next, &captured, prefix, generation_store_root)
}

pub(super) fn append_recovery(
    path: &Path,
    mut next: JournalEntry,
    authority: &SupervisorSessionAuthority,
    transition: RecoveryTransition,
    generation_store_root: &Path,
) -> Result<JournalEntry> {
    let captured = capture_journal_image(path)?;
    let prefix = captured.valid_prefix()?;
    let last = prefix.entries.last().ok_or_else(|| {
        PortableRuntimeError::new(
            "portable_journal_authority",
            "recovery append requires an existing journal tail",
        )
    })?;
    if transition.source_sequence != last.sequence
        || transition.source_entry_sha256 != prefix.head_sha256.as_deref().unwrap_or_default()
        || transition.from_phase != last.phase
        || transition.to_phase != next.phase
        || !valid_recovery_transition(
            transition.action,
            transition.from_phase,
            transition.to_phase,
        )
    {
        return Err(PortableRuntimeError::new(
            "portable_journal_authority",
            "recovery transition did not match the unchanged journal tail",
        ));
    }
    next.origin_session_sha256 = last.origin_session_sha256.clone();
    next.writer_session_sha256 = authority.transcript_sha256().to_owned();
    next.predecessor_writer_session_sha256 =
        if next.writer_session_sha256 == last.writer_session_sha256 {
            last.predecessor_writer_session_sha256.clone()
        } else {
            Some(last.writer_session_sha256.clone())
        };
    next.append_kind = JournalAppendKind::Recovery {
        action: transition.action,
        from_phase: transition.from_phase,
        to_phase: transition.to_phase,
        source_sequence: transition.source_sequence,
        source_entry_sha256: transition.source_entry_sha256,
    };
    append_with_capture(path, next, &captured, prefix, generation_store_root)
}

fn append_with_capture(
    path: &Path,
    mut next: JournalEntry,
    captured: &CapturedJournalImage,
    prefix: &ValidJournalPrefix,
    generation_store_root: &Path,
) -> Result<JournalEntry> {
    let (transaction_id, object_id) = journal_identity(path)?;
    if next.transaction_id != transaction_id {
        return Err(PortableRuntimeError::new(
            "portable_journal_path",
            "journal entry transaction did not match its canonical directory",
        ));
    }
    let last = prefix.entries.last();
    if !append_transition_is_legal(last, &next) {
        return Err(PortableRuntimeError::new(
            "portable_journal_transition",
            "journal phase transition was not legal for protocol v3",
        ));
    }
    if last.is_some_and(|entry| {
        entry.activation_id != next.activation_id
            || entry.selected_generation_sha256 != next.selected_generation_sha256
            || entry.previous_sha256 != next.previous_sha256
    }) {
        return Err(PortableRuntimeError::new(
            "portable_journal_invalid",
            "journal activation identity changed inside one reducer",
        ));
    }
    next.protocol = JOURNAL_PROTOCOL;
    next.sequence = prefix.entries.len() as u64 + 1;
    next.previous_entry_sha256 = prefix.head_sha256.clone();
    next.phase_receipt_sha256 = phase_receipt(&next)?;
    let plaintext = serde_json::to_vec(&next)
        .map_err(|error| PortableRuntimeError::new("portable_journal_encode", error.to_string()))?;
    let sealed = provenance::seal(SealDomain::Journal, &object_id, &plaintext)?;
    if captured.image().byte_len != prefix.valid_len
        || captured.image().sealed_head_sha256 != prefix.head_sha256
        || captured.image().last_sequence != prefix.entries.last().map(|entry| entry.sequence)
    {
        return Err(PortableRuntimeError::new(
            "portable_journal_invalid",
            "captured journal image did not match its semantic prefix",
        ));
    }
    let intent = outbox::new_append_intent(path, captured.image().clone(), next.phase, sealed)?;
    outbox::record_append_intent(generation_store_root, &intent)?;
    let parent = path.parent().ok_or_else(|| {
        PortableRuntimeError::new("portable_journal_path", "journal had no parent")
    })?;
    fs::create_dir_all(parent)?;
    let committed = ExactJournalMutation::append(path, captured, &intent.target_line)?;
    if !outbox::matches_exact_committed_target(&committed, captured, &intent)? {
        return Err(PortableRuntimeError::new(
            "portable_journal_outbox",
            "journal append did not produce its exact byte and semantic target",
        ));
    }
    let intent_key = outbox::append_intent_key(&intent)?;
    outbox::observe_exact_current(
        generation_store_root,
        path,
        &intent.transaction_id,
        &intent_key,
        &committed,
        outbox::ObservationOutcome::Committed,
    )?;
    provenance::observe(SealDomain::Journal, &object_id, &intent.target_line)?;
    Ok(next)
}
