//! Live/sidecar inspection helpers for Luma DLSS planning.

use std::fs;
use std::io;
use std::path::Path;

use renderpilot_domain::{ManagedAddonFile, ManagedFileBaseline, Sha256Hash};

use crate::ServiceError;
use crate::addons::luma::errors;
use crate::coordinated_files::CatalogPathClaim;

pub(super) fn inspect_live_dlss(
    target: &Path,
) -> Result<Option<(renderpilot_detection::DlssBinaryInfo, Sha256Hash)>, ServiceError> {
    match fs::metadata(target) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(errors::invalid(format!(
            "cannot inspect existing nvngx_dlss.dll: {error}"
        ))),
        Ok(metadata) if !metadata.is_file() || metadata.len() == 0 => {
            Err(errors::invalid(format!(
                "cannot use {} because it is not a readable non-empty file",
                target.display()
            )))
        }
        Ok(_) => {
            let info =
                renderpilot_detection::DlssBinaryInfo::from_path(target).map_err(|error| {
                    errors::invalid(format!(
                        "existing nvngx_dlss.dll cannot be verified safely: {error}"
                    ))
                })?;
            let hash = renderpilot_detection::sha256_file(target)?;
            Ok(Some((info, hash)))
        }
    }
}

pub(super) fn validate_active_live(
    target: &Path,
    existing: Option<&ManagedAddonFile>,
    claim: &CatalogPathClaim,
    live_hash: &Sha256Hash,
) -> Result<(), ServiceError> {
    let managed_matches = existing.is_some_and(|binding| binding.installed_sha256() == live_hash);
    let catalog_matches = claim.active_hashes().contains(live_hash);
    if existing.is_some() || !claim.active_hashes().is_empty() {
        if managed_matches || catalog_matches {
            Ok(())
        } else {
            Err(errors::failed(format!(
                "nvngx_dlss.dll was replaced outside RenderPilot at {}; repair is required",
                target.display()
            )))
        }
    } else {
        Ok(())
    }
}

pub(super) fn baseline_for_new_owner(
    claim: &CatalogPathClaim,
    sidecar_hash: Option<&Sha256Hash>,
    live_hash: &Sha256Hash,
) -> Result<ManagedFileBaseline, ServiceError> {
    if let Some(baseline) = claim.baseline() {
        return Ok(baseline.clone());
    }
    Ok(match sidecar_hash {
        Some(sha256) => ManagedFileBaseline::Present {
            sha256: sha256.clone(),
        },
        None => ManagedFileBaseline::Present {
            sha256: live_hash.clone(),
        },
    })
}

pub(super) fn validated_owned_baseline(
    existing: &ManagedAddonFile,
    claim: &CatalogPathClaim,
    sidecar_hash: Option<&Sha256Hash>,
) -> Result<ManagedFileBaseline, ServiceError> {
    if let Some(catalog) = claim.baseline()
        && catalog != existing.baseline()
    {
        return Err(errors::failed(
            "catalog and add-on disagree about the nvngx_dlss.dll baseline".to_owned(),
        ));
    }
    match existing.baseline() {
        ManagedFileBaseline::Absent if sidecar_hash.is_some() => Err(errors::failed(
            "owned nvngx_dlss.dll records an absent baseline but a sidecar exists".to_owned(),
        )),
        ManagedFileBaseline::Absent => Ok(ManagedFileBaseline::Absent),
        ManagedFileBaseline::Present { sha256 } if sidecar_hash == Some(sha256) => {
            Ok(existing.baseline().clone())
        }
        ManagedFileBaseline::Present { .. } => Err(errors::failed(
            "owned nvngx_dlss.dll baseline is missing or has the wrong hash".to_owned(),
        )),
    }
}

pub(super) fn validate_claim_sidecar(
    target: &Path,
    claim: &CatalogPathClaim,
    sidecar_hash: Option<&Sha256Hash>,
) -> Result<(), ServiceError> {
    match (claim.baseline(), sidecar_hash) {
        (Some(ManagedFileBaseline::Absent), Some(_)) => Err(errors::failed(format!(
            "catalog records an absent baseline but {}.bak exists",
            target.display()
        ))),
        (Some(ManagedFileBaseline::Present { sha256 }), Some(actual)) if sha256 == actual => Ok(()),
        (Some(ManagedFileBaseline::Present { .. }), _) => Err(errors::failed(format!(
            "catalog baseline sidecar is missing or mismatched for {}",
            target.display()
        ))),
        _ => Ok(()),
    }
}

/// Optional classic-baseline hash: missing sidecar is `Ok(None)`.
pub(super) fn read_sidecar_hash(target: &Path) -> Result<Option<Sha256Hash>, ServiceError> {
    let sidecar =
        crate::fs::backup_path(target).map_err(|error| errors::invalid(error.to_string()))?;
    match fs::metadata(&sidecar) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(errors::failed(format!(
            "cannot inspect classic baseline {}: {error}",
            sidecar.display()
        ))),
        Ok(_) => crate::fs::sha256_of_non_empty_file(&sidecar)
            .map(Some)
            .map_err(|_error| {
                errors::failed(format!(
                    "classic baseline is not a readable non-empty file: {}",
                    sidecar.display()
                ))
            }),
    }
}
