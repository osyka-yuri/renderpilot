fn rollback_directory(
    prepared: &mut PreparedFileMutation<'_>,
    index: usize,
    record: &DomainOperationRecord,
    effect: &DomainCreateDirectoryEffect,
) -> Result<(), ServiceError> {
    let path = PathBuf::from(effect.endpoint().path());
    let state = effect.state().clone();
    let expected = match &state {
        DomainCreateDirectoryState::Staged { stage }
        | DomainCreateDirectoryState::PublishIntent { stage } => native(stage),
        DomainCreateDirectoryState::Applied { live } => native(live),
        DomainCreateDirectoryState::DiscardIntent { directory } => native(directory),
        DomainCreateDirectoryState::StageIntent
        | DomainCreateDirectoryState::Planned
        | DomainCreateDirectoryState::PostimageDiscarded { .. }
        | DomainCreateDirectoryState::Preserved => expected_native(effect.endpoint()),
    };
    let live = observe(&path);
    let stage_observed = observe_private_artifact(prepared, index, ArtifactSlot::Stage)?;
    let discard_observed = observe_private_artifact(prepared, index, ArtifactSlot::Discard)?;
    let artifact_stage = artifact(record, ArtifactSlot::Stage);
    let artifact_discard = artifact(record, ArtifactSlot::Discard);
    match &state {
        DomainCreateDirectoryState::StageIntent => {
            if live != DiskObservation::Absent
                || artifact_stage != DiskObservation::Absent
                || artifact_discard != DiskObservation::Absent
                || stage_observed != DiskObservation::Absent
                    && !matches!(stage_observed, DiskObservation::Directory { .. })
                || discard_observed != DiskObservation::Absent
            {
                return Err(crate::failed("directory stage intent tuple is invalid"));
            }
        }
        DomainCreateDirectoryState::Staged {
            stage: expected_stage,
        } => {
            let expected_stage = native(expected_stage);
            if stage_observed != expected_stage
                || artifact_stage != expected_stage
                || live != DiskObservation::Absent
                || discard_observed != DiskObservation::Absent
                || artifact_discard != DiskObservation::Absent
            {
                return Err(crate::failed("directory staged tuple is invalid"));
            }
        }
        DomainCreateDirectoryState::PublishIntent {
            stage: expected_stage,
        } => {
            let expected_stage = native(expected_stage);
            let pre_publish = stage_observed == expected_stage && live == DiskObservation::Absent;
            let post_publish = stage_observed == DiskObservation::Absent && live == expected;
            if artifact_stage != expected_stage
                || artifact_discard != DiskObservation::Absent
                || discard_observed != DiskObservation::Absent
                || (!pre_publish && !post_publish)
            {
                return Err(crate::failed("directory publish tuple is invalid"));
            }
        }
        DomainCreateDirectoryState::Applied {
            live: expected_live,
        } => {
            let expected_live = native(expected_live);
            if live != expected_live
                || stage_observed != DiskObservation::Absent
                || artifact_stage != DiskObservation::Absent
                || discard_observed != DiskObservation::Absent
                || artifact_discard != DiskObservation::Absent
            {
                return Err(crate::failed("directory applied tuple is invalid"));
            }
        }
        DomainCreateDirectoryState::DiscardIntent { directory } => {
            let expected_directory = native(directory);
            if expected_directory != expected
                || artifact_stage != DiskObservation::Absent
                || stage_observed != DiskObservation::Absent
                || artifact_discard != DiskObservation::Absent
                || discard_observed != DiskObservation::Absent
                || (live != expected_directory && live != DiskObservation::Absent)
            {
                return Err(crate::failed("directory discard tuple is invalid"));
            }
        }
        DomainCreateDirectoryState::PostimageDiscarded {
            discard: _expected_discard,
        } => {
            if artifact_stage != DiskObservation::Absent
                || stage_observed != DiskObservation::Absent
                || artifact_discard != DiskObservation::Absent
                || discard_observed != DiskObservation::Absent
                || live != DiskObservation::Absent
            {
                return Err(crate::failed("directory discarded tuple is invalid"));
            }
        }
        DomainCreateDirectoryState::Planned | DomainCreateDirectoryState::Preserved => {}
    }
    if matches!(&state, DomainCreateDirectoryState::PublishIntent { .. }) && live == expected {
        prepared.set_directory_state_with_artifact(
            index,
            DomainCreateDirectoryState::Applied {
                live: durable(&expected),
            },
            Some(expected.clone()),
            ArtifactSlot::Stage,
            &DiskObservation::Absent,
        )?;
    }
    let current = prepared.journal.operations()[index].clone();
    let current_state = match current.effect() {
        DomainOperationEffect::CreateDirectory(value) => value.state().clone(),
        _ => return Err(crate::failed("directory rollback action changed")),
    };
    let abort_without_live_mutation = match &current_state {
        DomainCreateDirectoryState::Planned
        | DomainCreateDirectoryState::StageIntent
        | DomainCreateDirectoryState::Staged { .. } => true,
        DomainCreateDirectoryState::PublishIntent { .. } => live != expected,
        DomainCreateDirectoryState::Applied { .. }
        | DomainCreateDirectoryState::DiscardIntent { .. }
        | DomainCreateDirectoryState::PostimageDiscarded { .. }
        | DomainCreateDirectoryState::Preserved => false,
    };
    if abort_without_live_mutation {
        let mut next = prepared.journal.clone();
        if let DomainOperationEffect::CreateDirectory(value) =
            next.operations_mut()[index].effect_mut()
        {
            *value.state_mut() = DomainCreateDirectoryState::Preserved;
        }
        prepared.cas(next)?;
    } else if matches!(&current_state, DomainCreateDirectoryState::Applied { .. }) {
        let mut next = prepared.journal.clone();
        if let DomainOperationEffect::CreateDirectory(value) =
            next.operations_mut()[index].effect_mut()
        {
            *value.state_mut() = DomainCreateDirectoryState::DiscardIntent {
                directory: durable(&expected),
            };
        }
        prepared.cas(next)?;
    }
    let current = prepared.journal.operations()[index].clone();
    let current_state = match current.effect() {
        DomainOperationEffect::CreateDirectory(value) => value.state().clone(),
        _ => return Err(crate::failed("directory rollback action changed")),
    };
    if matches!(
        &current_state,
        DomainCreateDirectoryState::DiscardIntent { .. }
    ) {
        if observe(&path) == expected {
            remove_exact_leaf(&path, &private_entry_observation(&expected)?)?;
        }
        let mut next = prepared.journal.clone();
        if let DomainOperationEffect::CreateDirectory(value) =
            next.operations_mut()[index].effect_mut()
        {
            *value.state_mut() = DomainCreateDirectoryState::PostimageDiscarded {
                discard: durable(&expected),
            };
        }
        prepared.cas(next)?;
        let mut next = prepared.journal.clone();
        if let DomainOperationEffect::CreateDirectory(value) =
            next.operations_mut()[index].effect_mut()
        {
            *value.state_mut() = DomainCreateDirectoryState::Preserved;
        }
        prepared.cas(next)?;
    } else if matches!(
        &current_state,
        DomainCreateDirectoryState::PostimageDiscarded { .. }
    ) {
        let mut next = prepared.journal.clone();
        if let DomainOperationEffect::CreateDirectory(value) =
            next.operations_mut()[index].effect_mut()
        {
            *value.state_mut() = DomainCreateDirectoryState::Preserved;
        }
        prepared.cas(next)?;
    }
    Ok(())
}
