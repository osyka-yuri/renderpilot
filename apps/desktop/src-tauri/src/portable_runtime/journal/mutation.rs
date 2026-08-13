use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

use windows_sys::Win32::{
    Foundation::{GENERIC_READ, GENERIC_WRITE},
    Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ},
};

use crate::portable_runtime::error::{PortableRuntimeError, Result};

use super::image::{CapturedJournalImage, capture_journal_image_bytes};

/// The sole journal byte-mutation capability. Its retained Win32 handle admits
/// readers but denies write/delete/rename/replace access until capture B has
/// been reread through that exact same handle and it has been dropped.
pub(in crate::portable_runtime::journal) struct ExactJournalMutation {
    path: PathBuf,
    file: File,
    captured_a: CapturedJournalImage,
}

impl ExactJournalMutation {
    pub(in crate::portable_runtime::journal) fn append(
        path: &Path,
        captured_a: &CapturedJournalImage,
        line: &[u8],
    ) -> Result<CapturedJournalImage> {
        let mut mutation = Self::acquire(path, captured_a)?;
        mutation.file.seek(SeekFrom::End(0))?;
        mutation.file.write_all(line)?;
        mutation.file.write_all(b"\n")?;
        mutation.file.sync_all()?;
        let captured_b = mutation.capture_held()?;
        drop(mutation);
        Ok(captured_b)
    }

    pub(in crate::portable_runtime::journal) fn truncate(
        path: &Path,
        captured_a: &CapturedJournalImage,
        byte_len: u64,
    ) -> Result<CapturedJournalImage> {
        let mut mutation = Self::acquire(path, captured_a)?;
        mutation.file.set_len(byte_len)?;
        mutation.file.sync_all()?;
        let captured_b = mutation.capture_held()?;
        drop(mutation);
        Ok(captured_b)
    }

    fn acquire(path: &Path, captured_a: &CapturedJournalImage) -> Result<Self> {
        let mut options = restricted_options();
        let file = if captured_a.image().exists {
            options.open(path)?
        } else {
            if captured_a.bytes() != b"" || captured_a.image().byte_len != 0 {
                return Err(PortableRuntimeError::new(
                    "portable_journal_outbox",
                    "absent mutation capture was not the empty journal image",
                ));
            }
            options.create_new(true).open(path)?
        };
        let mut mutation = Self {
            path: path.to_owned(),
            file,
            captured_a: captured_a.clone(),
        };
        if captured_a.image().exists {
            mutation.assert_held_a()?;
        } else {
            mutation.assert_held_absent_origin()?;
        }
        Ok(mutation)
    }

    fn assert_held_a(&mut self) -> Result<()> {
        let held = self.capture_held()?;
        if !held.exactly_matches(&self.captured_a) {
            return Err(PortableRuntimeError::new(
                "portable_journal_outbox",
                "journal changed before exact held-handle mutation proof",
            ));
        }
        Ok(())
    }

    fn assert_held_absent_origin(&mut self) -> Result<()> {
        let held = self.capture_held()?;
        if held.bytes() != b"" || !held.is_valid() || held.image().byte_len != 0 {
            return Err(PortableRuntimeError::new(
                "portable_journal_outbox",
                "fresh exact journal mutation was not an empty held image",
            ));
        }
        Ok(())
    }

    fn capture_held(&mut self) -> Result<CapturedJournalImage> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        self.file.read_to_end(&mut bytes)?;
        capture_journal_image_bytes(&self.path, true, bytes)
    }
}

