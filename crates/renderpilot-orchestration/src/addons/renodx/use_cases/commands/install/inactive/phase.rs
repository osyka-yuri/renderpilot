use std::path::{Path, PathBuf};

use renderpilot_domain::{
    AddonKind, Architecture, GameId, InstalledAddon, InstalledAddonHostKind, PathRef,
};

use crate::addons::game_analysis::{analyze_game, install_target_dir};
use crate::addons::install_guard;
use crate::addons::renodx::errors;
use crate::addons::renodx::matcher::{
    RenoDxResolution, ResolvedInstall, generic_file_install_plan, resolve_external_install,
};
use crate::addons::renodx::types::RenoDxManifest;
use crate::addons::reshade::host_policy;
use crate::addons::reshade::proxy::HostKind;
use crate::addons::reshade::types::{ReshadeChannel, ReshadeSourceCatalog};
use crate::{Context, ServiceError};

/// Owned install plan snapshot used across the unlocked network prepare window.
pub(super) struct CatalogInstallSnapshot {
    pub(super) plan: ResolvedInstall,
    pub(super) target_dir: PathBuf,
    pub(super) channel: ReshadeChannel,
    pub(super) writes_host: bool,
    pub(super) registered_exe_path: Option<PathBuf>,
}

pub(super) fn resolve_catalog_install_snapshot(
    context: &Context,
    manifest: &RenoDxManifest,
    game_id: &GameId,
    requested_channel: ReshadeChannel,
) -> Result<CatalogInstallSnapshot, ServiceError> {
    let game = crate::addons::renodx::game_context::require_game(context, game_id)?;
    let override_path = crate::addons::renodx::game_context::executable_override(context, game_id);
    let (analysis, resolution) = crate::addons::renodx::game_context::analyze_and_resolve(
        &game,
        manifest,
        override_path.as_deref(),
    );
    let target_dir = install_target_dir(&analysis)?;
    let roots = install_guard::resolve_install_scan_roots(&analysis)?;
    install_guard::guard_exclusivity_and_torn(context, game_id, AddonKind::RenoDx, &roots)?;

    let plan: ResolvedInstall = match resolution {
        RenoDxResolution::Installable(plan) => *plan,
        RenoDxResolution::External { .. } => {
            return Err(errors::invalid(
                "RenoDX for this game is distributed externally; install it manually".to_owned(),
            ));
        }
        RenoDxResolution::NativeHdr => {
            return Err(errors::invalid(
                "this game has native HDR; RenoDX is not needed".to_owned(),
            ));
        }
        RenoDxResolution::UnsupportedSettings => {
            return Err(errors::unsupported_settings());
        }
        RenoDxResolution::Incompatible { reason } => {
            return Err(errors::invalid(format!(
                "RenoDX is not compatible with this game: {reason:?}"
            )));
        }
        RenoDxResolution::Blacklisted { message } => {
            return Err(errors::invalid(format!(
                "RenoDX is not supported for this game: {}",
                message.fallback_text
            )));
        }
        RenoDxResolution::NoMatch => {
            return Err(errors::invalid(
                "RenoDX has no profile for this game".to_owned(),
            ));
        }
    };

    let writes_host = resolve_writes_host(&plan, &target_dir)?;
    let registered_exe_path = analysis
        .primary_executable
        .as_ref()
        .map(|e| PathBuf::from(e.as_str()));
    Ok(CatalogInstallSnapshot {
        plan,
        target_dir,
        channel: requested_channel,
        writes_host,
        registered_exe_path,
    })
}

