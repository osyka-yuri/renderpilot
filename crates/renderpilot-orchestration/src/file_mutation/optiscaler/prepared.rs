mod cleanup;
mod delete;
mod directory;
mod queries;
mod relocate;
mod verify;
mod write;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::*;

/// Prepared state owns exactly one shared domain journal.
#[derive(Debug)]
pub(super) enum JournalAuthority {
    Consumed,
    // Every state owns the same large, move-only storage authority. Boxing
    // keeps the state tag compact without shortening the authority lifetime.
    ActivePreparing(Box<PreparingOptiScalerJournalAggregate>),
    ActivePrepared(Box<PreparedOptiScalerJournalAggregate>),
    ActiveCommitted(Box<CommittedOptiScalerJournalAggregate>),
    Recovering(Box<RecoveringOptiScalerJournalAggregate>),
}

pub(crate) struct PreparedFileMutation<'a> {
    pub(super) id: String,
    pub(super) game_id: renderpilot_domain::GameId,
    pub(super) executor: &'a PeerMutationExecutor,
    pub(super) authority: JournalAuthority,
    pub(super) transaction_root: PathBuf,
    pub(super) journal: OptiScalerJournal,
    pub(super) journal_json: String,
    pub(super) next_operation_id: usize,
    pub(super) managed_endpoint_roots: HashMap<String, PathBuf>,
}

