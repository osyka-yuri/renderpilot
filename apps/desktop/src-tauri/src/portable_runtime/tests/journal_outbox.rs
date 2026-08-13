use std::{collections::BTreeSet, fs, io::Write, path::Path};

use crate::portable_runtime::{
    journal::{
        JournalAppendKind, JournalEntry, JournalPhase, aborted_before_origin,
        append_normal_with_outbox, intended_prefix_len, journal_path, read_entries,
        reconcile_operation_outbox, record_origin_append_intent, terminal_receipt_exists,
    },
    recovery::recover_prior_transactions,
    signature::sha256_hex,
};

use super::{
    append_journal as append, error_code, hash, journal_entry, supervisor_session, temp_root,
};

fn entry(transaction_id: &str) -> JournalEntry {
    JournalEntry {
        protocol: 0,
        sequence: 0,
        phase: JournalPhase::Prepared,
        transaction_id: transaction_id.to_owned(),
        activation_id: hash('e'),
        selected_generation_sha256: hash('a'),
        previous_sha256: Some(hash('b')),
        transcript_sha256: sha256_hex(b"journal outbox test"),
        origin_session_sha256: String::new(),
        writer_session_sha256: String::new(),
        predecessor_writer_session_sha256: None,
        append_kind: JournalAppendKind::Normal,
        previous_entry_sha256: None,
        phase_receipt_sha256: String::new(),
        selection_record_sha256: Some(hash('c')),
    }
}

#[test]
fn reconciliation_repairs_every_unique_intended_prefix_and_skips_aborted_origin() {
    let seed = temp_root("journal-outbox-prefix-seed");
    let seed_generation_store = seed.path().join("generation-store");
    let seed_update = seed.path().join("update");
    let seed_transaction = hash('1');
    let seed_journal = journal_path(&seed_update, &seed_transaction);
    append_normal_with_outbox(
        &seed_journal,
        &seed_generation_store,
        entry(&seed_transaction),
        &supervisor_session('1'),
    )
    .expect("write seed origin intent");
    let committed = fs::read(&seed_journal).expect("read committed origin line");
    let intended = committed
        .strip_suffix(b"\n")
        .expect("append line ends in its newline");
    let target_len = intended.len();

    for prefix_len in 1..=target_len {
        assert_eq!(
            intended_prefix_len(b"", intended, &intended[..prefix_len]),
            Some(prefix_len),
            "every intended crash cut remains classifiable in memory"
        );
    }
    assert_eq!(intended_prefix_len(b"", intended, b""), None);
    assert_eq!(intended_prefix_len(b"", intended, &committed), None);
    let mut corrupt = intended.to_vec();
    corrupt[target_len - 1] ^= 1;
    assert_eq!(intended_prefix_len(b"", intended, &corrupt), None);

    let representative_prefixes = [1, target_len / 2, target_len - 1, target_len]
        .into_iter()
        .collect::<BTreeSet<_>>();
    for prefix_len in representative_prefixes {
        let root = temp_root(&format!("journal-outbox-prefix-cut-{prefix_len}"));
        let generation_store = root.path().join("generation-store");
        let update = root.path().join("update");
        let transaction = hash('2');
        let journal = journal_path(&update, &transaction);
        append_normal_with_outbox(
            &journal,
            &generation_store,
            entry(&transaction),
            &supervisor_session('1'),
        )
        .expect("write origin intent");
        let full = fs::read(&journal).expect("read origin");
        let intended = full.strip_suffix(b"\n").expect("read intended line");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&journal)
            .expect("open fixture journal");
        file.write_all(&intended[..prefix_len])
            .expect("write arbitrary intended prefix");
        file.sync_all().expect("sync prefix fixture");
        drop(file);

        reconcile_operation_outbox(&generation_store, &update).expect("repair exact unique prefix");
        assert_eq!(
            fs::metadata(&journal)
                .expect("retained repaired leaf")
                .len(),
            0
        );
        reconcile_operation_outbox(&generation_store, &update).expect("repeat is stable");
        assert_eq!(
            sealed_record_count(&generation_store.join("journal-operations/repair-intents"),),
            1,
            "each cut has one immutable tail-repair authorization"
        );
        assert_eq!(
            sealed_record_count(
                &generation_store
                    .join("journal-operations/observations")
                    .join(&transaction),
            ),
            4,
            "commit, repair, post-repair, and aborted-origin observations remain durable"
        );
        recover_twice(&generation_store, &update, root.path());
        assert!(
            aborted_before_origin(&generation_store, &journal)
                .expect("read retained aborted-origin bridge"),
            "recovery skips the exact aborted origin without fabricating a journal"
        );
    }

    let root = temp_root("journal-outbox-committed-cut");
    let generation_store = root.path().join("generation-store");
    let update = root.path().join("update");
    let transaction = hash('3');
    let journal = journal_path(&update, &transaction);
    append_normal_with_outbox(
        &journal,
        &generation_store,
        entry(&transaction),
        &supervisor_session('1'),
    )
    .expect("write committed m-plus-one image");
    let committed = fs::read(&journal).expect("read committed m-plus-one image");
    assert_eq!(
        committed.len(),
        committed
            .strip_suffix(b"\n")
            .expect("committed m-plus-one image ends in its newline")
            .len()
            + 1
    );
    reconcile_operation_outbox(&generation_store, &update).expect("observe committed image");
    reconcile_operation_outbox(&generation_store, &update).expect("repeat committed observation");
    assert_eq!(
        sealed_record_count(&generation_store.join("journal-operations/repair-intents")),
        0,
        "a complete line is never offered a repair intent"
    );
    assert_eq!(
        sealed_record_count(
            &generation_store
                .join("journal-operations/observations")
                .join(&transaction),
        ),
        1,
        "append and reconciliation bind the same committed image observation"
    );
    recover_twice(&generation_store, &update, root.path());
    assert!(terminal_receipt_exists(&journal).expect("read terminal receipt evidence"));
    assert!(
        !aborted_before_origin(&generation_store, &journal)
            .expect("committed image cannot bridge as an aborted origin")
    );
}

