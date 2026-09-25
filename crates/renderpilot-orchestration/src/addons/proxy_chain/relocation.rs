use std::path::Path;

use renderpilot_application::ProxyTopologyRepository;
use renderpilot_domain::{FileOwnership, GameProxyTopology};

use crate::addons::optiscaler::identity::path_ref;
use crate::file_mutation::optiscaler::PreparedFileMutation;
use crate::{Context, ServiceError, failed};

use super::install::execute_fresh_install;
use super::model::{DownstreamInstallPlan, ProxyInstallPlan};
use super::publication::{maybe_exact_receipt_from_live, publish_outer};

pub(super) fn execute_update(
    context: &Context,
    plan: &ProxyInstallPlan<'_>,
    outer_bytes: &[u8],
    mutation: &mut PreparedFileMutation<'_>,
    changed: &mut Vec<String>,
) -> Result<GameProxyTopology, ServiceError> {
    let mut topology = context
        .storage()
        .get_proxy_topology(plan.game_id)?
        .ok_or_else(|| failed("OptiScaler proxy topology is missing"))?;
    if crate::paths::same_path(Path::new(topology.root_slot.as_str()), plan.root_slot) {
        let downstream_unchanged = match (topology.downstream.as_ref(), plan.downstream.as_ref()) {
            (None, None) => true,
            (Some(existing), Some(requested)) => crate::paths::same_path(
                Path::new(existing.path.as_str()),
                requested.destination_path(),
            ),
            _ => false,
        };
        if !downstream_unchanged {
            return Err(failed(
                "OptiScaler proxy topology changed during the operation; retry",
            ));
        }
        mutation.create_planned_directories()?;
        let outer_receipt = publish_outer(
            plan.root_slot,
            Some(&topology.outer.receipt),
            &plan.outer_sha256,
            outer_bytes,
            mutation,
            changed,
        )?;
        topology.outer.receipt = outer_receipt;
        topology.outer.path = path_ref(plan.root_slot)?;
        return Ok(topology);
    }

    match (topology.downstream.as_ref(), plan.downstream.as_ref()) {
        (None, None) => release_for_relocation(&topology, mutation, changed)?,
        (Some(existing), Some(requested)) => {
            let DownstreamInstallPlan::Transfer {
                source_path,
                destination_path,
                destination_ownership,
                ..
            } = requested
            else {
                return Err(failed(
                    "OptiScaler proxy relocation requires the committed ReShade source",
                ));
            };
            let existing_path = Path::new(existing.path.as_str());
            if !crate::paths::same_path(existing_path, source_path) {
                return Err(failed(
                    "OptiScaler proxy topology and active ReShade source disagree; repair the chain before relocating",
                ));
            }
            if existing.receipt.ownership() != *destination_ownership {
                return Err(failed(
                    "OptiScaler proxy topology and peer receipt custody disagree; repair the chain before relocating",
                ));
            }
            if !crate::paths::same_path(existing_path, destination_path)
                && maybe_exact_receipt_from_live(destination_path, FileOwnership::Reused)?.is_some()
            {
                return Err(failed(format!(
                    "ReShade downstream destination {} is occupied",
                    destination_path.display()
                )));
            }
            if !crate::paths::same_path(existing_path, destination_path) {
                let source_receipt = existing.receipt.clone();
                mutation.relocate_file_exact(
                    existing_path,
                    destination_path,
                    &source_receipt,
                    source_receipt.ownership(),
                )?;
                changed.push(destination_path.to_string_lossy().into_owned());
            }
            // Release only the old outer slot. The active downstream was
            // transferred above and is represented by the new topology.
            release_for_relocation(&topology, mutation, changed)?;
        }
        _ => {
            return Err(failed(
                "OptiScaler proxy relocation would drop or invent an active ReShade chain",
            ));
        }
    }

    execute_fresh_install(plan, outer_bytes, mutation, changed)
}

/// Releases a topology root before installing the same outer proxy elsewhere.
pub(crate) fn release_for_relocation(
    topology: &GameProxyTopology,
    mutation: &mut PreparedFileMutation<'_>,
    changed: &mut Vec<String>,
) -> Result<(), ServiceError> {
    let root = Path::new(topology.root_slot.as_str());
    // Provenance does not reduce an exact-adopted OptiScaler outer's
    // lifecycle. `delete_file_exact` still requires the sealed receipt and
    // accepts Reused only through the OptiScaler-artifact journal authority;
    // arbitrary Reused paths never reach this topology release.
    mutation.delete_file_exact(root, &topology.outer.receipt)?;
    changed.push(root.to_string_lossy().into_owned());
    Ok(())
}
