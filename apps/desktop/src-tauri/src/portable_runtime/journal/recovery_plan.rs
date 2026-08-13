use std::path::Path;

use crate::portable_runtime::error::Result;

use super::{
    protocol::{JournalPhase, RecoveryAction},
    reader::read_valid_prefix,
    transition::RecoveryTransition,
};

pub(in crate::portable_runtime) fn plan_recovery(
    path: &Path,
) -> Result<Option<RecoveryTransition>> {
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
