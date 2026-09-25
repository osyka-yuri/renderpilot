fn rollback_relocate(
    prepared: &mut PreparedFileMutation<'_>,
    index: usize,
    effect: &DomainRelocateEffect,
) -> Result<(), ServiceError> {
    let source = PathBuf::from(effect.source().path());
    let destination = PathBuf::from(effect.destination().path());
    let source_before = prepared.resolve_before(index, effect.source())?;
    let destination_before = prepared.resolve_before(index, effect.destination())?;
    let destination_postimage = match effect.destination().expected_after() {
        JournalAfter::Known(value) => native(value),
        JournalAfter::Pending => source_before.clone(),
    };
    if !matches!(&source_before, DiskObservation::File { .. })
        || destination_before != DiskObservation::Absent
        || !matches!(&destination_postimage, DiskObservation::File { .. })
    {
        return Err(crate::failed(
            "relocation recovery tokens are not exact files",
        ));
    }
    let source_observed = observe(&source);
    let destination_observed = observe(&destination);
    let moved =
        source_observed == DiskObservation::Absent && destination_observed == destination_postimage;
    let untouched =
        source_observed == source_before && destination_observed == DiskObservation::Absent;
    let state = effect.state().clone();
    if !moved && !untouched {
        return Err(crate::failed("relocation recovery tuple is invalid"));
    }
    if matches!(&state, DomainRelocateState::Applied { .. }) && !moved {
        return Err(crate::failed("relocation applied tuple is invalid"));
    }
    if moved && matches!(&state, DomainRelocateState::MoveIntent) {
        prepared.set_relocate_state(
            index,
            DomainRelocateState::Applied {
                source_after: durable(&DiskObservation::Absent),
                destination_after: durable(&destination_postimage),
            },
            Some(DiskObservation::Absent),
            Some(destination_postimage.clone()),
        )?;
    }
    let current = prepared.journal.operations()[index].clone();
    let current_state = match current.effect() {
        DomainOperationEffect::Relocate(value) => value.state().clone(),
        _ => return Err(crate::failed("relocation rollback action changed")),
    };
    if matches!(&current_state, DomainRelocateState::Planned)
        || (untouched && matches!(&current_state, DomainRelocateState::MoveIntent))
    {
        let mut next = prepared.journal.clone();
        if let DomainOperationEffect::Relocate(value) = next.operations_mut()[index].effect_mut() {
            *value.state_mut() = DomainRelocateState::Preserved;
        }
        prepared.cas(next)?;
    } else if moved
        && matches!(
            &current_state,
            DomainRelocateState::Applied { .. } | DomainRelocateState::ReverseIntent
        )
    {
        if matches!(&current_state, DomainRelocateState::Applied { .. }) {
            prepared.set_relocate_state(index, DomainRelocateState::ReverseIntent, None, None)?;
        }
        move_exact_no_replace(&destination, &source, &destination_postimage)?;
        let mut next = prepared.journal.clone();
        if let DomainOperationEffect::Relocate(value) = next.operations_mut()[index].effect_mut() {
            *value.state_mut() = DomainRelocateState::Preserved;
        }
        prepared.cas(next)?;
    } else if untouched && matches!(&current_state, DomainRelocateState::ReverseIntent) {
        let mut next = prepared.journal.clone();
        if let DomainOperationEffect::Relocate(value) = next.operations_mut()[index].effect_mut() {
            *value.state_mut() = DomainRelocateState::Preserved;
        }
        prepared.cas(next)?;
    }
    Ok(())
}
