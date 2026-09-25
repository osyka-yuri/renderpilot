fn rollback_write(
    prepared: &mut PreparedFileMutation<'_>,
    index: usize,
    record: &DomainOperationRecord,
    effect: &DomainWriteEffect,
) -> Result<(), ServiceError> {
    let path = PathBuf::from(effect.endpoint().path());
    let expected = expected_native(effect.endpoint());
    let before = prepared.resolve_before(index, effect.endpoint())?;
    let live = observe(&path);
    let custody_observed = observe_private_artifact(prepared, index, ArtifactSlot::Custody)?;
    let stage_observed = observe_private_artifact(prepared, index, ArtifactSlot::Stage)?;
    let discard_observed = observe_private_artifact(prepared, index, ArtifactSlot::Discard)?;
    let state = effect.state().clone();
    let artifact_custody = artifact(record, ArtifactSlot::Custody);
    let artifact_stage = artifact(record, ArtifactSlot::Stage);
    let artifact_discard = artifact(record, ArtifactSlot::Discard);

    let recorded = RollbackRecorded {
        state: &state,
        artifact_custody: &artifact_custody,
        artifact_stage: &artifact_stage,
        artifact_discard: &artifact_discard,
    };
    let expected_facts = RollbackExpected {
        before: &before,
        expected: &expected,
    };
    let observed = RollbackObserved {
        live: &live,
        stage: &stage_observed,
        custody: &custody_observed,
        discard: &discard_observed,
    };

    let situation = classify_rollback_situation(&recorded, &expected_facts, &observed)?;

    match situation {
        RollbackSituation::AlreadyPreserved => Ok(()),

        RollbackSituation::PublishNotStarted => {
            let mut next = prepared.journal.clone();
            if let DomainOperationEffect::Write(value) = next.operations_mut()[index].effect_mut() {
                *value.state_mut() = DomainWriteState::Preserved;
            }
            prepared.cas(next)?;
            Ok(())
        }

        RollbackSituation::PublishTorn => {
            let (bytes, stage_entry) = {
                let stage = private_artifact(prepared, index, ArtifactSlot::Stage)?;
                read_private_artifact(&stage, None)?
            };
            if disk_observation(&stage_entry)? != stage_observed {
                return Err(crate::failed(
                    "write publish stage changed before retained-handle recovery",
                ));
            }
            let DiskObservation::File {
                identity: stable_identity,
                ..
            } = &before
            else {
                unreachable!();
            };
            let repaired =
                overwrite_file_with_stable_identity(&path, &live, stable_identity, &bytes)?;
            if repaired != expected {
                return Err(crate::failed(
                    "write publish retained-handle recovery did not reach its exact target",
                ));
            }
            let stage = private_artifact(prepared, index, ArtifactSlot::Stage)?;
            remove_private_artifact(&stage, &private_entry_observation(&stage_observed)?)?;
            let custody_value = artifact(record, ArtifactSlot::Custody);
            prepared.set_write_state_with_artifact(
                index,
                DomainWriteState::Applied {
                    live: durable(&repaired),
                    custody: durable(&custody_value),
                },
                Some(repaired),
                ArtifactSlot::Stage,
                &DiskObservation::Absent,
            )?;
            execute_rollback_from_applied(
                prepared,
                index,
                &path,
                &before,
                &expected,
                &custody_observed,
            )
        }

        RollbackSituation::PublishedStageRetained => {
            let stage = private_artifact(prepared, index, ArtifactSlot::Stage)?;
            remove_private_artifact(&stage, &private_entry_observation(&stage_observed)?)?;
            let custody_value = artifact(record, ArtifactSlot::Custody);
            prepared.set_write_state_with_artifact(
                index,
                DomainWriteState::Applied {
                    live: durable(&expected),
                    custody: durable(&custody_value),
                },
                Some(expected.clone()),
                ArtifactSlot::Stage,
                &DiskObservation::Absent,
            )?;
            execute_rollback_from_applied(
                prepared,
                index,
                &path,
                &before,
                &expected,
                &custody_observed,
            )
        }

        RollbackSituation::PublishCompletePendingAppliedRecord => {
            let custody_value = artifact(record, ArtifactSlot::Custody);
            prepared.set_write_state_with_artifact(
                index,
                DomainWriteState::Applied {
                    live: durable(&expected),
                    custody: durable(&custody_value),
                },
                Some(expected.clone()),
                ArtifactSlot::Stage,
                &DiskObservation::Absent,
            )?;
            execute_rollback_from_applied(
                prepared,
                index,
                &path,
                &before,
                &expected,
                &custody_observed,
            )
        }

        RollbackSituation::AppliedReadyForRollback => execute_rollback_from_applied(
            prepared,
            index,
            &path,
            &before,
            &expected,
            &custody_observed,
        ),

        RollbackSituation::DiscardPending => {
            execute_discard_rollback(prepared, index, &path, &expected, &artifact_custody)
        }

        RollbackSituation::PostimageDiscarded => {
            finalize_discard_to_preserved(prepared, index, &expected, &artifact_custody)
        }

        RollbackSituation::RestorePending => {
            execute_restore_rollback(prepared, index, &path, &before, &expected)
        }

        RollbackSituation::RestoreCompleteCleanupPending => {
            finalize_restore_cleanup(prepared, index)
        }
    }
}

fn execute_rollback_from_applied(
    prepared: &mut PreparedFileMutation<'_>,
    index: usize,
    path: &Path,
    before: &DiskObservation,
    expected: &DiskObservation,
    custody_observed: &DiskObservation,
) -> Result<(), ServiceError> {
    if before == &DiskObservation::Absent {
        execute_discard_rollback(prepared, index, path, expected, custody_observed)
    } else {
        execute_restore_rollback(prepared, index, path, before, expected)
    }
}

