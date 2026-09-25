use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PeerTopologyDirection {
    IntoOptiTopology,
    OutOfOptiTopology,
}

pub(super) fn validate_optiscaler_transition<'a>(
    transaction: &rusqlite::Transaction<'_>,
    game_id: &GameId,
    mutation: OptiScalerAggregateMutation<'a>,
) -> AppResult<Option<&'a str>> {
    if let OptiScalerAggregateMutation::AdoptExactMetadata { state, topology } = mutation {
        ensure_state_game(game_id, state)?;
        ensure_topology_game(game_id, topology)?;
        state.validate().map_err(crate::error::invalid_row)?;
        topology.validate().map_err(crate::error::invalid_row)?;
        let install_root = persisted_install_root(transaction, game_id)?;
        ensure_state_paths_within_install(&install_root, state)?;
        ensure_topology_paths_within_install(&install_root, topology)?;
        ensure_state_topology(Some(state), Some(topology), "adoption")?;
        ensure_exact_before_state(transaction, game_id, None)?;
        ensure_exact_before_topology(transaction, game_id, None)?;
        validate_exact_adoption_shape(transaction, state, topology)?;
        validate_optiscaler_receipt_metadata(state, topology)?;
        return Ok(None);
    }

    let (
        before_state,
        after_state,
        before_topology,
        after_topology,
        peer,
        auxiliary_preservations,
        retained_claims,
        mutation_id,
    ) = match mutation {
        OptiScalerAggregateMutation::Filesystem {
            before_state,
            after_state,
            before_topology,
            after_topology,
            peer,
            mutation_id,
        } => (
            before_state,
            after_state,
            before_topology,
            after_topology,
            peer,
            &[][..],
            &[][..],
            mutation_id,
        ),
        OptiScalerAggregateMutation::FilesystemWithAuxiliary {
            before_state,
            after_state,
            before_topology,
            after_topology,
            peer,
            auxiliary_preservations,
            retained_claims,
            mutation_id,
        } => (
            before_state,
            after_state,
            before_topology,
            after_topology,
            peer,
            auxiliary_preservations,
            retained_claims,
            mutation_id,
        ),
        OptiScalerAggregateMutation::AdoptExactMetadata { .. } => {
            return Err(renderpilot_application::AppError::invalid_input(
                "OptiScaler metadata adoption cannot use the filesystem transition path",
            ));
        }
    };

    if mutation_id.trim().is_empty() {
        return Err(renderpilot_application::AppError::invalid_input(
            "OptiScaler filesystem transition requires a mutation id",
        ));
    }
    if before_state.is_none() && after_state.is_none() {
        return Err(renderpilot_application::AppError::invalid_input(
            "OptiScaler filesystem transition must include a state",
        ));
    }
    let install_root = persisted_install_root(transaction, game_id)?;
    if let Some(state) = before_state {
        ensure_state_game(game_id, state)?;
        state.validate().map_err(crate::error::invalid_row)?;
    }
    if let Some(state) = after_state {
        ensure_state_game(game_id, state)?;
        state.validate().map_err(crate::error::invalid_row)?;
    }
    if let Some(topology) = before_topology {
        ensure_topology_game(game_id, topology)?;
        topology.validate().map_err(crate::error::invalid_row)?;
    }
    if let Some(state) = before_state {
        ensure_state_paths_within_install(&install_root, state)?;
    }
    if let Some(state) = after_state {
        ensure_state_paths_within_install(&install_root, state)?;
    }
    if let Some(topology) = before_topology {
        ensure_topology_paths_within_install(&install_root, topology)?;
    }
    if let Some(topology) = after_topology {
        ensure_topology_paths_within_install(&install_root, topology)?;
    }
    ensure_exact_before_state(transaction, game_id, before_state)?;
    ensure_exact_before_topology(transaction, game_id, before_topology)?;
    if let Some(topology) = after_topology {
        ensure_topology_game(game_id, topology)?;
        topology.validate().map_err(crate::error::invalid_row)?;
    }
    ensure_state_topology(before_state, before_topology, "before")?;
    ensure_state_topology(after_state, after_topology, "after")?;
    validate_retained_fsr_component_candidates(transaction, game_id, before_state, after_state)?;
    let topology_subject =
        stable_topology_subject(before_state, after_state, before_topology, after_topology)?;
    let expected_feature = optiscaler_transition_feature(before_state, after_state);
    let expected_peer_relocation = expected_peer_relocation(before_topology, after_topology)?;
    let persisted_peer = match peer {
        OptiScalerPeerMutation::Keep => {
            super::installed_addons::get_within_transaction(transaction, game_id)?
        }
        OptiScalerPeerMutation::Replace { before, after } => {
            ensure_peer_game(game_id, before)?;
            ensure_peer_game(game_id, after)?;
            ensure_peer_paths_within_install(&install_root, after)?;
            ensure_exact_before_peer(transaction, game_id, before)?;
            Some(before.clone())
        }
    };
    let peer_claims_source = expected_peer_relocation.as_ref().is_some_and(|expected| {
        persisted_peer
            .as_ref()
            .is_some_and(|peer| peer_receipt_claims_source(peer, expected))
    });
    let receipt_relocation = validate_peer_receipt_transition(
        expected_peer_relocation.as_ref(),
        peer,
        peer_claims_source,
    )?;
    // A topology relocation still contributes its filesystem transition when
    // no persisted peer receipt claims the source. Only a receipt-backed
    // relocation is consumed through Replace below.
    let peer_relocation = if matches!(peer, OptiScalerPeerMutation::Keep) && !peer_claims_source {
        expected_peer_relocation.as_ref()
    } else {
        receipt_relocation.as_ref()
    };
    let peer_binding = PeerAggregateBinding {
        mutation: peer,
        peer_relocation,
    };
    let binding = build_optiscaler_binding(
        before_state,
        after_state,
        before_topology,
        after_topology,
        peer_binding,
        auxiliary_preservations,
        retained_claims,
    )?;
    pending_file_mutations::validate_optiscaler_binding_within_transaction(
        transaction,
        game_id,
        mutation_id,
        expected_feature,
        topology_subject,
        &binding,
    )?;
    // The OptiScaler-specific custody proof closes the filesystem/aggregate
    // contract, but it does not replace the catalog authority CAS shared by
    // every durable game mutation. Both proofs must still name the same
    // prepared row before feature state can change.
    pending_file_mutations::validate_prepared_mutation_commit_within_transaction(
        transaction,
        game_id,
        mutation_id,
        None,
        false,
    )?;
    Ok(Some(mutation_id))
}