impl PreparedFileMutation<'_> {
    fn observe_endpoint(&self, path: &Path) -> Result<DiskObservation, ServiceError> {
        let key = crate::paths::normalized_key(path);
        let Some(root) = self.managed_endpoint_roots.get(&key) else {
            return Ok(observe(path));
        };
        observe_managed_descendant(root, path)
    }
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    fn take_authority(&mut self) -> Result<JournalAuthority, ServiceError> {
        match std::mem::replace(&mut self.authority, JournalAuthority::Consumed) {
            JournalAuthority::Consumed => {
                Err(crate::failed("OptiScaler journal authority was consumed"))
            }
            authority => Ok(authority),
        }
    }

    fn restore_authority(&mut self, authority: JournalAuthority) {
        debug_assert!(matches!(&self.authority, JournalAuthority::Consumed));
        self.authority = authority;
    }

    pub(super) fn reacquire_recovery_authority(&mut self) -> Result<(), ServiceError> {
        if !matches!(&self.authority, JournalAuthority::Consumed) {
            return Ok(());
        }
        let mut candidates = self
            .executor
            .recover_pending_file_mutation_candidates_for_game(&self.game_id, |row| {
                row.id == self.id
            })?;
        let candidate = match candidates.len() {
            1 => candidates.pop().expect("candidate length checked"),
            0 => {
                return Err(crate::failed(
                    "OptiScaler journal authority was consumed and could not be reacquired",
                ));
            }
            _ => {
                return Err(crate::failed(
                    "OptiScaler journal authority reacquired duplicate candidates",
                ));
            }
        };
        let JournalAuthority::Recovering(proof) = (match candidate {
            PendingFileMutationRecoveryCandidate::OptiScaler(proof) => {
                JournalAuthority::Recovering(proof)
            }
            PendingFileMutationRecoveryCandidate::NotAggregate(_) => {
                return Err(crate::failed(
                    "consumed OptiScaler journal authority is not an aggregate",
                ));
            }
        }) else {
            unreachable!("recovery candidate mapping is exhaustive")
        };
        self.journal = proof.journal().clone();
        proof
            .current_journal_json()
            .clone_into(&mut self.journal_json);
        self.next_operation_id = self
            .journal
            .operations()
            .iter()
            .position(|record| !operation_is_terminal(record))
            .unwrap_or_else(|| self.journal.operations().len());
        self.authority = JournalAuthority::Recovering(proof);
        Ok(())
    }

    fn current_operation_index(
        &self,
        path: &Path,
        expected_action: OptiScalerAction,
    ) -> Result<usize, ServiceError> {
        let index = self.next_operation_id;
        let record = self.journal.operations().get(index).ok_or_else(|| {
            crate::failed(format!(
                "no current OptiScaler operation for {}",
                path.display()
            ))
        })?;
        if action(record.effect()) != expected_action {
            return Err(crate::failed(format!(
                "operation {index} expects {:?}, not {:?}",
                action(record.effect()),
                expected_action
            )));
        }
        if endpoint_for(record.effect(), path).is_none() {
            return Err(crate::failed(format!(
                "operation {index} endpoint path mismatch for {}",
                path.display()
            )));
        }
        if operation_is_terminal(record) {
            return Err(crate::failed(format!(
                "operation {index} is already terminal"
            )));
        }
        Ok(index)
    }

    pub(super) fn resolve_before(
        &self,
        index: usize,
        endpoint: &DomainOperationEndpoint,
    ) -> Result<DiskObservation, ServiceError> {
        match endpoint.preimage() {
            DomainPreimage::Initial { observation, .. } => Ok(native(observation)),
            DomainPreimage::PriorPostimage {
                operation_id,
                endpoint: role,
            } => {
                let prior_index = usize::try_from(*operation_id)
                    .map_err(|_| crate::failed("prior operation id overflow"))?;
                if prior_index >= index {
                    return Err(crate::failed(
                        "prior postimage must precede the current ordinal",
                    ));
                }
                let prior = self
                    .journal
                    .operations()
                    .get(prior_index)
                    .ok_or_else(|| crate::failed("prior postimage is outside the journal"))?;
                let endpoint = endpoint_by_role(prior.effect(), *role)
                    .ok_or_else(|| crate::failed("prior endpoint role does not match action"))?;
                Ok(expected_native(endpoint))
            }
        }
    }

    fn set_write_state(
        &mut self,
        index: usize,
        state: DomainWriteState,
        postimage: Option<DiskObservation>,
    ) -> Result<(), ServiceError> {
        let mut next = self.journal.clone();
        if let DomainOperationEffect::Write(effect) = next.operations_mut()[index].effect_mut() {
            *effect.state_mut() = state;
            if let Some(value) = postimage {
                set_endpoint_expected(effect.endpoint_mut(), &value);
            }
        } else {
            return Err(crate::failed("write state applied to another action"));
        }
        self.cas(next)
    }
    pub(super) fn set_write_state_with_artifact(
        &mut self,
        index: usize,
        state: DomainWriteState,
        postimage: Option<DiskObservation>,
        which: ArtifactSlot,
        value: &DiskObservation,
    ) -> Result<(), ServiceError> {
        let mut next = self.journal.clone();
        if let DomainOperationEffect::Write(effect) = next.operations_mut()[index].effect_mut() {
            *effect.state_mut() = state;
            if let Some(postimage) = postimage {
                set_endpoint_expected(effect.endpoint_mut(), &postimage);
            }
        } else {
            return Err(crate::failed("write state applied to another action"));
        }
        set_artifact(&mut next.operations_mut()[index], which, value);
        self.cas(next)
    }
    fn set_delete_state(
        &mut self,
        index: usize,
        state: DomainDeleteState,
        postimage: Option<DiskObservation>,
    ) -> Result<(), ServiceError> {
        let mut next = self.journal.clone();
        if let DomainOperationEffect::Delete(effect) = next.operations_mut()[index].effect_mut() {
            *effect.state_mut() = state;
            if let Some(value) = postimage {
                set_endpoint_expected(effect.endpoint_mut(), &value);
            }
        } else {
            return Err(crate::failed("delete state applied to another action"));
        }
        self.cas(next)
    }
    fn set_delete_state_with_artifact(
        &mut self,
        index: usize,
        state: DomainDeleteState,
        postimage: Option<DiskObservation>,
        which: ArtifactSlot,
        value: &DiskObservation,
    ) -> Result<(), ServiceError> {
        let mut next = self.journal.clone();
        if let DomainOperationEffect::Delete(effect) = next.operations_mut()[index].effect_mut() {
            *effect.state_mut() = state;
            if let Some(postimage) = postimage {
                set_endpoint_expected(effect.endpoint_mut(), &postimage);
            }
        } else {
            return Err(crate::failed("delete state applied to another action"));
        }
        set_artifact(&mut next.operations_mut()[index], which, value);
        self.cas(next)
    }
    fn set_directory_state(
        &mut self,
        index: usize,
        state: DomainCreateDirectoryState,
        postimage: Option<DiskObservation>,
    ) -> Result<(), ServiceError> {
        let mut next = self.journal.clone();
        if let DomainOperationEffect::CreateDirectory(effect) =
            next.operations_mut()[index].effect_mut()
        {
            *effect.state_mut() = state;
            if let Some(value) = postimage {
                set_endpoint_expected(effect.endpoint_mut(), &value);
            }
        } else {
            return Err(crate::failed("directory state applied to another action"));
        }
        self.cas(next)
    }
    pub(super) fn set_directory_state_with_artifact(
        &mut self,
        index: usize,
        state: DomainCreateDirectoryState,
        postimage: Option<DiskObservation>,
        which: ArtifactSlot,
        value: &DiskObservation,
    ) -> Result<(), ServiceError> {
        let mut next = self.journal.clone();
        if let DomainOperationEffect::CreateDirectory(effect) =
            next.operations_mut()[index].effect_mut()
        {
            *effect.state_mut() = state;
            if let Some(postimage) = postimage {
                set_endpoint_expected(effect.endpoint_mut(), &postimage);
            }
        } else {
            return Err(crate::failed("directory state applied to another action"));
        }
        set_artifact(&mut next.operations_mut()[index], which, value);
        self.cas(next)
    }
    pub(super) fn set_relocate_state(
        &mut self,
        index: usize,
        state: DomainRelocateState,
        source: Option<DiskObservation>,
        destination: Option<DiskObservation>,
    ) -> Result<(), ServiceError> {
        let mut next = self.journal.clone();
        if let DomainOperationEffect::Relocate(effect) = next.operations_mut()[index].effect_mut() {
            *effect.state_mut() = state;
            if let Some(value) = source {
                set_endpoint_expected(effect.source_mut(), &value);
            }
            if let Some(value) = destination {
                set_endpoint_expected(effect.destination_mut(), &value);
            }
        } else {
            return Err(crate::failed("relocation state applied to another action"));
        }
        self.cas(next)
    }
    pub(super) fn cas(&mut self, next: OptiScalerJournal) -> Result<(), ServiceError> {
        next.validate()
            .map_err(|error| crate::failed(error.to_string()))?;
        let json = serde_json::to_string(&next).map_err(|error| {
            crate::failed(format!("failed to serialize OptiScaler journal: {error}"))
        })?;
        let authority = self.take_authority()?;
        let authority = match authority {
            JournalAuthority::Consumed => {
                return Err(crate::failed("OptiScaler journal authority was consumed"));
            }
            JournalAuthority::ActivePreparing(proof) => {
                JournalAuthority::ActivePreparing(Box::new(
                    self.executor
                        .cas_preparing_optiscaler_journal_aggregate(*proof, json.clone())
                        .map_err(ServiceError::from)?,
                ))
            }
            JournalAuthority::ActivePrepared(proof) => JournalAuthority::ActivePrepared(Box::new(
                self.executor
                    .cas_prepared_optiscaler_journal_aggregate(*proof, json.clone())
                    .map_err(ServiceError::from)?,
            )),
            JournalAuthority::ActiveCommitted(proof) => {
                JournalAuthority::ActiveCommitted(Box::new(
                    self.executor
                        .cas_committed_optiscaler_journal_aggregate(*proof, json.clone())
                        .map_err(ServiceError::from)?,
                ))
            }
            JournalAuthority::Recovering(proof) => JournalAuthority::Recovering(Box::new(
                self.executor
                    .cas_recovering_optiscaler_journal_aggregate(*proof, json.clone())
                    .map_err(ServiceError::from)?,
            )),
        };
        self.authority = authority;
        self.journal = next;
        self.journal_json = json;
        Ok(())
    }

    pub(super) fn finish_preparation(&mut self) -> Result<(), ServiceError> {
        let authority = self.take_authority()?;
        let JournalAuthority::ActivePreparing(proof) = authority else {
            self.restore_authority(authority);
            return Err(crate::failed(
                "OptiScaler journal finish requires a Preparing proof",
            ));
        };
        let proof = self
            .executor
            .finish_optiscaler_journal_aggregate(*proof, self.journal_json.clone())
            .map_err(ServiceError::from)?;
        self.authority = JournalAuthority::ActivePrepared(Box::new(proof));
        Ok(())
    }

    pub(crate) fn commit_journal_aggregate(
        &mut self,
        before: AggregateBefore,
        mutation: OptiScalerAggregateMutation<'_>,
    ) -> Result<(), ServiceError> {
        let authority = self.take_authority()?;
        let JournalAuthority::ActivePrepared(proof) = authority else {
            self.restore_authority(authority);
            return Err(crate::failed(
                "OptiScaler journal commit requires a Prepared proof",
            ));
        };
        let commit = OptiScalerJournalAggregateCommit::new(before, mutation);
        let proof = self
            .executor
            .commit_optiscaler_journal_aggregate(*proof, commit)
            .map_err(ServiceError::from)?;
        self.authority = JournalAuthority::ActiveCommitted(Box::new(proof));
        Ok(())
    }
}
