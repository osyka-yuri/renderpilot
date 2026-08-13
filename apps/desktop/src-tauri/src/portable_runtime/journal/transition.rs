use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    signature::sha256_hex,
};

use super::{
    paths::is_digest,
    protocol::{JOURNAL_PROTOCOL, JournalAppendKind, JournalEntry, JournalPhase, RecoveryAction},
};

#[derive(Clone, Debug)]
pub(in crate::portable_runtime) struct RecoveryTransition {
    pub(in crate::portable_runtime::journal) action: RecoveryAction,
    pub(in crate::portable_runtime::journal) from_phase: JournalPhase,
    pub(in crate::portable_runtime::journal) to_phase: JournalPhase,
    pub(in crate::portable_runtime::journal) source_sequence: u64,
    pub(in crate::portable_runtime::journal) source_entry_sha256: String,
}

impl RecoveryTransition {
    pub fn action(&self) -> RecoveryAction {
        self.action
    }

    pub fn target_phase(&self) -> JournalPhase {
        self.to_phase
    }
}

pub(in crate::portable_runtime::journal) fn entry_is_valid_after(
    entry: &JournalEntry,
    prior: &[JournalEntry],
    previous_entry_sha256: Option<&str>,
    transaction_id: &str,
) -> Result<bool> {
    let previous = prior.last();
    Ok(entry.protocol == JOURNAL_PROTOCOL
        && entry.sequence == prior.len() as u64 + 1
        && entry.transaction_id == transaction_id
        && entry.previous_entry_sha256.as_deref() == previous_entry_sha256
        && is_digest(&entry.activation_id)
        && is_digest(&entry.origin_session_sha256)
        && is_digest(&entry.writer_session_sha256)
        && is_digest(&entry.transcript_sha256)
        && entry
            .predecessor_writer_session_sha256
            .as_deref()
            .is_none_or(is_digest)
        && entry
            .selection_record_sha256
            .as_deref()
            .is_none_or(is_digest)
        && valid_session_lineage(entry, previous, previous_entry_sha256)
        && (!phase_requires_selection(entry.phase)
            || entry
                .selection_record_sha256
                .as_deref()
                .is_some_and(is_digest))
        && entry.phase_receipt_sha256 == phase_receipt(entry)?
        && !prior.first().is_some_and(|first| {
            first.activation_id != entry.activation_id
                || first.selected_generation_sha256 != entry.selected_generation_sha256
                || first.previous_sha256 != entry.previous_sha256
        })
        && append_transition_is_legal(previous, entry))
}

/// The only protocol-v3 transition predicate. Appends use it before mutation;
/// reader and durable-intent validation use it through `entry_is_valid_after`.
pub(in crate::portable_runtime::journal) fn append_transition_is_legal(
    previous: Option<&JournalEntry>,
    next: &JournalEntry,
) -> bool {
    legal_transition(previous.map(|entry| entry.phase), next.phase)
        || matches!(
            (&next.append_kind, previous),
            (JournalAppendKind::Recovery { action: RecoveryAction::FinalizeTerminalReceipt, from_phase, to_phase, .. }, Some(previous))
                if previous.phase == *from_phase
                    && *from_phase == *to_phase
                    && matches!(to_phase, JournalPhase::CommitObserved | JournalPhase::RolledBack)
        )
        || matches!(
            (&next.append_kind, previous),
            (JournalAppendKind::Recovery { action: RecoveryAction::NeedsManualRecovery, from_phase, to_phase, .. }, Some(previous))
                if previous.phase == JournalPhase::NeedsRecovery
                    && *from_phase == JournalPhase::NeedsRecovery
                    && *to_phase == JournalPhase::NeedsRecovery
        )
}

