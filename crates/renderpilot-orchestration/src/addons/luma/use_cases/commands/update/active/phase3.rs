//! Locked, local-only phase-three evidence for an active Luma update.
//!
//! Phase one is re-run under the recovered game guard before any authority or
//! catalog evidence is consumed.  This module owns only immutable evidence;
//! preparation and mutation remain in their respective later boundaries.

use std::path::{Path, PathBuf};

use renderpilot_domain::{AddonKind, PathRef};

use crate::Context;
use crate::ServiceError;
use crate::addons::exclusivity;
use crate::addons::luma::errors;
use crate::addons::luma::peer::{LumaActiveUpdatePrepared, LumaPeerRootAuthority};
use crate::addons::reshade::host_policy::{
    TopologyHostAssessment, assess_topology_downstream_from_snapshot,
};
use crate::catalog::cascade::{CascadeResult, cascade_for_managed_paths};
use crate::coordinated_files::{CatalogPathClaim, catalog_path_claim};
use crate::game_mutation_lock::GameMutationGuard;

use super::super::route::{UpdatePhase1, snapshot_update_route};
use super::model::ActiveUpdatePhase1;

/// Immutable phase-three evidence consumed by the active update composer.
#[derive(Debug)]
pub(crate) struct ActiveUpdatePhase3 {
    authority: LumaPeerRootAuthority,
    host_assessment: TopologyHostAssessment,
    catalog_claim: CatalogPathClaim,
    cascade: CascadeResult,
}

impl ActiveUpdatePhase3 {
    #[must_use]
    #[cfg(test)]
    pub(crate) fn authority(&self) -> &LumaPeerRootAuthority {
        &self.authority
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) fn host_assessment(&self) -> &TopologyHostAssessment {
        &self.host_assessment
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) fn catalog_claim(&self) -> &CatalogPathClaim {
        &self.catalog_claim
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) fn cascade(&self) -> &CascadeResult {
        &self.cascade
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        LumaPeerRootAuthority,
        TopologyHostAssessment,
        CatalogPathClaim,
        CascadeResult,
    ) {
        (
            self.authority,
            self.host_assessment,
            self.catalog_claim,
            self.cascade,
        )
    }
}

/// Revalidates phase one and captures all local evidence required by the
/// active update composer while the recovered per-game guard is held.
pub(crate) fn snapshot_active_update_phase3(
    context: &Context,
    manifest: &crate::addons::luma::types::LumaManifest,
    guard: &GameMutationGuard,
    phase1: &ActiveUpdatePhase1,
    prepared: &LumaActiveUpdatePrepared,
) -> Result<ActiveUpdatePhase3, ServiceError> {
    let current = snapshot_update_route(context, manifest, guard)?;
    require_unchanged_active(current, phase1)?;

    let authority = LumaPeerRootAuthority::resolve(
        phase1.canonical_game_root(),
        Path::new(phase1.downstream_path().as_str()),
    )?;
    authority.validate(Some(phase1.record()), None, None, &[])?;

    let sealed_roots = sealed_scan_roots(&authority);
    exclusivity::ensure_not_blocked(
        context,
        guard.game_id(),
        AddonKind::Luma,
        Some(sealed_roots.as_slice()),
    )?;

    let host_assessment = assess_topology_downstream_from_snapshot(
        phase1.canonical_game_root(),
        phase1.downstream_path(),
        phase1.downstream_snapshot(),
        authority.content(),
        "Luma",
        Some(phase1.minimum_reshade_version()),
    )?;

    let dlss_target = authority.effective_dlss_target()?;
    let catalog_claim = catalog_path_claim(
        context.storage(),
        guard.game_id(),
        Path::new(dlss_target.as_str()),
    )?;
    let owned_paths = cascade_owned_paths(phase1, prepared, &dlss_target)?;
    let cascade = cascade_for_managed_paths(context.storage(), guard.game_id(), &owned_paths)?;
    authority.validate(None, None, cascade.catalog_claim(), &[])?;

    Ok(ActiveUpdatePhase3 {
        authority,
        host_assessment,
        catalog_claim,
        cascade,
    })
}

fn require_unchanged_active(
    current: UpdatePhase1,
    expected: &ActiveUpdatePhase1,
) -> Result<ActiveUpdatePhase1, ServiceError> {
    let UpdatePhase1::Active(current) = current else {
        return Err(errors::state_changed_retry_update());
    };
    if current.as_ref() != expected {
        return Err(errors::state_changed_retry_update());
    }
    Ok(*current)
}

fn sealed_scan_roots(authority: &LumaPeerRootAuthority) -> Vec<&Path> {
    let mut roots = vec![authority.canonical_game_root()];
    if let Some(payload_root) = authority.external_capability_root() {
        roots.push(payload_root);
    }
    roots
}

fn cascade_owned_paths(
    phase1: &ActiveUpdatePhase1,
    prepared: &LumaActiveUpdatePrepared,
    target: &PathRef,
) -> Result<Vec<PathBuf>, ServiceError> {
    if !matches!(
        prepared.payload(),
        crate::addons::luma::peer::LumaActiveUpdatePayloadInput::Full(_)
    ) {
        return Ok(Vec::new());
    }
    if prepared.payload().contains_exact_dlss() {
        return Ok(Vec::new());
    }

    let owned =
        LumaActiveUpdatePrepared::persisted_effective_binding_is_owned(phase1.record(), target)?;
    if owned {
        Ok(vec![PathBuf::from(target.as_str())])
    } else {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests;
