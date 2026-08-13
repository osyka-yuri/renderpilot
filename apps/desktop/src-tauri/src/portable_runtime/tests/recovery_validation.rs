use super::recovery_support::*;

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
fn journal_rejects_a_nondigest_optional_selection_hash() {
    let root = temp_root("journal-nondigest-optional-selection");
    let transaction = hash('c');
    let journal = journal_path(root.path(), &transaction);
    let mut entry = test_entry(
        &transaction,
        JournalPhase::Prepared,
        &hash('a'),
        None,
        Some("not-a-sha256-digest"),
    );
    entry.append_kind = JournalAppendKind::Origin;
    crate::portable_runtime::journal::append_malformed_selection_digest_fixture(
        &journal,
        entry,
        &supervisor_session('1'),
    )
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
    let generation_store = root.path().join("generation-store");
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
            &generation_store,
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
