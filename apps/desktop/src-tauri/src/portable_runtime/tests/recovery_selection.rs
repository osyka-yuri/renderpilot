use super::recovery_support::*;

#[test]
fn precommit_selection_is_append_only_rolled_back_and_idempotent() {
    let root = temp_root("recovery-selection");
    let update = root.path().join("update");
    let catalog = root.path().join("catalog.db");
    let selection = root.path().join("selection");
    fs::write(&catalog, b"catalog").expect("write catalog");
    let transaction = hash('5');
    append_selected(&selection, &hash('b'), &hash('e'), 1).expect("publish previous selection");
    append_selected(&selection, &hash('a'), &transaction, 6)
        .expect("publish failed trial selection");
    let journal = journal_path(&update, &transaction);
    for phase in phase_prefix(JournalPhase::MigrationCommitted) {
        append_test_phase(
            &journal,
            &transaction,
            phase,
            &hash('a'),
            Some(&hash('b')),
            None,
        );
    }
    recover_prior_transactions(&update, &catalog, &selection).expect("rollback selection");
    assert_eq!(
        current_selection(&selection)
            .expect("read selection")
            .expect("selection exists")
            .generation_sha256,
        hash('b')
    );
    let count = read_selection(&selection)
        .expect("read append-only selection")
        .len();
    recover_prior_transactions(&update, &catalog, &selection).expect("repeat recovery");
    assert_eq!(
        read_selection(&selection)
            .expect("read append-only selection")
            .len(),
        count
    );
}

#[test]
fn same_generation_precommit_compensation_restores_the_semantic_predecessor() {
    let root = temp_root("recovery-selection-same-generation");
    let update = root.path().join("update");
    let catalog = root.path().join("catalog.db");
    let selection = root.path().join("selection");
    fs::write(&catalog, b"catalog").expect("write catalog");
    let generation = hash('a');
    let predecessor_transaction = hash('1');
    let transaction = hash('2');

    append_selected(&selection, &generation, &predecessor_transaction, 6)
        .expect("append semantic predecessor");
    let (_, failed_selection_hash) = append_selected(&selection, &generation, &transaction, 6)
        .expect("append failed same-generation selection");
    let journal = journal_path(&update, &transaction);
    for phase in phase_prefix(JournalPhase::PermitSent) {
        let selection_record = matches!(
            phase,
            JournalPhase::SelectionCommitted | JournalPhase::PermitSent
        )
        .then_some(failed_selection_hash.as_str());
        append_test_phase(
            &journal,
            &transaction,
            phase,
            &generation,
            Some(&generation),
            selection_record,
        );
    }

    recover_prior_transactions(&update, &catalog, &selection)
        .expect("compensate same-generation precommit selection");
    let selections = read_selection(&selection).expect("read compensated selection chain");
    assert_eq!(selections.len(), 3);
    assert_eq!(
        current_selection(&selection)
            .expect("read current selection")
            .expect("compensation retains generation")
            .generation_sha256,
        generation
    );
    assert!(matches!(
        &selections[2].record,
        SelectionRecord::V3(record)
            if record.state
                == (SelectionState::Selected {
                    generation_sha256: hash('a'),
                })
                && record.compensates_selection_record_sha256.as_deref()
                    == Some(failed_selection_hash.as_str())
    ));
}

#[test]
fn receipt_missing_commit_observed_recovery_rejects_a_reused_selection_record() {
    let root = temp_root("recovery-committed-reused-selection");
    let update = root.path().join("update");
    let catalog = root.path().join("catalog.db");
    let selection = root.path().join("selection");
    fs::write(&catalog, b"catalog").expect("write catalog");
    let reused_transaction = hash('1');
    let transaction = hash('2');
    let (_, reused_selection_hash) =
        append_selected(&selection, &hash('a'), &reused_transaction, 6)
            .expect("append a different transaction's canonical selection");
    let journal = journal_path(&update, &transaction);
    for phase in phase_prefix(JournalPhase::CommitObserved) {
        let selection_record = matches!(
            phase,
            JournalPhase::SelectionCommitted
                | JournalPhase::PermitSent
                | JournalPhase::ActivationAcknowledged
                | JournalPhase::Committed
                | JournalPhase::CommitObserved
        )
        .then_some(reused_selection_hash.as_str());
        append_test_phase(
            &journal,
            &transaction,
            phase,
            &hash('a'),
            None,
            selection_record,
        );
    }

    assert_eq!(
        error_code(recover_prior_transactions(&update, &catalog, &selection)),
        "portable_selection_invalid"
    );
    assert_eq!(
        read_entries(&journal)
            .expect("reused-selection journal remains readable")
            .last()
            .map(|entry| entry.phase),
        Some(JournalPhase::CommitObserved),
        "recovery must not finalize another transaction's selection"
    );
}

