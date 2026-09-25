fn complete_partial_materialization(
    prepared: &mut PreparedFileMutation<'_>,
) -> Result<(), ServiceError> {
    loop {
        match prepared.journal.materialization().clone() {
            DomainMaterializationState::Planned => {
                let mut next = prepared.journal.clone();
                next.set_materialization(DomainMaterializationState::ControlCreateIntent);
                prepared.cas(next)?;
            }
            DomainMaterializationState::ControlCreateIntent => {
                let capability = capability_from_hex(
                    prepared.journal.control_namespace().capability().as_str(),
                )?;
                let control_path = prepared.transaction_root.join(format!(
                    "control-{}-{}",
                    prepared.id,
                    prepared.journal.control_namespace().capability()
                ));
                let parent = open_or_create_control_parent(
                    control_path
                        .parent()
                        .ok_or_else(|| crate::failed("control namespace has no parent"))?,
                )?;
                let control = match observe(&control_path) {
                    DiskObservation::Absent => {
                        if prepared.journal.control_namespace().identity().is_some() {
                            return Err(crate::failed(
                                "control namespace identity was lost before materialization completed",
                            ));
                        }
                        ControlNamespace::create(
                            &parent,
                            &prepared.id,
                            &capability,
                            crate::fs::AuthorityMode::CooperativeSameUid,
                        )?
                    }
                    DiskObservation::Directory { .. }
                        if prepared.journal.control_namespace().identity().is_some() =>
                    {
                        ControlNamespace::reopen(
                            &parent,
                            &prepared.id,
                            &capability,
                            prepared
                                .journal
                                .control_namespace()
                                .identity()
                                .expect("checked above"),
                        )?
                    }
                    DiskObservation::Directory { .. } => {
                        ControlNamespace::reopen_observed(&parent, &prepared.id, &capability)?
                    }
                    observed => {
                        return Err(crate::failed(format!(
                            "control namespace materialization is not an exact directory: {observed:?}"
                        )));
                    }
                };
                let mut next = prepared.journal.clone();
                next.control_namespace_mut()
                    .set_identity(control.identity().to_owned())
                    .map_err(|error| crate::failed(error.to_string()))?;
                next.set_materialization(DomainMaterializationState::Workspaces {
                    next_workspace_id: 0,
                });
                prepared.cas(next)?;
            }
            DomainMaterializationState::Workspaces { next_workspace_id } => {
                let next_workspace_id = usize::try_from(next_workspace_id)
                    .map_err(|_| crate::failed("materialization ordinal overflow"))?;
                match next_workspace_id.cmp(&prepared.journal.private_workspaces().len()) {
                    std::cmp::Ordering::Equal => {
                        let mut next = prepared.journal.clone();
                        next.set_materialization(DomainMaterializationState::Ready);
                        prepared.cas(next)?;
                    }
                    std::cmp::Ordering::Less => {
                        let workspace_id = u32::try_from(next_workspace_id)
                            .map_err(|_| crate::failed("materialization ordinal overflow"))?;
                        let mut next = prepared.journal.clone();
                        next.set_materialization(
                            DomainMaterializationState::WorkspaceCreateIntent { workspace_id },
                        );
                        prepared.cas(next)?;
                    }
                    std::cmp::Ordering::Greater => {
                        return Err(crate::failed(
                            "materialization frontier exceeds the workspace program",
                        ));
                    }
                }
            }
            DomainMaterializationState::WorkspaceCreateIntent { workspace_id } => {
                let index = usize::try_from(workspace_id)
                    .map_err(|_| crate::failed("materialization ordinal overflow"))?;
                let path =
                    derived_private_workspace_path(&prepared.id, &prepared.journal, workspace_id)?;
                let identity = match observe(&path) {
                    DiskObservation::Absent => {
                        if prepared.journal.private_workspaces()[index]
                            .identity()
                            .is_some()
                        {
                            return Err(crate::failed(
                                "private workspace identity was lost before materialization completed",
                            ));
                        }
                        create_private_directory(&path)?
                    }
                    DiskObservation::Directory { identity } => {
                        let (parent, leaf) = crate::fs::verified_parent(&path)?;
                        let namespace = if prepared.journal.private_workspaces()[index]
                            .identity()
                            .is_some()
                        {
                            crate::fs::PrivateNamespace::reopen(
                                &parent,
                                &leaf,
                                prepared.journal.private_workspaces()[index]
                                    .identity()
                                    .expect("checked above"),
                            )?
                        } else {
                            crate::fs::PrivateNamespace::reopen_observed(&parent, &leaf)?
                        };
                        if namespace.identity() != identity {
                            return Err(crate::failed(
                                "private workspace identity changed during materialization",
                            ));
                        }
                        identity
                    }
                    observed => {
                        return Err(crate::failed(format!(
                            "private workspace materialization is not an exact directory: {observed:?}"
                        )));
                    }
                };
                let mut next = prepared.journal.clone();
                next.private_workspaces_mut()[index]
                    .set_identity(identity)
                    .map_err(|error| crate::failed(error.to_string()))?;
                next.set_materialization(DomainMaterializationState::Workspaces {
                    next_workspace_id: workspace_id
                        .checked_add(1)
                        .ok_or_else(|| crate::failed("materialization ordinal overflow"))?,
                });
                prepared.cas(next)?;
            }
            DomainMaterializationState::Ready => return Ok(()),
        }
    }
}

