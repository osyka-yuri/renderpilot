use super::*;

fn auxiliary_journal(
    parent_paths: &[&str],
    destination_path: &str,
) -> (OptiScalerJournal, FileReceipt) {
    let capability = capability();
    let destination_receipt = FileReceipt::owned("recovery-copy", hash()).expect("destination");
    let source_receipt = FileReceipt::owned("original-config", hash()).expect("source");
    let mut operations = Vec::new();
    for (index, path) in parent_paths.iter().enumerate() {
        let directory = DurableObservation::Directory {
            identity: format!("directory-{index}"),
        };
        let endpoint = OperationEndpoint::new(
            Endpoint::Single,
            *path,
            Preimage::Initial {
                observation: DurableObservation::Absent,
                receipt: None,
                owned_basis: None,
            },
            ExpectedAfter::Known(directory.clone()),
        )
        .expect("parent endpoint");
        let operation = OperationRecord::new(
            u32::try_from(index).expect("index fits u32"),
            (index > 0
                && direct_parent_lexical(path).is_some_and(|parent| {
                    normalized_path_key(&parent) == normalized_path_key(parent_paths[index - 1])
                }))
            .then(|| u32::try_from(index - 1).ok())
            .flatten()
            .into_iter()
            .collect(),
            Some(0),
            PrivateArtifactSlots::new(
                DurableObservation::Absent,
                DurableObservation::Absent,
                DurableObservation::Absent,
            )
            .expect("parent slots"),
            OperationEffect::CreateDirectory(
                CreateDirectoryEffect::new(
                    endpoint,
                    CreateDirectoryState::Applied { live: directory },
                )
                .expect("parent effect"),
            ),
        )
        .expect("parent operation");
        operations.push(operation);
    }
    let destination_id = u32::try_from(operations.len()).expect("len fits u32");
    let destination_endpoint = OperationEndpoint::new(
        Endpoint::Single,
        destination_path,
        Preimage::Initial {
            observation: DurableObservation::Absent,
            receipt: None,
            owned_basis: None,
        },
        ExpectedAfter::Known(file_observation(&destination_receipt)),
    )
    .expect("destination endpoint");
    let destination = OperationRecord::new(
        destination_id,
        parent_paths
            .iter()
            .enumerate()
            .rev()
            .find(|(_, path)| {
                direct_parent_lexical(destination_path)
                    .is_some_and(|parent| normalized_path_key(&parent) == normalized_path_key(path))
            })
            .map_or_else(Vec::new, |(index, _)| {
                vec![u32::try_from(index).expect("index fits u32")]
            }),
        Some(0),
        PrivateArtifactSlots::new(
            DurableObservation::Absent,
            DurableObservation::Absent,
            DurableObservation::Absent,
        )
        .expect("destination slots"),
        OperationEffect::Write(
            WriteEffect::new(
                destination_endpoint,
                WriteState::Applied {
                    live: file_observation(&destination_receipt),
                    custody: DurableObservation::Absent,
                },
            )
            .expect("destination effect"),
        ),
    )
    .expect("destination operation");
    operations.push(destination);

    let source_id = u32::try_from(operations.len()).expect("len fits u32");
    let source_endpoint = OperationEndpoint::new(
        Endpoint::Single,
        "C:/RenderPilot/original-config.ini",
        Preimage::Initial {
            observation: file_observation(&source_receipt),
            receipt: Some(source_receipt.clone()),
            owned_basis: Some(source_receipt.clone()),
        },
        ExpectedAfter::Known(file_observation(&source_receipt)),
    )
    .expect("source endpoint");
    operations.push(
        OperationRecord::new(
            source_id,
            Vec::new(),
            None,
            PrivateArtifactSlots::new(
                DurableObservation::Absent,
                DurableObservation::Absent,
                DurableObservation::Absent,
            )
            .expect("source slots"),
            OperationEffect::Verify(
                VerifyEffect::new(
                    source_endpoint,
                    VerifyState::Applied {
                        observed: file_observation(&source_receipt),
                    },
                )
                .expect("source effect"),
            ),
        )
        .expect("source operation"),
    );
    let journal = test_journal(
        vec!["C:/RenderPilot".to_owned()],
        ControlNamespaceBinding::new(
            format!("C:/RenderPilot/control-aux-{}", capability.as_str()),
            None,
            capability,
        )
        .expect("control namespace"),
        operations,
    );
    (journal, destination_receipt)
}

fn auxiliary_binding(
    destination_path: &str,
    destination_receipt: FileReceipt,
) -> OptiScalerAggregateBinding {
    OptiScalerAggregateBinding {
        paths: BTreeMap::new(),
        after_claims: BTreeMap::new(),
        retained_fsr_custody: BTreeMap::new(),
        known_paths: BTreeSet::from([normalized_path_key("C:/RenderPilot/original-config.ini")]),
        auxiliary: vec![OptiScalerAuxiliaryPreservation {
            source: "C:/RenderPilot/original-config.ini".to_owned(),
            destination: destination_path.to_owned(),
            receipt: destination_receipt,
        }],
        owned_preservations: Vec::new(),
    }
}

