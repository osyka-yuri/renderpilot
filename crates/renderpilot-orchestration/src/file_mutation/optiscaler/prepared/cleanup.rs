use std::path::PathBuf;

use renderpilot_domain::ArtifactSlot;

use super::super::{
    DiskObservation, JournalCleanup, OptiScalerAction, PendingFileMutationState, action, artifact,
    disk_observation, durable, native, operation_is_terminal, post_commit, private_artifact,
    private_entry_observation, remove_private_artifact, set_artifact, token_drift,
};
use super::{JournalAuthority, PreparedFileMutation};
use crate::ServiceError;

impl PreparedFileMutation<'_> {
    pub(in crate::file_mutation::optiscaler) fn delete_after_rollback(
        &mut self,
    ) -> Result<(), ServiceError> {
        let authority = self.take_authority()?;
        match authority {
            JournalAuthority::Consumed => {
                return Err(crate::failed("OptiScaler journal authority was consumed"));
            }
            JournalAuthority::ActivePreparing(proof) => self
                .executor
                .delete_preparing_optiscaler_journal_aggregate_after_rollback(*proof)?,
            JournalAuthority::ActivePrepared(proof) => self
                .executor
                .delete_prepared_optiscaler_journal_aggregate_after_rollback(*proof)?,
            JournalAuthority::ActiveCommitted(proof) => {
                self.restore_authority(JournalAuthority::ActiveCommitted(proof));
                return Err(crate::failed(
                    "committed OptiScaler journal cannot use rollback cleanup",
                ));
            }
            JournalAuthority::Recovering(proof) => match proof.state() {
                PendingFileMutationState::Preparing => self
                    .executor
                    .delete_preparing_recovering_optiscaler_journal_aggregate_after_rollback(
                        *proof,
                    )?,
                PendingFileMutationState::Prepared => self
                    .executor
                    .delete_prepared_recovering_optiscaler_journal_aggregate_after_rollback(
                        *proof,
                    )?,
                PendingFileMutationState::Committed => {
                    self.restore_authority(JournalAuthority::Recovering(proof));
                    return Err(crate::failed(
                        "committed recovery journal cannot use rollback cleanup",
                    ));
                }
            },
        }
        Ok(())
    }

    pub(in crate::file_mutation::optiscaler) fn delete_after_commit(
        &mut self,
    ) -> Result<(), ServiceError> {
        let authority = self.take_authority()?;
        match authority {
            JournalAuthority::Consumed => {
                return Err(crate::failed("OptiScaler journal authority was consumed"));
            }
            JournalAuthority::ActiveCommitted(proof) => self
                .executor
                .delete_committed_optiscaler_journal_aggregate(*proof)?,
            JournalAuthority::Recovering(proof)
                if proof.state() == PendingFileMutationState::Committed =>
            {
                self.executor
                    .delete_committed_recovering_optiscaler_journal_aggregate(*proof)?;
            }
            JournalAuthority::ActivePreparing(proof) => {
                self.restore_authority(JournalAuthority::ActivePreparing(proof));
                return Err(crate::failed(
                    "Preparing OptiScaler journal cannot use commit cleanup",
                ));
            }
            JournalAuthority::ActivePrepared(proof) => {
                self.restore_authority(JournalAuthority::ActivePrepared(proof));
                return Err(crate::failed(
                    "Prepared OptiScaler journal cannot use commit cleanup",
                ));
            }
            JournalAuthority::Recovering(proof) => {
                self.restore_authority(JournalAuthority::Recovering(proof));
                return Err(crate::failed("recovery journal is not Committed"));
            }
        }
        Ok(())
    }

    pub(in crate::file_mutation::optiscaler) fn complete_unmodified_operations(
        &mut self,
    ) -> Result<(), ServiceError> {
        while let Some(record) = self.journal.operations().get(self.next_operation_id) {
            if operation_is_terminal(record) {
                self.next_operation_id += 1;
                continue;
            }
            if action(record.effect()) == OptiScalerAction::PostCommitRemoveDirectory {
                break;
            }
            if action(record.effect()) == OptiScalerAction::CreateDirectory {
                let path = PathBuf::from(record.effect().endpoints()[0].path());
                self.create_directory(&path)?;
                continue;
            }
            if action(record.effect()) != OptiScalerAction::Verify {
                return Err(crate::failed(format!(
                    "operation {} was not applied",
                    self.next_operation_id
                )));
            }
            let path = PathBuf::from(record.effect().endpoints()[0].path());
            self.verify_unchanged(&path)?;
        }
        Ok(())
    }
    pub(in crate::file_mutation::optiscaler) fn apply_post_commit_directory_cleanup(
        &mut self,
    ) -> Result<(), ServiceError> {
        post_commit::apply(self)
    }

    /// Remove private children through the journal-owned cursor. The cursor is
    /// written before the unlink so recovery never infers ownership by name.
    pub(in crate::file_mutation::optiscaler) fn cleanup_private_artifacts(
        &mut self,
    ) -> Result<(), ServiceError> {
        let mut completed = Vec::new();
        let pending_intent = match self.journal.cleanup() {
            JournalCleanup::ArtifactRemoveIntent {
                operation_id,
                artifact: which,
                expected,
            } => Some((*operation_id, *which, native(expected))),
            _ => None,
        };
        if let Some((operation_id, which, expected)) = pending_intent {
            let artifact = private_artifact(
                self,
                usize::try_from(operation_id)
                    .map_err(|_| crate::failed("cleanup ordinal overflow"))?,
                which,
            )?;
            let observed = match artifact
                .namespace
                .directory()
                .observe_leaf(&artifact.leaf)?
            {
                None => DiskObservation::Absent,
                Some(entry) => disk_observation(&entry)?,
            };
            match observed {
                observed if observed == expected => {
                    remove_private_artifact(&artifact, &private_entry_observation(&expected)?)?;
                }
                DiskObservation::Absent => {}
                observed => return Err(token_drift(&artifact.path, &expected, &observed)),
            }
            completed.push((operation_id, which));
            let mut next = self.journal.clone();
            set_artifact(
                &mut next.operations_mut()[usize::try_from(operation_id)
                    .map_err(|_| crate::failed("cleanup ordinal overflow"))?],
                which,
                &DiskObservation::Absent,
            );
            next.set_cleanup(JournalCleanup::Inactive);
            self.cas(next)?;
        }
        if matches!(self.journal.cleanup(), JournalCleanup::Complete) {
            return Ok(());
        }
        for index in 0..self.journal.operations().len() {
            let op_id =
                u32::try_from(index).map_err(|_| crate::failed("operation index overflow"))?;
            for which in [
                ArtifactSlot::Custody,
                ArtifactSlot::Stage,
                ArtifactSlot::Discard,
            ] {
                if completed.contains(&(op_id, which)) {
                    continue;
                }
                let expected = artifact(&self.journal.operations()[index], which);
                if expected == DiskObservation::Absent {
                    continue;
                }
                let artifact = private_artifact(self, index, which)?;
                let observed = match artifact
                    .namespace
                    .directory()
                    .observe_leaf(&artifact.leaf)?
                {
                    None => DiskObservation::Absent,
                    Some(entry) => disk_observation(&entry)?,
                };
                if observed != expected {
                    return Err(token_drift(&artifact.path, &expected, &observed));
                }
                let mut next = self.journal.clone();
                next.set_cleanup(JournalCleanup::ArtifactRemoveIntent {
                    operation_id: op_id,
                    artifact: which,
                    expected: durable(&expected),
                });
                self.cas(next)?;
                remove_private_artifact(&artifact, &private_entry_observation(&expected)?)?;
                let mut next = self.journal.clone();
                set_artifact(
                    &mut next.operations_mut()[index],
                    which,
                    &DiskObservation::Absent,
                );
                next.set_cleanup(JournalCleanup::Inactive);
                self.cas(next)?;
                completed.push((op_id, which));
            }
        }
        Ok(())
    }
}
