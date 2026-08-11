use std::{
    fs::OpenOptions,
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{
    error::{PortableRuntimeError, Result},
    provenance::{self, SealDomain},
    publication::publish_bytes_no_replace,
    signature::sha256_hex,
    supervisor::authority::SupervisorSessionAuthority,
    win32::file::NoReplacePublication,
};

pub const JOURNAL_PROTOCOL: u16 = 3;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalPhase {
    Prepared,
    GenerationPublished,
    OldAppQuiesced,
    SnapshotCommitted,
    TrialSpawned,
    TrialReady,
    MigrationCommitted,
    SelectionCommitted,
    PermitSent,
    ActivationAcknowledged,
    Committed,
    CommitObserved,
    RollingBack,
    RolledBack,
    NeedsRecovery,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAction {
    RollBackPreCommit,
    RollForwardCommitted,
    FinalizeTerminalReceipt,
    NeedsManualRecovery,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JournalAppendKind {
    Origin,
    Normal,
    Recovery {
        action: RecoveryAction,
        from_phase: JournalPhase,
        to_phase: JournalPhase,
        source_sequence: u64,
        source_entry_sha256: String,
    },
}

impl JournalPhase {
    pub const fn is_committed_or_later(self) -> bool {
        matches!(self, Self::Committed | Self::CommitObserved)
    }
    pub const fn permits_rollback(self) -> bool {
        !self.is_committed_or_later() && !matches!(self, Self::NeedsRecovery)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct JournalEntry {
    pub protocol: u16,
    pub sequence: u64,
    pub phase: JournalPhase,
    pub transaction_id: String,
    pub activation_id: String,
    pub selected_generation_sha256: String,
    pub previous_sha256: Option<String>,
    pub transcript_sha256: String,
    pub origin_session_sha256: String,
    pub writer_session_sha256: String,
    pub predecessor_writer_session_sha256: Option<String>,
    pub append_kind: JournalAppendKind,
    pub previous_entry_sha256: Option<String>,
    pub phase_receipt_sha256: String,
    #[serde(default)]
    pub selection_record_sha256: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TerminalReceiptV3 {
    pub protocol: u16,
    pub phase: JournalPhase,
    pub transaction_id: String,
    pub selected_generation_sha256: String,
    pub selection_record_sha256: Option<String>,
    pub journal_head_sha256: String,
    pub origin_session_sha256: String,
    pub finalizer_session_sha256: String,
    pub predecessor_writer_session_sha256: Option<String>,
    pub terminal_journal_sequence: u64,
    pub terminal_journal_transcript_sha256: String,
    pub recovery_action: Option<RecoveryAction>,
    pub receipt_sha256: String,
}

#[derive(Clone, Debug)]
pub struct RecoveryTransition {
    action: RecoveryAction,
    from_phase: JournalPhase,
    to_phase: JournalPhase,
    source_sequence: u64,
    source_entry_sha256: String,
}

impl RecoveryTransition {
    pub fn action(&self) -> RecoveryAction {
        self.action
    }

    pub fn target_phase(&self) -> JournalPhase {
        self.to_phase
    }
}

pub fn journal_path(update_root: &Path, transaction_id: &str) -> PathBuf {
    update_root
        .join("transactions")
        .join(transaction_id)
        .join("journal.json")
}

pub fn read_entries(path: &Path) -> Result<Vec<JournalEntry>> {
    read_valid_prefix(path).map(|journal| journal.entries)
}

struct ValidJournalPrefix {
    entries: Vec<JournalEntry>,
    valid_len: u64,
    head_sha256: Option<String>,
}

fn read_valid_prefix(path: &Path) -> Result<ValidJournalPrefix> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ValidJournalPrefix {
                entries: Vec::new(),
                valid_len: 0,
                head_sha256: None,
            });
        }
        Err(error) => return Err(error.into()),
    };
    if bytes.last().is_some_and(|byte| *byte != b'\n') {
        return Err(PortableRuntimeError::new(
            "portable_journal_invalid",
            "torn journal tail was retained without mutation",
        ));
    }
    let (transaction_id, object_id) = journal_identity(path)?;
    let mut entries = Vec::new();
    let mut previous = None;
    let mut start = 0;
    let mut valid_len = 0;
    for end in bytes
        .iter()
        .enumerate()
        .filter_map(|(index, byte)| (*byte == b'\n').then_some(index))
    {
        let line = bytes.get(start..end).ok_or_else(|| {
            PortableRuntimeError::new("portable_journal_invalid", "journal line was invalid")
        })?;
        let plaintext = provenance::open(SealDomain::Journal, &object_id, line)?;
        let entry: JournalEntry = serde_json::from_slice(&plaintext).map_err(|error| {
            PortableRuntimeError::new("portable_journal_invalid", error.to_string())
        })?;
        if entry.protocol != JOURNAL_PROTOCOL
            || entry.sequence != entries.len() as u64 + 1
            || entry.transaction_id != transaction_id
            || entry.previous_entry_sha256 != previous
            || !is_digest(&entry.activation_id)
            || !is_digest(&entry.origin_session_sha256)
            || !is_digest(&entry.writer_session_sha256)
            || !is_digest(&entry.transcript_sha256)
            || entry
                .predecessor_writer_session_sha256
                .as_deref()
                .is_some_and(|value| !is_digest(value))
            || entry
                .selection_record_sha256
                .as_deref()
                .is_some_and(|value| !is_digest(value))
            || !valid_session_lineage(&entry, entries.last(), previous.as_deref())
            || (phase_requires_selection(entry.phase)
                && !entry
                    .selection_record_sha256
                    .as_deref()
                    .is_some_and(is_digest))
            || entry.phase_receipt_sha256 != phase_receipt(&entry)?
            || entries.first().is_some_and(|first: &JournalEntry| {
                first.transaction_id != entry.transaction_id
                    || first.activation_id != entry.activation_id
                    || first.selected_generation_sha256 != entry.selected_generation_sha256
                    || first.previous_sha256 != entry.previous_sha256
            })
            || !legal_entry_transition(entries.last(), &entry)
        {
            return Err(PortableRuntimeError::new(
                "portable_journal_invalid",
                "journal sequence or hash chain was invalid",
            ));
        }
        previous = Some(sha256_hex(line));
        entries.push(entry);
        valid_len = end + 1;
        start = end + 1;
    }
    Ok(ValidJournalPrefix {
        entries,
        valid_len: valid_len as u64,
        head_sha256: previous,
    })
}

pub(in crate::portable_runtime) fn append_normal(
    path: &Path,
    mut next: JournalEntry,
    authority: &SupervisorSessionAuthority,
) -> Result<JournalEntry> {
    let prefix = read_valid_prefix(path)?;
    let writer = authority.transcript_sha256();
    if let Some(last) = prefix.entries.last() {
        if last.writer_session_sha256 != writer {
            return Err(PortableRuntimeError::new(
                "portable_journal_authority",
                "normal append writer did not match the journal tail writer",
            ));
        }
        next.origin_session_sha256 = last.origin_session_sha256.clone();
        next.writer_session_sha256 = writer.to_owned();
        next.predecessor_writer_session_sha256 = last.predecessor_writer_session_sha256.clone();
        next.append_kind = JournalAppendKind::Normal;
    } else {
        next.origin_session_sha256 = writer.to_owned();
        next.writer_session_sha256 = writer.to_owned();
        next.predecessor_writer_session_sha256 = None;
        next.append_kind = JournalAppendKind::Origin;
    }
    append_with_prefix(path, next, prefix)
}

pub(in crate::portable_runtime) fn append_recovery(
    path: &Path,
    mut next: JournalEntry,
    authority: &SupervisorSessionAuthority,
    transition: RecoveryTransition,
) -> Result<JournalEntry> {
    let prefix = read_valid_prefix(path)?;
    let last = prefix.entries.last().ok_or_else(|| {
        PortableRuntimeError::new(
            "portable_journal_authority",
            "recovery append requires an existing journal tail",
        )
    })?;
    if transition.source_sequence != last.sequence
        || transition.source_entry_sha256 != prefix.head_sha256.as_deref().unwrap_or_default()
        || transition.from_phase != last.phase
        || transition.to_phase != next.phase
        || !valid_recovery_transition(
            transition.action,
            transition.from_phase,
            transition.to_phase,
        )
    {
        return Err(PortableRuntimeError::new(
            "portable_journal_authority",
            "recovery transition did not match the unchanged journal tail",
        ));
    }
    next.origin_session_sha256 = last.origin_session_sha256.clone();
    next.writer_session_sha256 = authority.transcript_sha256().to_owned();
    next.predecessor_writer_session_sha256 =
        if next.writer_session_sha256 == last.writer_session_sha256 {
            last.predecessor_writer_session_sha256.clone()
        } else {
            Some(last.writer_session_sha256.clone())
        };
    next.append_kind = JournalAppendKind::Recovery {
        action: transition.action,
        from_phase: transition.from_phase,
        to_phase: transition.to_phase,
        source_sequence: transition.source_sequence,
        source_entry_sha256: transition.source_entry_sha256,
    };
    append_with_prefix(path, next, prefix)
}

pub fn plan_recovery(path: &Path) -> Result<Option<RecoveryTransition>> {
    let prefix = read_valid_prefix(path)?;
    let Some(last) = prefix.entries.last() else {
        return Ok(None);
    };
    let (action, to_phase) = match last.phase {
        JournalPhase::Committed => (
            RecoveryAction::RollForwardCommitted,
            JournalPhase::CommitObserved,
        ),
        JournalPhase::CommitObserved | JournalPhase::RolledBack => {
            (RecoveryAction::FinalizeTerminalReceipt, last.phase)
        }
        JournalPhase::NeedsRecovery => (
            RecoveryAction::NeedsManualRecovery,
            JournalPhase::NeedsRecovery,
        ),
        JournalPhase::RollingBack => (RecoveryAction::RollBackPreCommit, JournalPhase::RolledBack),
        phase if phase.permits_rollback() => {
            (RecoveryAction::RollBackPreCommit, JournalPhase::RollingBack)
        }
        _ => (
            RecoveryAction::NeedsManualRecovery,
            JournalPhase::NeedsRecovery,
        ),
    };
    Ok(Some(RecoveryTransition {
        action,
        from_phase: last.phase,
        to_phase,
        source_sequence: last.sequence,
        source_entry_sha256: prefix.head_sha256.unwrap_or_default(),
    }))
}

fn append_with_prefix(
    path: &Path,
    mut next: JournalEntry,
    prefix: ValidJournalPrefix,
) -> Result<JournalEntry> {
    let (transaction_id, object_id) = journal_identity(path)?;
    if next.transaction_id != transaction_id {
        return Err(PortableRuntimeError::new(
            "portable_journal_path",
            "journal entry transaction did not match its canonical directory",
        ));
    }
    let last = prefix.entries.last();
    if !legal_entry_transition(last, &next) {
        return Err(PortableRuntimeError::new(
            "portable_journal_transition",
            "journal phase transition was not legal for protocol v3",
        ));
    }
    if last.is_some_and(|entry| {
        entry.transaction_id != next.transaction_id
            || entry.activation_id != next.activation_id
            || entry.selected_generation_sha256 != next.selected_generation_sha256
            || entry.previous_sha256 != next.previous_sha256
    }) {
        return Err(PortableRuntimeError::new(
            "portable_journal_invalid",
            "journal activation identity changed inside one reducer",
        ));
    }
    next.protocol = JOURNAL_PROTOCOL;
    next.sequence = prefix.entries.len() as u64 + 1;
    next.previous_entry_sha256 = prefix.head_sha256;
    next.phase_receipt_sha256 = phase_receipt(&next)?;
    let plaintext = serde_json::to_vec(&next)
        .map_err(|error| PortableRuntimeError::new("portable_journal_encode", error.to_string()))?;
    let sealed = provenance::seal(SealDomain::Journal, &object_id, &plaintext)?;
    let mut bytes = sealed.clone();
    bytes.push(b'\n');
    let parent = path.parent().ok_or_else(|| {
        PortableRuntimeError::new("portable_journal_path", "journal had no parent")
    })?;
    std::fs::create_dir_all(parent)?;
    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)?;
    if file.metadata()?.len() != prefix.valid_len {
        return Err(PortableRuntimeError::new(
            "portable_journal_invalid",
            "journal changed after validation; retained without truncation",
        ));
    }
    file.seek(SeekFrom::End(0))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    provenance::observe(SealDomain::Journal, &object_id, &sealed)?;
    Ok(next)
}

/// Creates one immutable, hash-bound terminal receipt. Existing receipts are
/// revalidated byte-for-byte; neither journal nor terminal evidence is ever
/// overwritten or garbage-collected by portable recovery.
pub(in crate::portable_runtime) fn write_terminal_receipt(
    path: &Path,
    authority: &SupervisorSessionAuthority,
) -> Result<()> {
    let prefix = read_valid_prefix(path)?;
    let last = prefix.entries.last().ok_or_else(|| {
        PortableRuntimeError::new("portable_terminal_receipt", "terminal journal was empty")
    })?;
    if !matches!(
        last.phase,
        JournalPhase::CommitObserved | JournalPhase::RolledBack
    ) {
        return Err(PortableRuntimeError::new(
            "portable_terminal_receipt",
            "journal was not terminal",
        ));
    }
    let journal_head_sha256 = prefix.head_sha256.clone().ok_or_else(|| {
        PortableRuntimeError::new(
            "portable_terminal_receipt",
            "terminal journal lacked a sealed head",
        )
    })?;
    let mut receipt = TerminalReceiptV3 {
        protocol: JOURNAL_PROTOCOL,
        phase: last.phase,
        transaction_id: last.transaction_id.clone(),
        selected_generation_sha256: last.selected_generation_sha256.clone(),
        selection_record_sha256: last.selection_record_sha256.clone(),
        journal_head_sha256,
        origin_session_sha256: last.origin_session_sha256.clone(),
        finalizer_session_sha256: last.writer_session_sha256.clone(),
        predecessor_writer_session_sha256: last.predecessor_writer_session_sha256.clone(),
        terminal_journal_sequence: last.sequence,
        terminal_journal_transcript_sha256: last.transcript_sha256.clone(),
        recovery_action: match &last.append_kind {
            JournalAppendKind::Recovery { action, .. } => Some(*action),
            JournalAppendKind::Origin | JournalAppendKind::Normal => None,
        },
        receipt_sha256: String::new(),
    };
    receipt.receipt_sha256 = sha256_hex(&serde_json::to_vec(&receipt).map_err(|error| {
        PortableRuntimeError::new("portable_terminal_receipt", error.to_string())
    })?);
    let plaintext = serde_json::to_vec(&receipt).map_err(|error| {
        PortableRuntimeError::new("portable_terminal_receipt", error.to_string())
    })?;
    let object_id = format!("terminal:{}", last.transaction_id);
    let receipt_path = path
        .parent()
        .ok_or_else(|| {
            PortableRuntimeError::new("portable_terminal_receipt", "journal had no parent")
        })?
        .join("terminal-receipt.json");
    match std::fs::read(&receipt_path) {
        Ok(existing_bytes) => {
            validate_existing_terminal_receipt(&existing_bytes, &object_id, &receipt)?;
            return provenance::observe(SealDomain::Terminal, &object_id, &existing_bytes);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    if last.writer_session_sha256 != authority.transcript_sha256() {
        return Err(PortableRuntimeError::new(
            "portable_terminal_receipt",
            "terminal receipt finalizer did not match the journal tail writer",
        ));
    }

    let bytes = provenance::seal(SealDomain::Terminal, &object_id, &plaintext)?;
    let pending_root = receipt_path
        .parent()
        .ok_or_else(|| {
            PortableRuntimeError::new("portable_terminal_receipt", "receipt had no parent")
        })?
        .join("publication-pending");
    let observed = match publish_bytes_no_replace(&receipt_path, &pending_root, &bytes)? {
        NoReplacePublication::Published => bytes,
        NoReplacePublication::Occupied => {
            let existing_bytes = std::fs::read(&receipt_path)?;
            validate_existing_terminal_receipt(&existing_bytes, &object_id, &receipt)?;
            existing_bytes
        }
    };
    provenance::observe(SealDomain::Terminal, &object_id, &observed)
}

pub fn terminal_receipt_exists(path: &Path) -> Result<bool> {
    Ok(path
        .parent()
        .ok_or_else(|| {
            PortableRuntimeError::new("portable_terminal_receipt", "journal had no parent")
        })?
        .join("terminal-receipt.json")
        .exists())
}

fn validate_existing_terminal_receipt(
    bytes: &[u8],
    object_id: &str,
    expected: &TerminalReceiptV3,
) -> Result<()> {
    let existing: TerminalReceiptV3 =
        serde_json::from_slice(&provenance::open(SealDomain::Terminal, object_id, bytes)?)
            .map_err(|error| {
                PortableRuntimeError::new("portable_terminal_receipt", error.to_string())
            })?;
    let digest = existing.receipt_sha256.clone();
    let mut unsigned = existing.clone();
    unsigned.receipt_sha256.clear();
    let valid_self_hash = digest
        == sha256_hex(&serde_json::to_vec(&unsigned).map_err(|error| {
            PortableRuntimeError::new("portable_terminal_receipt", error.to_string())
        })?);
    if !valid_self_hash || !valid_terminal_receipt_domains(&existing) || existing != *expected {
        return Err(PortableRuntimeError::new(
            "portable_terminal_receipt",
            "terminal receipt did not match immutable journal head",
        ));
    }
    Ok(())
}

fn journal_identity(path: &Path) -> Result<(String, String)> {
    if path.file_name().and_then(|value| value.to_str()) != Some("journal.json") {
        return Err(PortableRuntimeError::new(
            "portable_journal_path",
            "journal path was not the canonical transaction journal leaf",
        ));
    }
    let transaction = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_journal_path",
                "journal had no canonical transaction id",
            )
        })?;
    if !transaction.bytes().all(|byte| byte.is_ascii_hexdigit()) || transaction.len() != 64 {
        return Err(PortableRuntimeError::new(
            "portable_journal_path",
            "journal transaction id was not a canonical random nonce",
        ));
    }
    Ok((transaction.to_owned(), format!("journal:{transaction}")))
}