#[test]
fn completed_receipts_are_revalidated_without_consulting_a_later_selection_tip() {
    let root = temp_root("recovery-completed-history");
    let update = root.path().join("update");
    let catalog = root.path().join("catalog.db");
    let selection = root.path().join("selection");
    fs::write(&catalog, b"catalog").expect("write catalog");
    let first_transaction = hash('1');
    let second_transaction = hash('2');
    let first_generation = hash('a');
    let second_generation = hash('b');

    let (_, first_selection_hash) =
        append_selected(&selection, &first_generation, &first_transaction, 6)
            .expect("append first completed selection");
    let first_journal = journal_path(&update, &first_transaction);
    for phase in phase_prefix(JournalPhase::CommitObserved) {
        let selection_record = matches!(
            phase,
            JournalPhase::SelectionCommitted
                | JournalPhase::PermitSent
                | JournalPhase::ActivationAcknowledged
                | JournalPhase::Committed
                | JournalPhase::CommitObserved
        )
        .then_some(first_selection_hash.as_str());
        append_test_phase(
            &first_journal,
            &first_transaction,
            phase,
            &first_generation,
            None,
            selection_record,
        );
    }
    write_terminal_receipt(&first_journal, &supervisor_session('1'))
        .expect("seal first terminal receipt");

    let (_, second_selection_hash) =
        append_selected(&selection, &second_generation, &second_transaction, 6)
            .expect("append second completed selection");
    let second_journal = journal_path(&update, &second_transaction);
    for phase in phase_prefix(JournalPhase::CommitObserved) {
        let selection_record = matches!(
            phase,
            JournalPhase::SelectionCommitted
                | JournalPhase::PermitSent
                | JournalPhase::ActivationAcknowledged
                | JournalPhase::Committed
                | JournalPhase::CommitObserved
        )
        .then_some(second_selection_hash.as_str());
        append_test_phase(
            &second_journal,
            &second_transaction,
            phase,
            &second_generation,
            Some(&first_generation),
            selection_record,
        );
    }
    write_terminal_receipt(&second_journal, &supervisor_session('1'))
        .expect("seal second terminal receipt");

    let first_journal_bytes = fs::read(&first_journal).expect("read first terminal journal");
    let second_journal_bytes = fs::read(&second_journal).expect("read second terminal journal");
    let first_receipt = fs::read(
        first_journal
            .parent()
            .expect("first transaction root")
            .join("terminal-receipt.json"),
    )
    .expect("read first terminal receipt");
    let second_receipt = fs::read(
        second_journal
            .parent()
            .expect("second transaction root")
            .join("terminal-receipt.json"),
    )
    .expect("read second terminal receipt");

    recover_prior_transactions(&update, &catalog, &selection)
        .expect("later startup revalidates both completed receipts");

    assert_eq!(
        current_selection(&selection)
            .expect("read current selection")
            .expect("second selection remains canonical")
            .record_sha256,
        second_selection_hash
    );
    assert_eq!(
        fs::read(&first_journal).expect("reread first journal"),
        first_journal_bytes
    );
    assert_eq!(
        fs::read(&second_journal).expect("reread second journal"),
        second_journal_bytes
    );
    assert_eq!(
        fs::read(
            first_journal
                .parent()
                .expect("first transaction root")
                .join("terminal-receipt.json"),
        )
        .expect("reread first receipt"),
        first_receipt
    );
    assert_eq!(
        fs::read(
            second_journal
                .parent()
                .expect("second transaction root")
                .join("terminal-receipt.json"),
        )
        .expect("reread second receipt"),
        second_receipt
    );
}

#[test]
fn first_activation_recovery_accepts_a_cleared_predecessor_after_restart() {
    let root = temp_root("recovery-selection-cleared-predecessor");
    let update = root.path().join("update");
    let catalog = root.path().join("catalog.db");
    let selection = root.path().join("selection");
    fs::write(&catalog, b"catalog").expect("write catalog");

    for marker in ['1', '2'] {
        let transaction = hash(marker);
        let journal = journal_path(&update, &transaction);
        append_selected(&selection, &hash('a'), &transaction, 6)
            .expect("publish failed first-activation selection");
        for phase in phase_prefix(JournalPhase::MigrationCommitted) {
            append_test_phase(&journal, &transaction, phase, &hash('a'), None, None);
        }

        recover_prior_transactions(&update, &catalog, &selection)
            .expect("clear failed first-activation selection");
        assert!(
            current_selection(&selection)
                .expect("read cleared selection")
                .is_none(),
            "a cleared tip must reduce to no current generation"
        );
    }

    assert_eq!(
        read_selection(&selection)
            .expect("read append-only selection")
            .len(),
        4,
        "each failed activation appends one candidate and one compensation"
    );
}
