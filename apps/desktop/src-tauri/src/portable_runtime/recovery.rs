use std::path::Path;

use super::{
    app_protocol::committed_sequence_for_selection,
    cleanup::{cleanup_snapshot_after_terminal, cleanup_staging_after_terminal},
    error::{PortableRuntimeError, Result},
    journal::{
        JournalAppendKind, JournalEntry, JournalPhase, aborted_before_origin,
        append_recovery_with_outbox, journal_path, plan_recovery, read_entries,
        reconcile_operation_outbox, terminal_receipt_exists, write_terminal_receipt,
    },
    selection::{
        SelectionRecord, SelectionState, SelectionTip, append_cleared,
        append_compensating_selected, read_selection, require_canonical_normal_selection,
    },
    snapshot::{load_committed, restore},
};

use super::supervisor::authority::SupervisorSessionAuthority;

pub use super::journal::RecoveryAction;

/// Reduces only durable journal evidence. Timeout, PID, completion, and job
/// counters are deliberately not inputs to the authority decision.
pub fn recovery_action(journal_path: &Path) -> Result<RecoveryAction> {
    Ok(
        plan_recovery(journal_path)?.map_or(RecoveryAction::RollBackPreCommit, |transition| {
            transition.action()
        }),
    )
}

/// Completes every prior durable transaction before a fresh App is created.
/// Pre-Commit restores only the supervisor snapshot; at/after Committed it
/// never restores the catalog or selection and records a permit replay marker
/// for the next authenticated activation cycle.
pub(in crate::portable_runtime) fn recover_prior_transactions(
    generation_store_root: &Path,
    update_root: &Path,
    catalog: &Path,
    selection_root: &Path,
    authority: &SupervisorSessionAuthority,
) -> Result<()> {
    reconcile_operation_outbox(generation_store_root, update_root)?;
    let transactions = update_root.join("transactions");
    let entries = match std::fs::read_dir(&transactions) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() || entry.file_type()?.is_symlink() {
            return Err(PortableRuntimeError::new(
                "portable_recovery_namespace",
                "transaction namespace contained an unknown leaf",
            ));
        }
        let transaction = entry.file_name().to_string_lossy().to_string();
        if transaction.len() != 64 || !transaction.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(PortableRuntimeError::new(
                "portable_recovery_namespace",
                "transaction namespace contained a non-nonce directory retained without cleanup",
            ));
        }
        let journal = journal_path(update_root, &transaction);
        if aborted_before_origin(generation_store_root, &journal)? {
            // An authenticated origin intent proves this is the exact
            // pre-Prepared abort shape. Recovery neither fabricates nor
            // deletes a journal for it.
            continue;
        }
        if !journal.exists() {
            return Err(PortableRuntimeError::new(
                "portable_recovery_namespace",
                "transaction directory lacked its immutable journal",
            ));
        }
        match recovery_action(&journal)? {
            RecoveryAction::RollBackPreCommit => rollback_precommit(
                &journal,
                generation_store_root,
                catalog,
                selection_root,
                authority,
            )?,
            RecoveryAction::RollForwardCommitted => {
                roll_forward_committed(&journal, generation_store_root, selection_root, authority)?
            }
            RecoveryAction::FinalizeTerminalReceipt => complete_terminal_receipt(
                &journal,
                generation_store_root,
                selection_root,
                authority,
            )?,
            RecoveryAction::NeedsManualRecovery => {
                return Err(PortableRuntimeError::new(
                    "portable_recovery_manual",
                    "prior transaction requires manual recovery",
                ));
            }
        }
        cleanup_staging_after_terminal(&journal, &update_root.join("staging"))?;
        cleanup_snapshot_after_terminal(&journal)?;
    }
    Ok(())
}

fn complete_terminal_receipt(
    journal: &Path,
    generation_store_root: &Path,
    selection_root: &Path,
    authority: &SupervisorSessionAuthority,
) -> Result<()> {
    let entries = read_entries(journal)?;
    let last = entries.last().cloned().ok_or_else(|| {
        PortableRuntimeError::new("portable_recovery_invalid", "terminal journal was empty")
    })?;
    if !matches!(
        last.phase,
        JournalPhase::CommitObserved | JournalPhase::RolledBack
    ) {
        return Err(PortableRuntimeError::new(
            "portable_recovery_invalid",
            "terminal recovery did not end in a final phase",
        ));
    }
    if terminal_receipt_exists(journal)? {
        // A retained receipt is immutable evidence for this already-completed
        // transaction. `write_terminal_receipt` validates it byte-for-byte
        // against this journal tail without consulting a later selection tip.
        return write_terminal_receipt(journal, authority);
    }
    if last.phase == JournalPhase::CommitObserved {
        // No receipt means this is the immediate incomplete finalization, so
        // its selection must still be the canonical transaction-bound tip.
        validate_committed_selection(&entries, selection_root)?;
    }
    append_recovery_phase(
        journal,
        &last,
        last.phase,
        "terminal-receipt-finalization",
        last.selection_record_sha256.clone(),
        generation_store_root,
        authority,
    )?;
    write_terminal_receipt(journal, authority)
}

