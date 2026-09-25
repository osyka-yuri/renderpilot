//! Execution of the already-closed uninstall plan.

use super::super::*;
use super::plan::{self, UninstallStep};

struct UninstallExecution {
    operation: OptiScalerOperationResult,
    auxiliary_preservations: Vec<renderpilot_storage_sqlite::OptiScalerAuxiliaryPreservation>,
}

/// The concrete filesystem footprint of a prepared OptiScaler uninstall.
///
/// This is deliberately narrower than the mutation scope: catalog cleanup
/// uses it only to detect collisions with component rollback plans. Read-only
/// verification paths, workspace paths, and authority roots are not part of
/// this footprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OptiScalerManagedCleanupFootprint {
    pub(crate) exact_mutations: Vec<PathBuf>,
    pub(crate) removed_directories: Vec<PathBuf>,
}

struct PreparedUninstall {
    state: OptiScalerInstallState,
    topology: GameProxyTopology,
    plan: plan::UninstallFsPlan,
    retained_claims: Vec<renderpilot_storage_sqlite::OptiScalerRetainedClaim>,
}

fn prepare_uninstall_locked(
    context: &Context,
    game_id: &GameId,
) -> Result<PreparedUninstall, ServiceError> {
    let state = context
        .storage()
        .get_optiscaler_install_state(game_id)?
        .ok_or_else(|| failed("OptiScaler is not installed for this game"))?;
    let managed_files = managed_bindings_from_state(Some(&state));
    let topology = context
        .storage()
        .get_proxy_topology(game_id)?
        .ok_or_else(|| {
            failed("OptiScaler proxy topology is missing; run repair before uninstall")
        })?;
    let topology = crate::addons::proxy_chain::revalidate_for_removal(&topology)?;
    super::validate_release_file_receipt(&state, &topology)?;
    let other_claims = other_managed_claims(context, game_id)?;
    let other_claim_paths = other_claims
        .iter()
        .map(|claim| crate::paths::normalized_key(Path::new(claim.path.as_str())))
        .collect::<HashSet<_>>();
    let retained_claims = other_claims
        .iter()
        .filter(|claim| {
            managed_files.iter().any(|managed| {
                crate::paths::same_path(
                    Path::new(managed.path().as_str()),
                    Path::new(claim.path.as_str()),
                )
            })
        })
        .cloned()
        .collect::<Vec<_>>();

    let planned_peer_relocation =
        crate::addons::proxy_chain::planned_peer_host_relocation(&topology);
    let peer_host_transition = planned_peer_relocation
        .as_ref()
        .map(|relocation| {
            let restores_root = crate::paths::same_path(
                Path::new(relocation.to.as_str()),
                Path::new(topology.root_slot.as_str()),
            );
            let allowed_host = restores_root.then_some(topology.outer.receipt.digest());
            adoption::plan_peer_host_transition(
                adoption::PeerTopologyDirection::OutOfOptiTopology,
                context,
                game_id,
                &relocation.from,
                &relocation.to,
                allowed_host,
            )
        })
        .transpose()?
        .flatten();

    let plan =
        plan::build_uninstall_plan(&state, &topology, peer_host_transition, &other_claim_paths)?;
    Ok(PreparedUninstall {
        state,
        topology,
        plan,
        retained_claims,
    })
}

pub(super) fn footprint(plan: &plan::UninstallFsPlan) -> OptiScalerManagedCleanupFootprint {
    let mut exact_mutations = plan
        .precommit
        .iter()
        .flat_map(UninstallStep::mutation_paths)
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    exact_mutations.sort_by(|left, right| {
        crate::paths::normalized_key(left).cmp(&crate::paths::normalized_key(right))
    });
    exact_mutations.dedup_by(|left, right| crate::paths::same_path(left, right));
    let mut removed_directories = plan
        .postcommit_directories
        .iter()
        .map(|receipt| PathBuf::from(receipt.path.as_str()))
        .collect::<Vec<_>>();
    removed_directories.sort_by(|left, right| {
        crate::paths::normalized_key(left).cmp(&crate::paths::normalized_key(right))
    });
    removed_directories.dedup_by(|left, right| crate::paths::same_path(left, right));
    OptiScalerManagedCleanupFootprint {
        exact_mutations,
        removed_directories,
    }
}

pub(crate) fn preflight_managed_cleanup_uninstall_locked(
    context: &Context,
    game_id: &GameId,
    guard: &crate::game_mutation_lock::GameMutationGuard,
) -> Result<OptiScalerManagedCleanupFootprint, ServiceError> {
    if guard.game_id() != game_id {
        return Err(ServiceError::invalid_input(
            "OptiScaler cleanup guard does not match the requested game",
        ));
    }
    let prepared = prepare_uninstall_locked(context, game_id)?;
    Ok(footprint(&prepared.plan))
}

