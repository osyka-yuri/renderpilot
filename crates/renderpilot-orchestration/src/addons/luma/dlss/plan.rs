//! Install/update/release planning for Luma-managed `nvngx_dlss.dll`.

use std::path::{Path, PathBuf};

use renderpilot_domain::{
    GameId, ManagedAddonFile, ManagedFileBaseline, ManagedFileMode, Sha256Hash,
};

use super::binding::{NVNGX_DLSS_FILE_NAME, bundled_file, owned_binding, path_ref};
use super::inspect::{
    baseline_for_new_owner, inspect_live_dlss, read_sidecar_hash, validate_active_live,
    validate_claim_sidecar, validated_owned_baseline,
};
use crate::addons::luma::errors;
use crate::addons::luma::fetch::types::LumaPayloadFile;
use crate::coordinated_files::{
    CatalogPathClaim, CoordinatedFilePlan, ExpectedLive, OverlaySource, catalog_path_claim,
    execute_file_plan,
};
use crate::{Context, ServiceError};

/// A side-effect-free decision plus the exact binding to persist on success.
#[derive(Debug, Clone)]
pub(crate) struct PlannedDlss {
    pub(crate) action: CoordinatedFilePlan,
    pub(crate) binding: Option<ManagedAddonFile>,
}

impl PlannedDlss {
    fn none() -> Self {
        Self {
            action: CoordinatedFilePlan::Keep,
            binding: None,
        }
    }

    pub(crate) fn execute(&self) -> Result<(), ServiceError> {
        execute_file_plan(&self.action)
    }
}

pub(crate) fn plan_install(
    context: &Context,
    game_id: &GameId,
    addon_dir: &Path,
    payload: &[LumaPayloadFile],
) -> Result<PlannedDlss, ServiceError> {
    plan(context, game_id, addon_dir, payload, None, false)
}

pub(crate) fn plan_update(
    context: &Context,
    game_id: &GameId,
    addon_dir: &Path,
    payload: &[LumaPayloadFile],
    existing: Option<&ManagedAddonFile>,
    owned_already_unwound: bool,
) -> Result<PlannedDlss, ServiceError> {
    plan(
        context,
        game_id,
        addon_dir,
        payload,
        existing,
        owned_already_unwound,
    )
}

/// Plans release of an existing managed DLSS binding (uninstall, or update
/// payload that no longer ships DLSS). `owned_already_unwound` is true when a
/// catalog cascade already restored/removed the path.
pub(crate) fn plan_release_binding(
    context: &Context,
    game_id: &GameId,
    existing: &ManagedAddonFile,
    owned_already_unwound: bool,
) -> Result<PlannedDlss, ServiceError> {
    let target = PathBuf::from(existing.path().as_str());
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if !file_name.eq_ignore_ascii_case(NVNGX_DLSS_FILE_NAME) {
        return Err(errors::invalid(format!(
            "managed binding is not nvngx_dlss.dll: {}",
            existing.path().as_str()
        )));
    }
    let addon_dir = target.parent().ok_or_else(|| {
        errors::failed(format!(
            "managed file has no parent directory: {}",
            target.display()
        ))
    })?;
    plan(
        context,
        game_id,
        addon_dir,
        &[],
        Some(existing),
        owned_already_unwound,
    )
}

