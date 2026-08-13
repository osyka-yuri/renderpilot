use std::{fs, io::Write, path::Path};

use crate::portable_runtime::{
    journal::{JournalPhase, append_normal_with_outbox, journal_path, reconcile_operation_outbox},
    tests::{hash, journal_entry, supervisor_session, temp_root},
};

use super::{image::capture_journal_image, mutation::ExactJournalMutation, outbox};

fn entry(
    transaction_id: &str,
    phase: JournalPhase,
) -> crate::portable_runtime::journal::JournalEntry {
    let mut entry = journal_entry(phase);
    entry.transaction_id = transaction_id.to_owned();
    entry
}

fn sealed_record_count(root: &Path) -> usize {
    fs::read_dir(root)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().ends_with(".sealed"))
                .count()
        })
        .unwrap_or(0)
}

#[test]
fn journal_outbox_g_tail_retry_backfills_once_after_truncate_sync_boundary() {
    let root = temp_root("journal-outbox-tail-retry");
    let generation_store = root.path().join("generation-store");
    let update = root.path().join("update");
    let transaction = hash('7');
    let journal = journal_path(&update, &transaction);
    append_normal_with_outbox(
        &journal,
        &generation_store,
        entry(&transaction, JournalPhase::Prepared),
        &supervisor_session('1'),
    )
    .expect("record committed origin intent");
    let committed = capture_journal_image(&journal).expect("capture committed image");
    let full = fs::read(&journal).expect("read committed origin line");
    let torn_len = full.len() / 2;
    let mut torn = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&journal)
        .expect("open torn-tail fixture");
    torn.write_all(&full[..torn_len])
        .expect("write exact intended crash prefix");
    torn.sync_all().expect("sync torn-tail fixture");
    drop(torn);

    let before = capture_journal_image(&journal).expect("capture exact torn image");
    let authenticated = outbox::read_append_intents(&generation_store)
        .expect("read immutable intent")
        .pop()
        .expect("one immutable origin intent");
    let repair = outbox::record_repair_intent(
        &generation_store,
        &authenticated.key,
        &authenticated.intent,
        &before,
    )
    .expect("persist repair intent before truncate");
    let repaired = outbox::truncate_exact_tail(&journal, &repair, &before)
        .expect("truncate and sync exact tail through held handle");
    assert!(
        repaired.is_valid() && repaired.bytes().is_empty(),
        "an Origin intent restores the valid zero-byte unstarted image"
    );
    assert_eq!(
        sealed_record_count(
            &generation_store
                .join("journal-operations/observations")
                .join(&transaction)
        ),
        1,
        "the simulated pre-observation crash leaves only the prior commit receipt"
    );

    reconcile_operation_outbox(&generation_store, &update)
        .expect("valid-base retry backfills repair observations");
    reconcile_operation_outbox(&generation_store, &update)
        .expect("first subsequent reconciliation remains idempotent");
    reconcile_operation_outbox(&generation_store, &update)
        .expect("second subsequent reconciliation remains idempotent");

    let base = repaired.image().clone();
    assert!(
        outbox::observation_matches(
            &generation_store,
            &transaction,
            &outbox::observation_key(
                &authenticated.key,
                outbox::ObservationOutcome::Committed,
                committed.image(),
            ),
            committed.image().clone(),
            outbox::ObservationOutcome::Committed,
        )
        .expect("committed observation is retained")
    );
    for (subject, outcome) in [
        (
            outbox::repair_intent_key(&authenticated.key),
            outbox::ObservationOutcome::TailRemoved,
        ),
        (
            outbox::tail_repair_replay_subject(&authenticated.key),
            outbox::ObservationOutcome::NotCommittedAfterTailRepair,
        ),
        (
            authenticated.key.clone(),
            outbox::ObservationOutcome::AbortedBeforeOrigin,
        ),
    ] {
        assert!(
            outbox::observation_matches(
                &generation_store,
                &transaction,
                &outbox::observation_key(&subject, outcome, &base),
                base.clone(),
                outcome,
            )
            .expect("each post-tail boundary has one exact receipt")
        );
    }
    assert_eq!(
        sealed_record_count(
            &generation_store
                .join("journal-operations/observations")
                .join(&transaction)
        ),
        4,
        "commit, tail removal, post-tail outcome, and abort bridge are each exactly once"
    );
}

#[test]
fn journal_outbox_g_tail_untouched_base_emits_before_without_repair_outcomes() {
    let root = temp_root("journal-outbox-tail-untouched");
    let generation_store = root.path().join("generation-store");
    let update = root.path().join("update");
    let transaction = hash('8');
    let journal = journal_path(&update, &transaction);
    append_normal_with_outbox(
        &journal,
        &generation_store,
        entry(&transaction, JournalPhase::Prepared),
        &supervisor_session('1'),
    )
    .expect("record origin");
    let base = capture_journal_image(&journal).expect("capture untouched valid base");
    let deferred_generation_store = root.path().join("deferred-generation-store");
    append_normal_with_outbox(
        &journal,
        &deferred_generation_store,
        entry(&transaction, JournalPhase::GenerationPublished),
        &supervisor_session('1'),
    )
    .expect("produce a legal target through the durable facade");
    let committed_target = capture_journal_image(&journal).expect("capture durable target image");
    let target_line = committed_target.bytes()[base.bytes().len()..].to_vec();
    let target_line = target_line
        .strip_suffix(b"\n")
        .expect("sealed target has final newline")
        .to_vec();
    let repaired =
        ExactJournalMutation::truncate(&journal, &committed_target, base.bytes().len() as u64)
            .expect("simulate a durable pre-observation crash without a repair intent");
    assert!(repaired.exactly_matches(&base));
    let intent = outbox::new_append_intent(
        &journal,
        base.image().clone(),
        JournalPhase::GenerationPublished,
        target_line,
    )
    .expect("make immutable untouched-base intent");
    let key = outbox::append_intent_key(&intent).expect("derive intent key");
    outbox::record_append_intent(&generation_store, &intent).expect("record deferred intent");

    reconcile_operation_outbox(&generation_store, &update).expect("observe untouched base");
    reconcile_operation_outbox(&generation_store, &update)
        .expect("repeat untouched-base observation");
    assert!(
        outbox::observation_matches(
            &generation_store,
            &transaction,
            &outbox::observation_key(
                &key,
                outbox::ObservationOutcome::NotCommittedBeforeMutation,
                base.image(),
            ),
            base.image().clone(),
            outbox::ObservationOutcome::NotCommittedBeforeMutation,
        )
        .expect("untouched base gets the distinct Before receipt")
    );
    assert_eq!(
        sealed_record_count(&generation_store.join("journal-operations/repair-intents")),
        0,
        "untouched base never creates repair authority"
    );
    assert_eq!(
        sealed_record_count(
            &generation_store
                .join("journal-operations/observations")
                .join(&transaction)
        ),
        2,
        "origin commit and untouched Before are each exactly once"
    );
}
