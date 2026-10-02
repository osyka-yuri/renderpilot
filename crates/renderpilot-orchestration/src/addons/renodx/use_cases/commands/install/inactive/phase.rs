use std::path::{Path, PathBuf};

use renderpilot_domain::{
    AddonKind, Architecture, GameId, InstalledAddon, InstalledAddonHostKind, PathRef,
};

use crate::addons::external_proxy_owner::prepared_release::PreparedExternalOwnerRelease;
use crate::addons::external_proxy_owner::{self, InactiveExternalProxyOwner};
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
    pub(super) install_root: PathBuf,
    pub(super) target_dir: PathBuf,
    pub(super) channel: ReshadeChannel,
    pub(super) writes_host: bool,
    pub(super) registered_exe_path: Option<PathBuf>,
    pub(super) previous_owner: Option<ExistingInstallOwner>,
    pub(super) external_owner: Option<InactiveExternalProxyOwner>,
    pub(super) receipt_release_paths: Vec<PathBuf>,
    pub(super) receipt_release_roots: Vec<PathBuf>,
}

/// The same-kind persisted external-host owner captured before the unlocked
/// prepare window. It is compared again under the final game/shared locks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ExistingInstallOwner {
    pub(super) host_kind: Option<InstalledAddonHostKind>,
    pub(super) registered_exe_path: Option<PathBuf>,
}

