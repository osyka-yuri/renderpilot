//! Established Luma install lifecycle for games without a proxy topology.
//!
//! This route intentionally retains the ordinary scan, torn recovery, and
//! engine transaction semantics. It is not used when an active OptiScaler
//! topology exists.

mod plan;

#[cfg(test)]
mod tests;

use std::path::Path;

use renderpilot_domain::{AddonKind, InstalledAddon};

use crate::ServiceError;
use crate::addons::external_proxy_owner::prepared_release::PreparedExternalOwnerRelease;
use crate::addons::luma::fetch::prepare::prepare_install;
use crate::addons::luma::install::install as install_files;
use crate::addons::progress::emit_tool_finalizing;

pub(super) async fn install(
    request: super::InstallRequest<'_>,
) -> Result<InstalledAddon, ServiceError> {
    let super::InstallRequest {
        context,
        manifest,
        reshade_sources,
        game_id,
        safety,
        progress,
    } = request;

    let initial = {
        let _guard =
            crate::mutation_boundary::enter_game_mutation_boundary_async(context, game_id).await?;
        plan::resolve(context, manifest, game_id)?
    };
    let dgvoodoo = plan::preparation_for_plan(
        &initial.plan,
        &initial.snapshot.target_dir,
        initial.dgvoodoo_kind,
    )?;
    let prepared = prepare_install(
        &initial.plan,
        reshade_sources,
        game_id.clone(),
        initial.snapshot.writes_host,
        dgvoodoo,
        progress,
    )
    .await?;

    commit_prepared(
        context, manifest, game_id, &safety, progress, initial, prepared,
    )
    .await
}

async fn commit_prepared(
    context: &crate::Context,
    manifest: &crate::addons::luma::types::LumaManifest,
    game_id: &renderpilot_domain::GameId,
    safety: &crate::GameSafetyPermit,
    progress: Option<&crate::net::ProgressObserver<'_>>,
    initial: plan::ResolvedInstallSnapshot,
    mut prepared: crate::addons::luma::install::PreparedInstall,
) -> Result<InstalledAddon, ServiceError> {
    let guard =
        crate::mutation_boundary::enter_game_mutation_boundary_async(context, game_id).await?;
    let revalidated = plan::resolve(context, manifest, game_id)?;
    plan::ensure_matches(&initial.snapshot, &revalidated.snapshot)?;
    plan::refresh_adopted(
        &mut prepared,
        &revalidated.snapshot.target_dir,
        &revalidated.plan,
    )?;

    emit_tool_finalizing(progress, AddonKind::Luma);
    let min_version = manifest.min_reshade_version_parsed()?;
    let mut targets = crate::addons::luma::mutation_targets::install_targets(
        &revalidated.snapshot.target_dir,
        &prepared,
        &min_version,
    )?;
    let external_release = PreparedExternalOwnerRelease::prepare(
        context,
        game_id,
        revalidated.snapshot.external_owner.as_ref(),
    )?;
    if let (Some(owner), Some(release)) = (
        revalidated.snapshot.external_owner.as_ref(),
        external_release.as_ref(),
    ) {
        crate::addons::external_proxy_owner::augment_external_owner_targets(
            &mut targets,
            owner,
            release.affected_paths(),
        );
    }
    crate::FileSafetyAuthority::new().authorize_game_commit(
        context,
        crate::addons::mutation_features::LUMA_INSTALL,
        &guard,
        safety,
        || {
            let source_last_modified = prepared.source_last_modified.clone();
            let install = || {
                let (record, commit) = install_files(
                    context,
                    &revalidated.snapshot.target_dir,
                    prepared,
                    &min_version,
                )?;
                crate::fs::stamp_mtime_best_effort(
                    Path::new(record.addon_file().as_str()),
                    source_last_modified.as_deref(),
                    None,
                );
                Ok((record, commit))
            };
            match (
                revalidated.snapshot.external_owner.as_ref(),
                external_release.as_ref(),
            ) {
                (Some(owner), Some(release)) => {
                    let (component_set, baseline_mutations) =
                        release.file_only_catalog_projection();
                    crate::addons::durable::run_replace_expected_install_mutation(
                        crate::addons::durable::TargetsMutation {
                            context,
                            guard: &guard,
                            targets,
                            feature: crate::addons::mutation_features::LUMA_INSTALL,
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
                (None, None) => crate::addons::durable::run_install_mutation(
                    context,
                    &guard,
                    targets,
                    crate::addons::mutation_features::LUMA_INSTALL,
                    game_id,
                    install,
                ),
                _ => Err(crate::ServiceError::invalid_input(
                    "inactive Luma owner release changed after final validation",
                )),
            }
        },
    )
}
