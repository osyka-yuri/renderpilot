use super::recovery_support::*;

#[test]
fn normal_append_rejects_a_different_supervisor_writer() {
    let root = temp_root("journal-writer-mismatch");
    let generation_store = root.path().join("generation-store");
    let transaction = hash('7');
    let journal = journal_path(root.path(), &transaction);
    let first = test_entry(&transaction, JournalPhase::Prepared, &hash('a'), None, None);
    append_normal(&journal, &generation_store, first, &supervisor_session('1'))
        .expect("origin append");

    let second = test_entry(
        &transaction,
        JournalPhase::GenerationPublished,
        &hash('a'),
        None,
        None,
    );
    assert_eq!(
        error_code(append_normal(
            &journal,
            &generation_store,
            second,
            &supervisor_session('2'),
        )),
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
    let generation_store = root.path().join("generation-store");
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

    recover_with_authority(&generation_store, &update, &catalog, &selection, &recovery)
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
    let generation_store = root.path().join("generation-store");
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
        &generation_store,
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
    recover_with_authority(&generation_store, &update, &catalog, &selection, &writer_c)
        .expect("C completes recovery");

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
