use std::fs;

use rusqlite::{Connection, params};

use super::super::provenance::{self, SealDomain};
use super::{
    append_journal as append, error_code, hash, journal_entry, supervisor_session, temp_root,
};
use crate::portable_runtime::{
    journal::{
        JOURNAL_PROTOCOL, JournalAppendKind, JournalPhase, TerminalReceiptV3, append_normal,
        append_recovery, journal_path, plan_recovery, read_entries, write_terminal_receipt,
    },
    recovery::{
        RecoveryAction, recover_prior_transactions as recover_with_authority, recovery_action,
    },
    selection::{
        SelectionRecord, SelectionState, append_selected, current_selection, read_selection,
    },
    signature::sha256_hex,
    snapshot::create as create_snapshot,
};

fn recover_prior_transactions(
    update: &std::path::Path,
    catalog: &std::path::Path,
    selection: &std::path::Path,
) -> crate::portable_runtime::error::Result<()> {
    recover_with_authority(update, catalog, selection, &supervisor_session('2'))
}

fn create_catalog(path: &std::path::Path, value: &str) {
    let connection = Connection::open(path).expect("create catalog fixture");
    connection
        .execute_batch(
            "PRAGMA user_version = 15;
             CREATE TABLE fixture(value TEXT NOT NULL);",
        )
        .expect("create catalog fixture schema");
    connection
        .execute("INSERT INTO fixture(value) VALUES (?1)", params![value])
        .expect("write catalog fixture value");
}

fn set_catalog_value(path: &std::path::Path, value: &str) {
    Connection::open(path)
        .expect("open catalog fixture")
        .execute("UPDATE fixture SET value = ?1", params![value])
        .expect("update catalog fixture value");
}

fn catalog_value(path: &std::path::Path) -> String {
    Connection::open(path)
        .expect("open catalog fixture")
        .query_row("SELECT value FROM fixture", [], |row| row.get(0))
        .expect("read catalog fixture value")
}

fn test_entry(
    transaction: &str,
    phase: JournalPhase,
    selected_generation: &str,
    previous_generation: Option<&str>,
    selection_record: Option<&str>,
) -> crate::portable_runtime::journal::JournalEntry {
    let mut entry = journal_entry(phase);
    entry.transaction_id = transaction.to_owned();
    entry.selected_generation_sha256 = selected_generation.to_owned();
    entry.previous_sha256 = previous_generation.map(str::to_owned);
    entry.selection_record_sha256 = selection_record.map(str::to_owned);
    entry
}

fn append_test_phase(
    journal: &std::path::Path,
    transaction: &str,
    phase: JournalPhase,
    selected_generation: &str,
    previous_generation: Option<&str>,
    selection_record: Option<&str>,
) {
    append(
        journal,
        test_entry(
            transaction,
            phase,
            selected_generation,
            previous_generation,
            selection_record,
        ),
    )
    .unwrap_or_else(|error| panic!("append {phase:?}: {error}"));
}

fn phase_prefix(target: JournalPhase) -> Vec<JournalPhase> {
    use JournalPhase::*;
    match target {
        Prepared => vec![Prepared],
        GenerationPublished => vec![Prepared, GenerationPublished],
        OldAppQuiesced => vec![Prepared, GenerationPublished, OldAppQuiesced],
        TrialSpawned => vec![Prepared, GenerationPublished, TrialSpawned],
        TrialReady => vec![Prepared, GenerationPublished, TrialSpawned, TrialReady],
        SnapshotCommitted => vec![
            Prepared,
            GenerationPublished,
            TrialSpawned,
            TrialReady,
            SnapshotCommitted,
        ],
        MigrationCommitted => vec![
            Prepared,
            GenerationPublished,
            TrialSpawned,
            TrialReady,
            MigrationCommitted,
        ],
        SelectionCommitted => {
            let mut phases = phase_prefix(MigrationCommitted);
            phases.push(SelectionCommitted);
            phases
        }
        PermitSent => {
            let mut phases = phase_prefix(SelectionCommitted);
            phases.push(PermitSent);
            phases
        }
        ActivationAcknowledged => {
            let mut phases = phase_prefix(PermitSent);
            phases.push(ActivationAcknowledged);
            phases
        }
        Committed => {
            let mut phases = phase_prefix(ActivationAcknowledged);
            phases.push(Committed);
            phases
        }
        CommitObserved => {
            let mut phases = phase_prefix(Committed);
            phases.push(CommitObserved);
            phases
        }
        RollingBack => vec![Prepared, RollingBack],
        RolledBack => vec![Prepared, RollingBack, RolledBack],
        NeedsRecovery => vec![Prepared, NeedsRecovery],
    }
}