/// Rechecks the one externally sourced OptiScaler baseline inside the same
/// storage transaction that accepts the custody journal.  A retained original
/// is admitted only when the persisted catalog still names the same AMD FSR
/// component, exact active path, and digest; a path-only claim can never turn
/// an arbitrary DLL into an OptiScaler backup source.
fn validate_retained_fsr_component_candidates(
    transaction: &rusqlite::Transaction<'_>,
    game_id: &GameId,
    before_state: Option<&OptiScalerInstallState>,
    after_state: Option<&OptiScalerInstallState>,
) -> AppResult<()> {
    let Some(after_state) = after_state else {
        return Ok(());
    };
    let has_new_retained_baseline = after_state.release_files.iter().any(|file| {
        matches!(
            &file.baseline,
            OptiScalerReleaseFileBaseline::RetainedFsrEntryPoint { .. }
        ) && !before_state
            .into_iter()
            .flat_map(|state| state.release_files.iter())
            .any(|prior| {
                normalized_path_key(prior.path.as_str()) == normalized_path_key(file.path.as_str())
            })
    });
    let components = has_new_retained_baseline
        .then(|| components::list_components_for_game_within_transaction(transaction, game_id))
        .transpose()?;
    for file in &after_state.release_files {
        let OptiScalerReleaseFileBaseline::RetainedFsrEntryPoint {
            component_id,
            original,
            ..
        } = &file.baseline
        else {
            continue;
        };
        let prior = before_state
            .into_iter()
            .flat_map(|state| state.release_files.iter())
            .find(|prior| {
                normalized_path_key(prior.path.as_str()) == normalized_path_key(file.path.as_str())
            })
            .map(|prior| &prior.baseline);
        if let Some(prior) = prior {
            if prior != &file.baseline {
                return Err(renderpilot_application::AppError::invalid_input(format!(
                    "retained OptiScaler AMD FSR baseline changed at {}",
                    file.path
                )));
            }
            // A normal post-install rescan sees active OptiScaler bytes at
            // the target. Immutable custody proves the original thereafter;
            // catalog admission is required only for a new retained baseline.
            continue;
        }
        let target_file_key = normalized_path_key(file.path.as_str());
        let target_file_key = target_file_key.as_str();
        let candidates = components
            .as_ref()
            .expect("new retained FSR baseline loaded component catalog")
            .iter()
            .filter(|component| {
                component.technology().family() == renderpilot_domain::LibraryTechnology::AmdFsr
            })
            .flat_map(|component| {
                component.files().iter().filter_map(move |candidate_file| {
                    (normalized_path_key(candidate_file.path().as_str()) == target_file_key
                        && candidate_file
                            .sha256()
                            .is_some_and(|digest| digest == original.digest()))
                    .then_some(component)
                })
            })
            .collect::<Vec<_>>();
        let [candidate] = candidates.as_slice() else {
            return Err(renderpilot_application::AppError::invalid_input(format!(
                "retained OptiScaler AMD FSR original requires one exact persisted component candidate at {}",
                file.path
            )));
        };
        if candidate.id() != component_id {
            return Err(renderpilot_application::AppError::invalid_input(format!(
                "retained OptiScaler AMD FSR original component identity changed at {}",
                file.path
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::SqliteStorage;
    use renderpilot_application::{ComponentRepository, GameRepository};
    use renderpilot_domain::{
        ComponentFile, ComponentId, ComponentKind, LibraryComponent, LibraryTechnology,
        OptiScalerFileCleanup, OptiScalerFileReceipt, OptiScalerFileRole, PathRef, Swappability,
    };

    #[test]
    fn retained_fsr_admission_rejects_duplicate_exact_component_candidates() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let mut after_state = super::super::tests::transition_state();
        let game_id = after_state.game_id.clone();
        let game = super::super::tests::test_game(game_id.clone());
        storage.upsert_game(&game).expect("game");

        let target = PathRef::new("C:/Games/Test/amd_fidelityfx_dx12.dll").expect("target");
        let original_digest = Sha256Hash::new("e".repeat(64)).expect("original digest");
        let original = FileReceipt::reused("game:official-amd-fsr", original_digest.clone())
            .expect("original receipt");
        after_state.release_files.push(OptiScalerFileReceipt {
            path: target.clone(),
            installed: FileReceipt::owned(
                "optiscaler:amd-fsr-replacement",
                Sha256Hash::new("f".repeat(64)).expect("replacement digest"),
            )
            .expect("replacement receipt"),
            role: OptiScalerFileRole::Runtime,
            cleanup: OptiScalerFileCleanup::RemoveIfUnchanged,
            baseline: OptiScalerReleaseFileBaseline::RetainedFsrEntryPoint {
                component_id: ComponentId::new("component:official-amd-fsr-primary")
                    .expect("component id"),
                custody_path: PathRef::new(
                    "C:/Games/Test/.amd_fidelityfx_dx12.dll.renderpilot-optiscaler-original",
                )
                .expect("custody path"),
                original,
            },
        });

        let component = |id: &str| {
            LibraryComponent::new(
                ComponentId::new(id).expect("component id"),
                game_id.clone(),
                ComponentKind::NativeLibrary,
                LibraryTechnology::AmdFsr,
                Swappability::Swappable,
            )
            .with_file(ComponentFile::new(target.clone()).with_sha256(original_digest.clone()))
        };
        storage
            .replace_components_for_game(
                &game_id,
                &[
                    component("component:official-amd-fsr-primary"),
                    component("component:official-amd-fsr-duplicate"),
                ],
            )
            .expect("component catalog");

        let error = storage
            .with_transaction(|transaction| {
                validate_retained_fsr_component_candidates(
                    transaction,
                    &game_id,
                    None,
                    Some(&after_state),
                )
            })
            .expect_err("duplicate exact candidate must fail admission");
        assert!(
            error
                .to_string()
                .contains("requires one exact persisted component candidate")
        );
    }
}

/// Closes metadata-only adoption around the exact provenance shape that the
/// later custody binder understands. Adoption does not invent ownership: all
/// OptiScaler receipts start Reused, except that an already-persisted peer may
/// prove ownership of a relocated downstream through its typed managed-file
/// claim.
fn validate_exact_adoption_shape(
    transaction: &rusqlite::Transaction<'_>,
    state: &OptiScalerInstallState,
    topology: &GameProxyTopology,
) -> AppResult<()> {
    if state.adoption_state != renderpilot_domain::OptiScalerAdoptionState::AdoptedExact {
        return Err(renderpilot_application::AppError::invalid_input(
            "OptiScaler metadata adoption requires AdoptedExact state",
        ));
    }
    if !state.directory_receipts.is_empty()
        || state
            .release_files
            .iter()
            .any(|file| file.installed.ownership() != FileOwnership::Reused)
        || state
            .runtime_bindings
            .iter()
            .any(|binding| binding.installed.ownership() != FileOwnership::Reused)
    {
        return Err(renderpilot_application::AppError::invalid_input(
            "OptiScaler metadata adoption may contain only Reused file receipts and no directories",
        ));
    }
    let configuration = state
        .configuration_receipt()
        .map_err(crate::error::invalid_row)?;
    let renderpilot_domain::OptiScalerConfigurationBaseline::Present {
        receipt: baseline, ..
    } = state.configuration_baseline()
    else {
        return Err(renderpilot_application::AppError::invalid_input(
            "OptiScaler metadata adoption requires a present configuration baseline",
        ));
    };
    if configuration.installed.ownership() != FileOwnership::Reused
        || configuration.cleanup != OptiScalerFileCleanup::PreserveUnchanged
        || baseline.ownership() != FileOwnership::Reused
        || baseline != &configuration.installed
    {
        return Err(renderpilot_application::AppError::invalid_input(
            "OptiScaler metadata adoption has an invalid Reused configuration tuple",
        ));
    }
    if topology.outer.implementation != ProxyImplementation::OptiScaler
        || topology.outer.receipt.ownership() != FileOwnership::Reused
    {
        return Err(renderpilot_application::AppError::invalid_input(
            "OptiScaler metadata adoption requires a Reused OptiScaler topology outer",
        ));
    }
    let Some(downstream) = topology.downstream.as_ref() else {
        return Ok(());
    };
    if downstream.receipt.ownership() == FileOwnership::Reused {
        return Ok(());
    }

    let Some(peer) = super::installed_addons::get_within_transaction(transaction, &state.game_id)?
    else {
        return Err(renderpilot_application::AppError::invalid_input(
            "owned OptiScaler adoption downstream requires one persisted Luma or RenoDX peer claim",
        ));
    };
    if !matches!(peer.kind(), AddonKind::Luma | AddonKind::RenoDx) {
        return Err(renderpilot_application::AppError::invalid_input(
            "owned OptiScaler adoption downstream has an unsupported peer kind",
        ));
    }
    let exact_claims = peer
        .managed_files()
        .iter()
        .filter(|claim| {
            normalized_path_key(claim.path().as_str())
                == normalized_path_key(downstream.path.as_str())
                && claim.mode() == ManagedFileMode::Owned
                && claim.installed_sha256() == downstream.receipt.digest()
        })
        .count();
    if exact_claims != 1 {
        return Err(renderpilot_application::AppError::invalid_input(
            "owned OptiScaler adoption downstream has no unique exact peer managed-file claim",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ExpectedPeerRelocation {
    pub(super) source: PathRef,
    pub(super) destination: PathRef,
    pub(super) sha256: Sha256Hash,
    pub(super) ownership: FileOwnership,
    pub(super) direction: PeerTopologyDirection,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PeerAggregateBinding<'a> {
    pub(super) mutation: OptiScalerPeerMutation<'a>,
    pub(super) peer_relocation: Option<&'a ExpectedPeerRelocation>,
}