fn sealed_record_count(root: &Path) -> usize {
    match fs::read_dir(root) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.file_type().is_ok_and(|kind| kind.is_file())
                    && entry.file_name().to_string_lossy().ends_with(".sealed")
            })
            .count(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => panic!("read immutable outbox evidence: {error}"),
    }
}

fn recover_twice(generation_store: &Path, update: &Path, root: &Path) {
    let catalog = root.join("catalog.db");
    let selection = root.join("selection");
    for attempt in 1..=2 {
        recover_prior_transactions(
            generation_store,
            update,
            &catalog,
            &selection,
            &supervisor_session('2'),
        )
        .unwrap_or_else(|error| {
            panic!(
                "recovery attempt {attempt} is stable after a representative crash cut: {error:?}"
            )
        });
    }
}

#[test]
fn reconciliation_rejects_complete_invalid_and_ambiguous_tails() {
    let root = temp_root("journal-outbox-nonprefix");
    let generation_store = root.path().join("generation-store");
    let update = root.path().join("update");
    let transaction = hash('3');
    let journal = journal_path(&update, &transaction);
    append_normal_with_outbox(
        &journal,
        &generation_store,
        entry(&transaction),
        &supervisor_session('1'),
    )
    .expect("write origin intent");
    let full = fs::read(&journal).expect("read complete origin line");
    let mut complete_invalid = full
        .strip_suffix(b"\n")
        .expect("read complete intended line")
        .to_vec();
    let final_byte = complete_invalid.last_mut().expect("nonempty sealed line");
    *final_byte ^= 1;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&journal)
        .expect("open fixture journal");
    file.write_all(&complete_invalid)
        .expect("write complete invalid tail");
    file.sync_all().expect("sync complete invalid tail");
    drop(file);
    let retained = fs::read(&journal).expect("read retained invalid tail");

    assert_eq!(
        error_code(reconcile_operation_outbox(&generation_store, &update)),
        "portable_journal_outbox"
    );
    assert_eq!(fs::read(&journal).expect("reread invalid tail"), retained);

    let ambiguous_root = temp_root("journal-outbox-ambiguous");
    let ambiguous_generation_store = ambiguous_root.path().join("generation-store");
    let ambiguous_update = ambiguous_root.path().join("update");
    let transaction = hash('4');
    let journal = journal_path(&ambiguous_update, &transaction);
    append_normal_with_outbox(
        &journal,
        &ambiguous_generation_store,
        entry(&transaction),
        &supervisor_session('1'),
    )
    .expect("write first immutable intent");
    let first_line = fs::read(&journal)
        .expect("read first sealed line")
        .strip_suffix(b"\n")
        .expect("first line ends in newline")
        .to_vec();
    let alternate_journal = journal_path(
        &ambiguous_root.path().join("alternate-update"),
        &transaction,
    );
    let mut alternate = entry(&transaction);
    alternate.transcript_sha256 = sha256_hex(b"ambiguous alternate origin");
    append(&alternate_journal, alternate).expect("construct alternate sealed origin");
    let alternate_line = fs::read(&alternate_journal)
        .expect("read alternate sealed line")
        .strip_suffix(b"\n")
        .expect("alternate line ends in newline")
        .to_vec();
    assert_ne!(
        first_line, alternate_line,
        "ambiguity needs two distinct intents"
    );
    assert_eq!(
        intended_prefix_len(b"", &first_line, &first_line[..1]),
        Some(1)
    );
    assert_eq!(
        intended_prefix_len(b"", &alternate_line, &first_line[..1]),
        Some(1)
    );
    record_origin_append_intent(&ambiguous_generation_store, &journal, alternate_line)
        .expect("record second immutable origin intent");
    fs::write(&journal, &first_line[..1]).expect("write ambiguous first-byte crash cut");
    assert_eq!(
        error_code(reconcile_operation_outbox(
            &ambiguous_generation_store,
            &ambiguous_update
        )),
        "portable_journal_outbox"
    );
    assert_eq!(
        fs::read(&journal).expect("ambiguous suffix remains retained"),
        first_line[..1]
    );
}