fn restricted_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .access_mode(GENERIC_READ | GENERIC_WRITE)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    options
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    use crate::portable_runtime::{
        journal::{JournalAppendKind, JournalEntry, JournalPhase, journal_path},
        signature::sha256_hex,
        supervisor::authority::SupervisorSessionAuthority,
    };

    use super::ExactJournalMutation;
    use crate::portable_runtime::journal::image::capture_journal_image;

    static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(0);

    fn temp_root(label: &str) -> PathBuf {
        loop {
            let sequence = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "renderpilot-portable-journal-mutation-{label}-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&root) {
                Ok(()) => return root,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("create isolated mutation test root: {error}"),
            }
        }
    }

    fn hash(character: char) -> String {
        std::iter::repeat_n(character, 64).collect()
    }

    fn entry(transaction_id: &str) -> JournalEntry {
        JournalEntry {
            protocol: 0,
            sequence: 0,
            phase: JournalPhase::Prepared,
            transaction_id: transaction_id.to_owned(),
            activation_id: hash('e'),
            selected_generation_sha256: hash('a'),
            previous_sha256: Some(hash('b')),
            transcript_sha256: sha256_hex(b"journal mutation gate"),
            origin_session_sha256: String::new(),
            writer_session_sha256: String::new(),
            predecessor_writer_session_sha256: None,
            append_kind: JournalAppendKind::Normal,
            previous_entry_sha256: None,
            phase_receipt_sha256: String::new(),
            selection_record_sha256: Some(hash('c')),
        }
    }

    fn seed_journal(root: &Path, transaction: &str) -> PathBuf {
        let journal = journal_path(root, transaction);
        super::super::append_normal_with_outbox(
            &journal,
            &root.join("generation-store"),
            entry(transaction),
            &SupervisorSessionAuthority::for_test('1'),
        )
        .expect("seed valid journal through ordinary production append");
        journal
    }

    #[test]
    fn journal_outbox_g_mutation_held_handle_allows_read_but_denies_mutation_names() {
        let root = temp_root("held-access");
        let transaction = hash('1');
        let journal = seed_journal(&root, &transaction);
        let captured = capture_journal_image(&journal).expect("capture A");
        let mutation = ExactJournalMutation::acquire(&journal, &captured)
            .expect("hold exact journal mutation capability");
        assert!(
            std::fs::OpenOptions::new()
                .read(true)
                .open(&journal)
                .is_ok(),
            "FILE_SHARE_READ permits concurrent readers"
        );
        assert!(
            std::fs::OpenOptions::new()
                .write(true)
                .open(&journal)
                .is_err()
        );
        assert!(fs::remove_file(&journal).is_err());
        assert!(fs::rename(&journal, root.join("renamed-journal.json")).is_err());
        let replacement = root.join("replacement.json");
        fs::write(&replacement, b"replacement").expect("write replacement fixture");
        assert!(fs::rename(&replacement, &journal).is_err());
        drop(mutation);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn journal_outbox_g_mutation_rejects_preacquire_change_and_absent_occupation() {
        let root = temp_root("preacquire");
        let transaction = hash('2');
        let journal = seed_journal(&root, &transaction);
        let captured = capture_journal_image(&journal).expect("capture A");
        fs::write(&journal, b"changed before held acquire").expect("change before acquire");
        assert!(ExactJournalMutation::acquire(&journal, &captured).is_err());

        let absent_transaction = hash('3');
        let absent = journal_path(&root, &absent_transaction);
        let absent_capture = capture_journal_image(&absent).expect("capture absent A");
        let parent = absent.parent().expect("canonical journal parent");
        fs::create_dir_all(parent).expect("create transaction parent");
        fs::write(&absent, b"occupied after absent capture").expect("occupy absent target");
        assert!(ExactJournalMutation::acquire(&absent, &absent_capture).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn journal_outbox_g_mutation_appends_and_truncates_through_exact_handle() {
        let root = temp_root("append-truncate");
        let transaction = hash('4');
        let journal = seed_journal(&root, &transaction);
        let captured_a = capture_journal_image(&journal).expect("capture A");
        let captured_b = ExactJournalMutation::append(&journal, &captured_a, b"torn test tail")
            .expect("append and sync through held exact handle");
        assert_eq!(
            captured_b.bytes(),
            [captured_a.bytes(), b"torn test tail\n"].concat()
        );
        let restored =
            ExactJournalMutation::truncate(&journal, &captured_b, captured_a.bytes().len() as u64)
                .expect("truncate and sync through held exact handle");
        assert!(restored.exactly_matches(&captured_a));
        let _ = fs::remove_dir_all(root);
    }
}
