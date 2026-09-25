use renderpilot_domain::managed_sidecar_path;

use super::model::DlssAction;
use crate::addons::luma::peer::{
    active_update::{error::LumaActiveUpdateError, model::DlssProjection},
    catalog_cascade::lower_catalog_cascade,
    effects::{LumaPeerEffectAccumulator, LumaPeerEffectGroup},
    root_authority::LumaPeerRootAuthority,
};
use crate::catalog::cascade::CascadeResult;

pub(super) fn lower(
    action: DlssAction,
    authority: &LumaPeerRootAuthority,
    cascade: &CascadeResult,
    accumulator: &mut LumaPeerEffectAccumulator,
) -> Result<DlssProjection, LumaActiveUpdateError> {
    let binding = match action {
        DlssAction::Noop { binding } => return Ok(DlssProjection::new(binding)),
        DlssAction::Create { binding, bytes } => {
            accumulator.create(
                LumaPeerEffectGroup::DlssCascade,
                binding.path().clone(),
                bytes,
            )?;
            binding
        }
        DlssAction::Acquire {
            binding,
            live,
            bytes,
        } => {
            let sidecar =
                managed_sidecar_path(binding.path()).map_err(LumaActiveUpdateError::domain)?;
            accumulator.acquire_foreign(
                LumaPeerEffectGroup::DlssCascade,
                binding.path().clone(),
                sidecar,
                &live.file,
                live.bytes,
                bytes,
            )?;
            binding
        }
        DlssAction::Replace {
            binding,
            live,
            bytes,
        } => {
            accumulator.replace(
                LumaPeerEffectGroup::DlssCascade,
                binding.path().clone(),
                &live.file,
                bytes,
            )?;
            binding
        }
        DlssAction::Release {
            target,
            live,
            sidecar,
            baseline,
        } => {
            if let Some(sidecar) = sidecar {
                let baseline = baseline.ok_or_else(|| {
                    LumaActiveUpdateError::invalid_input(
                        "present DLSS baseline has no retained bytes",
                    )
                })?;
                accumulator.release_present(
                    LumaPeerEffectGroup::DlssCascade,
                    target.clone(),
                    managed_sidecar_path(&target).map_err(LumaActiveUpdateError::domain)?,
                    &live.file,
                    &sidecar.file,
                    baseline,
                )?;
            } else {
                accumulator.remove(LumaPeerEffectGroup::DlssCascade, target, &live.file)?;
            }
            return Ok(DlssProjection::new(None));
        }
        DlssAction::CascadeRelease => {
            lower_catalog_cascade(&cascade.rollback_specs, authority, accumulator)?;
            return Ok(DlssProjection::new(None));
        }
    };
    Ok(DlssProjection::new(Some(binding)))
}
