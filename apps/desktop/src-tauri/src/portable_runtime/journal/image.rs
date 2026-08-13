use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    signature::sha256_hex,
};

use super::{
    paths::journal_identity,
    protocol::JournalPhase,
    reader::{ValidJournalPrefix, read_valid_prefix_bytes},
};

/// Durable, serializable facts about one exact journal image. The bytes remain
/// only in `CapturedJournalImage`; records carry this compact witness.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(in crate::portable_runtime::journal) struct JournalImage {
    pub(in crate::portable_runtime::journal) exists: bool,
    pub(in crate::portable_runtime::journal) byte_len: u64,
    pub(in crate::portable_runtime::journal) file_sha256: String,
    pub(in crate::portable_runtime::journal) sealed_head_sha256: Option<String>,
    pub(in crate::portable_runtime::journal) last_sequence: Option<u64>,
    pub(in crate::portable_runtime::journal) last_phase: Option<JournalPhase>,
}

/// One source read and one in-memory semantic reduction. No classification
/// helper reads the journal path after this capture is made.
#[derive(Clone, Debug)]
pub(in crate::portable_runtime::journal) struct CapturedJournalImage {
    transaction_id: String,
    journal_object_id: String,
    bytes: Vec<u8>,
    image: JournalImage,
    semantic: CapturedJournalSemantic,
}

#[derive(Clone, Debug)]
enum CapturedJournalSemantic {
    Valid(ValidJournalPrefix),
    Invalid,
}

impl CapturedJournalImage {
    pub(in crate::portable_runtime::journal) fn image(&self) -> &JournalImage {
        &self.image
    }

    pub(in crate::portable_runtime::journal) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(in crate::portable_runtime::journal) fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    pub(in crate::portable_runtime::journal) fn journal_object_id(&self) -> &str {
        &self.journal_object_id
    }

    pub(in crate::portable_runtime::journal) fn is_valid(&self) -> bool {
        matches!(self.semantic, CapturedJournalSemantic::Valid(_))
    }

    pub(in crate::portable_runtime::journal) fn valid_prefix(&self) -> Result<&ValidJournalPrefix> {
        match &self.semantic {
            CapturedJournalSemantic::Valid(prefix) => Ok(prefix),
            CapturedJournalSemantic::Invalid => Err(PortableRuntimeError::new(
                "portable_journal_invalid",
                "captured journal image had no valid sealed prefix",
            )),
        }
    }

    pub(in crate::portable_runtime::journal) fn matches_base(&self, base: &JournalImage) -> bool {
        self.is_valid() && self.image == *base
    }

    pub(in crate::portable_runtime::journal) fn exactly_matches(&self, expected: &Self) -> bool {
        self.transaction_id == expected.transaction_id
            && self.journal_object_id == expected.journal_object_id
            && self.image == expected.image
            && self.bytes == expected.bytes
    }
}