fn plan(
    context: &Context,
    game_id: &GameId,
    addon_dir: &Path,
    payload: &[LumaPayloadFile],
    existing: Option<&ManagedAddonFile>,
    owned_already_unwound: bool,
) -> Result<PlannedDlss, ServiceError> {
    let target = addon_dir.join(NVNGX_DLSS_FILE_NAME);
    if let Some(existing) = existing
        && !crate::paths::same_path(Path::new(existing.path().as_str()), &target)
    {
        return Err(errors::invalid(format!(
            "managed nvngx_dlss.dll binding points outside the active payload root: {}",
            existing.path().as_str()
        )));
    }

    if bundled_file(payload).is_none() && owned_already_unwound {
        return Ok(PlannedDlss::none());
    }

    let claim = catalog_path_claim(context.storage(), game_id, &target)?;
    let sidecar_hash = read_sidecar_hash(&target)?;
    validate_claim_sidecar(&target, &claim, sidecar_hash.as_ref())?;

    let Some(bundled) = bundled_file(payload) else {
        return plan_release(
            target,
            existing,
            owned_already_unwound,
            &claim,
            sidecar_hash.as_ref(),
        );
    };
    let bundled_info = renderpilot_detection::DlssBinaryInfo::from_bytes(&bundled.bytes)
        .map_err(|error| errors::invalid(format!("bundled nvngx_dlss.dll is invalid: {error}")))?;
    let bundled_hash = renderpilot_detection::sha256_bytes(&bundled.bytes)?;
    let live = inspect_live_dlss(&target)?;

    let live_ctx = LivePlanContext {
        target,
        existing,
        claim: &claim,
        sidecar_hash: sidecar_hash.as_ref(),
    };
    let bundled = BundledDlss {
        file: bundled,
        hash: &bundled_hash,
    };

    let Some((live_info, live_hash)) = live else {
        return plan_overlay_for_absent_live(context, live_ctx, bundled);
    };

    validate_active_live(
        &live_ctx.target,
        live_ctx.existing,
        live_ctx.claim,
        &live_hash,
    )?;
    if !renderpilot_domain::dlss::versions_are_compatible(
        live_info.version(),
        bundled_info.version(),
    ) {
        return Err(errors::invalid(format!(
            "nvngx_dlss.dll generation {} is incompatible with bundled generation {}",
            live_info.version().as_str(),
            bundled_info.version().as_str()
        )));
    }

    if live_info.version() >= bundled_info.version() {
        return plan_reuse_for_newer_or_equal_live(live_ctx, live_hash);
    }

    plan_replace_older_live(context, live_ctx, live_hash, bundled)
}

/// Shared claim/target state for DLSS install/update plan branches.
struct LivePlanContext<'a> {
    target: PathBuf,
    existing: Option<&'a ManagedAddonFile>,
    claim: &'a CatalogPathClaim,
    sidecar_hash: Option<&'a Sha256Hash>,
}

/// Bundled payload bytes already verified as a DLSS binary.
#[derive(Clone, Copy)]
struct BundledDlss<'a> {
    file: &'a LumaPayloadFile,
    hash: &'a Sha256Hash,
}

fn plan_overlay_for_absent_live(
    context: &Context,
    live: LivePlanContext<'_>,
    bundled: BundledDlss<'_>,
) -> Result<PlannedDlss, ServiceError> {
    if live.existing.is_some() || !live.claim.active_hashes().is_empty() {
        return Err(errors::failed(format!(
            "managed or catalog-owned nvngx_dlss.dll is missing at {}; repair is required",
            live.target.display()
        )));
    }
    let baseline = match live.sidecar_hash {
        Some(sha256) => ManagedFileBaseline::Present {
            sha256: sha256.clone(),
        },
        None => ManagedFileBaseline::Absent,
    };
    let binding = owned_binding(&live.target, baseline.clone(), bundled.hash.clone())?;
    let source = stage_overlay_source(context, &bundled.file.bytes, bundled.hash)?;
    Ok(PlannedDlss {
        action: CoordinatedFilePlan::OverlayPreservingBaseline {
            path: live.target,
            baseline,
            expected_live: ExpectedLive::Absent,
            source,
        },
        binding: Some(binding),
    })
}

