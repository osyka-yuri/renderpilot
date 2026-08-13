use super::recovery_support::*;

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
fn snapshot_restore_uses_a_fresh_retained_nonce_attempt() {
    let root = temp_root("snapshot-restore-nonce");
    let update = root.path().join("update");
    let catalog = root.path().join("catalog.db");
    let transaction = hash('f');
    create_catalog(&catalog, "snapshot original value");
    let receipt =
        create_snapshot(&catalog, &update, &transaction).expect("create committed snapshot");
    set_catalog_value(&catalog, "migrated value");
    let retained = catalog.parent().expect("catalog parent").join(format!(
        ".renderpilot-restore-{transaction}.old-attempt.tmp"
    ));
    fs::write(&retained, b"interrupted predecessor restore").expect("write retained old attempt");

    restore_snapshot(&receipt, &catalog).expect("fresh nonce restore succeeds");
    assert_eq!(catalog_value(&catalog), "snapshot original value");
    assert!(
        retained.is_file(),
        "previous restore attempt remains retained"
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
