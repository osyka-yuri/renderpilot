use std::{fs, io::Write, path::Path};

use serde::Serialize;

use super::{append_journal as append, error_code, hash, journal_entry, temp_root};
use crate::portable_runtime::{
    generation::{load_selected, publish},
    journal::{JournalPhase, journal_path, read_entries},
    provenance::{self, SealDomain},
    publication::publish_bytes_no_replace,
    rpu::{
        MAXIMUM_SCHEMA, MINIMUM_SCHEMA, RPU_PROTOCOL, RpuManifest, SUPERVISOR_PROTOCOL, VerifiedRpu,
    },
    selection::{
        SELECTION_PROTOCOL, SelectionRecord, SelectionRecordV2, SelectionRecordV3, SelectionState,
        append_selected, current_selection, read_selection,
    },
    signature::sha256_hex,
    win32::file::NoReplacePublication,
};

fn publish_sealed_selection_record(root: &Path, record: &impl Serialize) {
    let plaintext = serde_json::to_vec(record).expect("encode selection fixture");
    let hash = sha256_hex(&plaintext);
    let bytes = provenance::seal(
        SealDomain::Selection,
        &format!("selection:{hash}"),
        &plaintext,
    )
    .expect("seal selection fixture");
    fs::create_dir_all(root).expect("create selection root");
    fs::write(root.join(format!("{hash}.json")), bytes).expect("write selection fixture");
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
        "portable_journal_invalid"
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

#[test]
fn no_replace_publication_preserves_a_complete_existing_record() {
    let root = temp_root("publication-no-replace");
    let destination = root.path().join("records/winner.json");
    let pending = root.path().join("pending");
    assert_eq!(
        publish_bytes_no_replace(&destination, &pending, b"complete winner")
            .expect("publish winner"),
        NoReplacePublication::Published
    );
    assert_eq!(
        publish_bytes_no_replace(&destination, &pending, b"different candidate")
            .expect("reject replacement"),
        NoReplacePublication::Occupied
    );
    assert_eq!(
        std::fs::read(destination).expect("read winner"),
        b"complete winner"
    );
    assert_eq!(
        std::fs::read_dir(pending)
            .expect("read publication staging")
            .count(),
        0,
        "the exact rejected candidate must be discarded"
    );
}

#[test]
fn generation_and_selection_become_visible_only_after_complete_publication() {
    let root = temp_root("generation-publication");
    let store = root.path().join("store");
    let mut app = vec![0_u8; 0x46];
    app[..2].copy_from_slice(b"MZ");
    app[0x3c..0x40].copy_from_slice(&0x40_u32.to_le_bytes());
    app[0x40..0x46].copy_from_slice(b"PE\0\0\x64\x86");
    let rpu_sha256 = hash('d');
    let rpu = VerifiedRpu {
        manifest: RpuManifest {
            protocol: RPU_PROTOCOL.to_owned(),
            platform: "windows-x86_64-portable".to_owned(),
            version: "1.9.1".to_owned(),
            app_sha256: sha256_hex(&app),
            app_length: app.len() as u64,
            minimum_supervisor_protocol: SUPERVISOR_PROTOCOL,
            minimum_schema: MINIMUM_SCHEMA,
            maximum_schema: MAXIMUM_SCHEMA,
            portable_role: "app".to_owned(),
        },
        app_bytes: app,
        rpu_sha256: rpu_sha256.clone(),
    };
    let stale = store.join("generation-pending").join(&rpu_sha256);
    std::fs::create_dir_all(&stale).expect("create interrupted pending generation");
    std::fs::write(stale.join("partial"), b"incomplete").expect("write interrupted generation");

    assert_eq!(
        error_code(publish(&store, &rpu)),
        "portable_generation_pending"
    );
    assert!(stale.exists(), "ambient interrupted state is retained");

    let clean_store = root.path().join("clean-store");
    publish(&clean_store, &rpu).expect("publish complete generation");
    let stored = load_selected(&clean_store, &rpu_sha256).expect("load complete generation");
    assert_eq!(stored.version, "1.9.1");

    let selection_root = clean_store.join("selection");
    append_selected(&selection_root, &rpu_sha256, &hash('a'), 1)
        .expect("publish complete selection record");
    assert_eq!(
        read_selection(&selection_root)
            .expect("read selection")
            .len(),
        1
    );
}

#[test]
fn selection_reads_legacy_v2_records() {
    let root = temp_root("selection-v2-compatibility");
    let selection_root = root.path().join("selection");
    let mut record = SelectionRecordV2 {
        protocol: 2,
        sequence: 1,
        generation_sha256: hash('a'),
        previous_record_sha256: None,
        journal_sequence: 1,
        record_sha256: String::new(),
    };
    record.record_sha256 =
        sha256_hex(&serde_json::to_vec(&record).expect("encode unsigned v2 selection"));
    publish_sealed_selection_record(&selection_root, &record);

    assert_eq!(
        current_selection(&selection_root)
            .expect("read v2 selection")
            .expect("v2 selection exists")
            .generation_sha256,
        hash('a')
    );
}

#[test]
fn same_generation_reactivation_appends_a_fresh_normal_v3_selection() {
    let root = temp_root("selection-reactivation");
    let selection_root = root.path().join("selection");
    let generation = hash('a');
    let first_transaction = hash('1');
    let second_transaction = hash('2');

    let (_, first_hash) = append_selected(&selection_root, &generation, &first_transaction, 6)
        .expect("append first activation selection");
    let (_, second_hash) = append_selected(&selection_root, &generation, &second_transaction, 6)
        .expect("append reactivation selection");
    let selections = read_selection(&selection_root).expect("read normal selection chain");

    assert_eq!(selections.len(), 2);
    assert_ne!(
        first_hash, second_hash,
        "each transaction owns its own record"
    );
    assert!(matches!(
        &selections[1].record,
        SelectionRecord::V3(record)
            if record.journal_transaction_id == second_transaction
                && record.journal_sequence == 6
                && record.compensates_selection_record_sha256.is_none()
    ));
    assert_eq!(
        selections[1].selected_generation_sha256(),
        Some(generation.as_str())
    );
}

#[test]
fn selection_rejects_an_unbound_cleared_record() {
    let root = temp_root("selection-unbound-cleared");
    let selection_root = root.path().join("selection");
    let mut record = SelectionRecordV3 {
        protocol: SELECTION_PROTOCOL,
        sequence: 1,
        state: SelectionState::Cleared,
        previous_record_sha256: None,
        journal_transaction_id: hash('b'),
        journal_sequence: 1,
        compensates_selection_record_sha256: None,
        record_sha256: String::new(),
    };
    record.record_sha256 =
        sha256_hex(&serde_json::to_vec(&record).expect("encode unsigned cleared selection"));
    publish_sealed_selection_record(&selection_root, &record);

    assert_eq!(
        error_code(read_selection(&selection_root)),
        "portable_selection_invalid"
    );
}
