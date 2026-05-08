use renderpilot_application::{
    AppError, AppResult, OperationItemRecord, OperationJournalEntry, OperationKind,
    OperationRecord, OperationRepository, OperationStatus, UnixTimestampMillis,
};
use renderpilot_domain::{ArtifactId, OperationId, PathRef};
use renderpilot_storage_sqlite::SqliteStorage;

use super::filesystem::current_timestamp_millis;

pub(super) struct RetryableReplaceOperation {
    pub(super) journal: OperationJournalEntry,
}

impl RetryableReplaceOperation {
    pub(super) fn load(
        storage: &SqliteStorage,
        operation_id: &OperationId,
        action_name: &str,
        action_phrase: &str,
    ) -> AppResult<Self> {
        Self::load_with_policy(
            storage,
            operation_id,
            action_name,
            action_phrase,
            ReplaceOperationLoadPolicy::Retry,
        )
    }

    pub(super) fn load_for_rollback(
        storage: &SqliteStorage,
        operation_id: &OperationId,
        action_name: &str,
        action_phrase: &str,
    ) -> AppResult<Self> {
        Self::load_with_policy(
            storage,
            operation_id,
            action_name,
            action_phrase,
            ReplaceOperationLoadPolicy::Rollback,
        )
    }

    pub(super) fn mark_running(&mut self, storage: &SqliteStorage) -> AppResult<()> {
        self.transition_to(storage, OperationStatus::Running, None)
    }

    pub(super) fn mark_completed(&mut self, storage: &SqliteStorage) -> AppResult<()> {
        self.transition_to_terminal(storage, OperationStatus::Completed)
    }

    pub(super) fn mark_failed(&mut self, storage: &SqliteStorage) -> AppResult<()> {
        self.transition_to_terminal(storage, OperationStatus::Failed)
    }

    pub(super) fn mark_blocked(&mut self, storage: &SqliteStorage) -> AppResult<()> {
        self.transition_to_terminal(storage, OperationStatus::Blocked)
    }

    pub(super) fn mark_rolled_back(&mut self, storage: &SqliteStorage) -> AppResult<()> {
        self.transition_to_terminal(storage, OperationStatus::RolledBack)
    }

    pub(super) fn capture_failure(&mut self, storage: &SqliteStorage, error: AppError) -> AppError {
        self.capture_transition_error(storage, error, Self::mark_failed, "failed")
    }

    pub(super) fn capture_blocked(&mut self, storage: &SqliteStorage, error: AppError) -> AppError {
        self.capture_transition_error(storage, error, Self::mark_blocked, "blocked")
    }

    fn load_with_policy(
        storage: &SqliteStorage,
        operation_id: &OperationId,
        action_name: &str,
        action_phrase: &str,
        policy: ReplaceOperationLoadPolicy,
    ) -> AppResult<Self> {
        let entry = storage
            .find_operation_entry(operation_id)?
            .ok_or_else(|| AppError::operation_not_found(operation_id.as_str()))?;

        if entry.operation().kind != OperationKind::ReplaceComponent {
            return Err(AppError::invalid_input(format!(
                "{action_name} is only supported for replace-component plans: {}",
                operation_id.as_str()
            )));
        }

        policy.ensure_allowed(entry.operation())?;

        if entry.is_empty() {
            return Err(AppError::invalid_input(format!(
                "operation has no items to {action_phrase}: {}",
                operation_id.as_str()
            )));
        }

        Ok(Self { journal: entry })
    }

    fn transition_to(
        &mut self,
        storage: &SqliteStorage,
        status: OperationStatus,
        completed_at: Option<UnixTimestampMillis>,
    ) -> AppResult<()> {
        self.journal = persist_operation_state(storage, &self.journal, status, completed_at)?;

        Ok(())
    }

    fn transition_to_terminal(
        &mut self,
        storage: &SqliteStorage,
        status: OperationStatus,
    ) -> AppResult<()> {
        self.transition_to(storage, status, Some(current_timestamp_millis()?))
    }

    fn capture_transition_error<F>(
        &mut self,
        storage: &SqliteStorage,
        error: AppError,
        transition: F,
        state_name: &str,
    ) -> AppError
    where
        F: FnOnce(&mut Self, &SqliteStorage) -> AppResult<()>,
    {
        match transition(self, storage) {
            Ok(()) => error,
            Err(transition_error) => AppError::provider_failed(format!(
                "{error}; failed to persist {state_name} operation state: {transition_error}"
            )),
        }
    }
}

#[derive(Clone, Copy)]
enum ReplaceOperationLoadPolicy {
    Retry,
    Rollback,
}

impl ReplaceOperationLoadPolicy {
    fn ensure_allowed(self, operation: &OperationRecord) -> AppResult<()> {
        let allowed = match self {
            Self::Retry => matches!(
                operation.status,
                OperationStatus::Planned | OperationStatus::Failed | OperationStatus::Blocked
            ),
            Self::Rollback => matches!(
                operation.status,
                OperationStatus::Completed
                    | OperationStatus::Failed
                    | OperationStatus::Blocked
                    | OperationStatus::RolledBack
            ),
        };

        if allowed {
            Ok(())
        } else {
            Err(AppError::invalid_operation_state(
                operation.id.as_str(),
                operation.status.clone(),
            ))
        }
    }
}

pub(super) struct ReplacementPlanItem<'a> {
    pub(super) artifact_id: &'a ArtifactId,
    pub(super) replacement_path: &'a PathRef,
}

pub(super) fn require_replacement_plan_item<'a>(
    operation: &OperationRecord,
    item: &'a OperationItemRecord,
) -> AppResult<ReplacementPlanItem<'a>> {
    let artifact_id = item
        .artifact_id
        .as_ref()
        .ok_or_else(|| invalid_replacement_plan_item(operation))?;

    let replacement_path = item
        .target_path
        .as_ref()
        .ok_or_else(|| invalid_replacement_plan_item(operation))?;

    Ok(ReplacementPlanItem {
        artifact_id,
        replacement_path,
    })
}

fn invalid_replacement_plan_item(operation: &OperationRecord) -> AppError {
    AppError::invalid_input(format!(
        "operation item is not a persisted replacement plan for operation {}",
        operation.id.as_str()
    ))
}

fn persist_operation_state(
    storage: &SqliteStorage,
    journal: &OperationJournalEntry,
    status: OperationStatus,
    completed_at: Option<UnixTimestampMillis>,
) -> AppResult<OperationJournalEntry> {
    let updated_operation = operation_with_state(journal.operation(), status.clone(), completed_at);
    let updated_items = items_with_state(journal.items(), status);

    let entry = OperationJournalEntry::try_new(updated_operation, updated_items)?;
    storage.save_operation_entry(&entry)?;

    Ok(entry)
}

fn operation_with_state(
    operation: &OperationRecord,
    status: OperationStatus,
    completed_at: Option<UnixTimestampMillis>,
) -> OperationRecord {
    let mut updated = operation.clone();

    updated.status = status;
    updated.completed_at = completed_at;

    updated
}

fn items_with_state(
    items: &[OperationItemRecord],
    status: OperationStatus,
) -> Vec<OperationItemRecord> {
    items
        .iter()
        .cloned()
        .map(|mut item| {
            item.status = status.clone();
            item
        })
        .collect()
}