fn append_prefix(journal: &std::path::Path, transaction: &str, target: JournalPhase) {
    let selection = hash('c');
    for phase in phase_prefix(target) {
        let selection_record = matches!(
            phase,
            JournalPhase::SelectionCommitted
                | JournalPhase::PermitSent
                | JournalPhase::ActivationAcknowledged
                | JournalPhase::Committed
                | JournalPhase::CommitObserved
        )
        .then_some(selection.as_str());
        append_test_phase(
            journal,
            transaction,
            phase,
            &hash('a'),
            Some(&hash('b')),
            selection_record,
        );
    }
}

fn sealed_tail_sha256(journal: &std::path::Path) -> String {
    let bytes = fs::read(journal).expect("read sealed journal");
    let tail = bytes
        .strip_suffix(b"\n")
        .and_then(|bytes| bytes.rsplit(|byte| *byte == b'\n').next())
        .expect("sealed journal tail");
    sha256_hex(tail)
}

fn terminal_receipt(journal: &std::path::Path, transaction: &str) -> TerminalReceiptV3 {
    let bytes = fs::read(
        journal
            .parent()
            .expect("transaction root")
            .join("terminal-receipt.json"),
    )
    .expect("read terminal receipt");
    serde_json::from_slice(
        &provenance::open(
            SealDomain::Terminal,
            &format!("terminal:{transaction}"),
            &bytes,
        )
        .expect("open terminal receipt"),
    )
    .expect("decode terminal receipt")
}

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

#[test]
fn partial_uncommitted_snapshot_is_never_restored() {
    let root = temp_root("recovery-partial-snapshot");
    let update = root.path().join("update");
    let catalog = root.path().join("catalog.db");
    let selection = root.path().join("selection");
    fs::write(&catalog, b"authoritative catalog").expect("write catalog");
    let transaction = hash('3');
    let journal = journal_path(&update, &transaction);
    let partial = journal
        .parent()
        .expect("transaction root")
        .join("snapshot/catalog.db");
    fs::create_dir_all(partial.parent().expect("snapshot parent")).expect("create snapshot root");
    fs::write(&partial, b"torn backup").expect("write torn backup");
    append_prefix(&journal, &transaction, JournalPhase::TrialReady);

    recover_prior_transactions(&update, &catalog, &selection)
        .expect("ignore uncommitted snapshot bytes");
    assert_eq!(
        fs::read(&catalog).expect("read catalog"),
        b"authoritative catalog"
    );
}

