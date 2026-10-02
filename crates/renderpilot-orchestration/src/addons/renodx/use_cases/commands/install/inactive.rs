//! RenoDX installation orchestration for games without an active peer topology.

mod commit;
mod local_file;
mod phase;

#[cfg(test)]
mod tests;

use std::path::Path;

use renderpilot_domain::{AddonKind, InstalledAddon};

use self::commit::{
    CombinedRenoDxInstallRequest, authorize_combined_install, authorize_install_commit,
};
use self::local_file::read_addon_file;
use self::phase::{
    ensure_catalog_install_snapshot_matches, ensure_requested_channel,
    resolve_catalog_install_snapshot, resolve_file_install_snapshot,
};
use crate::ServiceError;
use crate::addons::external_proxy_owner::augment_external_owner_targets;
use crate::addons::external_proxy_owner::prepared_release::PreparedExternalOwnerRelease;
use crate::addons::progress::emit_tool_finalizing;
use crate::addons::renodx::fetch::{LocalAddonSource, prepare_install, prepare_install_from_file};
use crate::addons::renodx::use_cases::commands::install::InstallRequest;

/// Executes the established inactive catalogue install flow.
pub(super) async fn install(request: InstallRequest<'_>) -> Result<InstalledAddon, ServiceError> {
    let InstallRequest {
        context,
        manifest,
        reshade_sources,
        game_id,
        requested_channel,
        safety,
        allow_shared_vulkan_layer_install,
        progress,
    } = request;
    ensure_requested_channel(reshade_sources, requested_channel)?;

    // Phase 1: snapshot under the per-game lock (fail-fast gates + plan).
    let snapshot = {
        let _guard =
            crate::mutation_boundary::enter_game_mutation_boundary_async(context, game_id).await?;
        resolve_catalog_install_snapshot(context, manifest, game_id, requested_channel)?
    };

    // Phase 2: downloads only — no game-folder mutation.
    let prepared = prepare_install(
        &snapshot.plan,
        reshade_sources,
        game_id.clone(),
        snapshot.channel,
        snapshot.writes_host,
        progress,
    )
    .await?;

    let shared_change =
        crate::addons::renodx::use_cases::commands::shared_vulkan_layer::prepare_for_inactive_install(
            crate::addons::renodx::use_cases::commands::shared_vulkan_layer::PrepareInstallRequest {
                plan: &snapshot.plan,
                reshade_config: reshade_sources,
                channel: snapshot.channel,
                allow_shared_vulkan_layer_install,
                exe_path: snapshot.registered_exe_path.as_deref(),
                progress,
            },
            snapshot
                .previous_owner
                .as_ref()
                .and_then(|owner| owner.shared_vulkan_exe_path()),
        )
        .await?;

    if snapshot.external_owner.as_ref().is_some_and(|owner| {
        shared_change.mutates_shared_resource()
            && (!matches!(owner.record.kind(), AddonKind::RenoDx | AddonKind::Luma)
                || owner.record.host_kind()
                    != Some(renderpilot_domain::InstalledAddonHostKind::Proxy)
                || snapshot.plan.host_kind != crate::addons::reshade::proxy::HostKind::Vulkan)
    }) {
        return Err(crate::ServiceError::invalid_input(
            "shared owner replacement requires an explicit inactive Proxy RenoDX or Luma owner and a Vulkan target",
        ));
    }

    // Phase 3: acquire the final boundary, revalidate, then begin one
    // synchronous shared+game commit with no safety checks after first write.
    let guards = crate::mutation_boundary::enter_mutation_boundary_async(
        context,
        game_id,
        shared_change.mutates_shared_resource(),
    )
    .await?;
    let revalidated =
        resolve_catalog_install_snapshot(context, manifest, game_id, requested_channel)?;
    ensure_catalog_install_snapshot_matches(&snapshot, &revalidated)?;

    emit_tool_finalizing(progress, AddonKind::RenoDx);
    let mut targets = crate::addons::renodx::mutation_targets::install_targets(
        &revalidated.target_dir,
        &prepared,
    )?;
    let receipt_release = prepare_revalidated_owner_release(context, game_id, &revalidated)?;
    if let (Some(owner), Some(release)) = (
        revalidated.external_owner.as_ref(),
        receipt_release.as_ref(),
    ) {
        augment_external_owner_targets(&mut targets, owner, release.affected_paths());
        targets.roots.extend(release.roots(&owner.record));
        deduplicate_targets(&mut targets);
    }
    let source_last_modified = prepared.source_last_modified.as_deref();
    if shared_change.mutates_shared_resource() {
        return authorize_combined_install(CombinedRenoDxInstallRequest {
            context,
            feature: crate::addons::mutation_features::RENODX_INSTALL,
            guards,
            safety: &safety,
            game_id,
            game_dir: &revalidated.target_dir,
            prepared: &prepared,
            registered_exe_path: revalidated.registered_exe_path.as_deref(),
            shared_change,
            source_last_modified,
            source_mtime: None,
            targets,
            expected_owner: revalidated
                .external_owner
                .as_ref()
                .map(|owner| &owner.record),
            receipt_release,
        });
    }
    authorize_install_commit(
        context,
        crate::addons::mutation_features::RENODX_INSTALL,
        guards,
        &safety,
        |guard| {
            let install = || {
                let (record, commit) =
                    crate::addons::renodx::install::install(&revalidated.target_dir, &prepared)?;
                let record = phase::annotate_install_record(
                    record,
                    revalidated.plan.host_kind,
                    revalidated.channel,
                    revalidated.registered_exe_path.as_deref(),
                )?;
                crate::fs::stamp_mtime_best_effort(
                    Path::new(record.addon_file().as_str()),
                    source_last_modified,
                    None,
                );
                Ok((record, commit))
            };
            match (
                revalidated.external_owner.as_ref(),
                receipt_release.as_ref(),
            ) {
                (Some(owner), Some(release)) => {
                    let (component_set, baseline_mutations) =
                        release.file_only_catalog_projection();
                    crate::addons::durable::run_replace_expected_install_mutation(
                        crate::addons::durable::TargetsMutation {
                            context,
                            guard,
                            targets,
                            feature: crate::addons::mutation_features::RENODX_INSTALL,
                            game_id,
                        },
                        &owner.record,
                        component_set,
                        &baseline_mutations,
                        || release.apply_filesystem_only(),
                        install,
                        |_| release.journal_after_commit(context, game_id),
                    )
                }
                (Some(_), None) => Err(ServiceError::command_failed(
                    "external owner replacement has no receipt-only release plan",
                )),
                (None, _) => crate::addons::durable::run_install_mutation(
                    context,
                    guard,
                    targets,
                    crate::addons::mutation_features::RENODX_INSTALL,
                    game_id,
                    install,
                ),
            }
        },
    )
}

