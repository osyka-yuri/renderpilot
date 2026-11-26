use super::super::super::identity::matches_optional as file_matches;
use super::super::adoption;
use super::super::*;
use super::execution::{ApplyExecution, ExecutionOutcome, PreparedApplyOutcome};
use super::plan::{CleanupPlan, FilesystemApplyPlan, PathLayout, RetainedFsrOriginalAction};
use super::retained_fsr::prepare_retained_fsr_originals;
use crate::file_mutation::optiscaler::{PlannedPreimage, ReusedMutationAuthority};

pub(in crate::addons::optiscaler) fn apply_release(
    prepared: &ApplyPlan<'_>,
) -> Result<OptiScalerOperationResult, ServiceError> {
    let mut plan = FilesystemApplyPlan::build(prepared)?;
    let merged_config = ApplyExecution {
        prepared,
        plan: &plan,
    }
    .merge_config();
    let merged_digest = renderpilot_detection::sha256_bytes(&merged_config.bytes)
        .map_err(|error| failed(format!("failed to hash merged OptiScaler config: {error}")))?;
    let config_key = crate::paths::normalized_key(&plan.config.destination_path);
    if plan.config.target_mode == super::plan::ConfigTargetMode::InPlace
        && plan
            .config
            .current_receipt
            .as_ref()
            .is_some_and(|receipt| receipt.digest() == &merged_digest)
    {
        plan.layout.release_write_paths.remove(&config_key);
    } else {
        plan.layout.release_write_paths.insert(config_key);
    }
    let subject_id = format!("optiscaler:{}", prepared.game_id.as_str());
    let old_directory_receipts = prepared
        .old_state
        .as_ref()
        .map_or(&[][..], |state| state.directory_receipts.as_slice());
    let post_commit_directory_receipts = super::super::post_commit_directory_receipts(
        old_directory_receipts,
        Some(&prepared.target.dir),
    );
    let mut write_paths = plan.layout.release_write_paths.clone();
    write_paths.extend(plan.layout.native_copy_paths.iter().cloned());
    write_paths.extend(
        plan.layout
            .artifact_targets
            .iter()
            .filter(|(_, path)| {
                plan.layout
                    .artifact_write_paths
                    .contains(&crate::paths::normalized_key(path))
            })
            .map(|(_, path)| crate::paths::normalized_key(path)),
    );
    // The proxy executor owns the actual outer publication. Include its
    // participant as a Write when the archive postimage differs, otherwise
    // Verify preserves a pre-existing exact file as Reused.
    if let Some(core) = prepared
        .release
        .members
        .iter()
        .find(|member| member.target == "$proxy")
        && let Ok(expected) = Sha256Hash::new(&core.sha256)
        && !file_matches(&prepared.target.proxy.slot, Some(&expected))
    {
        write_paths.insert(crate::paths::normalized_key(&prepared.target.proxy.slot));
    }
    for preservation in &plan.preservations {
        write_paths.insert(crate::paths::normalized_key(&preservation.destination));
    }
    let mut delete_paths = plan
        .cleanup
        .exact_removed_paths
        .iter()
        .map(|path| crate::paths::normalized_key(path))
        .collect::<HashSet<_>>();
    let proxy_relocation_source = prepared
        .context
        .storage()
        .get_proxy_topology(&prepared.game_id)?
        .and_then(|topology| {
            (!crate::paths::same_path(
                Path::new(topology.root_slot.as_str()),
                &prepared.target.proxy.slot,
            ))
            .then(|| PathBuf::from(topology.root_slot.as_str()))
        });
    if let Some(source) = &proxy_relocation_source {
        // Publishing an outer at a new slot first releases the sealed old
        // outer. `exact_state_preimage` below binds this Delete to either
        // Owned custody or the narrow Reused OptiScaler-artifact authority.
        delete_paths.insert(crate::paths::normalized_key(source));
    }
    // Relocated edited configs are copied to recovery first and then removed
    // by the preservation executor. Their source still needs a typed
    // Delete operation so the exact current receipt and persisted prior
    // Owned identity are both enforced by the journal.
    delete_paths.extend(
        plan.preservations
            .iter()
            .filter(|preservation| preservation.remove_source)
            .map(|preservation| crate::paths::normalized_key(&preservation.source)),
    );
    let mut relocations = plan
        .peer_host_transition
        .as_ref()
        .map_or_default(adoption::PeerHostTransitionPlan::relocations);
    relocations.extend(
        plan.retained_fsr
            .iter()
            .filter_map(|retained| match &retained.action {
                RetainedFsrOriginalAction::Acquire => {
                    Some((retained.target.clone(), retained.original_backup.clone()))
                }
                RetainedFsrOriginalAction::Restore { .. } => {
                    Some((retained.original_backup.clone(), retained.target.clone()))
                }
                RetainedFsrOriginalAction::Preserve => None,
            }),
    );
    if let (Some(source), Some(destination)) = (
        prepared.target.proxy.reshade_source_path.as_ref(),
        prepared.target.proxy.downstream_path.as_ref(),
    ) && !crate::paths::same_path(source, destination)
    {
        relocations.push((source.clone(), destination.clone()));
    }
    let mut exact_preimages: HashMap<String, PlannedPreimage> = HashMap::new();
    if let Some(state) = prepared.old_state.as_ref() {
        for receipt in &state.release_files {
            let path = Path::new(receipt.path.as_str());
            // The Reused configuration is deliberately the one mutable
            // content exception. `load_config_inputs` has already read its
            // bytes and identity together through one authority handle; do
            // not take a second observation here and accidentally bind a
            // different file generation than the three-way merge consumed.
            if receipt.role == OptiScalerFileRole::Configuration
                && receipt.installed.ownership() == FileOwnership::Reused
                && crate::paths::same_path(path, &plan.config.source_path)
            {
                continue;
            }
            let authority = match receipt.role {
                OptiScalerFileRole::Configuration => ReusedMutationAuthority::ConfigurationWrite,
                OptiScalerFileRole::Runtime => ReusedMutationAuthority::OptiScalerArtifact,
            };
            exact_preimages.insert(
                crate::paths::normalized_key(path),
                exact_state_preimage(
                    path,
                    &receipt.installed,
                    authority,
                    Some(Path::new(state.target_dir.as_str())),
                )?,
            );
        }
        for binding in &state.runtime_bindings {
            let path = Path::new(binding.path.as_str());
            exact_preimages.insert(
                crate::paths::normalized_key(path),
                exact_state_preimage(
                    path,
                    &binding.installed,
                    ReusedMutationAuthority::OptiScalerArtifact,
                    Some(Path::new(state.target_dir.as_str())),
                )?,
            );
        }
    }
    let config_source_key = crate::paths::normalized_key(&plan.config.source_path);
    let reused_configuration_source = prepared.old_state.as_ref().is_some_and(|state| {
        prior_release_receipt(Some(state), &plan.config.source_path).is_some_and(|receipt| {
            receipt.role == OptiScalerFileRole::Configuration
                && receipt.installed.ownership() == FileOwnership::Reused
        })
    });
    if reused_configuration_source {
        exact_preimages.insert(
            config_source_key,
            plan.config
                .current_receipt
                .as_ref()
                .map_or(PlannedPreimage::Absent, |current| {
                    PlannedPreimage::ExactReused {
                        current: current.clone(),
                        authority: ReusedMutationAuthority::ConfigurationWrite,
                    }
                }),
        );
    } else if let Some(current) = plan.config.current_receipt.as_ref() {
        exact_preimages
            .entry(config_source_key)
            .or_insert_with(|| PlannedPreimage::ExactReused {
                current: current.clone(),
                authority: ReusedMutationAuthority::ConfigurationWrite,
            });
    }
    for (source, _) in &relocations {
        let key = crate::paths::normalized_key(source);
        if let std::collections::hash_map::Entry::Vacant(entry) = exact_preimages.entry(key) {
            let current = super::super::exact_receipt_from_live(source, FileOwnership::Reused)?;
            entry.insert(PlannedPreimage::ExactReused {
                current,
                authority: ReusedMutationAuthority::RelocationSource,
            });
        }
    }
    for retained in &plan.retained_fsr {
        match &retained.action {
            RetainedFsrOriginalAction::Acquire => {
                exact_preimages.insert(
                    crate::paths::normalized_key(&retained.target),
                    PlannedPreimage::ExactReused {
                        current: retained.original.clone(),
                        authority: ReusedMutationAuthority::RelocationSource,
                    },
                );
            }
            RetainedFsrOriginalAction::Preserve => {
                exact_preimages.insert(
                    crate::paths::normalized_key(&retained.original_backup),
                    PlannedPreimage::ExactReused {
                        current: retained.original.clone(),
                        authority: ReusedMutationAuthority::ObservationOnly,
                    },
                );
            }
            RetainedFsrOriginalAction::Restore { .. } => {
                exact_preimages.insert(
                    crate::paths::normalized_key(&retained.original_backup),
                    PlannedPreimage::ExactReused {
                        current: retained.original.clone(),
                        authority: ReusedMutationAuthority::RelocationSource,
                    },
                );
            }
        }
    }
    if let Some(transition) = plan.peer_host_transition.as_ref()
        && let Some(destination) = transition.destination_receipt.as_ref()
    {
        exact_preimages
            .entry(crate::paths::normalized_key(Path::new(
                transition.to.as_str(),
            )))
            .or_insert_with(|| PlannedPreimage::Exact {
                current: destination.clone(),
                prior_owned: None,
            });
    }
    if let Some(sidecar) = plan
        .peer_host_transition
        .as_ref()
        .and_then(|transition| transition.sidecar.as_ref())
    {
        exact_preimages.insert(
            crate::paths::normalized_key(&sidecar.source),
            PlannedPreimage::ExactReused {
                current: sidecar.source_receipt.clone(),
                authority: ReusedMutationAuthority::RelocationSource,
            },
        );
    }
    if let Some(topology) = prepared
        .context
        .storage()
        .get_proxy_topology(&prepared.game_id)?
    {
        let outer_path = Path::new(topology.outer.path.as_str());
        exact_preimages.insert(
            crate::paths::normalized_key(outer_path),
            exact_state_preimage(
                outer_path,
                &topology.outer.receipt,
                ReusedMutationAuthority::OptiScalerArtifact,
                None,
            )?,
        );
        if let Some(downstream) = topology.downstream {
            let downstream_path = Path::new(downstream.path.as_str());
            exact_preimages.insert(
                crate::paths::normalized_key(downstream_path),
                exact_state_preimage(
                    downstream_path,
                    &downstream.receipt,
                    ReusedMutationAuthority::RelocationSource,
                    None,
                )?,
            );
        }
    }
    for preservation in &plan.preservations {
        if let Some(prior) =
            super::super::prior_release_receipt(prepared.old_state.as_ref(), &preservation.source)
            && prior.installed.authorizes_destructive_cleanup()
        {
            let current = preservation.owned_source_receipt().clone();
            exact_preimages.insert(
                crate::paths::normalized_key(&preservation.source),
                PlannedPreimage::Exact {
                    current,
                    prior_owned: Some(prior.installed.clone()),
                },
            );
        }
    }
    // `removed_old_paths` also carries deselected runtime bindings. They are
    // not release-layout files, so the earlier removal planner deliberately
    // leaves their producer to the managed-file pass. Give present Owned or
    // exact-adopted Reused artifacts a Delete ordinal; a manually missing
    // endpoint remains a typed Verify(Absent) and is therefore already
    // satisfied rather than becoming an invalid Delete(Absent).
    if let Some(state) = prepared.old_state.as_ref() {
        for path in &plan.layout.removed_old_paths {
            if plan.config.target_mode == super::plan::ConfigTargetMode::RetargetToAbsent
                && crate::paths::same_path(path, &plan.config.source_path)
            {
                // The former Reused configuration is a verify-only handoff,
                // not a release deletion. Its successor has an Absent
                // baseline at the new canonical path.
                continue;
            }
            let key = crate::paths::normalized_key(path);
            if plan.cleanup.other_claim_paths.contains(&key)
                || !matches!(
                    exact_preimages.get(&key),
                    Some(PlannedPreimage::Exact { .. } | PlannedPreimage::ExactReused { .. })
                )
            {
                continue;
            }
            if prior_release_receipt(Some(state), path).is_some()
                || prior_runtime_binding(Some(state), path).is_some()
            {
                delete_paths.insert(key);
            }
        }
    }
    let mut preferred_paths = plan
        .retained_fsr
        .iter()
        .filter(|retained| !matches!(&retained.action, RetainedFsrOriginalAction::Acquire))
        .map(|retained| retained.original_backup.as_path())
        .collect::<Vec<_>>();
    if let Some(source) = &proxy_relocation_source {
        // `execute_proxy_topology` performs the root release and publication
        // as one dependency chain. The custody program must expose exactly
        // that order before it reaches the independent configuration handoff.
        preferred_paths.push(source.as_path());
    }
    preferred_paths.extend(
        plan.preservations
            .iter()
            .flat_map(|preservation| [&preservation.destination, &preservation.source])
            .map(PathBuf::as_path),
    );
    preferred_paths.push(prepared.target.proxy.slot.as_path());
    // A removed endpoint with a sealed Absent preimage is a terminal
    // verification, not a prerequisite for publication at another path.
    // Keep the proxy root ahead of it so the topology executor can consume
    // its ordered directory frontier and root publication as one sequence.
    preferred_paths.extend(
        plan.cleanup
            .exact_removed_paths
            .iter()
            .map(PathBuf::as_path),
    );
    preferred_paths.extend(plan.layout.removed_old_paths.iter().map(PathBuf::as_path));
    preferred_paths.extend(
        selected_members(&prepared.release, &prepared.modules)
            .filter(|member| member.target != "$proxy")
            .filter_map(|member| {
                plan.layout
                    .new_paths
                    .get(&member.archive_path.to_ascii_lowercase())
                    .map(PathBuf::as_path)
            }),
    );
    preferred_paths.extend(
        plan.layout
            .native_targets
            .iter()
            .map(|target| target.destination.as_path()),
    );
    preferred_paths.extend(
        plan.layout
            .artifact_targets
            .iter()
            .map(|(_, path)| path.as_path()),
    );
    let operations = super::super::build_operations(
        &plan.journal.targets,
        &write_paths,
        &delete_paths,
        &relocations,
        &exact_preimages,
        &post_commit_directory_receipts,
        preferred_paths,
    )?;
    let managed_endpoint_roots = managed_apply_endpoint_roots(prepared, &plan)?;
    crate::FileSafetyAuthority::new().authorize_game_commit(
        prepared.context,
        prepared.intent.mutation_feature(),
        &prepared.guard,
        &prepared.safety,
        || {
            crate::file_mutation::optiscaler::run_optiscaler_mutation(
                &crate::file_mutation::optiscaler::OptiScalerMutation {
                    context: prepared.context,
                    guard: &prepared.guard,
                    scope: &plan.journal.scope,
                    feature: prepared.intent.mutation_feature(),
                    subject_id: Some(&subject_id),
                    operations,
                    managed_endpoint_roots,
                    threat_model: crate::file_mutation::optiscaler::ThreatModel::CooperativeSameUid,
                },
                |mutation| execute_prepared_apply(prepared, &plan, mutation),
                |prepared_mutation, outcome| {
                    let peer_before = match outcome.peer_transition.as_ref() {
                        Some(transition) => Some(transition.before.clone()),
                        None => prepared
                            .context
                            .storage()
                            .get_installed_addon(&prepared.game_id)
                            .map_err(ServiceError::from)?,
                    };
                    let before = renderpilot_storage_sqlite::AggregateBefore::new(
                        prepared.game_id.clone(),
                        prepared.old_state.clone(),
                        outcome.before_topology.clone(),
                        peer_before,
                    )
                    .map_err(ServiceError::from)?;
                    let mutation_id = prepared_mutation.id().to_owned();
                    let aggregate_mutation =
                        renderpilot_storage_sqlite::OptiScalerAggregateMutation::FilesystemWithAuxiliary {
                            before_state: prepared.old_state.as_ref(),
                            after_state: Some(&outcome.state),
                            before_topology: outcome.before_topology.as_ref(),
                            after_topology: Some(&outcome.topology),
                            peer: outcome
                                .peer_transition
                                .as_ref()
                                .map(|transition| {
                                    renderpilot_storage_sqlite::OptiScalerPeerMutation::Replace {
                                        before: &transition.before,
                                        after: &transition.after,
                                    }
                                })
                                .unwrap_or_default(),
                            auxiliary_preservations: &outcome.auxiliary_preservations,
                            retained_claims: &plan.retained_claims,
                            mutation_id: &mutation_id,
                        };
                    prepared_mutation.commit_journal_aggregate(before, aggregate_mutation)
                },
                |_| {},
                || {},
            )
            .map(|outcome| outcome.operation)
        },
    )
}