fn execute_discard_rollback(
    prepared: &mut PreparedFileMutation<'_>,
    index: usize,
    path: &Path,
    expected: &DiskObservation,
    custody_observed: &DiskObservation,
) -> Result<(), ServiceError> {
    let current_state = match prepared.journal.operations()[index].effect() {
        DomainOperationEffect::Write(value) => value.state().clone(),
        _ => return Err(crate::failed("write rollback action changed")),
    };
    if !matches!(&current_state, DomainWriteState::DiscardIntent { .. }) {
        let mut next = prepared.journal.clone();
        if let DomainOperationEffect::Write(value) = next.operations_mut()[index].effect_mut() {
            *value.state_mut() = DomainWriteState::DiscardIntent {
                postimage: durable(expected),
                custody: durable(custody_observed),
            };
        }
        prepared.cas(next)?;
    }
    if observe(path) == *expected {
        // An absent preimage has no custody to restore. The
        // rollback intent is the durable fence for the
        // exact unlink; do not manufacture a discard
        // artifact just to route the postimage through the
        // private namespace.
        remove_exact_leaf(path, &private_entry_observation(expected)?)?;
    }
    if observe(path) != DiskObservation::Absent {
        return Err(crate::failed(
            "write discard unlink did not produce an absent live path",
        ));
    }
    finalize_discard_to_preserved(prepared, index, expected, custody_observed)
}

fn finalize_discard_to_preserved(
    prepared: &mut PreparedFileMutation<'_>,
    index: usize,
    expected: &DiskObservation,
    custody_observed: &DiskObservation,
) -> Result<(), ServiceError> {
    let mut next = prepared.journal.clone();
    if let DomainOperationEffect::Write(value) = next.operations_mut()[index].effect_mut() {
        *value.state_mut() = DomainWriteState::PostimageDiscarded {
            discard: durable(expected),
            custody: durable(custody_observed),
        };
    }
    prepared.cas(next)?;
    let mut next = prepared.journal.clone();
    if let DomainOperationEffect::Write(value) = next.operations_mut()[index].effect_mut() {
        *value.state_mut() = DomainWriteState::Preserved;
    }
    prepared.cas(next)?;
    Ok(())
}

fn execute_restore_rollback(
    prepared: &mut PreparedFileMutation<'_>,
    index: usize,
    path: &Path,
    before: &DiskObservation,
    expected: &DiskObservation,
) -> Result<(), ServiceError> {
    let current_state = match prepared.journal.operations()[index].effect() {
        DomainOperationEffect::Write(value) => value.state().clone(),
        _ => return Err(crate::failed("write rollback action changed")),
    };
    if !matches!(&current_state, DomainWriteState::RestoreIntent { .. }) {
        let mut next = prepared.journal.clone();
        if let DomainOperationEffect::Write(value) = next.operations_mut()[index].effect_mut() {
            *value.state_mut() = DomainWriteState::RestoreIntent {
                preimage: durable(before),
                // The discard token is historical proof of
                // the published postimage. It is not a
                // second live artifact for an
                // identity-preserving restore.
                discard: durable(expected),
            };
        }
        prepared.cas(next)?;
    }
    let live_now = observe(path);
    if live_now == *expected || (same_file_identity(&live_now, before) && live_now != *before) {
        let custody_expected =
            artifact(&prepared.journal.operations()[index], ArtifactSlot::Custody);
        let (bytes_before, custody_observation) = {
            let custody = private_artifact(prepared, index, ArtifactSlot::Custody)?;
            read_private_artifact(&custody, None)?
        };
        let custody_live = disk_observation(&custody_observation)?;
        if custody_live != custody_expected || !same_file_digest(&custody_live, before) {
            return Err(crate::failed(
                "write restore custody changed before identity-preserving restore",
            ));
        }
        let restored = if live_now == *expected {
            overwrite_file_exact(path, expected, &bytes_before)?
        } else {
            let DiskObservation::File {
                identity: stable_identity,
                ..
            } = before
            else {
                unreachable!();
            };
            overwrite_file_with_stable_identity(path, &live_now, stable_identity, &bytes_before)?
        };
        if restored != *before {
            return Err(crate::failed(
                "write restore did not recover the exact preimage identity",
            ));
        }
    } else if live_now != *before {
        return Err(crate::failed(
            "write restore live tuple is neither postimage nor preimage",
        ));
    }
    finalize_restore_cleanup(prepared, index)
}

fn finalize_restore_cleanup(
    prepared: &mut PreparedFileMutation<'_>,
    index: usize,
) -> Result<(), ServiceError> {
    let custody_expected = artifact(&prepared.journal.operations()[index], ArtifactSlot::Custody);
    if observe_private_artifact(prepared, index, ArtifactSlot::Custody)? == custody_expected {
        let custody = private_artifact(prepared, index, ArtifactSlot::Custody)?;
        remove_private_artifact(&custody, &private_entry_observation(&custody_expected)?)?;
    } else if observe_private_artifact(prepared, index, ArtifactSlot::Custody)?
        != DiskObservation::Absent
    {
        return Err(crate::failed("write restore custody artifact changed"));
    }
    let mut next = prepared.journal.clone();
    if let DomainOperationEffect::Write(value) = next.operations_mut()[index].effect_mut() {
        *value.state_mut() = DomainWriteState::Preserved;
    }
    set_artifact(
        &mut next.operations_mut()[index],
        ArtifactSlot::Custody,
        &DiskObservation::Absent,
    );
    prepared.cas(next)?;
    Ok(())
}
