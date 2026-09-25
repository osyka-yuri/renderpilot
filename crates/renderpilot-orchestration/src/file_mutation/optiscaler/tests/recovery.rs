use super::*;

fn persisted_prepared_for<'a>(
    executor: &'a PeerMutationExecutor,
    root: &Path,
    id: &str,
    game_id: renderpilot_domain::GameId,
    mut journal: OptiScalerJournal,
) -> PreparedFileMutation<'a> {
    let storage = executor.repositories();
    seed_optiscaler_test_game(storage, &game_id, root);
    let initial_json = serde_json::to_string(&journal).expect("initial journal");
    let preparing = executor
        .begin_optiscaler_journal_aggregate(
            OptiScalerJournalAggregateBegin::new(
                id,
                game_id.clone(),
                "optiscaler_install",
                None,
                initial_json,
            )
            .expect("begin aggregate"),
        )
        .expect("reserve mutation");
    let preparing = materialize_namespaces(executor, preparing, &mut journal, &root.join(id))
        .expect("materialize namespaces");
    let journal_json = serde_json::to_string(&journal).expect("prepared journal");
    PreparedFileMutation {
        id: id.to_owned(),
        game_id,
        executor,
        authority: JournalAuthority::ActivePreparing(Box::new(preparing)),
        transaction_root: root.to_path_buf(),
        journal_json,
        journal,
        next_operation_id: 0,
        managed_endpoint_roots: std::collections::HashMap::new(),
    }
}

#[test]
fn native_observation_round_trips_without_projection() {
    let digest = Sha256Hash::new("00".repeat(32)).expect("digest");
    let value = DiskObservation::File {
        identity: "native:1".into(),
        digest,
    };
    assert_eq!(native(&durable(&value)), value);
}

#[test]
fn existing_owned_write_preserves_identity_while_changing_digest() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("owned.bin");
    fs::write(&path, b"before").expect("before");
    let before = observe(&path);
    let after = overwrite_file_exact(&path, &before, b"after").expect("in-place write");
    assert!(matches!(before, DiskObservation::File { .. }));
    assert!(matches!(after, DiskObservation::File { .. }));
    assert_eq!(
        match (&before, &after) {
            (DiskObservation::File { identity: left, .. }, DiskObservation::File { .. }) => left,
            _ => unreachable!(),
        },
        match &after {
            DiskObservation::File { identity, .. } => identity,
            _ => unreachable!(),
        }
    );
    assert_ne!(before, after);
    assert_eq!(fs::read(&path).expect("after"), b"after");
}

#[test]
fn torn_publish_recovery_reapplies_stage_through_the_retained_identity() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("owned.bin");
    fs::write(&path, b"before").expect("before");
    let before = observe(&path);
    let identity = match &before {
        DiskObservation::File { identity, .. } => identity.clone(),
        _ => panic!("before must be a file"),
    };
    let published = overwrite_file_exact(&path, &before, b"published").expect("publish");
    let torn = overwrite_file_exact(&path, &published, b"third-party").expect("drift");
    let repaired = overwrite_file_with_stable_identity(&path, &torn, &identity, b"published")
        .expect("retained-handle recovery");
    assert_eq!(repaired, published);
    assert_eq!(observe(&path), published);
}

#[test]
fn torn_restore_recovery_reapplies_custody_through_the_retained_identity() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("owned.bin");
    fs::write(&path, b"before").expect("before");
    let before = observe(&path);
    let identity = match &before {
        DiskObservation::File { identity, .. } => identity.clone(),
        _ => panic!("before must be a file"),
    };
    let published = overwrite_file_exact(&path, &before, b"published").expect("publish");
    let torn = overwrite_file_exact(&path, &published, b"third-party").expect("drift");
    let restored = overwrite_file_with_stable_identity(&path, &torn, &identity, b"before")
        .expect("retained-handle restore");
    assert_eq!(restored, before);
    assert_eq!(observe(&path), before);
}