/// Declares the exact OptiScaler endpoints for which a missing descendant may
/// be observed through a retained root. Existing state uses its persisted
/// target directory; new publication targets use the authoritative game root.
/// Nothing outside those two roots is relaxed, and only planned publications
/// (including their explicit missing parent directories) enter this map.
fn managed_apply_endpoint_roots(
    prepared: &ApplyPlan<'_>,
    plan: &FilesystemApplyPlan<'_>,
) -> Result<Vec<(PathBuf, PathBuf)>, ServiceError> {
    let game_root = PathBuf::from(
        prepared
            .context
            .storage()
            .require_game(&prepared.game_id)?
            .install_path()
            .as_str(),
    );
    let persisted_target = prepared
        .old_state
        .as_ref()
        .map(|state| PathBuf::from(state.target_dir.as_str()));
    let mut endpoints = std::collections::BTreeMap::new();
    let mut insert = |endpoint: PathBuf| {
        let root = persisted_target
            .as_ref()
            .filter(|root| {
                crate::paths::is_within(&endpoint, root)
                    && !crate::paths::same_path(&endpoint, root)
            })
            .cloned()
            .or_else(|| {
                (crate::paths::is_within(&endpoint, &game_root)
                    && !crate::paths::same_path(&endpoint, &game_root))
                .then(|| game_root.clone())
            });
        if let Some(root) = root {
            endpoints
                .entry(crate::paths::normalized_key(&endpoint))
                .or_insert((root, endpoint));
        }
    };
    if let Some(state) = prepared.old_state.as_ref() {
        for (_, endpoint) in super::super::managed_state_endpoint_roots(state) {
            insert(endpoint);
        }
    }
    let publication_keys = plan
        .layout
        .release_write_paths
        .iter()
        .chain(plan.layout.native_copy_paths.iter())
        .chain(plan.layout.artifact_write_paths.iter())
        .cloned()
        .collect::<HashSet<_>>();
    for target in &plan.journal.targets {
        let key = crate::paths::normalized_key(&target.path);
        if target.is_absent_directory() || publication_keys.contains(&key) {
            insert(target.path.clone());
        }
    }
    Ok(endpoints.into_values().collect())
}