fn phase_receipt(entry: &JournalEntry) -> Result<String> {
    let mut unsigned = entry.clone();
    unsigned.phase_receipt_sha256.clear();
    serde_json::to_vec(&unsigned)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| PortableRuntimeError::new("portable_journal_encode", error.to_string()))
}

fn is_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn phase_requires_selection(phase: JournalPhase) -> bool {
    matches!(
        phase,
        JournalPhase::SelectionCommitted
            | JournalPhase::PermitSent
            | JournalPhase::ActivationAcknowledged
            | JournalPhase::Committed
            | JournalPhase::CommitObserved
    )
}

fn valid_session_lineage(
    entry: &JournalEntry,
    previous: Option<&JournalEntry>,
    previous_entry_sha256: Option<&str>,
) -> bool {
    match (previous, &entry.append_kind) {
        (None, JournalAppendKind::Origin) => {
            entry.origin_session_sha256 == entry.writer_session_sha256
                && entry.predecessor_writer_session_sha256.is_none()
        }
        (Some(previous), JournalAppendKind::Normal) => {
            entry.origin_session_sha256 == previous.origin_session_sha256
                && entry.writer_session_sha256 == previous.writer_session_sha256
                && entry.predecessor_writer_session_sha256
                    == previous.predecessor_writer_session_sha256
        }
        (
            Some(previous),
            JournalAppendKind::Recovery {
                action,
                from_phase,
                to_phase,
                source_sequence,
                source_entry_sha256,
            },
        ) => {
            entry.origin_session_sha256 == previous.origin_session_sha256
                && entry.predecessor_writer_session_sha256
                    == if entry.writer_session_sha256 == previous.writer_session_sha256 {
                        previous.predecessor_writer_session_sha256.clone()
                    } else {
                        Some(previous.writer_session_sha256.clone())
                    }
                && *from_phase == previous.phase
                && *to_phase == entry.phase
                && *source_sequence == previous.sequence
                && Some(source_entry_sha256.as_str()) == previous_entry_sha256
                && valid_recovery_transition(*action, *from_phase, *to_phase)
        }
        _ => false,
    }
}

