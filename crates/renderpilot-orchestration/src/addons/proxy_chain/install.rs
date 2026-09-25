use renderpilot_domain::{
    FileOwnership, GameProxyTopology, ProxyImplementation, ProxyLink, ProxyRootPrestate,
};

use crate::addons::optiscaler::identity::path_ref;
use crate::file_mutation::optiscaler::PreparedFileMutation;
use crate::{ServiceError, failed};

use super::model::{DownstreamInstallPlan, ProxyInstallPlan};
use super::publication::{exact_receipt_from_live, maybe_exact_receipt_from_live, publish_outer};

pub(super) fn execute_fresh_install(
    plan: &ProxyInstallPlan<'_>,
    outer_bytes: &[u8],
    mutation: &mut PreparedFileMutation<'_>,
    changed: &mut Vec<String>,
) -> Result<GameProxyTopology, ServiceError> {
    let mut root_prestate = ProxyRootPrestate::Absent;
    let mut downstream = None;
    let mut downstream_origin = None;
    if let Some(requested) = &plan.downstream {
        let DownstreamInstallPlan::Transfer {
            source_path,
            expected_source_sha256,
            destination_path,
            destination_ownership,
        } = requested
        else {
            return Err(failed(
                "an existing ReShade downstream requires an installed OptiScaler topology",
            ));
        };
        let source_receipt = exact_receipt_from_live(source_path, *destination_ownership)?;
        if source_receipt.digest() != *expected_source_sha256 {
            return Err(failed(
                "the ReShade chain source changed while OptiScaler was downloading; retry",
            ));
        }
        if crate::paths::same_path(source_path, plan.root_slot) {
            root_prestate = ProxyRootPrestate::RelocatedDownstream;
        }
        let destination_receipt = if crate::paths::same_path(source_path, destination_path) {
            source_receipt
        } else {
            if maybe_exact_receipt_from_live(destination_path, FileOwnership::Reused)?.is_some() {
                return Err(failed(
                    "ReShade64.dll appeared while OptiScaler was being prepared; retry",
                ));
            }
            let destination_receipt = mutation.relocate_file_exact(
                source_path,
                destination_path,
                &source_receipt,
                *destination_ownership,
            )?;
            changed.push(destination_path.to_string_lossy().into_owned());
            destination_receipt
        };
        // The origin is the exact return target for the peer after outer
        // removal. It is always the root slot; `root_prestate` separately
        // records whether that slot historically contained the peer before
        // this topology was created.
        downstream_origin = Some(path_ref(plan.root_slot)?);
        downstream = Some(ProxyLink {
            implementation: ProxyImplementation::ReShade,
            path: path_ref(destination_path)?,
            receipt: destination_receipt,
        });
    }

    mutation.create_planned_directories()?;
    let topology = GameProxyTopology {
        id: format!("optiscaler:{}", plan.game_id.as_str()),
        game_id: plan.game_id.clone(),
        root_slot: path_ref(plan.root_slot)?,
        outer: ProxyLink {
            implementation: ProxyImplementation::OptiScaler,
            path: path_ref(plan.root_slot)?,
            receipt: publish_outer(
                plan.root_slot,
                None,
                &plan.outer_sha256,
                outer_bytes,
                mutation,
                changed,
            )?,
        },
        downstream,
        downstream_origin,
        root_prestate,
    };
    topology
        .validate()
        .map_err(|error| failed(format!("OptiScaler proxy topology is invalid: {error}")))?;
    Ok(topology)
}