#[test]
fn aborted_origin_requires_exact_intent_for_zero_and_absent_journal_images() {
    for (label, make_absent) in [("zero", false), ("absent", true)] {
        let root = temp_root(&format!("journal-outbox-origin-{label}"));
        let generation_store = root.path().join("generation-store");
        let update = root.path().join("update");
        let transaction = hash('4');
        let journal = journal_path(&update, &transaction);
        append_normal_with_outbox(
            &journal,
            &generation_store,
            entry(&transaction),
            &supervisor_session('1'),
        )
        .expect("record exact origin intent");
        if make_absent {
            let retained = journal.with_file_name("retained-origin-journal.json");
            fs::rename(&journal, retained).expect("retain fixture instead of deleting journal");
        } else {
            fs::write(&journal, b"").expect("make zero-byte origin image");
        }

        reconcile_operation_outbox(&generation_store, &update)
            .expect("record exact aborted-origin observation");
        reconcile_operation_outbox(&generation_store, &update)
            .expect("repeat exact aborted-origin observation");
        assert!(
            aborted_before_origin(&generation_store, &journal)
                .expect("read exact aborted-origin bridge"),
            "only the immutable Origin -> Prepared intent bridges the empty image"
        );
        assert_eq!(
            journal.exists(),
            !make_absent,
            "reconciliation never fabricates a journal"
        );
        assert_eq!(
            sealed_record_count(
                &generation_store
                    .join("journal-operations/observations")
                    .join(&transaction),
            ),
            2,
            "commit and aborted-origin evidence remain immutable"
        );
        recover_twice(&generation_store, &update, root.path());
        assert!(
            aborted_before_origin(&generation_store, &journal)
                .expect("recovery retains exact aborted-origin evidence")
        );
    }
}

#[test]
fn journal_outbox_g_tx_buckets_authenticated_foreign_intents_before_prefix_matching() {
    let root = temp_root("journal-outbox-g-tx");
    let generation_store = root.path().join("generation-store");
    let update = root.path().join("update");
    let transaction = hash('8');
    let foreign_transaction = hash('9');
    let journal = journal_path(&update, &transaction);
    let foreign_journal = journal_path(&update, &foreign_transaction);

    append_normal_with_outbox(
        &journal,
        &generation_store,
        entry(&transaction),
        &supervisor_session('1'),
    )
    .expect("record local immutable intent");
    append(&foreign_journal, entry(&foreign_transaction)).expect("write foreign sealed line");
    let local_line = fs::read(&journal)
        .expect("read local sealed line")
        .strip_suffix(b"\n")
        .expect("local line ends in newline")
        .to_vec();
    let foreign_line = fs::read(&foreign_journal)
        .expect("read foreign sealed line")
        .strip_suffix(b"\n")
        .expect("foreign line ends in newline")
        .to_vec();
    record_origin_append_intent(&generation_store, &foreign_journal, foreign_line.clone())
        .expect("authenticate foreign intent");
    let shared_prefix_len = local_line
        .iter()
        .zip(&foreign_line)
        .take_while(|(left, right)| left == right)
        .count();
    assert!(
        shared_prefix_len > 0,
        "fixture needs a prefix that would be ambiguous without transaction bucketing"
    );
    fs::write(&journal, &local_line[..shared_prefix_len])
        .expect("write local torn prefix sharing a foreign prefix");

    reconcile_operation_outbox(&generation_store, &update)
        .expect("foreign authenticated intent cannot ambiguate this transaction bucket");
    assert_eq!(
        fs::metadata(&journal)
            .expect("tail repair retains the local journal leaf")
            .len(),
        0
    );
}

#[test]
fn journal_retains_a_torn_tail_and_rejects_semantic_repair() {
    let root = temp_root("journal-torn-tail");
    let transaction = hash('1');
    let journal = journal_path(root.path(), &transaction);
    let mut prepared = journal_entry(JournalPhase::Prepared);
    prepared.transaction_id = transaction.clone();
    append(&journal, prepared).expect("append valid head");
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&journal)
        .expect("open journal tail");
    file.write_all(b"{\"sequence\":2,\"phase\":")
        .expect("write torn tail");
    file.sync_all().expect("sync torn-tail fixture");
    drop(file);
    let retained = std::fs::read(&journal).expect("read retained torn journal");

    assert_eq!(
        error_code(read_entries(&journal)),
        "portable_journal_invalid"
    );
    assert_eq!(
        error_code(append(&journal, {
            let mut entry = journal_entry(JournalPhase::GenerationPublished);
            entry.transaction_id = transaction;
            entry
        })),
        "portable_journal_outbox",
        "ordinary append first fails closed at mandatory durable reconciliation"
    );
    assert_eq!(
        std::fs::read(&journal).expect("reread retained torn journal"),
        retained
    );
}

#[test]
fn journal_rejects_an_entry_bound_to_a_different_transaction_directory() {
    let root = temp_root("journal-transaction-binding");
    let journal = journal_path(root.path(), &hash('1'));

    assert_eq!(
        error_code(append(&journal, journal_entry(JournalPhase::Prepared))),
        "portable_journal_path"
    );
    assert!(!journal.exists());
}
