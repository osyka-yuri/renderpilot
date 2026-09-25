//! DLSS and catalog-cascade projection for one active Luma update.
//!
//! The update path has a different authority model from the initial install:
//! a persisted `Owned` binding is authoritative for its live bytes, while a
//! `None`/`Reused` binding may rely on the catalog's active claim.  This
//! facade keeps that distinction explicit and delegates validation and effect
//! construction to focused modules.

mod classification;
mod evidence;
mod lowering;
mod model;
mod routes;
mod validation;

use renderpilot_domain::{InstalledAddon, PathRef};

use crate::addons::luma::peer::{
    active_update::{error::LumaActiveUpdateError, model::LumaActiveUpdateDlssInput},
    effects::LumaPeerEffectAccumulator,
    root_authority::LumaPeerRootAuthority,
};
use crate::catalog::cascade::CascadeResult;
use crate::coordinated_files::CatalogPathClaim;

use self::model::PersistedDlss;
use super::model::DlssProjection;

/// Projects the exact DLSS endpoint and, when requested, its catalog cascade.
///
/// All validation is completed before `accumulator` is changed.  In
/// particular, a persisted `Owned` claim is never routed through the initial
/// install classifier: its installed digest and baseline remain the authority
/// for update and release decisions.
pub(super) fn project_dlss(
    before: &InstalledAddon,
    authority: &LumaPeerRootAuthority,
    input: LumaActiveUpdateDlssInput,
    catalog_claim: &CatalogPathClaim,
    cascade: &CascadeResult,
    accumulator: &mut LumaPeerEffectAccumulator,
) -> Result<DlssProjection, LumaActiveUpdateError> {
    let target = validation::target(authority)?;
    let persisted = validation::persisted_binding(before, &target)?;
    validation::validate_cascade(
        before,
        &target,
        cascade,
        &input,
        matches!(&persisted, PersistedDlss::Owned(_)),
    )?;

    let result =
        classification::classify(authority, target, persisted, input, catalog_claim, cascade)?;
    lowering::lower(result, authority, cascade, accumulator)
}

/// Reuses the complete persisted-binding validation for the phase-three
/// cascade selector.  A positive result therefore means exactly one
/// effective-target binding exists and its mode is not `Reused`.
pub(crate) fn persisted_effective_binding_is_owned(
    before: &InstalledAddon,
    target: &PathRef,
) -> Result<bool, LumaActiveUpdateError> {
    Ok(matches!(
        validation::persisted_binding(before, target)?,
        PersistedDlss::Owned(_)
    ))
}

#[cfg(test)]
mod tests;