impl ExistingInstallOwner {
    pub(super) fn shared_vulkan_exe_path(&self) -> Option<&Path> {
        if self.host_kind == Some(InstalledAddonHostKind::SharedVulkanLayer) {
            self.registered_exe_path.as_deref()
        } else {
            None
        }
    }
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
    let install_root = PathBuf::from(game.install_path().as_str());
    let target_dir = install_target_dir(&analysis)?;
    let roots = install_guard::resolve_install_scan_roots(&analysis)?;
    let external_owner = external_proxy_owner::resolve_inactive_external_proxy_owner(
        context,
        game_id,
        &install_root,
    )?;
    let (receipt_release_paths, receipt_release_roots) =
        external_owner_release_scope(context, game_id, external_owner.as_ref())?;
    let guard_owner = external_owner.as_ref();
    install_guard::guard_exclusivity_and_torn_with_external_owner(
        context,
        game_id,
        AddonKind::RenoDx,
        &roots,
        guard_owner,
    )?;

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
    let previous_owner = resolve_existing_owner(context, game_id)?;
    ensure_same_kind_owner_admissible(
        context,
        game_id,
        previous_owner.as_ref(),
        external_owner.as_ref(),
    )?;
    Ok(CatalogInstallSnapshot {
        plan,
        install_root,
        target_dir,
        channel: requested_channel,
        writes_host,
        registered_exe_path,
        previous_owner,
        external_owner,
        receipt_release_paths,
        receipt_release_roots,
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
    let install_root = PathBuf::from(game.install_path().as_str());
    let target_dir = install_target_dir(&analysis)?;
    let roots = install_guard::resolve_install_scan_roots(&analysis)?;
    let external_owner = external_proxy_owner::resolve_inactive_external_proxy_owner(
        context,
        game_id,
        &install_root,
    )?;
    let (receipt_release_paths, receipt_release_roots) =
        external_owner_release_scope(context, game_id, external_owner.as_ref())?;
    let guard_owner = external_owner.as_ref();
    install_guard::guard_exclusivity_and_torn_with_external_owner(
        context,
        game_id,
        AddonKind::RenoDx,
        &roots,
        guard_owner,
    )?;

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
    let previous_owner = resolve_existing_owner(context, game_id)?;
    ensure_same_kind_owner_admissible(
        context,
        game_id,
        previous_owner.as_ref(),
        external_owner.as_ref(),
    )?;
    Ok(CatalogInstallSnapshot {
        plan,
        install_root,
        target_dir,
        channel: requested_channel,
        writes_host,
        registered_exe_path,
        previous_owner,
        external_owner,
        receipt_release_paths,
        receipt_release_roots,
    })
}

fn external_owner_release_scope(
    context: &Context,
    game_id: &GameId,
    owner: Option<&InactiveExternalProxyOwner>,
) -> Result<(Vec<PathBuf>, Vec<PathBuf>), ServiceError> {
    let Some(owner) = owner else {
        return Ok((Vec::new(), Vec::new()));
    };
    let release = PreparedExternalOwnerRelease::prepare(context, game_id, Some(owner))?
        .ok_or_else(|| ServiceError::command_failed("external owner release was not prepared"))?;
    Ok((release.affected_paths(), release.roots(&owner.record)))
}

fn ensure_same_kind_owner_admissible(
    context: &Context,
    game_id: &GameId,
    previous_owner: Option<&ExistingInstallOwner>,
    external_owner: Option<&InactiveExternalProxyOwner>,
) -> Result<(), ServiceError> {
    let Some(previous_owner) = previous_owner else {
        return Ok(());
    };
    if previous_owner.host_kind == Some(InstalledAddonHostKind::SharedVulkanLayer) {
        return Ok(());
    }
    if external_owner.is_some_and(|owner| owner.record.kind() == AddonKind::RenoDx) {
        return Ok(());
    }
    if let Some(record) =
        crate::addons::records::record_of_kind(context, game_id, AddonKind::RenoDx)?
        && crate::addons::tool::record_is_active_for_game(context, &record)?
    {
        // RenoDX's existing active reinstall path remains unchanged.
        return Ok(());
    }
    Err(errors::invalid(
        "RenoDX already has a persisted owner for this game; the inactive external receipt could not be safely replaced",
    ))
}

fn resolve_existing_owner(
    context: &Context,
    game_id: &GameId,
) -> Result<Option<ExistingInstallOwner>, ServiceError> {
    let Some(record) = crate::addons::records::record_of_kind(context, game_id, AddonKind::RenoDx)?
    else {
        return Ok(None);
    };
    let host_kind = record.host_kind();
    let registered_exe_path = record
        .registered_exe_path()
        .map(|path| PathBuf::from(path.as_str()));
    match (host_kind, registered_exe_path.as_ref()) {
        (Some(InstalledAddonHostKind::SharedVulkanLayer), None) => {
            return Err(errors::invalid(
                "the existing Shared Vulkan RenoDX owner has no registered executable".to_owned(),
            ));
        }
        (Some(InstalledAddonHostKind::Proxy), Some(_)) | (None, Some(_)) => {
            return Err(errors::invalid(
                "the existing RenoDX host metadata has an executable binding without a Shared Vulkan host".to_owned(),
            ));
        }
        _ => {}
    }
    Ok(Some(ExistingInstallOwner {
        host_kind,
        registered_exe_path,
    }))
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
        || !same_previous_owner(
            snapshot.previous_owner.as_ref(),
            current.previous_owner.as_ref(),
        )
        || snapshot.external_owner != current.external_owner
        || !same_ordered_paths(
            &snapshot.receipt_release_paths,
            &current.receipt_release_paths,
        )
        || !same_ordered_paths(
            &snapshot.receipt_release_roots,
            &current.receipt_release_roots,
        )
        || !same_path(&snapshot.install_root, &current.install_root)
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

pub(super) fn same_ordered_paths(left: &[PathBuf], right: &[PathBuf]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| crate::paths::same_path(left, right))
}

fn same_previous_owner(
    snapshot: Option<&ExistingInstallOwner>,
    current: Option<&ExistingInstallOwner>,
) -> bool {
    use crate::paths::same_path;

    match (snapshot, current) {
        (None, None) => true,
        (Some(snapshot), Some(current)) if snapshot.host_kind == current.host_kind => {
            match (
                snapshot.registered_exe_path.as_deref(),
                current.registered_exe_path.as_deref(),
            ) {
                (None, None) => true,
                (Some(left), Some(right)) => same_path(left, right),
                _ => false,
            }
        }
        _ => false,
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