fn retained_auxiliary_five_effect_fixture() -> (OptiScalerJournal, OptiScalerAggregateBinding) {
    let capability = capability();
    let config = FileReceipt::owned("config-id", hash()).expect("config receipt");
    let recovery = FileReceipt::owned("recovery-id", hash()).expect("recovery receipt");
    let runtime = FileReceipt::reused("runtime-id", hash()).expect("runtime receipt");
    let outer = FileReceipt::owned("outer-id", hash()).expect("outer receipt");
    let transaction_id = "rollback-fixture";
    let directory = DurableObservation::Directory {
        identity: "recovery-directory".to_owned(),
    };
    let namespace = |_| Some(0);
    let slots = || {
        PrivateArtifactSlots::new(
            DurableObservation::Absent,
            DurableObservation::Absent,
            DurableObservation::Absent,
        )
        .expect("slots")
    };
    let create_endpoint = OperationEndpoint::new(
        Endpoint::Single,
        "C:/Game/recovery",
        Preimage::Initial {
            observation: DurableObservation::Absent,
            receipt: None,
            owned_basis: None,
        },
        ExpectedAfter::Known(directory.clone()),
    )
    .expect("create endpoint");
    let create = OperationRecord::new(
        0,
        Vec::new(),
        namespace(0),
        slots(),
        OperationEffect::CreateDirectory(
            CreateDirectoryEffect::new(
                create_endpoint,
                CreateDirectoryState::Applied { live: directory },
            )
            .expect("create effect"),
        ),
    )
    .expect("create operation");
    let destination_endpoint = OperationEndpoint::new(
        Endpoint::Single,
        "C:/Game/recovery/config.ini",
        Preimage::Initial {
            observation: DurableObservation::Absent,
            receipt: None,
            owned_basis: None,
        },
        ExpectedAfter::Known(file_observation(&recovery)),
    )
    .expect("destination endpoint");
    let destination = OperationRecord::new(
        1,
        vec![0],
        namespace(1),
        slots(),
        OperationEffect::Write(
            WriteEffect::new(
                destination_endpoint,
                WriteState::Applied {
                    live: file_observation(&recovery),
                    custody: DurableObservation::Absent,
                },
            )
            .expect("destination effect"),
        ),
    )
    .expect("destination operation");
    let config_endpoint = OperationEndpoint::new(
        Endpoint::Single,
        "C:/Game/OptiScaler.ini",
        Preimage::Initial {
            observation: file_observation(&config),
            receipt: Some(config.clone()),
            owned_basis: Some(config.clone()),
        },
        ExpectedAfter::Known(DurableObservation::Absent),
    )
    .expect("config endpoint");
    let config_delete = OperationRecord::new(
        2,
        Vec::new(),
        namespace(2),
        PrivateArtifactSlots::new(
            file_observation(&config),
            DurableObservation::Absent,
            DurableObservation::Absent,
        )
        .expect("config delete slots"),
        OperationEffect::Delete(
            DeleteEffect::new(
                config_endpoint,
                DeleteState::Applied {
                    custody: file_observation(&config),
                },
            )
            .expect("config delete effect"),
        ),
    )
    .expect("config delete operation");
    let runtime_endpoint = OperationEndpoint::new(
        Endpoint::Single,
        "C:/Game/runtime.dll",
        Preimage::Initial {
            observation: file_observation(&runtime),
            receipt: Some(runtime.clone()),
            owned_basis: None,
        },
        ExpectedAfter::Known(file_observation(&runtime)),
    )
    .expect("runtime endpoint");
    let runtime_verify = OperationRecord::new(
        3,
        Vec::new(),
        None,
        slots(),
        OperationEffect::Verify(
            VerifyEffect::new(
                runtime_endpoint,
                VerifyState::Applied {
                    observed: file_observation(&runtime),
                },
            )
            .expect("runtime verify effect"),
        ),
    )
    .expect("runtime verify operation");
    let outer_endpoint = OperationEndpoint::new(
        Endpoint::Single,
        "C:/Game/dxgi.dll",
        Preimage::Initial {
            observation: file_observation(&outer),
            receipt: Some(outer.clone()),
            owned_basis: Some(outer.clone()),
        },
        ExpectedAfter::Known(DurableObservation::Absent),
    )
    .expect("outer endpoint");
    let outer_delete = OperationRecord::new(
        4,
        Vec::new(),
        namespace(4),
        PrivateArtifactSlots::new(
            file_observation(&outer),
            DurableObservation::Absent,
            DurableObservation::Absent,
        )
        .expect("outer delete slots"),
        OperationEffect::Delete(
            DeleteEffect::new(
                outer_endpoint,
                DeleteState::Applied {
                    custody: file_observation(&outer),
                },
            )
            .expect("outer delete effect"),
        ),
    )
    .expect("outer delete operation");
    let mut journal = test_journal(
        vec!["C:/Game".to_owned()],
        ControlNamespaceBinding::new(
            format!("C:/Game/control-{transaction_id}-{}", capability.as_str()),
            None,
            capability,
        )
        .expect("control namespace"),
        vec![
            create,
            destination,
            config_delete,
            runtime_verify,
            outer_delete,
        ],
    );
    journal
        .control_namespace_mut()
        .set_identity("control-id")
        .expect("control identity");
    journal.private_workspaces_mut()[0]
        .set_identity("workspace-id")
        .expect("workspace identity");
    journal.set_materialization(MaterializationState::Ready);

    let mut paths = BTreeMap::new();
    paths.insert(
        normalized_path_key("C:/Game/OptiScaler.ini"),
        OptiScalerBoundPath {
            path: "C:/Game/OptiScaler.ini".to_owned(),
            transition: OptiScalerBoundTransition::RemoveOwnedFile {
                installed: config.clone(),
                restoration: None,
                allow_absent: false,
            },
        },
    );
    paths.insert(
        normalized_path_key("C:/Game/runtime.dll"),
        OptiScalerBoundPath {
            path: "C:/Game/runtime.dll".to_owned(),
            transition: OptiScalerBoundTransition::RelinquishReusedFile { installed: runtime },
        },
    );
    paths.insert(
        normalized_path_key("C:/Game/dxgi.dll"),
        OptiScalerBoundPath {
            path: "C:/Game/dxgi.dll".to_owned(),
            transition: OptiScalerBoundTransition::RemoveOwnedFile {
                installed: outer,
                restoration: None,
                allow_absent: false,
            },
        },
    );
    let binding = OptiScalerAggregateBinding {
        paths,
        after_claims: BTreeMap::new(),
        retained_fsr_custody: BTreeMap::new(),
        known_paths: BTreeSet::from([
            normalized_path_key("C:/Game/OptiScaler.ini"),
            normalized_path_key("C:/Game/recovery/config.ini"),
            normalized_path_key("C:/Game/runtime.dll"),
            normalized_path_key("C:/Game/dxgi.dll"),
        ]),
        auxiliary: vec![OptiScalerAuxiliaryPreservation {
            source: "C:/Game/OptiScaler.ini".to_owned(),
            destination: "C:/Game/recovery/config.ini".to_owned(),
            receipt: recovery,
        }],
        owned_preservations: vec![OptiScalerOwnedPreservation {
            source: "C:/Game/OptiScaler.ini".to_owned(),
            prior: config.clone(),
            current: config,
        }],
    };
    (journal, binding)
}