#[test]
fn partial_stage_intent_is_adopted_only_for_cleanup_and_keeps_endpoint_pending() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("target.bin");
    let journal = planned_journal(
        root.path(),
        &[OptiScalerPlannedOperation::Write(PlannedParticipant {
            path,
            preimage: PlannedPreimage::Absent,
        })],
    );
    let executor = PeerMutationExecutor::new(SqliteStorage::in_memory().expect("storage"));
    let game_id =
        renderpilot_domain::GameId::new("test:optiscaler-partial-stage").expect("game id");
    let mut prepared =
        persisted_prepared_for(&executor, root.path(), "transaction-id", game_id, journal);
    let partial = {
        let stage = private_artifact(&prepared, 0, ArtifactSlot::Stage).expect("stage");
        write_private_artifact(&stage, b"incomplete-stage").expect("partial stage")
    };
    let mut next = prepared.journal.clone();
    if let DomainOperationEffect::Write(effect) = next.operations_mut()[0].effect_mut() {
        *effect.state_mut() = DomainWriteState::StageIntent {
            target_digest: hash_bytes(b"never-required-for-abort").expect("digest"),
        };
    }
    prepared.cas(next).expect("stage intent");
    adopt_intent_authorized_artifacts(&mut prepared).expect("adopt partial stage");
    let record = &prepared.journal.operations()[0];
    assert!(
        matches!(record.effect(), DomainOperationEffect::Write(effect) if matches!(effect.state(), DomainWriteState::Preserved))
    );
    assert_eq!(artifact(record, ArtifactSlot::Stage), partial);
    assert_eq!(
        observe_private_artifact(&prepared, 0, ArtifactSlot::Stage).expect("stage"),
        partial
    );
    assert!(matches!(
        record.effect().endpoints()[0].expected_after(),
        JournalAfter::Pending
    ));
}

#[test]
fn partial_custody_intent_is_adopted_without_preimage_digest_assumption() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("target.bin");
    fs::write(&path, b"before").expect("before");
    let receipt = owned_receipt(&path);
    let journal = planned_journal(
        root.path(),
        &[OptiScalerPlannedOperation::Write(PlannedParticipant {
            path,
            preimage: PlannedPreimage::Exact {
                current: receipt.clone(),
                prior_owned: Some(receipt),
            },
        })],
    );
    let executor = PeerMutationExecutor::new(SqliteStorage::in_memory().expect("storage"));
    let game_id =
        renderpilot_domain::GameId::new("test:optiscaler-partial-custody").expect("game id");
    let mut prepared =
        persisted_prepared_for(&executor, root.path(), "transaction-id", game_id, journal);
    let stage_observed = {
        let stage = private_artifact(&prepared, 0, ArtifactSlot::Stage).expect("stage");
        write_private_artifact(&stage, b"stage").expect("stage")
    };
    let custody_observed = {
        let custody = private_artifact(&prepared, 0, ArtifactSlot::Custody).expect("custody");
        write_private_artifact(&custody, b"incomplete-custody").expect("custody")
    };
    let mut next = prepared.journal.clone();
    if let DomainOperationEffect::Write(effect) = next.operations_mut()[0].effect_mut() {
        *effect.state_mut() = DomainWriteState::StageIntent {
            target_digest: hash_bytes(b"stage").expect("digest"),
        };
    }
    prepared.cas(next).expect("stage intent");
    let mut next = prepared.journal.clone();
    if let DomainOperationEffect::Write(effect) = next.operations_mut()[0].effect_mut() {
        *effect.state_mut() = DomainWriteState::Staged {
            stage: durable(&stage_observed),
        };
    }
    set_artifact(
        &mut next.operations_mut()[0],
        ArtifactSlot::Stage,
        &stage_observed,
    );
    prepared.cas(next).expect("staged");
    let mut next = prepared.journal.clone();
    if let DomainOperationEffect::Write(effect) = next.operations_mut()[0].effect_mut() {
        *effect.state_mut() = DomainWriteState::CaptureIntent {
            stage: durable(&stage_observed),
        };
    }
    prepared.cas(next).expect("capture intent");
    adopt_intent_authorized_artifacts(&mut prepared).expect("adopt partial custody");
    let record = &prepared.journal.operations()[0];
    assert!(
        matches!(record.effect(), DomainOperationEffect::Write(effect) if matches!(effect.state(), DomainWriteState::Preserved))
    );
    assert_eq!(artifact(record, ArtifactSlot::Custody), custody_observed);
    assert_eq!(
        observe_private_artifact(&prepared, 0, ArtifactSlot::Custody).expect("custody"),
        custody_observed
    );
    assert!(matches!(
        record.effect().endpoints()[0].expected_after(),
        JournalAfter::Pending
    ));
}

