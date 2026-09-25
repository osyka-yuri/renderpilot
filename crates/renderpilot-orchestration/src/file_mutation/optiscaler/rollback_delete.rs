fn rollback_delete(
    prepared: &mut PreparedFileMutation<'_>,
    index: usize,
    record: &DomainOperationRecord,
    effect: &DomainDeleteEffect,
) -> Result<(), ServiceError> {
    let path = PathBuf::from(effect.endpoint().path());
    let custody = private_artifact(prepared, index, ArtifactSlot::Custody)?;
    let before = prepared.resolve_before(index, effect.endpoint())?;
    let live = observe(&path);
    let custody_observed = observe_private_artifact(prepared, index, ArtifactSlot::Custody)?;
    let state = effect.state().clone();
    let custody_artifact = artifact(record, ArtifactSlot::Custody);
    if !matches!(before, DiskObservation::File { .. }) {
        return Err(crate::failed("delete preimage is not an exact file"));
    }
    match &state {
        DomainDeleteState::CaptureIntent => {
            if custody_artifact != DiskObservation::Absent
                || ((live != before || custody_observed != DiskObservation::Absent)
                    && (live != DiskObservation::Absent || custody_observed != before))
            {
                return Err(crate::failed("delete capture tuple is invalid"));
            }
            if live == DiskObservation::Absent && custody_observed == before {
                move_from_private_artifact(&custody, &path, &before)?;
            }
            if live == before && custody_observed == DiskObservation::Absent {
                let mut next = prepared.journal.clone();
                if let DomainOperationEffect::Delete(value) =
                    next.operations_mut()[index].effect_mut()
                {
                    *value.state_mut() = DomainDeleteState::Preserved;
                }
                prepared.cas(next)?;
            }
        }
        DomainDeleteState::Captured {
            custody: captured_custody,
        } => {
            let expected_custody = native(captured_custody);
            if expected_custody != before
                || custody_artifact != expected_custody
                || live != DiskObservation::Absent
                || custody_observed != before
            {
                return Err(crate::failed("delete captured tuple is invalid"));
            }
            let mut next = prepared.journal.clone();
            if let DomainOperationEffect::Delete(value) = next.operations_mut()[index].effect_mut()
            {
                *value.state_mut() = DomainDeleteState::Applied {
                    custody: durable(&expected_custody),
                };
            }
            prepared.cas(next)?;
            let mut next = prepared.journal.clone();
            if let DomainOperationEffect::Delete(value) = next.operations_mut()[index].effect_mut()
            {
                *value.state_mut() = DomainDeleteState::RestoreIntent {
                    preimage: durable(&before),
                };
            }
            prepared.cas(next)?;
            move_from_private_artifact(&custody, &path, &before)?;
            let mut next = prepared.journal.clone();
            if let DomainOperationEffect::Delete(value) = next.operations_mut()[index].effect_mut()
            {
                *value.state_mut() = DomainDeleteState::Preserved;
            }
            set_artifact(
                &mut next.operations_mut()[index],
                ArtifactSlot::Custody,
                &DiskObservation::Absent,
            );
            prepared.cas(next)?;
        }
        DomainDeleteState::Applied {
            custody: applied_custody,
        } => {
            let expected_custody = native(applied_custody);
            if expected_custody != before
                || custody_artifact != expected_custody
                || live != DiskObservation::Absent
                || custody_observed != before
            {
                return Err(crate::failed("delete applied tuple is invalid"));
            }
            let mut next = prepared.journal.clone();
            if let DomainOperationEffect::Delete(value) = next.operations_mut()[index].effect_mut()
            {
                *value.state_mut() = DomainDeleteState::RestoreIntent {
                    preimage: durable(&before),
                };
            }
            prepared.cas(next)?;
            move_from_private_artifact(&custody, &path, &before)?;
            let mut next = prepared.journal.clone();
            if let DomainOperationEffect::Delete(value) = next.operations_mut()[index].effect_mut()
            {
                *value.state_mut() = DomainDeleteState::Preserved;
            }
            set_artifact(
                &mut next.operations_mut()[index],
                ArtifactSlot::Custody,
                &DiskObservation::Absent,
            );
            prepared.cas(next)?;
        }
        DomainDeleteState::RestoreIntent { preimage } => {
            let expected_preimage = native(preimage);
            if expected_preimage != before || custody_artifact != before {
                return Err(crate::failed("delete restore token changed"));
            }
            let restored = live == before && custody_observed == DiskObservation::Absent;
            let pending = live == DiskObservation::Absent && custody_observed == before;
            if !restored && !pending {
                return Err(crate::failed("delete restore tuple is invalid"));
            }
            if pending {
                move_from_private_artifact(&custody, &path, &before)?;
            }
            let mut next = prepared.journal.clone();
            if let DomainOperationEffect::Delete(value) = next.operations_mut()[index].effect_mut()
            {
                *value.state_mut() = DomainDeleteState::Preserved;
            }
            set_artifact(
                &mut next.operations_mut()[index],
                ArtifactSlot::Custody,
                &DiskObservation::Absent,
            );
            prepared.cas(next)?;
        }
        DomainDeleteState::Planned => {
            let mut next = prepared.journal.clone();
            if let DomainOperationEffect::Delete(value) = next.operations_mut()[index].effect_mut()
            {
                *value.state_mut() = DomainDeleteState::Preserved;
            }
            prepared.cas(next)?;
        }
        DomainDeleteState::Preserved => {}
    }
    Ok(())
}