fn valid_terminal_receipt_domains(receipt: &TerminalReceiptV3) -> bool {
    receipt.protocol == JOURNAL_PROTOCOL
        && matches!(
            receipt.phase,
            JournalPhase::CommitObserved | JournalPhase::RolledBack
        )
        && is_digest(&receipt.transaction_id)
        && is_digest(&receipt.selected_generation_sha256)
        && receipt
            .selection_record_sha256
            .as_deref()
            .is_none_or(is_digest)
        && is_digest(&receipt.journal_head_sha256)
        && is_digest(&receipt.origin_session_sha256)
        && is_digest(&receipt.finalizer_session_sha256)
        && receipt
            .predecessor_writer_session_sha256
            .as_deref()
            .is_none_or(is_digest)
        && is_digest(&receipt.terminal_journal_transcript_sha256)
        && is_digest(&receipt.receipt_sha256)
}

fn legal_entry_transition(previous: Option<&JournalEntry>, next: &JournalEntry) -> bool {
    legal_transition(previous.map(|entry| entry.phase), next.phase)
        || matches!(
            (&next.append_kind, previous),
            (
                JournalAppendKind::Recovery {
                    action: RecoveryAction::FinalizeTerminalReceipt,
                    from_phase,
                    to_phase,
                    ..
                },
                Some(previous),
            ) if previous.phase == *from_phase
                && *from_phase == *to_phase
                && matches!(to_phase, JournalPhase::CommitObserved | JournalPhase::RolledBack)
        )
}

