//! Typed OptiScaler uninstall facade and receipt validation.

use super::*;

mod execution;
mod plan;

#[cfg(test)]
mod tests;

/// Safely removes owned paths and restores remaining proxy/file consumers.
pub async fn uninstall(
    context: &Context,
    game_id: &GameId,
) -> Result<OptiScalerOperationResult, ServiceError> {
    let guard =
        crate::mutation_boundary::enter_game_mutation_boundary_async(context, game_id).await?;
    let started = Instant::now();
    let result = execution::uninstall_locked(context, game_id, &guard);
    tracing::debug!(
        "OptiScaler uninstall for {} finished in {} ms",
        game_id.as_str(),
        started.elapsed().as_millis()
    );
    result
}

pub(super) fn validate_release_file_receipt(
    state: &OptiScalerInstallState,
    topology: &GameProxyTopology,
) -> Result<(), ServiceError> {
    if state.release_files.is_empty() {
        return Err(failed(
            "OptiScaler install receipt has no release-file hashes; run repair before uninstall",
        ));
    }
    if topology.game_id != state.game_id {
        return Err(failed(
            "OptiScaler state and proxy topology belong to different games",
        ));
    }
    if state.proxy_topology_id.as_deref() != Some(topology.id.as_str()) {
        return Err(failed(
            "OptiScaler state does not reference the persisted proxy topology",
        ));
    }
    let proxy_path = Path::new(topology.root_slot.as_str());
    if !crate::paths::same_path(proxy_path, Path::new(topology.outer.path.as_str()))
        || topology.outer.implementation != ProxyImplementation::OptiScaler
    {
        return Err(failed(
            "OptiScaler proxy topology does not describe the root proxy slot",
        ));
    }
    let target_dir = Path::new(state.target_dir.as_str());
    let mut receipt_paths = HashSet::new();
    let mut configuration_count = 0_usize;
    for receipt in &state.release_files {
        let path = Path::new(receipt.path.as_str());
        let key = crate::paths::normalized_key(path);
        if !receipt_paths.insert(key) {
            return Err(failed(format!(
                "OptiScaler install receipt contains a duplicate release path {}",
                receipt.path.as_str()
            )));
        }
        if crate::paths::same_path(path, proxy_path) {
            return Err(failed(
                "OptiScaler proxy identity must be stored only in the persisted proxy topology",
            ));
        }
        if !crate::paths::is_within(path, target_dir) {
            return Err(failed(format!(
                "OptiScaler release receipt escapes the install target: {}",
                receipt.path.as_str()
            )));
        }
        match receipt.role {
            OptiScalerFileRole::Configuration => {
                configuration_count += 1;
                let canonical = match (
                    state.configuration_baseline(),
                    receipt.installed.ownership(),
                    receipt.cleanup,
                ) {
                    (
                        renderpilot_domain::OptiScalerConfigurationBaseline::Absent,
                        FileOwnership::Owned,
                        OptiScalerFileCleanup::RemoveIfUnchanged,
                    ) => true,
                    (
                        renderpilot_domain::OptiScalerConfigurationBaseline::Present {
                            receipt: baseline,
                            ..
                        },
                        FileOwnership::Owned,
                        OptiScalerFileCleanup::PreserveCurrentThenRestoreBaseline,
                    ) => baseline.ownership() == FileOwnership::Reused,
                    (
                        renderpilot_domain::OptiScalerConfigurationBaseline::Present {
                            receipt: baseline,
                            ..
                        },
                        FileOwnership::Reused,
                        OptiScalerFileCleanup::PreserveUnchanged,
                    ) => baseline == &receipt.installed,
                    _ => false,
                };
                if !canonical {
                    return Err(failed(
                        "OptiScaler configuration receipt has no canonical lifecycle policy",
                    ));
                }
            }
            OptiScalerFileRole::Runtime => {
                if receipt.cleanup != OptiScalerFileCleanup::RemoveIfUnchanged {
                    return Err(failed(
                        "OptiScaler runtime receipt must use RemoveIfUnchanged",
                    ));
                }
            }
        }
    }
    if configuration_count != 1 {
        return Err(failed(
            "OptiScaler uninstall receipt must contain exactly one configuration file",
        ));
    }
    Ok(())
}

pub(crate) fn uninstall_locked(
    context: &Context,
    game_id: &GameId,
    guard: &crate::game_mutation_lock::GameMutationGuard,
) -> Result<OptiScalerOperationResult, ServiceError> {
    execution::uninstall_locked(context, game_id, guard)
}

pub(crate) use execution::OptiScalerManagedCleanupFootprint;

pub(crate) fn preflight_managed_cleanup_uninstall_locked(
    context: &Context,
    game_id: &GameId,
    guard: &crate::game_mutation_lock::GameMutationGuard,
) -> Result<OptiScalerManagedCleanupFootprint, ServiceError> {
    execution::preflight_managed_cleanup_uninstall_locked(context, game_id, guard)
}