fn exact_state_preimage(
    path: &Path,
    persisted: &FileReceipt,
    reused_authority: crate::file_mutation::optiscaler::ReusedMutationAuthority,
    managed_root: Option<&Path>,
) -> Result<crate::file_mutation::optiscaler::PlannedPreimage, ServiceError> {
    use crate::file_mutation::optiscaler::PlannedPreimage;

    let current = match managed_root {
        Some(root) => {
            super::super::maybe_exact_managed_receipt_from_live(root, path, persisted.ownership())?
        }
        None => super::super::maybe_exact_receipt_from_live(path, persisted.ownership())?,
    };
    let Some(current) = current else {
        return Ok(PlannedPreimage::Absent);
    };
    let is_configuration = matches!(
        reused_authority,
        crate::file_mutation::optiscaler::ReusedMutationAuthority::ConfigurationWrite
    );
    let exact_persisted_match =
        current.identity() == persisted.identity() && current.digest() == persisted.digest();
    if (persisted.ownership() == FileOwnership::Owned || !is_configuration)
        && !exact_persisted_match
    {
        return Err(failed(format!(
            "OptiScaler persisted participant drifted at {}",
            path.display()
        )));
    }
    Ok(match persisted.ownership() {
        FileOwnership::Owned => PlannedPreimage::Exact {
            current,
            prior_owned: Some(persisted.clone()),
        },
        FileOwnership::Reused => PlannedPreimage::ExactReused {
            current,
            authority: reused_authority,
        },
    })
}