#[test]
fn create_directory_stage_intent_is_preserved_with_a_cleanup_only_directory() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("created");
    let journal = planned_journal(
        root.path(),
        &[OptiScalerPlannedOperation::CreateDirectory(
            PlannedParticipant {
                path,
                preimage: PlannedPreimage::Absent,
            },
        )],
    );
    let executor = PeerMutationExecutor::new(SqliteStorage::in_memory().expect("storage"));
    let game_id =
        renderpilot_domain::GameId::new("test:optiscaler-partial-directory").expect("game id");
    let mut prepared =
        persisted_prepared_for(&executor, root.path(), "transaction-id", game_id, journal);
    let partial = {
        let stage = private_artifact(&prepared, 0, ArtifactSlot::Stage).expect("stage");
        create_private_artifact_directory(&stage).expect("partial directory")
    };
    let mut next = prepared.journal.clone();
    if let DomainOperationEffect::CreateDirectory(effect) = next.operations_mut()[0].effect_mut() {
        *effect.state_mut() = DomainCreateDirectoryState::StageIntent;
    }
    prepared.cas(next).expect("directory stage intent");
    adopt_intent_authorized_artifacts(&mut prepared).expect("adopt partial directory");
    let record = &prepared.journal.operations()[0];
    assert!(
        matches!(record.effect(), DomainOperationEffect::CreateDirectory(effect) if matches!(effect.state(), DomainCreateDirectoryState::Preserved))
    );
    assert_eq!(artifact(record, ArtifactSlot::Stage), partial);
    assert_eq!(
        observe_private_artifact(&prepared, 0, ArtifactSlot::Stage).expect("stage"),
        partial
    );
    assert!(matches!(
        record.effect().endpoints()[0].expected_after(),
        JournalAfter::Pending
    ));
}

#[test]
fn create_directory_abort_rejects_an_occupied_private_discard() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("created");
    let journal = planned_journal(
        root.path(),
        &[OptiScalerPlannedOperation::CreateDirectory(
            PlannedParticipant {
                path,
                preimage: PlannedPreimage::Absent,
            },
        )],
    );
    let executor = PeerMutationExecutor::new(SqliteStorage::in_memory().expect("storage"));
    let game_id =
        renderpilot_domain::GameId::new("test:optiscaler-occupied-discard").expect("game id");
    let mut prepared =
        persisted_prepared_for(&executor, root.path(), "transaction-id", game_id, journal);
    {
        let stage = private_artifact(&prepared, 0, ArtifactSlot::Stage).expect("stage");
        create_private_artifact_directory(&stage).expect("stage directory");
    }
    {
        let discard = private_artifact(&prepared, 0, ArtifactSlot::Discard).expect("discard");
        write_private_artifact(&discard, b"foreign-discard").expect("foreign discard");
    }
    let mut next = prepared.journal.clone();
    if let DomainOperationEffect::CreateDirectory(effect) = next.operations_mut()[0].effect_mut() {
        *effect.state_mut() = DomainCreateDirectoryState::StageIntent;
    }
    prepared.cas(next).expect("directory stage intent");
    assert!(adopt_intent_authorized_artifacts(&mut prepared).is_err());
    assert!(matches!(
        observe_private_artifact(&prepared, 0, ArtifactSlot::Discard).expect("discard"),
        DiskObservation::File { .. }
    ));
}