fn valid_recovery_transition(
    action: RecoveryAction,
    from_phase: JournalPhase,
    to_phase: JournalPhase,
) -> bool {
    match action {
        RecoveryAction::RollBackPreCommit => {
            (from_phase == JournalPhase::RollingBack && to_phase == JournalPhase::RolledBack)
                || (from_phase.permits_rollback() && to_phase == JournalPhase::RollingBack)
        }
        RecoveryAction::RollForwardCommitted => {
            from_phase == JournalPhase::Committed && to_phase == JournalPhase::CommitObserved
        }
        RecoveryAction::FinalizeTerminalReceipt => {
            from_phase == to_phase
                && matches!(
                    to_phase,
                    JournalPhase::CommitObserved | JournalPhase::RolledBack
                )
        }
        RecoveryAction::NeedsManualRecovery => {
            from_phase == JournalPhase::NeedsRecovery && to_phase == JournalPhase::NeedsRecovery
        }
    }
}

fn legal_transition(previous: Option<JournalPhase>, next: JournalPhase) -> bool {
    use JournalPhase::*;
    matches!(
        (previous, next),
        (None, Prepared)
            | (
                Some(Prepared),
                GenerationPublished | RollingBack | NeedsRecovery
            )
            | (
                Some(GenerationPublished),
                OldAppQuiesced | TrialSpawned | RollingBack | NeedsRecovery
            )
            | (
                Some(OldAppQuiesced),
                TrialSpawned | RollingBack | NeedsRecovery
            )
            | (Some(TrialSpawned), TrialReady | RollingBack | NeedsRecovery)
            | (
                Some(TrialReady),
                SnapshotCommitted | MigrationCommitted | RollingBack | NeedsRecovery
            )
            | (
                Some(SnapshotCommitted),
                MigrationCommitted | RollingBack | NeedsRecovery
            )
            | (
                Some(MigrationCommitted),
                SelectionCommitted | RollingBack | NeedsRecovery
            )
            | (
                Some(SelectionCommitted),
                PermitSent | RollingBack | NeedsRecovery
            )
            | (
                Some(PermitSent),
                ActivationAcknowledged | RollingBack | NeedsRecovery
            )
            | (
                Some(ActivationAcknowledged),
                Committed | RollingBack | NeedsRecovery
            )
            | (Some(Committed), CommitObserved)
            | (Some(RollingBack), RolledBack | NeedsRecovery)
    )
}
