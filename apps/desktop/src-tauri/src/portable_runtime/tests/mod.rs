use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use super::{
    error::Result,
    journal::{JournalAppendKind, JournalEntry, JournalPhase, append_normal},
    signature::sha256_hex,
    supervisor::authority::SupervisorSessionAuthority,
};

mod activation;
mod admission;
mod compatibility;
mod faults;
mod job;
mod packaging;
mod publication;
mod recovery;

static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(0);

pub(super) struct TempRoot(PathBuf);

impl TempRoot {
    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(super) fn temp_root(label: &str) -> TempRoot {
    let sequence = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "renderpilot-portable-runtime-{label}-{}-{sequence}",
        std::process::id()
    ));
    std::fs::create_dir(&root).expect("create an isolated portable-runtime test root");
    TempRoot(root)
}

pub(super) fn hash(character: char) -> String {
    std::iter::repeat_n(character, 64).collect()
}

pub(super) fn journal_entry(phase: JournalPhase) -> JournalEntry {
    JournalEntry {
        protocol: 0,
        sequence: 0,
        phase,
        transaction_id: hash('d'),
        activation_id: hash('e'),
        selected_generation_sha256: hash('a'),
        previous_sha256: Some(hash('b')),
        transcript_sha256: sha256_hex(b"test transcript"),
        origin_session_sha256: String::new(),
        writer_session_sha256: String::new(),
        predecessor_writer_session_sha256: None,
        append_kind: JournalAppendKind::Normal,
        previous_entry_sha256: None,
        phase_receipt_sha256: String::new(),
        selection_record_sha256: Some(hash('c')),
    }
}

pub(super) fn supervisor_session(seed: char) -> SupervisorSessionAuthority {
    SupervisorSessionAuthority::for_test(seed)
}

pub(super) fn append_journal(path: &Path, entry: JournalEntry) -> Result<JournalEntry> {
    append_normal(path, entry, &supervisor_session('1'))
}

pub(super) fn error_code<T>(result: Result<T>) -> &'static str {
    match result {
        Ok(_) => panic!("operation unexpectedly succeeded"),
        Err(error) => error.code(),
    }
}
