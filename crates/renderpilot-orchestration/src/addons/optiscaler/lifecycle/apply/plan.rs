use super::super::adoption;
use super::super::*;

mod assembly;
mod config;
mod guards;
mod models;
mod removed;
mod targets;
mod topology;

pub(super) use super::retained_fsr::{RetainedFsrEntryPointPlan, RetainedFsrOriginalAction};
pub(super) use models::{
    CleanupPlan, ConfigInputs, ConfigTargetMode, FilesystemApplyPlan, MutationJournal, PathLayout,
    RemovedPathPlan, RemovedPathRequest,
};

impl<'a> FilesystemApplyPlan<'a> {
    pub(super) fn build(prepared: &'a ApplyPlan<'_>) -> Result<Self, ServiceError> {
        let ApplyPlan {
            game_id,
            old_state,
            release,
            artifacts,
            target,
            ..
        } = prepared;
        let targets = targets::plan_targets(prepared)?;
        let topology = topology::plan_topology(prepared, &targets)?;
        let config = config::load_config_inputs(
            release,
            &artifacts.archive,
            old_state.as_ref(),
            &target.dir,
        )?;
        let rewrite_guards = guards::plan_rewrite_guards(
            &targets,
            &topology,
            &config,
            topology.existing_topology.as_ref(),
        )?;
        let removed = removed::plan_removed_paths(RemovedPathRequest {
            game_id,
            old_state: old_state.as_ref(),
            old_managed_files: &prepared.old_managed_files,
            old_proxy_path: &topology.old_proxy_path,
            new_proxy_path: &targets.proxy_path,
            target_dir: &target.dir,
            removed_paths: &topology.removed_old_paths,
            other_claim_paths: topology.other_claim_paths.clone(),
            expected_hashes: &topology.old_expected,
        })?;
        assembly::assemble(prepared, targets, topology, config, rewrite_guards, removed)
    }
}

pub(in crate::addons::optiscaler) fn plan_apply_peer_transition(
    context: &Context,
    game_id: &GameId,
    old_state: Option<&OptiScalerInstallState>,
    existing_topology: Option<&GameProxyTopology>,
    proxy: &EvaluatedProxyPlan,
) -> Result<Option<adoption::PeerHostTransitionPlan>, ServiceError> {
    let transition = match (
        existing_topology
            .and_then(|topology| topology.downstream.as_ref())
            .map(|link| PathBuf::from(link.path.as_str())),
        proxy.downstream_path.clone(),
    ) {
        (Some(from), Some(to)) if !crate::paths::same_path(&from, &to) => Some((from, to, None)),
        (None, Some(to)) if old_state.is_none() && proxy.chain_reshade => {
            let from = proxy
                .reshade_source_path
                .clone()
                .unwrap_or_else(|| proxy.slot.clone());
            Some((from, to, None))
        }
        (Some(_), None) => {
            return Err(failed(
                "the active ReShade downstream disappeared while OptiScaler was being prepared",
            ));
        }
        _ => None,
    };
    let Some((from, to, allowed_destination)) = transition else {
        return Ok(None);
    };
    let from = path_ref(&from)?;
    let to = path_ref(&to)?;
    adoption::plan_peer_host_transition(
        adoption::PeerTopologyDirection::IntoOptiTopology,
        context,
        game_id,
        &from,
        &to,
        allowed_destination,
    )
}
