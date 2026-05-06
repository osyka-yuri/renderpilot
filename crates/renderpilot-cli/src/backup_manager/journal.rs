use renderpilot_application::{
    AppError, AppResult, OperationItemRecord, OperationKind, OperationRecord, OperationRepository,
    OperationStatus, UnixTimestampMillis,
};
use renderpilot_domain::{ArtifactId, OperationId, PathRef};
use renderpilot_storage_sqlite::SqliteStorage;

use super::filesystem::current_timestamp_millis;

pub(super) struct RetryableReplaceOperation {
    pub(super) operation: OperationRecord,
    pub(super) items: Vec<OperationItemRecord>,
}

impl RetryableReplaceOperation {
    pub(super) fn load(
        storage: &SqliteStorage,
        operation_id: &OperationId,
        action_name: &str,
        action_phrase: &str,
    ) -> AppResult<Self> {
        let operation = load_retryable_replace_operation(storage, operation_id, action_name)?;
        let items = load_operation_items(storage, &operation, action_phrase)?;

        Ok(Self { operation, items })
    }

    pub(super) fn load_for_rollback(
        storage: &SqliteStorage,
        operation_id: &OperationId,
        action_name: &str,
        action_phrase: &str,
    ) -> AppResult<Self> {
        let operation = load_rollbackable_replace_operation(storage, operation_id, action_name)?;
        let items = load_operation_items(storage, &operation, action_phrase)?;

        Ok(Self { operation, items })
    }

    pub(super) fn mark_running(&mut self, storage: &SqliteStorage) -> AppResult<()> {
        self.mark_state(storage, OperationStatus::Running, None)
    }

    pub(super) fn mark_completed(&mut self, storage: &SqliteStorage) -> AppResult<()> {
        self.mark_terminal_state(storage, OperationStatus::Completed)
    }

    pub(super) fn mark_failed(&mut self, storage: &SqliteStorage) -> AppResult<()> {
        self.mark_terminal_state(storage, OperationStatus::Failed)
    }

    pub(super) fn mark_blocked(&mut self, storage: &SqliteStorage) -> AppResult<()> {
        self.mark_terminal_state(storage, OperationStatus::Blocked)
    }

    pub(super) fn mark_rolled_back(&mut self, storage: &SqliteStorage) -> AppResult<()> {
        self.mark_terminal_state(storage, OperationStatus::RolledBack)
    }

    pub(super) fn capture_failure(&mut self, storage: &SqliteStorage, error: AppError) -> AppError {
        self.capture_state_error(storage, error, Self::mark_failed, "failed")
    }

    pub(super) fn capture_blocked(&mut self, storage: &SqliteStorage, error: AppError) -> AppError {
        self.capture_state_error(storage, error, Self::mark_blocked, "blocked")
    }

    fn mark_state(
        &mut self,
        storage: &SqliteStorage,
        status: OperationStatus,
        completed_at: Option<UnixTimestampMillis>,
    ) -> AppResult<()> {
        let updated =
            mark_operation_state(storage, &self.operation, &self.items, status, completed_at)?;

        self.operation = updated.0;
        self.items = updated.1;

        Ok(())
    }

    fn mark_terminal_state(
        &mut self,
        storage: &SqliteStorage,
        status: OperationStatus,
    ) -> AppResult<()> {
        self.mark_state(storage, status, Some(current_timestamp_millis()?))
    }

    fn capture_state_error(
        &mut self,
        storage: &SqliteStorage,
        error: AppError,
        mark_state: fn(&mut Self, &SqliteStorage) -> AppResult<()>,
        state_name: &str,
    ) -> AppError {
        match mark_state(self, storage) {
            Ok(()) => error,
            Err(mark_state_error) => AppError::provider_failed(format!(
                "{error}; failed to persist {state_name} operation state: {mark_state_error}"
            )),
        }
    }
}