fn rollback_precommit(
    journal: &Path,
    generation_store_root: &Path,
    catalog: &Path,
    selection_root: &Path,
    authority: &SupervisorSessionAuthority,
) -> Result<()> {
    let entries = read_entries(journal)?;
    let last = entries.last().ok_or_else(|| {
        PortableRuntimeError::new("portable_recovery_invalid", "rollback journal was empty")
    })?;
    // The source normal journal and the entire selection chain are validated
    // before recovery writes `RollingBack`. A mismatched or later selection tip
    // therefore remains fail-closed rather than receiving a false terminal
    // rollback receipt.
    let rollback_selection_hash = plan_and_compensate_selection(&entries, selection_root)?;
    if last.phase != JournalPhase::RollingBack {
        append_recovery_phase(
            journal,
            last,
            JournalPhase::RollingBack,
            "precommit-rollback",
            rollback_selection_hash.clone(),
            generation_store_root,
            authority,
        )?;
    }

    if let Some(snapshot_entry) = entries
        .iter()
        .find(|entry| entry.phase == JournalPhase::SnapshotCommitted)
    {
        let transaction_root = journal.parent().ok_or_else(|| {
            PortableRuntimeError::new("portable_recovery_invalid", "journal had no parent")
        })?;
        let receipt = load_committed(
            transaction_root,
            &snapshot_entry.transaction_id,
            &snapshot_entry.transcript_sha256,
        )?;
        restore(&receipt, catalog)?;
    }

    let entries = read_entries(journal)?;
    let last = entries.last().ok_or_else(|| {
        PortableRuntimeError::new("portable_recovery_invalid", "rollback journal was empty")
    })?;
    append_recovery_phase(
        journal,
        last,
        JournalPhase::RolledBack,
        "precommit-rollback-complete",
        rollback_selection_hash,
        generation_store_root,
        authority,
    )?;
    write_terminal_receipt(journal, authority)?;
    Ok(())
}

#[derive(Clone)]
struct SelectionRollbackTarget {
    transaction_id: String,
    journal_sequence: u64,
    selected_generation_sha256: String,
    previous_generation_sha256: Option<String>,
    expected_selection_record_sha256: Option<String>,
}

fn plan_and_compensate_selection(
    entries: &[JournalEntry],
    selection_root: &Path,
) -> Result<Option<String>> {
    let source_normal = entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.append_kind,
                JournalAppendKind::Origin | JournalAppendKind::Normal
            )
        })
        .collect::<Vec<_>>();
    let Some(target) = selection_rollback_target(&source_normal)? else {
        return Ok(None);
    };
    let chain = read_selection(selection_root)?;
    let candidate_index = chain.iter().enumerate().find_map(|(index, tip)| {
        candidate_matches_target(tip, index, &chain, &target).then_some(index)
    });
    let Some(candidate_index) = candidate_index else {
        if target.expected_selection_record_sha256.is_none()
            && !chain.iter().any(|tip| {
                matches!(
                    &tip.record,
                    SelectionRecord::V3(record)
                        if record.journal_transaction_id == target.transaction_id
                            && record.journal_sequence == target.journal_sequence
                )
            })
        {
            return Ok(chain.last().map(|tip| tip.record_sha256.clone()));
        }
        return Err(PortableRuntimeError::new(
            "portable_recovery_selection",
            "selection chain lacked the journal-bound failed candidate",
        ));
    };
    let candidate = &chain[candidate_index];
    match chain.get(candidate_index + 1) {
        None => append_selection_compensation(selection_root, candidate, &target),
        Some(compensation)
            if candidate_index + 2 == chain.len()
                && compensation_matches_target(compensation, candidate, &target) =>
        {
            Ok(Some(compensation.record_sha256.clone()))
        }
        Some(_) => Err(PortableRuntimeError::new(
            "portable_recovery_selection",
            "selection chain contained later or mismatched durable authority",
        )),
    }
}