fn plan_reuse_for_newer_or_equal_live(
    live: LivePlanContext<'_>,
    live_hash: Sha256Hash,
) -> Result<PlannedDlss, ServiceError> {
    let binding = match live.existing {
        Some(existing) if existing.mode() == ManagedFileMode::Owned => owned_binding(
            &live.target,
            validated_owned_baseline(existing, live.claim, live.sidecar_hash)?,
            live_hash.clone(),
        )?,
        _ => ManagedAddonFile::reused(path_ref(&live.target)?, live_hash.clone()),
    };
    Ok(PlannedDlss {
        action: CoordinatedFilePlan::Reuse {
            path: live.target,
            sha256: live_hash,
        },
        binding: Some(binding),
    })
}

fn plan_replace_older_live(
    context: &Context,
    live: LivePlanContext<'_>,
    live_hash: Sha256Hash,
    bundled: BundledDlss<'_>,
) -> Result<PlannedDlss, ServiceError> {
    let baseline = if let Some(existing) = live.existing
        && existing.mode() == ManagedFileMode::Owned
    {
        validated_owned_baseline(existing, live.claim, live.sidecar_hash)?
    } else {
        baseline_for_new_owner(live.claim, live.sidecar_hash, &live_hash)?
    };
    let binding = owned_binding(&live.target, baseline.clone(), bundled.hash.clone())?;
    let source = stage_overlay_source(context, &bundled.file.bytes, bundled.hash)?;
    let action =
        if matches!(baseline, ManagedFileBaseline::Present { .. }) && live.sidecar_hash.is_none() {
            CoordinatedFilePlan::CreateBaselineAndOverlay {
                path: live.target,
                expected_live: live_hash,
                source,
            }
        } else {
            CoordinatedFilePlan::OverlayPreservingBaseline {
                path: live.target,
                baseline,
                expected_live: ExpectedLive::Hashes(vec![live_hash]),
                source,
            }
        };
    Ok(PlannedDlss {
        action,
        binding: Some(binding),
    })
}

/// Stages DLSS overlay bytes under the durable mutation root (content-addressed)
/// so execute uses path-sourced copy instead of cloning DLL bytes into the plan.
fn stage_overlay_source(
    context: &Context,
    bytes: &[u8],
    hash: &Sha256Hash,
) -> Result<OverlaySource, ServiceError> {
    let dir = context.file_mutation_root().join("overlay-staging");
    std::fs::create_dir_all(&dir).map_err(|error| {
        errors::failed(format!(
            "failed to create overlay staging directory {}: {error}",
            dir.display()
        ))
    })?;
    let path = dir.join(hash.as_str());
    if !path.exists() {
        crate::fs::write_file_atomically(&path, bytes)?;
    }
    Ok(path)
}

fn plan_release(
    target: PathBuf,
    existing: Option<&ManagedAddonFile>,
    owned_already_unwound: bool,
    claim: &CatalogPathClaim,
    sidecar_hash: Option<&Sha256Hash>,
) -> Result<PlannedDlss, ServiceError> {
    let Some(existing) = existing else {
        return Ok(PlannedDlss::none());
    };
    if existing.mode() == ManagedFileMode::Reused || owned_already_unwound {
        return Ok(PlannedDlss::none());
    }
    let live_hash = renderpilot_detection::sha256_file(&target).map_err(|error| {
        errors::failed(format!(
            "owned nvngx_dlss.dll cannot be released safely at {}: {error}",
            target.display()
        ))
    })?;
    validate_active_live(&target, Some(existing), claim, &live_hash)?;
    let baseline = validated_owned_baseline(existing, claim, sidecar_hash)?;
    let mut expected_live = vec![existing.installed_sha256().clone()];
    for hash in claim.active_hashes() {
        if !expected_live.contains(hash) {
            expected_live.push(hash.clone());
        }
    }
    let action = match baseline {
        ManagedFileBaseline::Present { sha256 } => CoordinatedFilePlan::RestoreAndRelease {
            path: target,
            baseline_sha256: sha256,
            expected_live,
        },
        ManagedFileBaseline::Absent => CoordinatedFilePlan::RemoveAndRelease {
            path: target,
            expected_live,
        },
    };
    Ok(PlannedDlss {
        action,
        binding: None,
    })
}