#[test]
fn corrupted_committed_snapshot_fails_closed_without_touching_catalog() {
    let root = temp_root("recovery-corrupt-snapshot");
    let update = root.path().join("update");
    let catalog = root.path().join("catalog.db");
    let selection = root.path().join("selection");
    create_catalog(&catalog, "original catalog value");
    let transaction = hash('4');
    let journal = journal_path(&update, &transaction);
    for phase in [
        JournalPhase::Prepared,
        JournalPhase::GenerationPublished,
        JournalPhase::TrialSpawned,
        JournalPhase::TrialReady,
    ] {
        append_test_phase(&journal, &transaction, phase, &hash('a'), None, None);
    }
    let receipt = create_snapshot(&catalog, &update, &transaction).expect("create snapshot");
    let mut committed = test_entry(
        &transaction,
        JournalPhase::SnapshotCommitted,
        &hash('a'),
        None,
        None,
    );
    committed.transcript_sha256 = sha256_hex(receipt.receipt_sha256.as_bytes());
    append(&journal, committed).expect("commit snapshot receipt");
    fs::write(&receipt.backup_path, b"corrupt backup").expect("corrupt backup");
    set_catalog_value(&catalog, "current catalog remains");

    assert_eq!(
        error_code(recover_prior_transactions(&update, &catalog, &selection)),
        "portable_snapshot_receipt"
    );
    assert_eq!(catalog_value(&catalog), "current catalog remains");
}

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

#[test]
fn uncertain_durable_state_retains_authority_without_semantic_repair() {
    let root = temp_root("recovery-uncertain");
    let update = root.path().join("update");
    let catalog = root.path().join("catalog.db");
    let selection = root.path().join("selection");
    fs::write(&catalog, b"catalog stays untouched").expect("write catalog");
    let transaction = hash('6');
    let journal = journal_path(&update, &transaction);
    append_test_phase(
        &journal,
        &transaction,
        JournalPhase::Prepared,
        &hash('a'),
        None,
        None,
    );
    append_test_phase(
        &journal,
        &transaction,
        JournalPhase::NeedsRecovery,
        &hash('a'),
        None,
        None,
    );

    assert_eq!(
        error_code(recover_prior_transactions(&update, &catalog, &selection)),
        "portable_recovery_manual"
    );
    assert_eq!(
        fs::read(&catalog).expect("read unchanged catalog"),
        b"catalog stays untouched"
    );
    assert_eq!(
        read_entries(&journal)
            .expect("read unchanged journal")
            .len(),
        2
    );
}

#[test]
fn normal_append_rejects_a_different_supervisor_writer() {
    let root = temp_root("journal-writer-mismatch");
    let transaction = hash('7');
    let journal = journal_path(root.path(), &transaction);
    let first = test_entry(&transaction, JournalPhase::Prepared, &hash('a'), None, None);
    append_normal(&journal, first, &supervisor_session('1')).expect("origin append");

    let second = test_entry(
        &transaction,
        JournalPhase::GenerationPublished,
        &hash('a'),
        None,
        None,
    );
    assert_eq!(
        error_code(append_normal(&journal, second, &supervisor_session('2'))),
        "portable_journal_authority"
    );
    assert_eq!(read_entries(&journal).expect("retained journal").len(), 1);
}

#[test]
fn normal_session_preserves_distinct_predecessor_and_seals_current_terminal_head() {
    let root = temp_root("journal-same-writer-terminal-receipt");
    let transaction = hash('a');
    let journal = journal_path(root.path(), &transaction);
    append_prefix(&journal, &transaction, JournalPhase::CommitObserved);

    let writer = supervisor_session('1');
    let writer_sha256 = writer.transcript_sha256().to_owned();
    write_terminal_receipt(&journal, &writer).expect("write terminal receipt");

    let entries = read_entries(&journal).expect("validated same-writer journal");
    assert!(entries.iter().all(|entry| {
        entry.origin_session_sha256 == writer_sha256
            && entry.writer_session_sha256 == writer_sha256
            && entry.predecessor_writer_session_sha256.is_none()
    }));
    let receipt = terminal_receipt(&journal, &transaction);
    assert_eq!(receipt.origin_session_sha256, writer_sha256);
    assert_eq!(receipt.finalizer_session_sha256, writer_sha256);
    assert_eq!(receipt.predecessor_writer_session_sha256, None);
    assert_eq!(receipt.journal_head_sha256, sealed_tail_sha256(&journal));
}