/// Executes the established inactive local-file install flow.
pub(super) async fn install_from_file(
    request: InstallRequest<'_>,
    file_path: &str,
) -> Result<InstalledAddon, ServiceError> {
    let InstallRequest {
        context,
        manifest,
        reshade_sources,
        game_id,
        requested_channel,
        safety,
        allow_shared_vulkan_layer_install,
        progress,
    } = request;
    ensure_requested_channel(reshade_sources, requested_channel)?;

    // Read the user file outside the game lock (local I/O only).
    let (addon_bytes, source_mtime) = read_addon_file(file_path)?;
    let file_arch = crate::addons::renodx::arch_from_addon_file(file_path).ok_or_else(|| {
        crate::addons::renodx::errors::invalid(
            "the selected file is not a RenoDX add-on (.addon64 / .addon32)".to_owned(),
        )
    })?;

    // Phase 1: snapshot under the per-game lock.
    let snapshot = {
        let _guard =
            crate::mutation_boundary::enter_game_mutation_boundary_async(context, game_id).await?;
        resolve_file_install_snapshot(context, manifest, game_id, requested_channel, file_arch)?
    };

    // Phase 2: host download (when needed) — no game-folder mutation.
    let prepared = prepare_install_from_file(
        &snapshot.plan,
        reshade_sources,
        game_id.clone(),
        LocalAddonSource {
            bytes: addon_bytes,
            last_modified: source_mtime.map(crate::fs::format_http_date),
        },
        snapshot.channel,
        snapshot.writes_host,
        progress,
    )
    .await?;

    let shared_change =
        crate::addons::renodx::use_cases::commands::shared_vulkan_layer::prepare_for_inactive_install(
            crate::addons::renodx::use_cases::commands::shared_vulkan_layer::PrepareInstallRequest {
                plan: &snapshot.plan,
                reshade_config: reshade_sources,
                channel: snapshot.channel,
                allow_shared_vulkan_layer_install,
                exe_path: snapshot.registered_exe_path.as_deref(),
                progress,
            },
            snapshot
                .previous_owner
                .as_ref()
                .and_then(|owner| owner.shared_vulkan_exe_path()),
        )
        .await?;

    if snapshot.external_owner.as_ref().is_some_and(|owner| {
        shared_change.mutates_shared_resource()
            && (!matches!(owner.record.kind(), AddonKind::RenoDx | AddonKind::Luma)
                || owner.record.host_kind()
                    != Some(renderpilot_domain::InstalledAddonHostKind::Proxy)
                || snapshot.plan.host_kind != crate::addons::reshade::proxy::HostKind::Vulkan)
    }) {
        return Err(crate::ServiceError::invalid_input(
            "shared owner replacement requires an explicit inactive Proxy RenoDX or Luma owner and a Vulkan target",
        ));
    }

    // Phase 3: final combined boundary and one synchronous commit.
    let guards = crate::mutation_boundary::enter_mutation_boundary_async(
        context,
        game_id,
        shared_change.mutates_shared_resource(),
    )
    .await?;
    let revalidated =
        resolve_file_install_snapshot(context, manifest, game_id, requested_channel, file_arch)?;
    ensure_catalog_install_snapshot_matches(&snapshot, &revalidated)?;

    emit_tool_finalizing(progress, AddonKind::RenoDx);
    let mut targets = crate::addons::renodx::mutation_targets::install_targets(
        &revalidated.target_dir,
        &prepared,
    )?;
    let receipt_release = prepare_revalidated_owner_release(context, game_id, &revalidated)?;
    if let (Some(owner), Some(release)) = (
        revalidated.external_owner.as_ref(),
        receipt_release.as_ref(),
    ) {
        augment_external_owner_targets(&mut targets, owner, release.affected_paths());
        targets.roots.extend(release.roots(&owner.record));
        deduplicate_targets(&mut targets);
    }
    if shared_change.mutates_shared_resource() {
        return authorize_combined_install(CombinedRenoDxInstallRequest {
            context,
            feature: crate::addons::mutation_features::RENODX_INSTALL_FROM_FILE,
            guards,
            safety: &safety,
            game_id,
            game_dir: &revalidated.target_dir,
            prepared: &prepared,
            registered_exe_path: revalidated.registered_exe_path.as_deref(),
            shared_change,
            source_last_modified: None,
            source_mtime,
            targets,
            expected_owner: revalidated
                .external_owner
                .as_ref()
                .map(|owner| &owner.record),
            receipt_release,
        });
    }
    authorize_install_commit(
        context,
        crate::addons::mutation_features::RENODX_INSTALL_FROM_FILE,
        guards,
        &safety,
        |guard| {
            let install = || {
                let (record, commit) =
                    crate::addons::renodx::install::install(&revalidated.target_dir, &prepared)?;
                let record = phase::annotate_install_record(
                    record,
                    revalidated.plan.host_kind,
                    revalidated.channel,
                    revalidated.registered_exe_path.as_deref(),
                )?;
                crate::fs::stamp_mtime_best_effort(
                    Path::new(record.addon_file().as_str()),
                    None,
                    source_mtime,
                );
                Ok((record, commit))
            };
            match (
                revalidated.external_owner.as_ref(),
                receipt_release.as_ref(),
            ) {
                (Some(owner), Some(release)) => {
                    let (component_set, baseline_mutations) =
                        release.file_only_catalog_projection();
                    crate::addons::durable::run_replace_expected_install_mutation(
                        crate::addons::durable::TargetsMutation {
                            context,
                            guard,
                            targets,
                            feature: crate::addons::mutation_features::RENODX_INSTALL_FROM_FILE,
                            game_id,
                        },
                        &owner.record,
                        component_set,
                        &baseline_mutations,
                        || release.apply_filesystem_only(),
                        install,
                        |_| release.journal_after_commit(context, game_id),
                    )
                }
                (Some(_), None) => Err(ServiceError::command_failed(
                    "external owner replacement has no receipt-only release plan",
                )),
                (None, _) => crate::addons::durable::run_install_mutation(
                    context,
                    guard,
                    targets,
                    crate::addons::mutation_features::RENODX_INSTALL_FROM_FILE,
                    game_id,
                    install,
                ),
            }
        },
    )
}