fn owned_configuration_restore_fixture() -> (
    OptiScalerJournal,
    FileReceipt,
    FileReceipt,
    OptiScalerAggregateBinding,
) {
    let capability = capability();
    let installed = FileReceipt::owned(
        "config-id",
        Sha256Hash::new("a".repeat(64)).expect("installed digest"),
    )
    .expect("installed receipt");
    let restoration = FileReceipt::reused(
        "config-id",
        Sha256Hash::new("b".repeat(64)).expect("restoration digest"),
    )
    .expect("restoration receipt");
    let path = "game/OptiScaler.ini";
    let endpoint = OperationEndpoint::new(
        Endpoint::Single,
        path,
        Preimage::Initial {
            observation: file_observation(&installed),
            receipt: Some(installed.clone()),
            owned_basis: Some(installed.clone()),
        },
        ExpectedAfter::Known(file_observation(&restoration)),
    )
    .expect("configuration endpoint");
    let operation = OperationRecord::new(
        0,
        Vec::new(),
        Some(0),
        PrivateArtifactSlots::new(
            DurableObservation::Absent,
            DurableObservation::Absent,
            DurableObservation::Absent,
        )
        .expect("artifact slots"),
        OperationEffect::Write(
            WriteEffect::new(
                endpoint,
                WriteState::Applied {
                    live: file_observation(&restoration),
                    custody: DurableObservation::Absent,
                },
            )
            .expect("write effect"),
        ),
    )
    .expect("write operation");
    let mut journal = test_journal(
        vec!["game".to_owned()],
        ControlNamespaceBinding::new(
            format!("game/control-restore-{}", capability.as_str()),
            None,
            capability,
        )
        .expect("control namespace"),
        vec![operation],
    );
    journal
        .control_namespace_mut()
        .set_identity("control-restore")
        .expect("control identity");
    journal.private_workspaces_mut()[0]
        .set_identity("participant-restore")
        .expect("workspace identity");
    journal.set_materialization(MaterializationState::Ready);

    let binding = OptiScalerAggregateBinding {
        paths: BTreeMap::from([(
            normalized_path_key(path),
            OptiScalerBoundPath {
                path: path.to_owned(),
                transition: OptiScalerBoundTransition::RestoreOwnedFile {
                    installed: installed.clone(),
                    restoration: restoration.clone(),
                },
            },
        )]),
        after_claims: BTreeMap::new(),
        retained_fsr_custody: BTreeMap::new(),
        known_paths: BTreeSet::new(),
        auxiliary: Vec::new(),
        owned_preservations: Vec::new(),
    };
    (journal, installed, restoration, binding)
}

#[test]
fn auxiliary_preservation_authorizes_one_or_multiple_missing_parents() {
    let (journal, destination) = auxiliary_journal(
        &["C:/RenderPilot/recovery"],
        "C:/RenderPilot/recovery/config.ini",
    );
    validate_binding_against_journal(
        &journal,
        &auxiliary_binding("C:/RenderPilot/recovery/config.ini", destination),
    )
    .expect("one missing parent");

    let (journal, destination) = auxiliary_journal(
        &["C:/RenderPilot/recovery", "C:/RenderPilot/recovery/state"],
        "C:/RenderPilot/recovery/state/config.ini",
    );
    validate_binding_against_journal(
        &journal,
        &auxiliary_binding("C:/RenderPilot/recovery/state/config.ini", destination),
    )
    .expect("multiple missing parents");
}