#[test]
fn cleanup_artifact_intent_retries_an_unlink_after_a_crash() {
    let root = tempfile::tempdir().expect("root");
    let journal = planned_journal(
        root.path(),
        &[OptiScalerPlannedOperation::Write(PlannedParticipant {
            path: root.path().join("target.bin"),
            preimage: PlannedPreimage::Absent,
        })],
    );
    let executor = PeerMutationExecutor::new(SqliteStorage::in_memory().expect("storage"));
    let game_id =
        renderpilot_domain::GameId::new("test:optiscaler-cleanup-retry").expect("game id");
    let mut prepared =
        persisted_prepared_for(&executor, root.path(), "transaction-id", game_id, journal);
    let expected = {
        let stage = private_artifact(&prepared, 0, ArtifactSlot::Stage).expect("stage");
        write_private_artifact(&stage, b"cleanup").expect("stage")
    };
    let mut next = prepared.journal.clone();
    if let DomainOperationEffect::Write(effect) = next.operations_mut()[0].effect_mut() {
        *effect.state_mut() = DomainWriteState::StageIntent {
            target_digest: hash_bytes(b"cleanup").expect("digest"),
        };
    }
    prepared.cas(next).expect("persist stage intent");
    let mut next = prepared.journal.clone();
    if let DomainOperationEffect::Write(effect) = next.operations_mut()[0].effect_mut() {
        *effect.state_mut() = DomainWriteState::Staged {
            stage: durable(&expected),
        };
    }
    set_artifact(
        &mut next.operations_mut()[0],
        ArtifactSlot::Stage,
        &expected,
    );
    prepared.cas(next).expect("persist staged");
    let mut next = prepared.journal.clone();
    if let DomainOperationEffect::Write(effect) = next.operations_mut()[0].effect_mut() {
        *effect.state_mut() = DomainWriteState::Preserved;
    }
    prepared.cas(next).expect("persist preserved");
    let mut next = prepared.journal.clone();
    next.set_cleanup(JournalCleanup::ArtifactRemoveIntent {
        operation_id: 0,
        artifact: ArtifactSlot::Stage,
        expected: durable(&expected),
    });
    prepared.cas(next).expect("persist cleanup intent");
    prepared
        .cleanup_private_artifacts()
        .expect("retry cleanup unlink");
    assert_eq!(
        observe_private_artifact(&prepared, 0, ArtifactSlot::Stage).expect("stage"),
        DiskObservation::Absent
    );
    assert_eq!(
        artifact(&prepared.journal.operations()[0], ArtifactSlot::Stage),
        DiskObservation::Absent
    );
    assert!(matches!(
        prepared.journal.cleanup(),
        JournalCleanup::Inactive
    ));
}

#[test]
fn rollback_of_an_absent_write_unlinks_the_postimage_without_a_discard_file() {
    let root = tempfile::tempdir().expect("root");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id =
        renderpilot_domain::GameId::new("test:optiscaler-absent-rollback").expect("game id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let scope = MutationScope::single(root.path()).expect("scope");
    let path = root.path().join("created.bin");
    let mutation = OptiScalerMutation {
        context: &context,
        guard: &guard,
        scope: &scope,
        feature: renderpilot_domain::mutation_features::OPTISCALER_INSTALL,
        subject_id: Some("optiscaler:absent-rollback"),
        operations: vec![OptiScalerPlannedOperation::Write(PlannedParticipant {
            path: path.clone(),
            preimage: PlannedPreimage::Absent,
        })],
        managed_endpoint_roots: Vec::new(),
        threat_model: ThreatModel::CooperativeSameUid,
    };

    let result = run_optiscaler_mutation(
        &mutation,
        |prepared| {
            prepared.write_file(&path, b"postimage")?;
            Err::<(), ServiceError>(crate::failed("injected rollback"))
        },
        |_, ()| Ok(()),
        |()| {},
        || {},
    );

    assert!(result.is_err());
    assert_eq!(observe(&path), DiskObservation::Absent);
    assert!(!root.path().join("discard").exists());
    assert!(
        !root
            .path()
            .read_dir()
            .expect("root entries")
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains("discard"))
    );
}