fn selection_rollback_target(
    source_normal: &[&JournalEntry],
) -> Result<Option<SelectionRollbackTarget>> {
    if let Some(selection) = source_normal
        .iter()
        .copied()
        .find(|entry| entry.phase == JournalPhase::SelectionCommitted)
    {
        let expected_selection_record_sha256 =
            selection.selection_record_sha256.clone().ok_or_else(|| {
                PortableRuntimeError::new(
                    "portable_recovery_selection",
                    "SelectionCommitted lacked its immutable record hash",
                )
            })?;
        return Ok(Some(SelectionRollbackTarget {
            transaction_id: selection.transaction_id.clone(),
            journal_sequence: selection.sequence,
            selected_generation_sha256: selection.selected_generation_sha256.clone(),
            previous_generation_sha256: selection.previous_sha256.clone(),
            expected_selection_record_sha256: Some(expected_selection_record_sha256),
        }));
    }
    let Some(last) = source_normal.last().copied() else {
        return Err(PortableRuntimeError::new(
            "portable_recovery_selection",
            "rollback journal lacked source normal evidence",
        ));
    };
    if last.phase != JournalPhase::MigrationCommitted {
        return Ok(None);
    }
    Ok(Some(SelectionRollbackTarget {
        transaction_id: last.transaction_id.clone(),
        journal_sequence: last.sequence + 1,
        selected_generation_sha256: last.selected_generation_sha256.clone(),
        previous_generation_sha256: last.previous_sha256.clone(),
        expected_selection_record_sha256: None,
    }))
}

fn candidate_matches_target(
    candidate: &SelectionTip,
    index: usize,
    chain: &[SelectionTip],
    target: &SelectionRollbackTarget,
) -> bool {
    if target
        .expected_selection_record_sha256
        .as_deref()
        .is_some_and(|hash| hash != candidate.record_sha256)
        || candidate.selected_generation_sha256()
            != Some(target.selected_generation_sha256.as_str())
    {
        return false;
    }
    match &candidate.record {
        SelectionRecord::V2(record) => {
            target.expected_selection_record_sha256.is_some()
                && record.journal_sequence == target.journal_sequence
                && candidate_predecessor_matches(index, chain, target)
        }
        SelectionRecord::V3(record) => {
            record.journal_transaction_id == target.transaction_id
                && record.journal_sequence == target.journal_sequence
                && record.compensates_selection_record_sha256.is_none()
                && candidate_predecessor_matches(index, chain, target)
        }
    }
}

fn candidate_predecessor_matches(
    index: usize,
    chain: &[SelectionTip],
    target: &SelectionRollbackTarget,
) -> bool {
    match (&target.previous_generation_sha256, index.checked_sub(1)) {
        (None, None) => true,
        (None, Some(previous_index)) => matches!(
            &chain[previous_index].record,
            SelectionRecord::V3(record) if record.state == SelectionState::Cleared
        ),
        (Some(previous_generation), Some(previous_index)) => {
            chain[previous_index].selected_generation_sha256() == Some(previous_generation.as_str())
        }
        _ => false,
    }
}

fn compensation_matches_target(
    compensation: &SelectionTip,
    candidate: &SelectionTip,
    target: &SelectionRollbackTarget,
) -> bool {
    let SelectionRecord::V3(record) = &compensation.record else {
        return false;
    };
    record.previous_record_sha256.as_deref() == Some(candidate.record_sha256.as_str())
        && record.compensates_selection_record_sha256.as_deref()
            == Some(candidate.record_sha256.as_str())
        && record.journal_transaction_id == target.transaction_id
        && record.journal_sequence == target.journal_sequence
        && match (&target.previous_generation_sha256, &record.state) {
            (Some(previous_generation), SelectionState::Selected { generation_sha256 }) => {
                generation_sha256 == previous_generation
            }
            (None, SelectionState::Cleared) => true,
            _ => false,
        }
}

fn append_selection_compensation(
    selection_root: &Path,
    candidate: &SelectionTip,
    target: &SelectionRollbackTarget,
) -> Result<Option<String>> {
    let (_, compensation_hash) = match &target.previous_generation_sha256 {
        Some(previous_generation) => append_compensating_selected(
            selection_root,
            previous_generation,
            &target.transaction_id,
            target.journal_sequence,
            &candidate.record_sha256,
        )?,
        None => append_cleared(
            selection_root,
            &target.transaction_id,
            target.journal_sequence,
            &candidate.record_sha256,
        )?,
    };
    Ok(Some(compensation_hash))
}