pub(in crate::portable_runtime::journal) fn valid_recovery_transition(
    action: RecoveryAction,
    from_phase: JournalPhase,
    to_phase: JournalPhase,
) -> bool {
    match action {
        RecoveryAction::RollBackPreCommit => {
            (from_phase == JournalPhase::RollingBack && to_phase == JournalPhase::RolledBack)
                || (from_phase.permits_rollback() && to_phase == JournalPhase::RollingBack)
        }
        RecoveryAction::RollForwardCommitted => {
            from_phase == JournalPhase::Committed && to_phase == JournalPhase::CommitObserved
        }
        RecoveryAction::FinalizeTerminalReceipt => {
            from_phase == to_phase
                && matches!(
                    to_phase,
                    JournalPhase::CommitObserved | JournalPhase::RolledBack
                )
        }
        RecoveryAction::NeedsManualRecovery => {
            from_phase == JournalPhase::NeedsRecovery && to_phase == JournalPhase::NeedsRecovery
        }
    }
}

pub(in crate::portable_runtime::journal) fn phase_receipt(entry: &JournalEntry) -> Result<String> {
    let mut unsigned = entry.clone();
    unsigned.phase_receipt_sha256.clear();
    serde_json::to_vec(&unsigned)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| PortableRuntimeError::new("portable_journal_encode", error.to_string()))
}

fn phase_requires_selection(phase: JournalPhase) -> bool {
    matches!(
        phase,
        JournalPhase::SelectionCommitted
            | JournalPhase::PermitSent
            | JournalPhase::ActivationAcknowledged
            | JournalPhase::Committed
            | JournalPhase::CommitObserved
    )
}

fn valid_session_lineage(
    entry: &JournalEntry,
    previous: Option<&JournalEntry>,
    previous_entry_sha256: Option<&str>,
) -> bool {
    match (previous, &entry.append_kind) {
        (None, JournalAppendKind::Origin) => {
            entry.origin_session_sha256 == entry.writer_session_sha256
                && entry.predecessor_writer_session_sha256.is_none()
        }
        (Some(previous), JournalAppendKind::Normal) => {
            entry.origin_session_sha256 == previous.origin_session_sha256
                && entry.writer_session_sha256 == previous.writer_session_sha256
                && entry.predecessor_writer_session_sha256
                    == previous.predecessor_writer_session_sha256
        }
        (
            Some(previous),
            JournalAppendKind::Recovery {
                action,
                from_phase,
                to_phase,
                source_sequence,
                source_entry_sha256,
            },
        ) => {
            entry.origin_session_sha256 == previous.origin_session_sha256
                && entry.predecessor_writer_session_sha256
                    == if entry.writer_session_sha256 == previous.writer_session_sha256 {
                        previous.predecessor_writer_session_sha256.clone()
                    } else {
                        Some(previous.writer_session_sha256.clone())
                    }
                && *from_phase == previous.phase
                && *to_phase == entry.phase
                && *source_sequence == previous.sequence
                && Some(source_entry_sha256.as_str()) == previous_entry_sha256
                && valid_recovery_transition(*action, *from_phase, *to_phase)
        }
        _ => false,
    }
}

fn legal_transition(previous: Option<JournalPhase>, next: JournalPhase) -> bool {
    use JournalPhase::*;
    matches!(
        (previous, next),
        (None, Prepared)
            | (
                Some(Prepared),
                GenerationPublished | RollingBack | NeedsRecovery
            )
            | (
                Some(GenerationPublished),
                OldAppQuiesced | TrialSpawned | RollingBack | NeedsRecovery
            )
            | (
                Some(OldAppQuiesced),
                TrialSpawned | RollingBack | NeedsRecovery
            )
            | (Some(TrialSpawned), TrialReady | RollingBack | NeedsRecovery)
            | (
                Some(TrialReady),
                SnapshotCommitted | MigrationCommitted | RollingBack | NeedsRecovery
            )
            | (
                Some(SnapshotCommitted),
                MigrationCommitted | RollingBack | NeedsRecovery
            )
            | (
                Some(MigrationCommitted),
                SelectionCommitted | RollingBack | NeedsRecovery
            )
            | (
                Some(SelectionCommitted),
                PermitSent | RollingBack | NeedsRecovery
            )
            | (
                Some(PermitSent),
                ActivationAcknowledged | RollingBack | NeedsRecovery
            )
            | (
                Some(ActivationAcknowledged),
                Committed | RollingBack | NeedsRecovery
            )
            | (Some(Committed), CommitObserved)
            | (Some(RollingBack), RolledBack | NeedsRecovery)
    )
}