#[test]
fn commit_failure_restores_deleted_file_and_clears_the_pending_row() {
    let root = tempfile::tempdir().expect("root");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id =
        renderpilot_domain::GameId::new("test:optiscaler-delete-rollback").expect("game id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let scope = MutationScope::single(root.path()).expect("scope");
    let path = root.path().join("owned.bin");
    fs::write(&path, b"owned-before").expect("seed owned file");
    let receipt = owned_receipt(&path);
    let mutation = OptiScalerMutation {
        context: &context,
        guard: &guard,
        scope: &scope,
        feature: renderpilot_domain::mutation_features::OPTISCALER_UNINSTALL,
        subject_id: Some("optiscaler:delete-rollback"),
        operations: vec![OptiScalerPlannedOperation::Delete(PlannedParticipant {
            path: path.clone(),
            preimage: PlannedPreimage::Exact {
                current: receipt.clone(),
                prior_owned: Some(receipt.clone()),
            },
        })],
        managed_endpoint_roots: Vec::new(),
        threat_model: ThreatModel::CooperativeSameUid,
    };

    let result = run_optiscaler_mutation(
        &mutation,
        |prepared| {
            prepared.delete_file_exact(&path, &receipt)?;
            Ok::<(), ServiceError>(())
        },
        |_, ()| Err(crate::failed("injected commit failure")),
        |()| {},
        || {},
    );

    assert!(result.is_err());
    assert_eq!(fs::read(&path).expect("restored file"), b"owned-before");
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(&game_id)
            .expect("pending rows")
            .is_empty()
    );
}

fn advance_write_to_captured(
    prepared: &mut PreparedFileMutation<'_>,
    stage: &DiskObservation,
    custody: &DiskObservation,
) {
    let stage_digest = match stage {
        DiskObservation::File { digest, .. } => digest.clone(),
        _ => panic!("stage must be a file"),
    };
    let mut next = prepared.journal.clone();
    if let DomainOperationEffect::Write(effect) = next.operations_mut()[0].effect_mut() {
        *effect.state_mut() = DomainWriteState::StageIntent {
            target_digest: stage_digest,
        };
    }
    prepared.cas(next).expect("cas to stage intent");

    let mut next = prepared.journal.clone();
    if let DomainOperationEffect::Write(effect) = next.operations_mut()[0].effect_mut() {
        *effect.state_mut() = DomainWriteState::Staged {
            stage: durable(stage),
        };
    }
    set_artifact(&mut next.operations_mut()[0], ArtifactSlot::Stage, stage);
    prepared.cas(next).expect("cas to staged");

    let mut next = prepared.journal.clone();
    if let DomainOperationEffect::Write(effect) = next.operations_mut()[0].effect_mut() {
        *effect.state_mut() = DomainWriteState::CaptureIntent {
            stage: durable(stage),
        };
    }
    prepared.cas(next).expect("cas to capture intent");

    let mut next = prepared.journal.clone();
    if let DomainOperationEffect::Write(effect) = next.operations_mut()[0].effect_mut() {
        *effect.state_mut() = DomainWriteState::Captured {
            stage: durable(stage),
            custody: durable(custody),
        };
    }
    set_artifact(
        &mut next.operations_mut()[0],
        ArtifactSlot::Custody,
        custody,
    );
    prepared.cas(next).expect("cas to captured");
}

fn advance_write_to_publish_intent(
    prepared: &mut PreparedFileMutation<'_>,
    stage: &DiskObservation,
    custody: &DiskObservation,
) {
    advance_write_to_captured(prepared, stage, custody);
    let mut next = prepared.journal.clone();
    if let DomainOperationEffect::Write(effect) = next.operations_mut()[0].effect_mut() {
        *effect.state_mut() = DomainWriteState::PublishIntent {
            stage: durable(stage),
            custody: durable(custody),
        };
    }
    prepared.cas(next).expect("cas to publish intent");
}