fn execute_prepared_apply(
    prepared: &ApplyPlan<'_>,
    plan: &FilesystemApplyPlan<'_>,
    mutation: &mut PreparedFileMutation<'_>,
) -> Result<PreparedApplyOutcome, ServiceError> {
    let ApplyPlan {
        context,
        game_id,
        old_state,
        release,
        artifacts,
        target,
        ..
    } = prepared;
    let old_state = old_state.as_ref();
    let PathLayout {
        removed_old_paths,
        old_proxy_path,
        ..
    } = &plan.layout;
    let CleanupPlan {
        other_claim_paths,
        preplanned_nonmanaged_keys,
        exact_removed_paths,
        preplanned_preserved,
    } = &plan.cleanup;

    let mut changed = Vec::new();
    let mut preserved = preplanned_preserved.clone();
    let mut auxiliary_preservations = Vec::new();
    let proxy_relocation_precedes_retained_fsr = plan.peer_host_transition.is_some()
        || matches!(
            (
                target.proxy.reshade_source_path.as_ref(),
                target.proxy.downstream_path.as_ref(),
            ),
            (Some(source), Some(destination)) if !crate::paths::same_path(source, destination)
        );
    let mut retained_fsr_baselines = if proxy_relocation_precedes_retained_fsr {
        HashMap::new()
    } else {
        prepare_retained_fsr_originals(plan, mutation, &mut changed)?
    };
    for preservation in &plan.preservations {
        let recovery = preservation.execute(mutation, true)?;
        changed.push(preservation.source.to_string_lossy().into_owned());
        preserved.push(recovery.destination.to_string_lossy().into_owned());
        let source_current = preservation.owned_source_receipt().clone();
        if recovery.destination_receipt.digest() != source_current.digest() {
            return Err(failed(format!(
                "OptiScaler recovery destination digest differs from source: {}",
                recovery.destination.display()
            )));
        }
        auxiliary_preservations.push(
            renderpilot_storage_sqlite::OptiScalerAuxiliaryPreservation {
                source: path_ref(&preservation.source)?,
                destination: path_ref(&recovery.destination)?,
                source_current,
                destination_receipt: recovery.destination_receipt,
            },
        );
    }
    let topology = execute_proxy_topology(
        ProxyTopologyExecution {
            context,
            game_id,
            proxy: &target.proxy,
            release,
            archive: &artifacts.archive,
            updating: old_state.is_some(),
            downstream_ownership: plan.peer_host_transition.as_ref().map_or(
                FileOwnership::Reused,
                adoption::PeerHostTransitionPlan::destination_ownership,
            ),
        },
        mutation,
        &mut changed,
    )?;
    if let Some(peer_transition) = &plan.peer_host_transition {
        let Some(downstream) = topology.downstream.as_ref() else {
            return Err(failed(
                "peer host transition completed without a downstream topology",
            ));
        };
        if downstream.receipt.digest() != &peer_transition.live_sha256 {
            return Err(failed(
                "relocated ReShade peer bytes do not match the preflighted host",
            ));
        }
        peer_transition.execute_sidecar(mutation, &mut changed)?;
    }
    if proxy_relocation_precedes_retained_fsr {
        retained_fsr_baselines = prepare_retained_fsr_originals(plan, mutation, &mut changed)?;
    }
    for path in exact_removed_paths {
        if let Some(receipt) = prior_release_receipt(old_state, path) {
            // A manifest-proven release artifact is managed by the persisted
            // state root. On repair drift, `release_exact_file` consumes its
            // sealed Verify(Absent); otherwise it performs the exact delete.
            release_exact_file(
                mutation,
                path,
                &receipt.installed,
                &OptiScalerFileBaseline::Absent,
                old_state.map(|state| Path::new(state.target_dir.as_str())),
                &mut changed,
            )?;
        } else if let Some(binding) = prior_runtime_binding(old_state, path) {
            release_exact_file(
                mutation,
                path,
                &binding.installed,
                &binding.baseline,
                old_state.map(|state| Path::new(state.target_dir.as_str())),
                &mut changed,
            )?;
        } else if let Some(receipt) =
            context
                .storage()
                .get_proxy_topology(game_id)?
                .and_then(|stored| {
                    (crate::paths::same_path(Path::new(stored.outer.path.as_str()), path))
                        .then_some(stored.outer.receipt)
                })
        {
            mutation.delete_file_exact(path, &receipt)?;
            changed.push(path.to_string_lossy().into_owned());
        } else {
            // Digest-only planning evidence never authorizes a destructive
            // syscall in the custody lifecycle.
            preserved.push(path.to_string_lossy().into_owned());
        }
    }
    let proxy_relocated =
        !crate::paths::same_path(old_proxy_path, Path::new(topology.root_slot.as_str()));

    if plan.config.target_mode == super::plan::ConfigTargetMode::RetargetToAbsent {
        // The old configuration is deliberately retained outside the
        // successor state. Consume its sealed Verify ordinal before the new
        // Absent-baseline configuration is published; it is never a delete,
        // relocation, or auxiliary-preservation source.
        mutation.verify_unchanged(&plan.config.source_path)?;
        preserved.push(plan.config.source_path.to_string_lossy().into_owned());
    }

    for old_path in removed_old_paths {
        // A proxy-slot relocation is released by the topology transition:
        // either the original baseline was restored or an originally
        // absent slot was removed. Do not run the generic release-layout
        // cleanup over that path a second time.
        if proxy_relocated && crate::paths::same_path(old_path, old_proxy_path) {
            continue;
        }
        if plan.retained_fsr.iter().any(|retained| {
            matches!(&retained.action, RetainedFsrOriginalAction::Restore { .. })
                && crate::paths::same_path(old_path, &retained.target)
        }) {
            // This endpoint's deletion (or sealed Absent verification) is
            // already immediately followed by restoration of its game-owned
            // original. Never run generic release cleanup over it a second
            // time.
            continue;
        }
        let key = crate::paths::normalized_key(old_path);
        if preplanned_nonmanaged_keys.contains(&key) {
            continue;
        }
        if !other_claim_paths.contains(&key) {
            if let Some(receipt) = prior_release_receipt(old_state, old_path) {
                let baseline = OptiScalerFileBaseline::Absent;
                release_exact_file(
                    mutation,
                    old_path,
                    &receipt.installed,
                    &baseline,
                    old_state.map(|state| Path::new(state.target_dir.as_str())),
                    &mut changed,
                )?;
            } else if let Some(binding) = prior_runtime_binding(old_state, old_path) {
                release_exact_file(
                    mutation,
                    old_path,
                    &binding.installed,
                    &binding.baseline,
                    old_state.map(|state| Path::new(state.target_dir.as_str())),
                    &mut changed,
                )?;
            }
        }
    }

    let merge = ApplyExecution { prepared, plan }.merge_config();

    let (release_files, fresh_configuration_baseline) = ApplyExecution { prepared, plan }
        .write_release_files(
            &merge.bytes,
            mutation,
            &mut changed,
            &retained_fsr_baselines,
        )?;

    let managed_files =
        ApplyExecution { prepared, plan }.materialize_managed_files(mutation, &mut changed)?;
    let ExecutionOutcome { state, .. } = ApplyExecution { prepared, plan }.build_receipts(
        mutation,
        &topology,
        release_files,
        managed_files,
        fresh_configuration_baseline,
    )?;
    let before_topology = context.storage().get_proxy_topology(game_id)?;
    let operation_state = Some((&state).into());
    Ok(PreparedApplyOutcome {
        state,
        topology,
        before_topology,
        peer_transition: plan
            .peer_host_transition
            .as_ref()
            .map(|transition| transition.receipt.clone()),
        auxiliary_preservations,
        operation: OptiScalerOperationResult {
            kind: AddonKind::OptiScaler,
            state: operation_state,
            changed_paths: changed,
            preserved_paths: preserved,
            config_conflicts: merge.conflicts,
        },
    })
}
