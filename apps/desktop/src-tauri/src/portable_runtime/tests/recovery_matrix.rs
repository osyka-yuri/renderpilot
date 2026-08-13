use super::recovery_support::*;

#[test]
fn every_durable_phase_has_an_explicit_recovery_reducer_result() {
    let root = temp_root("recovery-matrix");
    let rollback = [
        JournalPhase::Prepared,
        JournalPhase::GenerationPublished,
        JournalPhase::OldAppQuiesced,
        JournalPhase::SnapshotCommitted,
        JournalPhase::TrialSpawned,
        JournalPhase::TrialReady,
        JournalPhase::MigrationCommitted,
        JournalPhase::SelectionCommitted,
        JournalPhase::PermitSent,
        JournalPhase::ActivationAcknowledged,
        JournalPhase::RollingBack,
    ];
    for (index, phase) in rollback.into_iter().enumerate() {
        let transaction = format!("{:064x}", index + 1);
        let journal = journal_path(root.path(), &transaction);
        append_prefix(&journal, &transaction, phase);
        assert_eq!(
            recovery_action(&journal).expect("reduce durable journal"),
            RecoveryAction::RollBackPreCommit,
            "{phase:?} remains pre-commit"
        );
    }
    for (index, (phase, expected)) in [
        (
            JournalPhase::Committed,
            RecoveryAction::RollForwardCommitted,
        ),
        (
            JournalPhase::CommitObserved,
            RecoveryAction::FinalizeTerminalReceipt,
        ),
        (
            JournalPhase::RolledBack,
            RecoveryAction::FinalizeTerminalReceipt,
        ),
        (
            JournalPhase::NeedsRecovery,
            RecoveryAction::NeedsManualRecovery,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let transaction = format!("{:064x}", index + rollback.len() + 1);
        let journal = journal_path(root.path(), &transaction);
        append_prefix(&journal, &transaction, phase);
        assert_eq!(
            recovery_action(&journal).expect("reduce terminal phase"),
            expected
        );
    }
}

#[test]
fn precommit_recovers_snapshot_but_committed_never_restores_it() {
    let root = temp_root("recovery-cuts");
    let update = root.path().join("update");
    let catalog = root.path().join("catalog.db");
    let selection = root.path().join("selection");
    create_catalog(&catalog, "precommit backup value");

    let precommit_transaction = hash('1');
    let precommit = journal_path(&update, &precommit_transaction);
    for phase in [
        JournalPhase::Prepared,
        JournalPhase::GenerationPublished,
        JournalPhase::TrialSpawned,
        JournalPhase::TrialReady,
    ] {
        append_test_phase(
            &precommit,
            &precommit_transaction,
            phase,
            &hash('a'),
            None,
            None,
        );
    }
    let receipt =
        create_snapshot(&catalog, &update, &precommit_transaction).expect("create snapshot");
    let mut snapshot_committed = test_entry(
        &precommit_transaction,
        JournalPhase::SnapshotCommitted,
        &hash('a'),
        None,
        None,
    );
    snapshot_committed.transcript_sha256 = sha256_hex(receipt.receipt_sha256.as_bytes());
    append(&precommit, snapshot_committed).expect("commit snapshot receipt");
    set_catalog_value(&catalog, "new catalog value");
    let (_, selection_hash) = append_selected(&selection, &hash('a'), &precommit_transaction, 7)
        .expect("publish precommit selection");
    for phase in [
        JournalPhase::MigrationCommitted,
        JournalPhase::SelectionCommitted,
        JournalPhase::PermitSent,
        JournalPhase::ActivationAcknowledged,
    ] {
        append_test_phase(
            &precommit,
            &precommit_transaction,
            phase,
            &hash('a'),
            None,
            matches!(
                phase,
                JournalPhase::SelectionCommitted
                    | JournalPhase::PermitSent
                    | JournalPhase::ActivationAcknowledged
            )
            .then_some(selection_hash.as_str()),
        );
    }
    recover_prior_transactions(&update, &catalog, &selection)
        .expect("recover pre-commit transaction");
    assert_eq!(catalog_value(&catalog), "precommit backup value");
    assert!(matches!(
        read_entries(&precommit)
            .expect("read rollback journal")
            .last()
            .map(|entry| entry.phase),
        Some(JournalPhase::RolledBack)
    ));
    assert!(
        precommit
            .parent()
            .expect("transaction root")
            .join("snapshot/catalog.db")
            .is_file(),
        "cleanup retains rollback evidence without exact live handles"
    );
    assert!(
        precommit
            .parent()
            .expect("transaction root")
            .join("snapshot-receipt.json")
            .is_file(),
        "cleanup retains the immutable receipt audit trail"
    );

    set_catalog_value(&catalog, "committed catalog value");
    let committed_transaction = hash('2');
    let (_, committed_selection_hash) =
        append_selected(&selection, &hash('a'), &committed_transaction, 6)
            .expect("append exact committed selection");
    let committed = journal_path(&update, &committed_transaction);
    let committed_snapshot = committed
        .parent()
        .expect("transaction root")
        .join("snapshot/catalog.db");
    fs::create_dir_all(committed_snapshot.parent().expect("snapshot parent"))
        .expect("create snapshot");
    fs::write(&committed_snapshot, b"must never be restored").expect("write snapshot");
    for phase in phase_prefix(JournalPhase::Committed) {
        let selection_record = matches!(
            phase,
            JournalPhase::SelectionCommitted
                | JournalPhase::PermitSent
                | JournalPhase::ActivationAcknowledged
                | JournalPhase::Committed
        )
        .then_some(committed_selection_hash.as_str());
        append_test_phase(
            &committed,
            &committed_transaction,
            phase,
            &hash('a'),
            Some(&hash('b')),
            selection_record,
        );
    }
    recover_prior_transactions(&update, &catalog, &selection)
        .expect("roll forward committed transaction");
    assert_eq!(catalog_value(&catalog), "committed catalog value");
    assert!(matches!(
        read_entries(&committed)
            .expect("read committed journal")
            .last()
            .map(|entry| entry.phase),
        Some(JournalPhase::CommitObserved)
    ));
    assert!(
        committed_snapshot.exists(),
        "terminal cleanup retains an unowned snapshot path"
    );
    recover_prior_transactions(&update, &catalog, &selection)
        .expect("receipt and permit replay are idempotent");
}