impl UninstallStep {
    fn execute(
        &self,
        mutation: &mut crate::file_mutation::optiscaler::PreparedFileMutation<'_>,
        changed: &mut Vec<String>,
        preserved: &mut Vec<String>,
        auxiliary: &mut Vec<renderpilot_storage_sqlite::OptiScalerAuxiliaryPreservation>,
    ) -> Result<(), ServiceError> {
        match self {
            Self::CreateDirectory { path } => {
                mutation.create_directory(path)?;
            }
            Self::PreserveConfiguration { plan } => {
                let recovery = plan.execute(mutation, false)?;
                changed.push(recovery.destination.to_string_lossy().into_owned());
                preserved.push(recovery.destination.to_string_lossy().into_owned());
                auxiliary.push(
                    renderpilot_storage_sqlite::OptiScalerAuxiliaryPreservation {
                        source: path_ref(&plan.source)?,
                        destination: path_ref(&recovery.destination)?,
                        source_current: plan.owned_source_receipt().clone(),
                        destination_receipt: recovery.destination_receipt,
                    },
                );
            }
            Self::RestoreConfiguration {
                path,
                receipt: _,
                bytes,
            } => {
                mutation.write_file(path, bytes)?;
                changed.push(path.to_string_lossy().into_owned());
            }
            Self::DeleteOwned { path, receipt }
            | Self::DeleteReusedArtifact { path, receipt }
            | Self::DeleteTopologyOuter { path, receipt } => {
                mutation.delete_file_exact(path, receipt)?;
                changed.push(path.to_string_lossy().into_owned());
            }
            Self::VerifyNoMutation { path, .. } => {
                mutation.verify_unchanged(path)?;
            }
            Self::RelocatePeer {
                source,
                destination,
                receipt,
            }
            | Self::RelocatePeerSidecar {
                source,
                destination,
                receipt,
            } => {
                mutation.relocate_file_exact(source, destination, receipt, receipt.ownership())?;
                changed.push(source.to_string_lossy().into_owned());
                changed.push(destination.to_string_lossy().into_owned());
            }
            Self::RestoreRetainedFsrOriginal {
                original_backup,
                target,
                original,
            } => {
                mutation.relocate_file_exact(
                    original_backup,
                    target,
                    original,
                    FileOwnership::Reused,
                )?;
                changed.push(original_backup.to_string_lossy().into_owned());
                changed.push(target.to_string_lossy().into_owned());
            }
        }
        Ok(())
    }
}

pub(super) fn uninstall_locked(
    context: &Context,
    game_id: &GameId,
    guard: &crate::game_mutation_lock::GameMutationGuard,
) -> Result<OptiScalerOperationResult, ServiceError> {
    let prepared = prepare_uninstall_locked(context, game_id)?;
    let PreparedUninstall {
        state,
        topology,
        plan,
        retained_claims,
    } = prepared;
    let scope = plan::uninstall_scope(&state, &topology, &plan)?;
    let operations = plan.operations();
    let peer = plan.peer_transition.as_ref().map(|transition| {
        renderpilot_storage_sqlite::OptiScalerPeerMutation::Replace {
            before: &transition.receipt.before,
            after: &transition.receipt.after,
        }
    });
    crate::file_mutation::optiscaler::run_optiscaler_mutation(
        &crate::file_mutation::optiscaler::OptiScalerMutation {
            context,
            guard,
            scope: &scope,
            feature: renderpilot_domain::mutation_features::OPTISCALER_UNINSTALL,
            subject_id: Some(topology.id.as_str()),
            operations,
            managed_endpoint_roots: super::super::managed_state_endpoint_roots(&state),
            threat_model: crate::file_mutation::optiscaler::ThreatModel::CooperativeSameUid,
        },
        |mutation| {
            let mut changed = Vec::new();
            let mut preserved = plan
                .preserved_paths
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            let mut auxiliary_preservations = Vec::new();
            for step in &plan.precommit {
                step.execute(
                    mutation,
                    &mut changed,
                    &mut preserved,
                    &mut auxiliary_preservations,
                )?;
            }
            Ok::<_, ServiceError>(UninstallExecution {
                operation: OptiScalerOperationResult {
                    kind: AddonKind::OptiScaler,
                    state: None,
                    changed_paths: changed,
                    preserved_paths: preserved,
                    config_conflicts: Vec::new(),
                },
                auxiliary_preservations,
            })
        },
        |prepared_mutation, execution| {
            let peer_mutation = peer.unwrap_or_default();
            let peer_before = match peer_mutation {
                renderpilot_storage_sqlite::OptiScalerPeerMutation::Keep => context
                    .storage()
                    .get_installed_addon(game_id)
                    .map_err(ServiceError::from)?,
                renderpilot_storage_sqlite::OptiScalerPeerMutation::Replace { before, .. } => {
                    Some(before.clone())
                }
            };
            let before = renderpilot_storage_sqlite::AggregateBefore::new(
                game_id.clone(),
                Some(state.clone()),
                Some(topology.clone()),
                peer_before,
            )
            .map_err(ServiceError::from)?;
            let mutation_id = prepared_mutation.id().to_owned();
            let aggregate_mutation =
                renderpilot_storage_sqlite::OptiScalerAggregateMutation::FilesystemWithAuxiliary {
                    before_state: Some(&state),
                    after_state: None,
                    before_topology: Some(&topology),
                    after_topology: None,
                    peer: peer_mutation,
                    auxiliary_preservations: &execution.auxiliary_preservations,
                    retained_claims: &retained_claims,
                    mutation_id: &mutation_id,
                };
            prepared_mutation.commit_journal_aggregate(before, aggregate_mutation)
        },
        |_| {},
        || {},
    )
    .map(|execution| execution.operation)
}