pub(in crate::portable_runtime::journal) fn capture_journal_image(
    path: &Path,
) -> Result<CapturedJournalImage> {
    let (exists, bytes) = match fs::read(path) {
        Ok(bytes) => (true, bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (false, Vec::new()),
        Err(error) => return Err(error.into()),
    };
    capture_journal_image_bytes(path, exists, bytes)
}

/// Reduces only supplied bytes. Held-handle mutation uses this after each
/// exact reread, so its hash, byte length, and semantics have one source.
pub(in crate::portable_runtime::journal) fn capture_journal_image_bytes(
    path: &Path,
    exists: bool,
    bytes: Vec<u8>,
) -> Result<CapturedJournalImage> {
    let (transaction_id, journal_object_id) = journal_identity(path)?;
    let semantic = match read_valid_prefix_bytes(path, &bytes) {
        Ok(prefix) => CapturedJournalSemantic::Valid(prefix),
        // The captured bytes remain authoritative even when their journal
        // semantics are malformed; repair classifiers then operate only on
        // this one invalid image and never reread a path to infer a prefix.
        Err(_) => CapturedJournalSemantic::Invalid,
    };
    let image = match &semantic {
        CapturedJournalSemantic::Valid(prefix) => JournalImage {
            exists,
            byte_len: bytes.len() as u64,
            file_sha256: sha256_hex(&bytes),
            sealed_head_sha256: prefix.head_sha256.clone(),
            last_sequence: prefix.entries.last().map(|entry| entry.sequence),
            last_phase: prefix.entries.last().map(|entry| entry.phase),
        },
        CapturedJournalSemantic::Invalid => JournalImage {
            exists,
            byte_len: bytes.len() as u64,
            file_sha256: sha256_hex(&bytes),
            sealed_head_sha256: None,
            last_sequence: None,
            last_phase: None,
        },
    };
    Ok(CapturedJournalImage {
        transaction_id,
        journal_object_id,
        bytes,
        image,
        semantic,
    })
}

/// Re-captures the source and rejects any byte, existence, identity, or
/// semantic-image drift around a destructive or observational boundary.
pub(in crate::portable_runtime::journal) fn capture_exact_current(
    path: &Path,
    expected: &CapturedJournalImage,
) -> Result<CapturedJournalImage> {
    let current = capture_journal_image(path)?;
    if !current.exactly_matches(expected) {
        return Err(PortableRuntimeError::new(
            "portable_journal_outbox",
            "journal image changed across the exact-current fence",
        ));
    }
    Ok(current)
}

pub(in crate::portable_runtime::journal) fn absent_image() -> JournalImage {
    JournalImage {
        exists: false,
        byte_len: 0,
        file_sha256: sha256_hex(b""),
        sealed_head_sha256: None,
        last_sequence: None,
        last_phase: None,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use crate::portable_runtime::{
        journal::{JournalAppendKind, JournalEntry, JournalPhase, journal_path},
        signature::sha256_hex,
        supervisor::authority::SupervisorSessionAuthority,
    };

    use super::{capture_exact_current, capture_journal_image};

    static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(0);

    fn temp_root(label: &str) -> PathBuf {
        let sequence = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "renderpilot-portable-journal-image-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("create isolated image test root");
        root
    }

    fn hash(character: char) -> String {
        std::iter::repeat_n(character, 64).collect()
    }

    fn entry(transaction_id: &str, phase: JournalPhase) -> JournalEntry {
        JournalEntry {
            protocol: 0,
            sequence: 0,
            phase,
            transaction_id: transaction_id.to_owned(),
            activation_id: hash('e'),
            selected_generation_sha256: hash('a'),
            previous_sha256: Some(hash('b')),
            transcript_sha256: sha256_hex(b"journal image gate"),
            origin_session_sha256: String::new(),
            writer_session_sha256: String::new(),
            predecessor_writer_session_sha256: None,
            append_kind: JournalAppendKind::Normal,
            previous_entry_sha256: None,
            phase_receipt_sha256: String::new(),
            selection_record_sha256: Some(hash('c')),
        }
    }

    fn append(
        root: &std::path::Path,
        journal: &std::path::Path,
        entry: JournalEntry,
        authority: &SupervisorSessionAuthority,
    ) {
        super::super::append_normal_with_outbox(
            journal,
            &root.join("generation-store"),
            entry,
            authority,
        )
        .expect("append durable test journal image");
    }

    #[test]
    fn journal_outbox_g_image_capture_fence_detects_source_change() {
        let root = temp_root("g-image");
        let transaction = hash('1');
        let journal = journal_path(&root, &transaction);
        let authority = SupervisorSessionAuthority::for_test('1');
        append(
            &root,
            &journal,
            entry(&transaction, JournalPhase::Prepared),
            &authority,
        );
        let captured = capture_journal_image(&journal).expect("capture exact source image");

        capture_exact_current(&journal, &captured).expect("unchanged source passes fence");
        fs::write(&journal, b"untrusted replacement").expect("replace source after capture");
        let error = capture_exact_current(&journal, &captured)
            .expect_err("changed source fails exact-current fence");
        assert_eq!(error.code(), "portable_journal_outbox");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn journal_outbox_g_source_target_proof_uses_only_the_captured_image() {
        let root = temp_root("g-source");
        let transaction = hash('2');
        let journal = journal_path(&root, &transaction);
        let authority = SupervisorSessionAuthority::for_test('1');
        append(
            &root,
            &journal,
            entry(&transaction, JournalPhase::Prepared),
            &authority,
        );
        let base = capture_journal_image(&journal).expect("capture base image");
        append(
            &root,
            &journal,
            entry(&transaction, JournalPhase::GenerationPublished),
            &authority,
        );
        let committed = capture_journal_image(&journal).expect("capture committed image");
        let target_line = committed.bytes()[base.bytes().len()..]
            .strip_suffix(b"\n")
            .expect("committed target ends in newline")
            .to_vec();
        let intent = super::super::outbox::new_append_intent(
            &journal,
            base.image().clone(),
            JournalPhase::GenerationPublished,
            target_line,
        )
        .expect("construct target proof");
        assert!(
            super::super::outbox::matches_exact_committed_target(&committed, &base, &intent)
                .expect("prove captured target")
        );

        fs::write(&journal, b"source changed after capture").expect("change source after capture");
        assert!(
            super::super::outbox::matches_exact_committed_target(&committed, &base, &intent)
                .expect("pure target proof never rereads source")
        );
        assert!(capture_exact_current(&journal, &committed).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