#[test]
fn auxiliary_and_retained_claims_close_a_five_effect_uninstall_fold() {
    let (journal, binding) = retained_auxiliary_five_effect_fixture();
    validate_binding_against_journal(&journal, &binding)
        .expect("auxiliary and retained five-effect fold");
}

#[test]
fn owned_configuration_restore_accepts_exact_in_place_baseline_write() {
    let (journal, _, _, binding) = owned_configuration_restore_fixture();
    validate_binding_against_journal(&journal, &binding)
        .expect("exact owned-to-reused configuration restoration");
}

#[test]
fn owned_configuration_restore_rejects_wrong_path_identity_digest_and_ownership() {
    let (journal, installed, restoration, mut binding) = owned_configuration_restore_fixture();

    let path_key = normalized_path_key("game/OptiScaler.ini");
    let wrong_path = "game/Other.ini";
    let path_entry = binding.paths.remove(&path_key).expect("path binding");
    binding.paths.insert(
        normalized_path_key(wrong_path),
        OptiScalerBoundPath {
            path: wrong_path.to_owned(),
            ..path_entry
        },
    );
    assert!(
        validate_binding_against_journal(&journal, &binding).is_err(),
        "restoration must be bound to the canonical journal path"
    );

    let (_, _, _, mut binding) = owned_configuration_restore_fixture();
    let wrong_identity = FileReceipt::reused("other-config-id", restoration.digest().clone())
        .expect("wrong identity receipt");
    binding
        .paths
        .get_mut(&path_key)
        .expect("path binding")
        .transition = OptiScalerBoundTransition::RestoreOwnedFile {
        installed: installed.clone(),
        restoration: wrong_identity,
    };
    let (journal_identity, _, _, _) = owned_configuration_restore_fixture();
    assert!(
        validate_binding_against_journal(&journal_identity, &binding).is_err(),
        "restoration must preserve the native identity"
    );

    let (_, _, _, mut binding) = owned_configuration_restore_fixture();
    let wrong_digest = FileReceipt::reused(
        restoration.identity().to_owned(),
        Sha256Hash::new("c".repeat(64)).expect("wrong digest"),
    )
    .expect("wrong digest receipt");
    binding
        .paths
        .get_mut(&path_key)
        .expect("path binding")
        .transition = OptiScalerBoundTransition::RestoreOwnedFile {
        installed: installed.clone(),
        restoration: wrong_digest,
    };
    let (journal_digest, _, _, _) = owned_configuration_restore_fixture();
    assert!(
        validate_binding_against_journal(&journal_digest, &binding).is_err(),
        "restoration must preserve the exact baseline digest"
    );

    let (_, _, _, mut binding) = owned_configuration_restore_fixture();
    let wrong_ownership = FileReceipt::owned(
        restoration.identity().to_owned(),
        restoration.digest().clone(),
    )
    .expect("wrong ownership receipt");
    binding
        .paths
        .get_mut(&path_key)
        .expect("path binding")
        .transition = OptiScalerBoundTransition::RestoreOwnedFile {
        installed,
        restoration: wrong_ownership,
    };
    let (journal_ownership, _, _, _) = owned_configuration_restore_fixture();
    assert!(
        validate_binding_against_journal(&journal_ownership, &binding).is_err(),
        "restoration must end in Reused custody"
    );
}