pub(super) struct ReplacementPlanItem<'a> {
    pub(super) artifact_id: &'a ArtifactId,
    pub(super) replacement_path: &'a PathRef,
}

pub(super) fn load_retryable_replace_operation(
    storage: &SqliteStorage,
    operation_id: &OperationId,
    action_name: &str,
) -> AppResult<OperationRecord> {
    let operation = load_replace_component_operation(storage, operation_id, action_name)?;

    if !matches!(
        operation.status,
        OperationStatus::Planned | OperationStatus::Failed | OperationStatus::Blocked
    ) {
        return Err(AppError::invalid_input(format!(
            "{action_name} can only run for planned, failed, or blocked operations: {}",
            operation_id.as_str()
        )));
    }

    Ok(operation)
}

fn load_rollbackable_replace_operation(
    storage: &SqliteStorage,
    operation_id: &OperationId,
    action_name: &str,
) -> AppResult<OperationRecord> {
    let operation = load_replace_component_operation(storage, operation_id, action_name)?;

    if !matches!(
        operation.status,
        OperationStatus::Completed
            | OperationStatus::Failed
            | OperationStatus::Blocked
            | OperationStatus::RolledBack
    ) {
        return Err(AppError::invalid_input(format!(
            "{action_name} can only run for completed, failed, blocked, or rolled_back operations: {}",
            operation_id.as_str()
        )));
    }

    Ok(operation)
}

fn load_replace_component_operation(
    storage: &SqliteStorage,
    operation_id: &OperationId,
    action_name: &str,
) -> AppResult<OperationRecord> {
    let operation = storage.find_operation(operation_id)?.ok_or_else(|| {
        AppError::invalid_input(format!("operation not found: {}", operation_id.as_str()))
    })?;

    if operation.kind != OperationKind::ReplaceComponent {
        return Err(AppError::invalid_input(format!(
            "{action_name} is only supported for replace-component plans: {}",
            operation_id.as_str()
        )));
    }

    Ok(operation)
}

fn load_operation_items(
    storage: &SqliteStorage,
    operation: &OperationRecord,
    action_phrase: &str,
) -> AppResult<Vec<OperationItemRecord>> {
    let items = storage.list_operation_items(&operation.id)?;

    if items.is_empty() {
        return Err(AppError::invalid_input(format!(
            "operation has no items to {action_phrase}: {}",
            operation.id.as_str()
        )));
    }

    Ok(items)
}

pub(super) fn require_replacement_plan_item<'a>(
    operation: &OperationRecord,
    item: &'a OperationItemRecord,
) -> AppResult<ReplacementPlanItem<'a>> {
    let artifact_id = item.artifact_id.as_ref().ok_or_else(|| {
        AppError::invalid_input(format!(
            "operation item is not a persisted replacement plan for operation {}",
            operation.id.as_str()
        ))
    })?;
    let replacement_path = item.target_path.as_ref().ok_or_else(|| {
        AppError::invalid_input(format!(
            "operation item is not a persisted replacement plan for operation {}",
            operation.id.as_str()
        ))
    })?;

    Ok(ReplacementPlanItem {
        artifact_id,
        replacement_path,
    })
}

fn mark_operation_state(
    storage: &SqliteStorage,
    operation: &OperationRecord,
    items: &[OperationItemRecord],
    status: OperationStatus,
    completed_at: Option<UnixTimestampMillis>,
) -> AppResult<(OperationRecord, Vec<OperationItemRecord>)> {
    let mut updated_operation = operation.clone();
    updated_operation.status = status.clone();
    updated_operation.completed_at = completed_at;

    let updated_items = items
        .iter()
        .cloned()
        .map(|mut item| {
            item.status = status.clone();
            item
        })
        .collect::<Vec<_>>();

    storage.upsert_operation(&updated_operation)?;
    storage.replace_operation_items(&updated_operation.id, &updated_items)?;

    Ok((updated_operation, updated_items))
}
