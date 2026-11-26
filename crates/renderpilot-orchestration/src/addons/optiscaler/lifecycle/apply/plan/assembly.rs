use super::*;

pub(super) fn assemble<'a>(
    prepared: &'a ApplyPlan<'_>,
    targets: super::targets::TargetPlan<'a>,
    topology: super::topology::TopologyPlan,
    config: ConfigInputs,
    rewrite_guards: Vec<MutationTarget>,
    removed: RemovedPathPlan,
) -> Result<FilesystemApplyPlan<'a>, ServiceError> {
    let ApplyPlan {
        context,
        game_id,
        old_state,
        old_managed_files,
        release,
        target,
        ..
    } = prepared;
    let cleanup = removed.cleanup;
    let mut preservations = removed.preservations;
    if let (Some(old_state), Some(current_receipt)) =
        (old_state.as_ref(), config.current_receipt.as_ref())
        && let Some(prior) = old_state.release_files.iter().find(|receipt| {
            receipt.role == OptiScalerFileRole::Configuration
                && crate::paths::same_path(Path::new(receipt.path.as_str()), &config.source_path)
        })
        && prior.installed.authorizes_destructive_cleanup()
        && prior.installed.identity() == current_receipt.identity()
        && prior.installed.digest() != current_receipt.digest()
    {
        // A same-identity edit of an Owned config is user state. Capture that
        // exact preimage before the release write is allowed to replace it;
        // the journal binds the write to both this receipt and persisted
        // prior ownership.
        preservations.push(ConfigPreservationPlan::for_game_with_receipt(
            game_id,
            &config.source_path,
            current_receipt.digest().clone(),
            current_receipt.clone(),
            false,
        )?);
    }
    let removed_fast_deletes = removed.quarantine;
    let retained_fsr = targets.retained_fsr.clone();
    let proxy_relocated = !crate::paths::same_path(&topology.old_proxy_path, &targets.proxy_path);
    let all_paths = topology
        .removed_old_paths
        .iter()
        .filter(|path| {
            removed_fast_deletes
                .iter()
                .any(|planned| crate::paths::same_path(&planned.path, path))
                || (proxy_relocated && crate::paths::same_path(path, &topology.old_proxy_path))
                || old_managed_files.iter().any(|managed| {
                    crate::paths::same_path(Path::new(managed.path().as_str()), path)
                })
        })
        .cloned()
        .chain(targets.new_paths.values().cloned())
        .chain(retained_fsr.iter().map(|plan| plan.target.clone()))
        .chain(retained_fsr.iter().map(|plan| plan.original_backup.clone()))
        .chain(
            targets
                .native_targets
                .iter()
                .map(|target| target.destination.clone()),
        )
        .chain(
            targets
                .artifact_targets
                .iter()
                .map(|(_, path)| path.clone()),
        )
        .chain(topology.existing_topology.iter().flat_map(|topology| {
            topology
                .participant_paths()
                .map(|path| PathBuf::from(path.as_str()))
        }))
        .chain(target.proxy.downstream_path.iter().cloned())
        .chain(target.proxy.reshade_source_path.iter().cloned())
        .chain(
            topology
                .peer_host_transition
                .as_ref()
                .into_iter()
                .flat_map(adoption::PeerHostTransitionPlan::workset_paths),
        )
        .chain(
            preservations
                .iter()
                .map(|preservation| preservation.destination.clone()),
        )
        // A Reused configuration may be retargeted to a different canonical
        // directory. The old path is not otherwise a release target, but it
        // remains a sealed Verify-only participant so storage can derive the
        // Reused-configuration handoff without granting mutation authority.
        .chain(std::iter::once(config.source_path.clone()))
        .chain(std::iter::once(config.destination_path.clone()))
        .collect::<Vec<_>>();
    let mut roots = old_state.as_ref().map_or_else(
        || vec![target.dir.clone()],
        |state| vec![PathBuf::from(state.target_dir.as_str()), target.dir.clone()],
    );
    roots.push(PathBuf::from(
        context
            .storage()
            .require_game(game_id)?
            .install_path()
            .as_str(),
    ));
    roots.extend(
        preservations
            .iter()
            .map(ConfigPreservationPlan::scope_root)
            .collect::<Result<Vec<_>, _>>()?,
    );
    let scope = MutationScope::new(roots)?;
    let peer_transition_targets = topology
        .peer_host_transition
        .as_ref()
        .map_or_default(adoption::PeerHostTransitionPlan::mutation_targets);
    let publication_paths = targets
        .new_paths
        .values()
        .filter(|path| {
            targets
                .release_write_paths
                .contains(&crate::paths::normalized_key(path))
        })
        .cloned()
        .chain(
            targets
                .native_targets
                .iter()
                .filter(|target| {
                    targets
                        .native_copy_paths
                        .contains(&crate::paths::normalized_key(&target.destination))
                })
                .map(|target| target.destination.clone()),
        )
        .chain(
            targets
                .artifact_targets
                .iter()
                .filter(|(_, path)| {
                    targets
                        .artifact_write_paths
                        .contains(&crate::paths::normalized_key(path))
                })
                .map(|(_, path)| path.clone()),
        )
        .chain(
            (config.target_mode == ConfigTargetMode::RetargetToAbsent)
                .then(|| config.destination_path.clone()),
        );
    let journal_targets = apply_snapshot_overrides(
        all_paths,
        removed_fast_deletes
            .into_iter()
            .chain(rewrite_guards)
            .chain(peer_transition_targets),
        publication_paths,
        scope.roots(),
    )?;
    let release_source_url =
        super::super::super::super::source::release_download_url(release)?.to_string();
    Ok(FilesystemApplyPlan {
        layout: PathLayout {
            new_paths: targets.new_paths,
            release_write_paths: targets.release_write_paths,
            native_targets: targets.native_targets,
            native_paths: targets.native_paths,
            artifact_targets: targets.artifact_targets,
            artifact_write_paths: targets.artifact_write_paths,
            runtime_plans: targets.runtime_plans,
            native_expected_sha256: targets.native_expected_sha256,
            native_copy_paths: targets.native_copy_paths,
            removed_old_paths: topology.removed_old_paths,
            old_proxy_path: topology.old_proxy_path,
        },
        cleanup,
        preservations,
        journal: MutationJournal {
            scope,
            targets: journal_targets,
        },
        peer_host_transition: topology.peer_host_transition,
        release_source_url,
        retained_claims: topology.retained_claims,
        retained_fsr,
        config,
    })
}