#[test]
fn rejected_five_effect_commit_keeps_each_reverse_rollback_edge_legal() {
    let (journal, mut invalid_binding) = retained_auxiliary_five_effect_fixture();
    invalid_binding.auxiliary[0].destination = "C:/Game/recovery-evil/config.ini".to_owned();
    assert!(validate_binding_against_journal(&journal, &invalid_binding).is_err());

    let config = FileReceipt::owned("config-id", hash()).expect("config receipt");
    let recovery = FileReceipt::owned("recovery-id", hash()).expect("recovery receipt");
    let outer = FileReceipt::owned("outer-id", hash()).expect("outer receipt");
    let mut current = journal;
    let assert_edge = |current: &mut OptiScalerJournal, next: OptiScalerJournal, label: &str| {
        let result = validate_optiscaler_journal_for_cas(
            &serde_json::to_string(&current).expect("rollback current json"),
            &serde_json::to_string(&next).expect("rollback next json"),
            "prepared",
        );
        assert!(result.is_ok(), "{label}: {result:?}");
        *current = next;
    };

    let mut next = current.clone();
    if let OperationEffect::Delete(effect) = next.operations_mut()[4].effect_mut() {
        *effect.state_mut() = DeleteState::RestoreIntent {
            preimage: file_observation(&outer),
        };
    }
    assert_edge(&mut current, next, "outer delete restore intent");
    let mut next = current.clone();
    if let OperationEffect::Delete(effect) = next.operations_mut()[4].effect_mut() {
        *effect.state_mut() = DeleteState::Preserved;
    }
    let mut invalid_preserved = next.clone();
    let invalid_result = validate_optiscaler_journal_for_cas(
        &serde_json::to_string(&current).expect("invalid outer current json"),
        &serde_json::to_string(&invalid_preserved).expect("invalid outer next json"),
        "prepared",
    );
    assert!(
        invalid_result
            .expect_err("delete preserved must clear custody before its CAS")
            .to_string()
            .contains("artifact slots do not match its action state")
    );
    *invalid_preserved.operations_mut()[4]
        .slots_mut()
        .custody_mut() = DurableObservation::Absent;
    assert_edge(&mut current, invalid_preserved, "outer delete preserved");

    let mut next = current.clone();
    if let OperationEffect::Verify(effect) = next.operations_mut()[3].effect_mut() {
        *effect.state_mut() = VerifyState::Preserved;
    }
    assert_edge(&mut current, next, "retained verify preserved");

    let mut next = current.clone();
    if let OperationEffect::Delete(effect) = next.operations_mut()[2].effect_mut() {
        *effect.state_mut() = DeleteState::RestoreIntent {
            preimage: file_observation(&config),
        };
    }
    assert_edge(&mut current, next, "config delete restore intent");
    let mut next = current.clone();
    if let OperationEffect::Delete(effect) = next.operations_mut()[2].effect_mut() {
        *effect.state_mut() = DeleteState::Preserved;
    }
    let mut invalid_preserved = next.clone();
    let invalid_result = validate_optiscaler_journal_for_cas(
        &serde_json::to_string(&current).expect("invalid config current json"),
        &serde_json::to_string(&invalid_preserved).expect("invalid config next json"),
        "prepared",
    );
    assert!(
        invalid_result
            .expect_err("delete preserved must clear custody before its CAS")
            .to_string()
            .contains("artifact slots do not match its action state")
    );
    *invalid_preserved.operations_mut()[2]
        .slots_mut()
        .custody_mut() = DurableObservation::Absent;
    assert_edge(&mut current, invalid_preserved, "config delete preserved");

    let mut next = current.clone();
    if let OperationEffect::Write(effect) = next.operations_mut()[1].effect_mut() {
        *effect.state_mut() = WriteState::DiscardIntent {
            postimage: file_observation(&recovery),
            custody: DurableObservation::Absent,
        };
    }
    assert_edge(&mut current, next, "auxiliary write discard intent");
    let mut next = current.clone();
    if let OperationEffect::Write(effect) = next.operations_mut()[1].effect_mut() {
        *effect.state_mut() = WriteState::PostimageDiscarded {
            discard: file_observation(&recovery),
            custody: DurableObservation::Absent,
        };
    }
    assert_edge(&mut current, next, "auxiliary write postimage discarded");
    let mut next = current.clone();
    if let OperationEffect::Write(effect) = next.operations_mut()[1].effect_mut() {
        *effect.state_mut() = WriteState::Preserved;
    }
    assert_edge(&mut current, next, "auxiliary write preserved");

    let mut next = current.clone();
    if let OperationEffect::CreateDirectory(effect) = next.operations_mut()[0].effect_mut() {
        *effect.state_mut() = CreateDirectoryState::DiscardIntent {
            directory: DurableObservation::Directory {
                identity: "recovery-directory".to_owned(),
            },
        };
    }
    assert_edge(&mut current, next, "directory discard intent");
    let mut next = current.clone();
    if let OperationEffect::CreateDirectory(effect) = next.operations_mut()[0].effect_mut() {
        *effect.state_mut() = CreateDirectoryState::PostimageDiscarded {
            discard: DurableObservation::Directory {
                identity: "recovery-directory".to_owned(),
            },
        };
    }
    assert_edge(&mut current, next, "directory postimage discarded");
    let mut next = current.clone();
    if let OperationEffect::CreateDirectory(effect) = next.operations_mut()[0].effect_mut() {
        *effect.state_mut() = CreateDirectoryState::Preserved;
    }
    assert_edge(&mut current, next, "directory preserved");
}

#[test]
fn auxiliary_parent_closure_rejects_arbitrary_or_sibling_create_directories() {
    let (journal, destination) = auxiliary_journal(
        &["C:/RenderPilot/recovery", "C:/RenderPilot/other"],
        "C:/RenderPilot/recovery/config.ini",
    );
    assert!(
        validate_binding_against_journal(
            &journal,
            &auxiliary_binding("C:/RenderPilot/recovery/config.ini", destination),
        )
        .is_err()
    );

    let (journal, destination) = auxiliary_journal(
        &["C:/RenderPilot/recovery-evil"],
        "C:/RenderPilot/recovery/config.ini",
    );
    assert!(
        validate_binding_against_journal(
            &journal,
            &auxiliary_binding("C:/RenderPilot/recovery/config.ini", destination),
        )
        .is_err()
    );
}

