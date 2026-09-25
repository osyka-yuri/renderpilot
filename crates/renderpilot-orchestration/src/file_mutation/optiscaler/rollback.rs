fn rollback_operation(
    prepared: &mut PreparedFileMutation<'_>,
    index: usize,
    record: &DomainOperationRecord,
) -> Result<(), ServiceError> {
    match record.effect() {
        DomainOperationEffect::Write(effect)
            if matches!(
                effect.state(),
                DomainWriteState::StageIntent { .. }
                    | DomainWriteState::Staged { .. }
                    | DomainWriteState::CaptureIntent { .. }
                    | DomainWriteState::Captured { .. }
                    | DomainWriteState::PublishIntent { .. }
                    | DomainWriteState::Applied { .. }
                    | DomainWriteState::DiscardIntent { .. }
                    | DomainWriteState::PostimageDiscarded { .. }
                    | DomainWriteState::RestoreIntent { .. }
                    | DomainWriteState::Planned
            ) =>
        {
            rollback_write(prepared, index, record, effect)
        }
        DomainOperationEffect::Delete(effect)
            if matches!(
                effect.state(),
                DomainDeleteState::Planned
                    | DomainDeleteState::CaptureIntent
                    | DomainDeleteState::Captured { .. }
                    | DomainDeleteState::Applied { .. }
                    | DomainDeleteState::RestoreIntent { .. }
            ) =>
        {
            rollback_delete(prepared, index, record, effect)
        }
        DomainOperationEffect::Relocate(effect)
            if matches!(
                effect.state(),
                DomainRelocateState::Planned
                    | DomainRelocateState::MoveIntent
                    | DomainRelocateState::Applied { .. }
                    | DomainRelocateState::ReverseIntent
            ) =>
        {
            rollback_relocate(prepared, index, effect)
        }
        DomainOperationEffect::CreateDirectory(effect)
            if matches!(
                effect.state(),
                DomainCreateDirectoryState::Planned
                    | DomainCreateDirectoryState::StageIntent
                    | DomainCreateDirectoryState::Staged { .. }
                    | DomainCreateDirectoryState::PublishIntent { .. }
                    | DomainCreateDirectoryState::Applied { .. }
                    | DomainCreateDirectoryState::DiscardIntent { .. }
                    | DomainCreateDirectoryState::PostimageDiscarded { .. }
            ) =>
        {
            rollback_directory(prepared, index, record, effect)
        }
        DomainOperationEffect::Verify(effect)
            if matches!(
                effect.state(),
                DomainVerifyState::Planned | DomainVerifyState::Applied { .. }
            ) =>
        {
            rollback_verify(prepared, index, effect)
        }
        DomainOperationEffect::PostCommitRemoveDirectory(effect) => {
            post_commit::ensure_rollback_planned(effect)
        }
        _ => Ok(()),
    }
}

fn rollback_prepared(mut prepared: PreparedFileMutation<'_>) -> Result<(), ServiceError> {
    prepared.reacquire_recovery_authority()?;
    complete_partial_materialization(&mut prepared)?;
    adopt_intent_authorized_artifacts(&mut prepared)?;
    for index in (0..prepared.journal.operations().len()).rev() {
        let record = prepared.journal.operations()[index].clone();
        rollback_operation(&mut prepared, index, &record)?;
    }
    loop {
        prepared.cleanup_private_artifacts()?;
        match prepared.journal.cleanup().clone() {
            JournalCleanup::Inactive => {
                let mut next = prepared.journal.clone();
                if let Some(workspace_id) =
                    prepared.journal.private_workspaces().len().checked_sub(1)
                {
                    let expected_identity = prepared.journal.private_workspaces()[workspace_id]
                        .identity()
                        .ok_or_else(|| crate::failed("private workspace has no durable identity"))?
                        .to_owned();
                    let target_id = u32::try_from(workspace_id)
                        .map_err(|_| crate::failed("workspace ordinal overflow"))?;
                    next.set_cleanup(JournalCleanup::WorkspaceRemoveIntent {
                        workspace_id: target_id,
                        expected_identity,
                    });
                } else {
                    let expected_identity = prepared
                        .journal
                        .control_namespace()
                        .identity()
                        .ok_or_else(|| crate::failed("control namespace has no durable identity"))?
                        .to_owned();
                    next.set_cleanup(JournalCleanup::ControlRemoveIntent { expected_identity });
                }
                prepared.cas(next)?;
            }
            JournalCleanup::WorkspaceRemoveIntent {
                workspace_id,
                expected_identity,
            } => {
                let workspace_id = usize::try_from(workspace_id)
                    .map_err(|_| crate::failed("cleanup ordinal overflow"))?;
                cleanup_private_workspace(&prepared, workspace_id, &expected_identity)?;
                let mut next = prepared.journal.clone();
                next.set_cleanup(next_workspace_cleanup_state(
                    &prepared.journal,
                    workspace_id,
                )?);
                prepared.cas(next)?;
            }
            JournalCleanup::ControlRemoveIntent { expected_identity } => {
                cleanup_transaction_namespace(
                    &prepared.transaction_root,
                    &prepared.id,
                    &prepared.journal,
                    Some(&expected_identity),
                )?;
                let mut next = prepared.journal.clone();
                next.set_cleanup(JournalCleanup::Complete);
                prepared.cas(next)?;
            }
            JournalCleanup::Complete => break,
            JournalCleanup::ArtifactRemoveIntent { .. } => {
                return Err(crate::failed(
                    "rolled-back cleanup artifact intent did not resolve to the inactive cursor",
                ));
            }
        }
    }
    prepared.delete_after_rollback()
}