#[test]
fn crash_recovery_records_origin_writer_and_successor_lineage() {
    let root = temp_root("journal-cross-session-recovery");
    let update = root.path().join("update");
    let catalog = root.path().join("catalog.db");
    let selection = root.path().join("selection");
    fs::write(&catalog, b"catalog").expect("catalog fixture");
    let transaction = hash('8');
    let journal = journal_path(&update, &transaction);
    append_test_phase(
        &journal,
        &transaction,
        JournalPhase::Prepared,
        &hash('a'),
        None,
        None,
    );
    let origin = supervisor_session('1').transcript_sha256().to_owned();
    let recovery = supervisor_session('2');
    let successor = recovery.transcript_sha256().to_owned();

    recover_with_authority(&update, &catalog, &selection, &recovery)
        .expect("cross-session rollback");
    let entries = read_entries(&journal).expect("validated recovery journal");
    assert!(
        entries
            .iter()
            .all(|entry| entry.origin_session_sha256 == origin)
    );
    let recovery_entries = entries
        .iter()
        .filter(|entry| matches!(entry.append_kind, JournalAppendKind::Recovery { .. }))
        .collect::<Vec<_>>();
    assert_eq!(recovery_entries.len(), 2, "recovery writes two B entries");
    assert!(recovery_entries.iter().all(|entry| {
        entry.writer_session_sha256 == successor
            && entry.predecessor_writer_session_sha256.as_deref() == Some(origin.as_str())
    }));
    assert_eq!(
        entries.last().map(|entry| entry.phase),
        Some(JournalPhase::RolledBack)
    );
    let receipt = terminal_receipt(&journal, &transaction);
    assert_eq!(receipt.origin_session_sha256, origin);
    assert_eq!(receipt.finalizer_session_sha256, successor);
    assert_eq!(
        receipt.predecessor_writer_session_sha256.as_deref(),
        Some(origin.as_str())
    );
    assert_eq!(receipt.selection_record_sha256, None);
    assert_eq!(receipt.journal_head_sha256, sealed_tail_sha256(&journal));
}

#[test]
fn successive_recoveries_record_the_immediate_distinct_predecessor() {
    let root = temp_root("journal-three-writer-terminal-receipt");
    let update = root.path().join("update");
    let catalog = root.path().join("catalog.db");
    let selection = root.path().join("selection");
    fs::write(&catalog, b"catalog").expect("catalog fixture");
    let transaction = hash('b');
    let journal = journal_path(&update, &transaction);
    append_test_phase(
        &journal,
        &transaction,
        JournalPhase::Prepared,
        &hash('a'),
        None,
        None,
    );
    let origin = supervisor_session('1').transcript_sha256().to_owned();
    let writer_b = supervisor_session('2');
    let writer_b_sha256 = writer_b.transcript_sha256().to_owned();
    let transition = plan_recovery(&journal)
        .expect("plan B recovery")
        .expect("B recovery transition");
    append_recovery(
        &journal,
        test_entry(
            &transaction,
            JournalPhase::RollingBack,
            &hash('a'),
            None,
            None,
        ),
        &writer_b,
        transition,
    )
    .expect("B records recovery before crash");

    let writer_c = supervisor_session('3');
    let writer_c_sha256 = writer_c.transcript_sha256().to_owned();
    recover_with_authority(&update, &catalog, &selection, &writer_c).expect("C completes recovery");

    let entries = read_entries(&journal).expect("validated three-writer journal");
    assert!(
        entries
            .iter()
            .all(|entry| entry.origin_session_sha256 == origin)
    );
    assert_eq!(
        entries
            .iter()
            .find(|entry| entry.writer_session_sha256 == writer_b_sha256)
            .and_then(|entry| entry.predecessor_writer_session_sha256.as_deref()),
        Some(origin.as_str())
    );
    let receipt = terminal_receipt(&journal, &transaction);
    assert_eq!(receipt.origin_session_sha256, origin);
    assert_eq!(receipt.finalizer_session_sha256, writer_c_sha256);
    assert_eq!(
        receipt.predecessor_writer_session_sha256.as_deref(),
        Some(writer_b_sha256.as_str())
    );
    assert_eq!(receipt.journal_head_sha256, sealed_tail_sha256(&journal));
}