#[test]
fn auxiliary_parent_closure_rejects_non_create_effect_wrong_fold_and_wrong_order() {
    let (mut journal, destination) = auxiliary_journal(
        &["C:/RenderPilot/recovery"],
        "C:/RenderPilot/recovery/config.ini",
    );
    let directory = DurableObservation::Directory {
        identity: "directory-0".to_owned(),
    };
    let endpoint = OperationEndpoint::new(
        Endpoint::Single,
        "C:/RenderPilot/recovery",
        Preimage::Initial {
            observation: directory.clone(),
            receipt: None,
            owned_basis: None,
        },
        ExpectedAfter::Known(directory.clone()),
    )
    .expect("verify parent endpoint");
    *journal.operations_mut()[0].effect_mut() = OperationEffect::Verify(
        VerifyEffect::new(
            endpoint,
            VerifyState::Applied {
                observed: directory,
            },
        )
        .expect("verify parent effect"),
    );
    assert!(
        validate_binding_against_journal(
            &journal,
            &auxiliary_binding("C:/RenderPilot/recovery/config.ini", destination),
        )
        .is_err()
    );

    let (mut journal, destination) = auxiliary_journal(
        &["C:/RenderPilot/recovery"],
        "C:/RenderPilot/recovery/config.ini",
    );
    if let OperationEffect::CreateDirectory(effect) = journal.operations_mut()[0].effect_mut() {
        *effect.state_mut() = CreateDirectoryState::Preserved;
    }
    assert!(
        validate_binding_against_journal(
            &journal,
            &auxiliary_binding("C:/RenderPilot/recovery/config.ini", destination),
        )
        .is_err()
    );

    let (journal, destination) = auxiliary_journal(
        &["C:/RenderPilot/recovery/state", "C:/RenderPilot/recovery"],
        "C:/RenderPilot/recovery/state/config.ini",
    );
    assert!(
        validate_binding_against_journal(
            &journal,
            &auxiliary_binding("C:/RenderPilot/recovery/state/config.ini", destination),
        )
        .is_err()
    );
}

fn absent_configuration_acquisition_fixture()
-> (OptiScalerJournal, OptiScalerAggregateBinding, FileReceipt) {
    let capability = capability();
    let prior = FileReceipt::reused("adopted-config-id", hash()).expect("adopted receipt");
    let installed = FileReceipt::owned(
        "recreated-config-id",
        Sha256Hash::new("c".repeat(64)).expect("installed hash"),
    )
    .expect("installed receipt");
    let endpoint = OperationEndpoint::new(
        Endpoint::Single,
        "C:/Game/OptiScaler.ini",
        Preimage::Initial {
            observation: DurableObservation::Absent,
            receipt: None,
            owned_basis: None,
        },
        ExpectedAfter::Known(file_observation(&installed)),
    )
    .expect("configuration endpoint");
    let operation = OperationRecord::new(
        0,
        Vec::new(),
        Some(0),
        PrivateArtifactSlots::new(
            DurableObservation::Absent,
            DurableObservation::Absent,
            DurableObservation::Absent,
        )
        .expect("slots"),
        OperationEffect::Write(
            WriteEffect::new(
                endpoint,
                WriteState::Applied {
                    live: file_observation(&installed),
                    custody: DurableObservation::Absent,
                },
            )
            .expect("configuration write"),
        ),
    )
    .expect("operation");
    let mut journal = test_journal(
        vec!["C:/Game".to_owned()],
        ControlNamespaceBinding::new(
            format!("C:/Game/control-config-{}", capability.as_str()),
            None,
            capability,
        )
        .expect("control namespace"),
        vec![operation],
    );
    journal
        .control_namespace_mut()
        .set_identity("control-config")
        .expect("control identity");
    journal.private_workspaces_mut()[0]
        .set_identity("workspace-config")
        .expect("workspace identity");
    journal.set_materialization(MaterializationState::Ready);

    let path = "C:/Game/OptiScaler.ini".to_owned();
    let binding = OptiScalerAggregateBinding {
        paths: BTreeMap::from([(
            normalized_path_key(&path),
            OptiScalerBoundPath {
                path,
                transition: OptiScalerBoundTransition::AcquireReusedFile {
                    prior: prior.clone(),
                    installed,
                    mode: ReusedAcquisitionMode::Configuration,
                    configuration_baseline: Some(prior.clone()),
                },
            },
        )]),
        after_claims: BTreeMap::new(),
        retained_fsr_custody: BTreeMap::new(),
        known_paths: BTreeSet::new(),
        auxiliary: Vec::new(),
        owned_preservations: Vec::new(),
    };
    (journal, binding, prior)
}

#[test]
fn optiscaler_absent_reused_configuration_acquisition_requires_the_adopted_baseline() {
    let (journal, binding, prior) = absent_configuration_acquisition_fixture();
    validate_binding_against_journal(&journal, &binding)
        .expect("missing configuration may be recreated from its adopted baseline");

    let unrelated = FileReceipt::reused("unrelated-config-id", prior.digest().clone())
        .expect("unrelated baseline");
    let mut invalid = binding;
    let OptiScalerBoundTransition::AcquireReusedFile {
        configuration_baseline,
        ..
    } = &mut invalid
        .paths
        .get_mut(&normalized_path_key("C:/Game/OptiScaler.ini"))
        .expect("configuration binding")
        .transition
    else {
        unreachable!("fixture has acquisition transition");
    };
    *configuration_baseline = Some(unrelated);
    assert!(validate_binding_against_journal(&journal, &invalid).is_err());
}