pub(super) fn resolve_file_install_snapshot(
    context: &Context,
    manifest: &RenoDxManifest,
    game_id: &GameId,
    requested_channel: ReshadeChannel,
    file_arch: Architecture,
) -> Result<CatalogInstallSnapshot, ServiceError> {
    let game = crate::addons::renodx::game_context::require_game(context, game_id)?;
    let analysis = analyze_game(
        &game,
        crate::addons::renodx::game_context::executable_override(context, game_id).as_deref(),
    );
    let target_dir = install_target_dir(&analysis)?;
    let roots = install_guard::resolve_install_scan_roots(&analysis)?;
    install_guard::guard_exclusivity_and_torn(context, game_id, AddonKind::RenoDx, &roots)?;

    if let Some(game_arch) = analysis.facts.graphics.architecture()
        && game_arch != file_arch
    {
        return Err(errors::invalid(format!(
            "this add-on is {} but the game is {} — download the matching add-on",
            super::local_file::arch_label(file_arch),
            super::local_file::arch_label(game_arch),
        )));
    }

    if crate::addons::renodx::matcher::has_unsupported_settings(manifest, &analysis.facts) {
        return Err(errors::unsupported_settings());
    }

    let plan = resolve_external_install(manifest, &analysis.facts)
        .or_else(|| generic_file_install_plan(&analysis.facts, file_arch))
        .ok_or_else(|| {
            errors::invalid(
                "RenoDX cannot be installed for this game: its renderer is not Direct3D".to_owned(),
            )
        })?;
    super::local_file::ensure_addon_arch(file_arch, plan.arch)?;

    let writes_host = resolve_writes_host(&plan, &target_dir)?;
    let registered_exe_path = analysis
        .primary_executable
        .as_ref()
        .map(|e| PathBuf::from(e.as_str()));
    Ok(CatalogInstallSnapshot {
        plan,
        target_dir,
        channel: requested_channel,
        writes_host,
        registered_exe_path,
    })
}

fn resolve_writes_host(plan: &ResolvedInstall, target_dir: &Path) -> Result<bool, ServiceError> {
    // DirectX host policy is irrelevant for Vulkan — the host is a shared layer.
    if matches!(plan.host_kind, HostKind::Vulkan) {
        return Ok(false);
    }
    let host = host_policy::assess(target_dir, &plan.proxy_dll_name);
    host.ensure_initial_installable(&plan.proxy_dll_name)?;
    Ok(host.initial_writes_host())
}

pub(super) fn ensure_catalog_install_snapshot_matches(
    snapshot: &CatalogInstallSnapshot,
    current: &CatalogInstallSnapshot,
) -> Result<(), ServiceError> {
    use crate::paths::same_path;

    if snapshot.plan.slug != current.plan.slug
        || snapshot.plan.addon_url != current.plan.addon_url
        || snapshot.plan.arch != current.plan.arch
        || snapshot.plan.host_kind != current.plan.host_kind
        || snapshot.plan.proxy_dll_name != current.plan.proxy_dll_name
        || snapshot.plan.processing_path != current.plan.processing_path
        || snapshot.plan.profile_id != current.plan.profile_id
        || snapshot.plan.renodx_config != current.plan.renodx_config
        || snapshot.channel != current.channel
        || snapshot.writes_host != current.writes_host
        || !same_path(&snapshot.target_dir, &current.target_dir)
    {
        return Err(errors::state_changed_retry_install());
    }
    match (
        snapshot.registered_exe_path.as_deref(),
        current.registered_exe_path.as_deref(),
    ) {
        (None, None) => Ok(()),
        (Some(a), Some(b)) if same_path(a, b) => Ok(()),
        _ => Err(errors::state_changed_retry_install()),
    }
}

pub(super) fn annotate_install_record(
    record: InstalledAddon,
    host_kind: HostKind,
    channel: ReshadeChannel,
    registered_exe_path: Option<&Path>,
) -> Result<InstalledAddon, ServiceError> {
    let mut record = record
        .with_host_kind(match host_kind {
            HostKind::Proxy => InstalledAddonHostKind::Proxy,
            HostKind::Vulkan => InstalledAddonHostKind::SharedVulkanLayer,
        })
        .with_reshade_channel(channel.as_str());

    if matches!(host_kind, HostKind::Vulkan) {
        let exe_path = registered_exe_path.ok_or_else(|| {
            errors::invalid(
                "cannot record Vulkan install metadata without a registered executable".to_owned(),
            )
        })?;
        record = record.with_registered_exe_path(path_ref(exe_path)?);
    }

    Ok(record)
}

fn path_ref(path: &Path) -> Result<PathRef, ServiceError> {
    PathRef::new(path.to_string_lossy().into_owned())
        .map_err(|error| errors::failed(format!("invalid install metadata path: {error}")))
}

pub(super) fn ensure_requested_channel(
    reshade_sources: &ReshadeSourceCatalog,
    requested_channel: ReshadeChannel,
) -> Result<(), ServiceError> {
    if reshade_sources.supports_channel(requested_channel) {
        Ok(())
    } else {
        Err(errors::channel_unavailable(requested_channel))
    }
}