fn adopt_intent_authorized_artifacts(
    prepared: &mut PreparedFileMutation<'_>,
) -> Result<(), ServiceError> {
    for index in 0..prepared.journal.operations().len() {
        let record = prepared.journal.operations()[index].clone();
        if !record.effect().requires_private_workspace() {
            continue;
        }
        let stage_observed = observe_private_artifact(prepared, index, ArtifactSlot::Stage)?;
        let custody_observed = observe_private_artifact(prepared, index, ArtifactSlot::Custody)?;
        let discard_observed = observe_private_artifact(prepared, index, ArtifactSlot::Discard)?;
        if artifact(&record, ArtifactSlot::Discard) == DiskObservation::Absent
            && discard_observed != DiskObservation::Absent
        {
            return Err(crate::failed(
                "private rollback discard is not bound by its durable intent",
            ));
        }
        match record.effect() {
            DomainOperationEffect::Write(effect) => {
                let artifact_stage = artifact(&record, ArtifactSlot::Stage);
                let artifact_custody = artifact(&record, ArtifactSlot::Custody);
                match effect.state() {
                    DomainWriteState::StageIntent { .. }
                        if artifact_stage == DiskObservation::Absent
                            && stage_observed != DiskObservation::Absent =>
                    {
                        if !matches!(stage_observed, DiskObservation::File { .. }) {
                            return Err(crate::failed(
                                "write stage intent artifact is not an exact file",
                            ));
                        }
                        let mut next = prepared.journal.clone();
                        if let DomainOperationEffect::Write(value) =
                            next.operations_mut()[index].effect_mut()
                        {
                            *value.state_mut() = DomainWriteState::Preserved;
                        }
                        set_artifact(
                            &mut next.operations_mut()[index],
                            ArtifactSlot::Stage,
                            &stage_observed,
                        );
                        prepared.cas(next)?;
                    }
                    DomainWriteState::CaptureIntent {
                        stage: expected_stage,
                    } if artifact_custody == DiskObservation::Absent
                        && custody_observed != DiskObservation::Absent =>
                    {
                        let expected_stage = native(expected_stage);
                        if stage_observed != expected_stage {
                            return Err(crate::failed(
                                "write capture intent stage changed before artifact adoption",
                            ));
                        }
                        if !matches!(custody_observed, DiskObservation::File { .. }) {
                            return Err(crate::failed(
                                "write capture intent artifact is not an exact file",
                            ));
                        }
                        let mut next = prepared.journal.clone();
                        if let DomainOperationEffect::Write(value) =
                            next.operations_mut()[index].effect_mut()
                        {
                            *value.state_mut() = DomainWriteState::Preserved;
                        }
                        set_artifact(
                            &mut next.operations_mut()[index],
                            ArtifactSlot::Stage,
                            &stage_observed,
                        );
                        set_artifact(
                            &mut next.operations_mut()[index],
                            ArtifactSlot::Custody,
                            &custody_observed,
                        );
                        prepared.cas(next)?;
                    }
                    _ => {
                        if artifact_stage == DiskObservation::Absent
                            && stage_observed != DiskObservation::Absent
                        {
                            return Err(crate::failed(
                                "write private stage is not bound by its durable intent",
                            ));
                        }
                        if artifact_custody == DiskObservation::Absent
                            && custody_observed != DiskObservation::Absent
                        {
                            return Err(crate::failed(
                                "write private custody is not bound by its durable intent",
                            ));
                        }
                    }
                }
            }
            DomainOperationEffect::Delete(effect)
                if matches!(effect.state(), DomainDeleteState::CaptureIntent)
                    && artifact(&record, ArtifactSlot::Custody) == DiskObservation::Absent
                    && custody_observed != DiskObservation::Absent =>
            {
                let before = prepared.resolve_before(index, effect.endpoint())?;
                if custody_observed != before
                    || observe(Path::new(effect.endpoint().path())) != DiskObservation::Absent
                {
                    return Err(crate::failed(
                        "delete capture intent custody is not an exact captured preimage",
                    ));
                }
                let mut next = prepared.journal.clone();
                if let DomainOperationEffect::Delete(value) =
                    next.operations_mut()[index].effect_mut()
                {
                    *value.state_mut() = DomainDeleteState::Captured {
                        custody: durable(&custody_observed),
                    };
                }
                set_artifact(
                    &mut next.operations_mut()[index],
                    ArtifactSlot::Custody,
                    &custody_observed,
                );
                prepared.cas(next)?;
            }
            DomainOperationEffect::CreateDirectory(effect)
                if matches!(effect.state(), DomainCreateDirectoryState::StageIntent)
                    && artifact(&record, ArtifactSlot::Stage) == DiskObservation::Absent
                    && stage_observed != DiskObservation::Absent =>
            {
                if !matches!(stage_observed, DiskObservation::Directory { .. }) {
                    return Err(crate::failed(
                        "directory stage intent artifact is not an exact directory",
                    ));
                }
                let mut next = prepared.journal.clone();
                if let DomainOperationEffect::CreateDirectory(value) =
                    next.operations_mut()[index].effect_mut()
                {
                    *value.state_mut() = DomainCreateDirectoryState::Preserved;
                }
                set_artifact(
                    &mut next.operations_mut()[index],
                    ArtifactSlot::Stage,
                    &stage_observed,
                );
                prepared.cas(next)?;
            }
            _ => {
                for (which, observed) in [
                    (ArtifactSlot::Stage, stage_observed),
                    (ArtifactSlot::Custody, custody_observed),
                ] {
                    if artifact(&record, which) == DiskObservation::Absent
                        && observed != DiskObservation::Absent
                    {
                        return Err(crate::failed(
                            "private rollback artifact is not bound by its durable intent",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}