#[test]
fn journal_rejects_a_nondigest_optional_selection_hash() {
    let root = temp_root("journal-nondigest-optional-selection");
    let transaction = hash('c');
    let journal = journal_path(root.path(), &transaction);
    let entry = test_entry(
        &transaction,
        JournalPhase::Prepared,
        &hash('a'),
        None,
        Some("not-a-sha256-digest"),
    );
    append_normal(&journal, entry, &supervisor_session('1'))
        .expect("append structurally sealed journal entry");

    assert_eq!(
        error_code(read_entries(&journal)),
        "portable_journal_invalid",
        "all present optional selection hashes are digest-domain values"
    );
}

#[test]
fn terminal_receipt_rejects_a_sealed_non_v3_receipt() {
    let root = temp_root("terminal-receipt-non-v3");
    let transaction = hash('d');
    let journal = journal_path(root.path(), &transaction);
    append_prefix(&journal, &transaction, JournalPhase::CommitObserved);
    let writer = supervisor_session('1');
    let last = read_entries(&journal)
        .expect("read current terminal journal")
        .last()
        .cloned()
        .expect("terminal journal entry");
    let mut old_receipt = TerminalReceiptV3 {
        protocol: JOURNAL_PROTOCOL - 1,
        phase: JournalPhase::CommitObserved,
        transaction_id: transaction.clone(),
        selected_generation_sha256: last.selected_generation_sha256,
        selection_record_sha256: last.selection_record_sha256,
        journal_head_sha256: sealed_tail_sha256(&journal),
        origin_session_sha256: last.origin_session_sha256,
        finalizer_session_sha256: last.writer_session_sha256,
        predecessor_writer_session_sha256: last.predecessor_writer_session_sha256,
        terminal_journal_sequence: last.sequence,
        terminal_journal_transcript_sha256: last.transcript_sha256,
        recovery_action: None,
        receipt_sha256: String::new(),
    };
    old_receipt.receipt_sha256 =
        sha256_hex(&serde_json::to_vec(&old_receipt).expect("encode unsigned old receipt"));
    let bytes = provenance::seal(
        SealDomain::Terminal,
        &format!("terminal:{transaction}"),
        &serde_json::to_vec(&old_receipt).expect("encode old receipt"),
    )
    .expect("seal non-v3 receipt fixture");
    fs::write(
        journal
            .parent()
            .expect("transaction root")
            .join("terminal-receipt.json"),
        bytes,
    )
    .expect("write sealed non-v3 receipt fixture");

    assert_eq!(
        error_code(write_terminal_receipt(&journal, &writer)),
        "portable_terminal_receipt"
    );
}

#[test]
fn stale_recovery_transition_is_rejected_without_repair() {
    let root = temp_root("journal-stale-recovery-transition");
    let transaction = hash('9');
    let journal = journal_path(root.path(), &transaction);
    append_test_phase(
        &journal,
        &transaction,
        JournalPhase::Prepared,
        &hash('a'),
        None,
        None,
    );
    let stale = plan_recovery(&journal)
        .expect("plan recovery")
        .expect("recovery transition");
    append_test_phase(
        &journal,
        &transaction,
        JournalPhase::GenerationPublished,
        &hash('a'),
        None,
        None,
    );
    let rolling_back = test_entry(
        &transaction,
        JournalPhase::RollingBack,
        &hash('a'),
        None,
        None,
    );
    assert_eq!(
        error_code(append_recovery(
            &journal,
            rolling_back,
            &supervisor_session('2'),
            stale,
        )),
        "portable_journal_authority"
    );
    assert_eq!(
        read_entries(&journal)
            .expect("unchanged journal")
            .last()
            .map(|entry| entry.phase),
        Some(JournalPhase::GenerationPublished)
    );
}
