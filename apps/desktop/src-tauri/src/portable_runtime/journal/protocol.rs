use serde::{Deserialize, Serialize};

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

impl JournalPhase {
    pub const fn is_committed_or_later(self) -> bool {
        matches!(self, Self::Committed | Self::CommitObserved)
    }

    pub const fn permits_rollback(self) -> bool {
        !self.is_committed_or_later() && !matches!(self, Self::NeedsRecovery)
    }
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
