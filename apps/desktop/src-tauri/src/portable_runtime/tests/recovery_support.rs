pub(super) use std::fs;

pub(super) use rusqlite::{Connection, params};

pub(super) use super::super::provenance::{self, SealDomain};
pub(super) use super::{
    append_journal as append, error_code, hash, journal_entry, supervisor_session, temp_root,
};
pub(super) use crate::portable_runtime::{
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
    snapshot::{create as create_snapshot, restore as restore_snapshot},
};

pub(super) fn recover_prior_transactions(
    update: &std::path::Path,
    catalog: &std::path::Path,
    selection: &std::path::Path,
) -> crate::portable_runtime::error::Result<()> {
    recover_with_authority(
        &update
            .parent()
            .expect("recovery test update root has a sandbox parent")
            .join("generation-store"),
        update,
        catalog,
        selection,
        &supervisor_session('2'),
    )
}

pub(super) fn create_catalog(path: &std::path::Path, value: &str) {
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

pub(super) fn set_catalog_value(path: &std::path::Path, value: &str) {
    Connection::open(path)
        .expect("open catalog fixture")
        .execute("UPDATE fixture SET value = ?1", params![value])
        .expect("update catalog fixture value");
}

pub(super) fn catalog_value(path: &std::path::Path) -> String {
    Connection::open(path)
        .expect("open catalog fixture")
        .query_row("SELECT value FROM fixture", [], |row| row.get(0))
        .expect("read catalog fixture value")
}

pub(super) fn test_entry(
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

pub(super) fn append_test_phase(
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

pub(super) fn phase_prefix(target: JournalPhase) -> Vec<JournalPhase> {
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

pub(super) fn append_prefix(journal: &std::path::Path, transaction: &str, target: JournalPhase) {
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

pub(super) fn sealed_tail_sha256(journal: &std::path::Path) -> String {
    let bytes = fs::read(journal).expect("read sealed journal");
    let tail = bytes
        .strip_suffix(b"\n")
        .and_then(|bytes| bytes.rsplit(|byte| *byte == b'\n').next())
        .expect("sealed journal tail");
    sha256_hex(tail)
}

pub(super) fn terminal_receipt(journal: &std::path::Path, transaction: &str) -> TerminalReceiptV3 {
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