#[test]
fn rollback_write_captured_missing_live_rejects_without_journal_or_disk_mutation() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("target.bin");
    fs::write(&path, b"preimage-content").expect("write preimage");
    let receipt = owned_receipt(&path);
    let journal = planned_journal(
        root.path(),
        &[OptiScalerPlannedOperation::Write(PlannedParticipant {
            path: path.clone(),
            preimage: PlannedPreimage::Exact {
                current: receipt.clone(),
                prior_owned: Some(receipt),
            },
        })],
    );
    let executor = PeerMutationExecutor::new(SqliteStorage::in_memory().expect("storage"));
    let game_id =
        renderpilot_domain::GameId::new("test:optiscaler-captured-missing-live").expect("game id");
    let mut prepared =
        persisted_prepared_for(&executor, root.path(), "transaction-id", game_id, journal);

    let stage_observed = {
        let stage = private_artifact(&prepared, 0, ArtifactSlot::Stage).expect("stage");
        write_private_artifact(&stage, b"postimage-content").expect("write stage")
    };
    let custody_observed = {
        let custody = private_artifact(&prepared, 0, ArtifactSlot::Custody).expect("custody");
        write_private_artifact(&custody, b"preimage-content").expect("write custody")
    };

    advance_write_to_captured(&mut prepared, &stage_observed, &custody_observed);

    // Anomaly: live file is deleted from disk before rollback
    fs::remove_file(&path).expect("remove live file");
    assert_eq!(observe(&path), DiskObservation::Absent);

    let record = prepared.journal.operations()[0].clone();
    let result = rollback_operation(&mut prepared, 0, &record);

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("write captured tuple has missing live file")
    );

    // Invariant: journal state remains at Captured
    let current_op = &prepared.journal.operations()[0];
    assert!(matches!(
        current_op.effect(),
        DomainOperationEffect::Write(effect) if matches!(effect.state(), DomainWriteState::Captured { .. })
    ));

    // Invariant: private artifacts are untouched
    assert_eq!(
        observe_private_artifact(&prepared, 0, ArtifactSlot::Stage).expect("stage"),
        stage_observed
    );
    assert_eq!(
        observe_private_artifact(&prepared, 0, ArtifactSlot::Custody).expect("custody"),
        custody_observed
    );
}

#[test]
fn rollback_write_publish_intent_missing_live_rejects_without_journal_or_disk_mutation() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("target.bin");
    fs::write(&path, b"preimage-content").expect("write preimage");
    let receipt = owned_receipt(&path);
    let journal = planned_journal(
        root.path(),
        &[OptiScalerPlannedOperation::Write(PlannedParticipant {
            path: path.clone(),
            preimage: PlannedPreimage::Exact {
                current: receipt.clone(),
                prior_owned: Some(receipt),
            },
        })],
    );
    let executor = PeerMutationExecutor::new(SqliteStorage::in_memory().expect("storage"));
    let game_id =
        renderpilot_domain::GameId::new("test:optiscaler-publish-missing-live").expect("game id");
    let mut prepared =
        persisted_prepared_for(&executor, root.path(), "transaction-id", game_id, journal);

    let stage_observed = {
        let stage = private_artifact(&prepared, 0, ArtifactSlot::Stage).expect("stage");
        write_private_artifact(&stage, b"postimage-content").expect("write stage")
    };
    let custody_observed = {
        let custody = private_artifact(&prepared, 0, ArtifactSlot::Custody).expect("custody");
        write_private_artifact(&custody, b"preimage-content").expect("write custody")
    };

    advance_write_to_publish_intent(&mut prepared, &stage_observed, &custody_observed);

    // Anomaly: live file is deleted from disk before publish started
    fs::remove_file(&path).expect("remove live file");
    assert_eq!(observe(&path), DiskObservation::Absent);

    let record = prepared.journal.operations()[0].clone();
    let result = rollback_operation(&mut prepared, 0, &record);

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("write publish tuple has missing live file")
    );

    // Invariant: journal state remains at PublishIntent
    let current_op = &prepared.journal.operations()[0];
    assert!(matches!(
        current_op.effect(),
        DomainOperationEffect::Write(effect) if matches!(effect.state(), DomainWriteState::PublishIntent { .. })
    ));

    // Invariant: private artifacts are untouched
    assert_eq!(
        observe_private_artifact(&prepared, 0, ArtifactSlot::Stage).expect("stage"),
        stage_observed
    );
    assert_eq!(
        observe_private_artifact(&prepared, 0, ArtifactSlot::Custody).expect("custody"),
        custody_observed
    );
}