#[test]
fn optiscaler_present_reused_configuration_acquisition_rebases_to_the_sealed_live_user_bytes() {
    let capability = capability();
    let persisted = FileReceipt::reused("adopted-config-id", hash()).expect("persisted receipt");
    let live = FileReceipt::reused(
        "live-user-config-id",
        Sha256Hash::new("c".repeat(64)).expect("live digest"),
    )
    .expect("live receipt");
    let installed = FileReceipt::owned(
        live.identity().to_owned(),
        Sha256Hash::new("d".repeat(64)).expect("installed digest"),
    )
    .expect("installed receipt");
    let endpoint = OperationEndpoint::new(
        Endpoint::Single,
        "C:/Game/OptiScaler.ini",
        Preimage::Initial {
            observation: file_observation(&live),
            receipt: Some(live.clone()),
            owned_basis: None,
        },
        ExpectedAfter::Known(file_observation(&installed)),
    )
    .expect("configuration endpoint");
    let operation = OperationRecord::new(
        0,
        Vec::new(),
        Some(0),
        PrivateArtifactSlots::new(
            DurableObservation::Absent,
            DurableObservation::Absent,
            DurableObservation::Absent,
        )
        .expect("slots"),
        OperationEffect::Write(
            WriteEffect::new(
                endpoint,
                WriteState::Applied {
                    live: file_observation(&installed),
                    custody: file_observation(&live),
                },
            )
            .expect("configuration write"),
        ),
    )
    .expect("operation");
    let mut journal = test_journal(
        vec!["C:/Game".to_owned()],
        ControlNamespaceBinding::new(
            format!("C:/Game/control-acquire-{}", capability.as_str()),
            None,
            capability,
        )
        .expect("control"),
        vec![operation],
    );
    journal
        .control_namespace_mut()
        .set_identity("control-acquire")
        .expect("control identity");
    journal.private_workspaces_mut()[0]
        .set_identity("workspace-acquire")
        .expect("workspace identity");
    journal.set_materialization(MaterializationState::Ready);
    let path = "C:/Game/OptiScaler.ini".to_owned();
    let binding = OptiScalerAggregateBinding {
        paths: BTreeMap::from([(
            normalized_path_key(&path),
            OptiScalerBoundPath {
                path,
                transition: OptiScalerBoundTransition::AcquireReusedFile {
                    prior: persisted,
                    installed,
                    mode: ReusedAcquisitionMode::Configuration,
                    configuration_baseline: Some(live),
                },
            },
        )]),
        after_claims: BTreeMap::new(),
        retained_fsr_custody: BTreeMap::new(),
        known_paths: BTreeSet::new(),
        auxiliary: Vec::new(),
        owned_preservations: Vec::new(),
    };
    validate_binding_against_journal(&journal, &binding)
        .expect("first configuration write must bind the exact current user bytes, not stale adoption digest");
}

fn reused_configuration_observation_fixture(
    observed: Option<FileReceipt>,
) -> (OptiScalerJournal, OptiScalerAggregateBinding, FileReceipt) {
    let capability = capability();
    let persisted = FileReceipt::reused("adopted-config-id", hash()).expect("persisted receipt");
    let observation = observed
        .as_ref()
        .map_or(DurableObservation::Absent, file_observation);
    let receipt = observed;
    let endpoint = OperationEndpoint::new(
        Endpoint::Single,
        "C:/Game/OptiScaler.ini",
        Preimage::Initial {
            observation: observation.clone(),
            receipt,
            owned_basis: None,
        },
        ExpectedAfter::Known(observation.clone()),
    )
    .expect("configuration observation endpoint");
    let operation = OperationRecord::new(
        0,
        Vec::new(),
        None,
        PrivateArtifactSlots::new(
            DurableObservation::Absent,
            DurableObservation::Absent,
            DurableObservation::Absent,
        )
        .expect("slots"),
        OperationEffect::Verify(
            VerifyEffect::new(
                endpoint,
                VerifyState::Applied {
                    observed: observation,
                },
            )
            .expect("configuration verify"),
        ),
    )
    .expect("operation");
    let mut journal = test_journal(
        vec!["C:/Game".to_owned()],
        ControlNamespaceBinding::new(
            format!("C:/Game/control-observe-{}", capability.as_str()),
            None,
            capability,
        )
        .expect("control"),
        vec![operation],
    );
    journal
        .control_namespace_mut()
        .set_identity("control-observe")
        .expect("control identity");
    journal.set_materialization(MaterializationState::Ready);
    let path = "C:/Game/OptiScaler.ini".to_owned();
    let binding = OptiScalerAggregateBinding {
        paths: BTreeMap::from([(
            normalized_path_key(&path),
            OptiScalerBoundPath {
                path,
                transition: OptiScalerBoundTransition::ObserveReusedConfiguration {
                    persisted: persisted.clone(),
                },
            },
        )]),
        after_claims: BTreeMap::new(),
        retained_fsr_custody: BTreeMap::new(),
        known_paths: BTreeSet::new(),
        auxiliary: Vec::new(),
        owned_preservations: Vec::new(),
    };
    (journal, binding, persisted)
}

#[test]
fn reused_configuration_observation_binds_the_current_live_receipt_but_not_its_adoption_digest() {
    let live = FileReceipt::reused(
        "replaced-live-config-id",
        Sha256Hash::new("c".repeat(64)).expect("live digest"),
    )
    .expect("live receipt");
    let (journal, binding, persisted) = reused_configuration_observation_fixture(Some(live));
    validate_binding_against_journal(&journal, &binding)
        .expect("current user configuration may differ from adopted receipt");
    let transition = &binding
        .paths
        .get(&normalized_path_key("C:/Game/OptiScaler.ini"))
        .expect("configuration transition")
        .transition;
    assert!(matches!(
        transition,
        OptiScalerBoundTransition::ObserveReusedConfiguration { persisted: actual }
            if actual == &persisted
    ));
}