fn deduplicate_targets(targets: &mut crate::addons::mutation_targets::MutationTargets) {
    let mut seen = std::collections::HashSet::new();
    targets
        .paths
        .retain(|path| seen.insert(crate::paths::normalized_key(path)));
    let mut seen = std::collections::HashSet::new();
    targets
        .roots
        .retain(|path| seen.insert(crate::paths::normalized_key(path)));
}

fn prepare_revalidated_owner_release(
    context: &crate::Context,
    game_id: &renderpilot_domain::GameId,
    snapshot: &phase::CatalogInstallSnapshot,
) -> Result<Option<PreparedExternalOwnerRelease>, ServiceError> {
    let release =
        PreparedExternalOwnerRelease::prepare(context, game_id, snapshot.external_owner.as_ref())?;
    if let (Some(owner), Some(release)) = (snapshot.external_owner.as_ref(), release.as_ref()) {
        if !phase::same_ordered_paths(&release.affected_paths(), &snapshot.receipt_release_paths)
            || !phase::same_ordered_paths(
                &release.roots(&owner.record),
                &snapshot.receipt_release_roots,
            )
        {
            return Err(crate::ServiceError::invalid_input(
                "external owner release changed during install preparation; retry the install",
            ));
        }
    } else if snapshot.external_owner.is_some() != release.is_some() {
        return Err(crate::ServiceError::invalid_input(
            "external owner release changed during install preparation; retry the install",
        ));
    }
    Ok(release)
}