fn roll_forward_committed(
    journal: &Path,
    generation_store_root: &Path,
    selection_root: &Path,
    authority: &SupervisorSessionAuthority,
) -> Result<()> {
    let entries = read_entries(journal)?;
    let last = entries.last().ok_or_else(|| {
        PortableRuntimeError::new("portable_recovery_invalid", "committed journal was empty")
    })?;
    if last.phase == JournalPhase::Committed {
        validate_committed_selection(&entries, selection_root)?;
        append_recovery_phase(
            journal,
            last,
            JournalPhase::CommitObserved,
            "committed-permit-replay-required",
            last.selection_record_sha256.clone(),
            generation_store_root,
            authority,
        )?;
    }
    write_terminal_receipt(journal, authority)?;
    Ok(())
}

fn validate_committed_selection(entries: &[JournalEntry], selection_root: &Path) -> Result<()> {
    let selection = entries
        .iter()
        .find(|entry| entry.phase == JournalPhase::SelectionCommitted)
        .ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_recovery_selection",
                "committed journal lacked SelectionCommitted evidence",
            )
        })?;
    let committed = entries
        .iter()
        .find(|entry| entry.phase == JournalPhase::Committed)
        .ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_recovery_selection",
                "committed journal lacked Committed evidence",
            )
        })?;
    let selection_record_sha256 =
        selection
            .selection_record_sha256
            .as_deref()
            .ok_or_else(|| {
                PortableRuntimeError::new(
                    "portable_recovery_selection",
                    "SelectionCommitted lacked its immutable record hash",
                )
            })?;
    if committed.sequence != committed_sequence_for_selection(selection.sequence)?
        || committed.transaction_id != selection.transaction_id
        || committed.selected_generation_sha256 != selection.selected_generation_sha256
        || committed.selection_record_sha256.as_deref() != Some(selection_record_sha256)
    {
        return Err(PortableRuntimeError::new(
            "portable_recovery_selection",
            "Committed did not exactly bind SelectionCommitted",
        ));
    }
    let last = entries.last().ok_or_else(|| {
        PortableRuntimeError::new("portable_recovery_selection", "committed journal was empty")
    })?;
    if last.phase == JournalPhase::CommitObserved
        && (last.transaction_id != selection.transaction_id
            || last.selected_generation_sha256 != selection.selected_generation_sha256
            || last.selection_record_sha256.as_deref() != Some(selection_record_sha256))
    {
        return Err(PortableRuntimeError::new(
            "portable_recovery_selection",
            "CommitObserved did not exactly bind SelectionCommitted",
        ));
    }
    require_canonical_normal_selection(
        selection_root,
        &selection.transaction_id,
        selection.sequence,
        &selection.selected_generation_sha256,
        selection_record_sha256,
    )
}

fn append_recovery_phase(
    journal: &Path,
    last: &JournalEntry,
    phase: JournalPhase,
    transcript: &str,
    selection_record_sha256: Option<String>,
    generation_store_root: &Path,
    authority: &SupervisorSessionAuthority,
) -> Result<()> {
    let transition = plan_recovery(journal)?.ok_or_else(|| {
        PortableRuntimeError::new(
            "portable_recovery_invalid",
            "recovery transition lacked a durable journal tail",
        )
    })?;
    if transition.target_phase() != phase {
        return Err(PortableRuntimeError::new(
            "portable_recovery_invalid",
            "recovery transition target did not match the requested phase",
        ));
    }
    append_recovery_with_outbox(
        journal,
        generation_store_root,
        JournalEntry {
            protocol: 0,
            sequence: 0,
            phase,
            transaction_id: last.transaction_id.clone(),
            activation_id: last.activation_id.clone(),
            selected_generation_sha256: last.selected_generation_sha256.clone(),
            previous_sha256: last.previous_sha256.clone(),
            transcript_sha256: super::signature::sha256_hex(transcript.as_bytes()),
            origin_session_sha256: String::new(),
            writer_session_sha256: String::new(),
            predecessor_writer_session_sha256: None,
            append_kind: JournalAppendKind::Normal,
            previous_entry_sha256: None,
            phase_receipt_sha256: String::new(),
            selection_record_sha256,
        },
        authority,
        transition,
    )?;
    Ok(())
}