#[test]
fn reused_configuration_observation_accepts_absence_but_rejects_a_non_reused_persisted_receipt() {
    let (journal, binding, persisted) = reused_configuration_observation_fixture(None);
    validate_binding_against_journal(&journal, &binding)
        .expect("a manually removed user configuration is already absent");

    let mut invalid = binding;
    invalid
        .paths
        .get_mut(&normalized_path_key("C:/Game/OptiScaler.ini"))
        .expect("configuration transition")
        .transition = OptiScalerBoundTransition::ObserveReusedConfiguration {
        persisted: FileReceipt::owned(persisted.identity().to_owned(), persisted.digest().clone())
            .expect("invalid owned receipt"),
    };
    assert!(validate_binding_against_journal(&journal, &invalid).is_err());
}

fn retained_fsr_custody_fixture(
    observation: DurableObservation,
    receipt: Option<FileReceipt>,
) -> (OptiScalerJournal, OptiScalerAggregateBinding, FileReceipt) {
    let capability = capability();
    let original = FileReceipt::reused("official-amd-fsr-id", hash()).expect("original receipt");
    let endpoint = OperationEndpoint::new(
        Endpoint::Single,
        "C:/Game/.amd_fidelityfx_dx12.dll.renderpilot-optiscaler-original",
        Preimage::Initial {
            observation: observation.clone(),
            receipt,
            owned_basis: None,
        },
        ExpectedAfter::Known(observation.clone()),
    )
    .expect("custody endpoint");
    let operation = OperationRecord::new(
        0,
        Vec::new(),
        None,
        PrivateArtifactSlots::new(
            DurableObservation::Absent,
            DurableObservation::Absent,
            DurableObservation::Absent,
        )
        .expect("slots"),
        OperationEffect::Verify(
            VerifyEffect::new(
                endpoint,
                VerifyState::Applied {
                    observed: observation,
                },
            )
            .expect("custody verify"),
        ),
    )
    .expect("custody operation");
    let mut journal = test_journal(
        vec!["C:/Game".to_owned()],
        ControlNamespaceBinding::new(
            format!("C:/Game/control-retained-fsr-{}", capability.as_str()),
            None,
            capability,
        )
        .expect("control"),
        vec![operation],
    );
    journal
        .control_namespace_mut()
        .set_identity("control-retained-fsr")
        .expect("control identity");
    journal.set_materialization(MaterializationState::Ready);
    let path = "C:/Game/.amd_fidelityfx_dx12.dll.renderpilot-optiscaler-original".to_owned();
    let key = normalized_path_key(&path);
    let binding = OptiScalerAggregateBinding {
        paths: BTreeMap::from([(
            key.clone(),
            OptiScalerBoundPath {
                path,
                transition: OptiScalerBoundTransition::RelinquishReusedFile {
                    installed: original.clone(),
                },
            },
        )]),
        after_claims: BTreeMap::new(),
        retained_fsr_custody: BTreeMap::from([(key, original.clone())]),
        known_paths: BTreeSet::new(),
        auxiliary: Vec::new(),
        owned_preservations: Vec::new(),
    };
    (journal, binding, original)
}

#[test]
fn retained_fsr_custody_requires_one_exact_original_verify() {
    let original = FileReceipt::reused("official-amd-fsr-id", hash()).expect("original receipt");
    let (journal, binding, _) =
        retained_fsr_custody_fixture(file_observation(&original), Some(original));
    validate_binding_against_journal(&journal, &binding).expect("exact retained original verify");

    let omitted_capability = capability();
    let neutral_observation = DurableObservation::Absent;
    let neutral_endpoint = OperationEndpoint::new(
        Endpoint::Single,
        "C:/Game/neutral.txt",
        Preimage::Initial {
            observation: neutral_observation.clone(),
            receipt: None,
            owned_basis: None,
        },
        ExpectedAfter::Known(neutral_observation.clone()),
    )
    .expect("neutral endpoint");
    let neutral_operation = OperationRecord::new(
        0,
        Vec::new(),
        None,
        PrivateArtifactSlots::new(
            DurableObservation::Absent,
            DurableObservation::Absent,
            DurableObservation::Absent,
        )
        .expect("neutral slots"),
        OperationEffect::Verify(
            VerifyEffect::new(
                neutral_endpoint,
                VerifyState::Applied {
                    observed: neutral_observation,
                },
            )
            .expect("neutral verify"),
        ),
    )
    .expect("neutral operation");
    let mut omitted = OptiScalerJournal::new(
        vec!["C:/Game".to_owned()],
        ControlNamespaceBinding::new(
            format!(
                "C:/Game/control-retained-fsr-omitted-{}",
                omitted_capability.as_str()
            ),
            None,
            omitted_capability,
        )
        .expect("omitted control"),
        Vec::new(),
        vec![neutral_operation],
    )
    .expect("omitted journal");
    omitted
        .control_namespace_mut()
        .set_identity("control-retained-fsr-omitted")
        .expect("omitted control identity");
    omitted.set_materialization(MaterializationState::Ready);
    let mut omitted_binding = binding;
    omitted_binding
        .known_paths
        .insert(normalized_path_key("C:/Game/neutral.txt"));
    assert!(validate_binding_against_journal(&omitted, &omitted_binding).is_err());

    let (absent_journal, absent_binding, _) =
        retained_fsr_custody_fixture(DurableObservation::Absent, None);
    assert!(validate_binding_against_journal(&absent_journal, &absent_binding).is_err());
}
