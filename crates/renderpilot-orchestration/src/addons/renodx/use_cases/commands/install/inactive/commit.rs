use std::path::Path;
use std::time::SystemTime;

use renderpilot_domain::{GameId, InstalledAddon};

use crate::addons::external_proxy_owner::prepared_release::PreparedExternalOwnerRelease as PreparedOwnerRelease;
use crate::addons::renodx::errors;
use crate::addons::renodx::install::install as install_files;
use crate::addons::renodx::use_cases::commands::shared_vulkan_layer;
use crate::addons::reshade::proxy::HostKind;
use crate::addons::reshade::types::ReshadeChannel;
use crate::{Context, ServiceError};

/// Applies the ordinary game-only commit safety authorization.
pub(super) fn authorize_install_commit<T>(
    context: &Context,
    feature: &'static str,
    guards: crate::mutation_boundary::GameMutationBoundary,
    safety: &crate::GameMutationSafetyPermits,
    game_commit: impl FnOnce(&crate::game_mutation_lock::GameMutationGuard) -> Result<T, ServiceError>,
) -> Result<T, ServiceError> {
    let authority = crate::FileSafetyAuthority::new();
    match guards {
        crate::mutation_boundary::GameMutationBoundary::Game(guard) => authority
            .authorize_game_commit(context, feature, &guard, safety.game(), || {
                game_commit(&guard)
            }),
        crate::mutation_boundary::GameMutationBoundary::GameShared(_) => {
            Err(ServiceError::command_failed(
                "a shared Vulkan install requires the combined mutation boundary",
            ))
        }
    }
}

/// Executes a Vulkan install whose game-file and shared-layer changes must be
/// published by one SVAM reservation. The engine and platform planners have
/// already captured their exact before/after states; this function only
/// composes them and supplies the durable database projections.
pub(super) struct CombinedRenoDxInstallRequest<'a> {
    pub(super) context: &'a Context,
    pub(super) feature: &'static str,
    pub(super) guards: crate::mutation_boundary::GameMutationBoundary,
    pub(super) safety: &'a crate::GameMutationSafetyPermits,
    pub(super) game_id: &'a GameId,
    pub(super) game_dir: &'a Path,
    pub(super) prepared: &'a crate::addons::renodx::install::PreparedInstall,
    pub(super) registered_exe_path: Option<&'a Path>,
    pub(super) shared_change: shared_vulkan_layer::PreparedInstallChange,
    pub(super) source_last_modified: Option<&'a str>,
    pub(super) source_mtime: Option<SystemTime>,
    pub(super) targets: crate::addons::mutation_targets::MutationTargets,
    pub(super) expected_owner: Option<&'a InstalledAddon>,
    pub(super) receipt_release: Option<PreparedOwnerRelease>,
}

pub(super) fn authorize_combined_install(
    request: CombinedRenoDxInstallRequest<'_>,
) -> Result<InstalledAddon, ServiceError> {
    let CombinedRenoDxInstallRequest {
        context,
        feature,
        guards,
        safety,
        game_id,
        game_dir,
        prepared,
        registered_exe_path,
        shared_change,
        source_last_modified,
        source_mtime,
        targets,
        expected_owner,
        mut receipt_release,
    } = request;
    let crate::mutation_boundary::GameMutationBoundary::GameShared(guards) = guards else {
        return Err(ServiceError::command_failed(
            "a shared Vulkan install requires the combined mutation boundary",
        ));
    };
    if expected_owner.is_some_and(|owner| {
        !matches!(
            owner.kind(),
            renderpilot_domain::AddonKind::RenoDx | renderpilot_domain::AddonKind::Luma
        ) || owner.host_kind() != Some(renderpilot_domain::InstalledAddonHostKind::Proxy)
            || prepared.host_kind != HostKind::Vulkan
    }) {
        return Err(ServiceError::invalid_input(
            "shared native owner replacement requires an explicit Proxy RenoDX or Luma owner and a Vulkan target",
        ));
    }
    if expected_owner.is_some() && receipt_release.is_none() {
        return Err(ServiceError::command_failed(
            "external RenoDX replacement has no receipt-only release plan",
        ));
    }
    let locked_plan = shared_change.resolve_locked_inactive_plan(context, game_id)?;
    let authority = crate::FileSafetyAuthority::new();
    if locked_plan.as_ref().is_none_or(|plan| plan.is_noop()) {
        return authority.authorize_game_shared_commit(context, feature, &guards, safety, || {
            let install = || {
                let (record, commit) = install_files(game_dir, prepared)?;
                let record = super::phase::annotate_install_record(
                    record,
                    prepared.host_kind,
                    prepared.reshade_channel.unwrap_or(ReshadeChannel::Stable),
                    registered_exe_path,
                )?;
                if source_last_modified.is_some() || source_mtime.is_some() {
                    crate::fs::stamp_mtime_best_effort(
                        Path::new(record.addon_file().as_str()),
                        source_last_modified,
                        source_mtime,
                    );
                }
                Ok((record, commit))
            };
            if let Some(expected_owner) = expected_owner {
                let release = receipt_release.ok_or_else(|| {
                    ServiceError::command_failed(
                        "external owner replacement has no receipt-only release plan",
                    )
                })?;
                if expected_owner.kind() == renderpilot_domain::AddonKind::Luma {
                    release.ensure_native_catalog_effects_empty()?;
                }
                let (component_set, baseline_mutations) =
                    if expected_owner.kind() == renderpilot_domain::AddonKind::Luma {
                        (None, Vec::new())
                    } else {
                        release.file_only_catalog_projection()
                    };
                crate::addons::durable::run_replace_expected_install_mutation(
                    crate::addons::durable::TargetsMutation {
                        context,
                        guard: guards.game(),
                        targets,
                        feature,
                        game_id,
                    },
                    expected_owner,
                    component_set,
                    &baseline_mutations,
                    || release.apply_filesystem_only(),
                    install,
                    |_| release.journal_after_commit(context, game_id),
                )
            } else {
                crate::addons::durable::run_install_mutation(
                    context,
                    guards.game(),
                    targets,
                    feature,
                    game_id,
                    install,
                )
            }
        });
    }
    let locked_plan = locked_plan.ok_or_else(|| {
        ServiceError::command_failed("combined RenoDX install lost its shared-layer plan")
    })?;
    let input = shared_change
        .into_transaction_input(locked_plan)
        .ok_or_else(|| {
            ServiceError::command_failed("combined RenoDX install has no shared-layer plan")
        })?;
    authority.authorize_game_shared_commit(context, feature, &guards, safety, || {
        let shared_vulkan_layer::SharedLayerTransactionInput {
            plan: shared_plan,
            layer_dir,
            source,
        } = input;
        let (participants, unannotated_record, mut game_roots) = match prepared.host_kind {
            HostKind::Vulkan => {
                let participants = crate::addons::renodx::install::build_vulkan_game_participants(
                    prepared, game_dir,
                )?;
                let record = crate::addons::renodx::install::build_vulkan_record(
                    prepared,
                    game_dir,
                    &participants,
                )?;
                (participants, record, vec![game_dir.to_path_buf()])
            }
            HostKind::Proxy => {
                let prepared_proxy =
                    crate::addons::renodx::install::prepare_proxy_install(game_dir, prepared)?;
                prepared_proxy.into_combined_parts(prepared)?
            }
        };
        let record = super::phase::annotate_install_record(
            unannotated_record,
            prepared.host_kind,
            prepared.reshade_channel.unwrap_or(ReshadeChannel::Stable),
            registered_exe_path,
        )?;
        let writes_canonical = shared_plan.files.iter().any(|file| {
            matches!(
                file.path.file_name().and_then(|name| name.to_str()),
                Some("ReShade64.dll" | "ReShade64.json")
            )
        });
        let shared_record = if writes_canonical {
            source
                .as_ref()
                .map(|(source, download)| {
                    crate::addons::renodx::platform::vulkan::shared_artifact::downloaded_record(
                        &layer_dir, source, download,
                    )
                })
                .transpose()?
        } else {
            None
        };
        let deletes_last_shared_artifact = shared_plan.authorizes_canonical_layer_removal();
        let mut composed =
            crate::addons::shared_vulkan_mutation::compose(Some(participants), Some(shared_plan))?;
        if let Some(expected_owner) = expected_owner {
            let release = receipt_release.as_mut().ok_or_else(|| {
                ServiceError::command_failed(
                    "external RenoDX replacement has no receipt-only release plan",
                )
            })?;
            release.ensure_native_catalog_effects_empty()?;
            game_roots.extend(release.roots(expected_owner));
            composed.prepend_files(release.file_intents()?)?;
        }
        let game_scope = crate::file_mutation::MutationScope::new(game_roots)?;
        let roots = crate::addons::shared_vulkan_mutation::TrustedRoots::game_shared(
            &game_scope,
            &layer_dir,
        )?;
        let shared_artifact = if deletes_last_shared_artifact {
            renderpilot_storage_sqlite::SharedArtifactMutation::Delete(
                renderpilot_domain::SharedArtifactKind::RenoDxVulkanLayer,
            )
        } else {
            match shared_record.as_ref() {
                Some(record) => renderpilot_storage_sqlite::SharedArtifactMutation::Upsert(record),
                None => renderpilot_storage_sqlite::SharedArtifactMutation::Keep,
            }
        };
        let registry = crate::addons::renodx::platform::vulkan::native_registry()
            .ok_or_else(errors::vulkan_unsupported_platform)?;
        let mutation_id = ulid::Ulid::generate().to_string();
        let scope = expected_owner.map_or_else(
            || crate::addons::shared_vulkan_mutation::ScopeSpec::game_upsert(game_id, &record),
            |expected| {
                crate::addons::shared_vulkan_mutation::ScopeSpec::game_replace_expected(
                    game_id, expected, &record,
                )
            },
        );
        let identity = crate::addons::shared_vulkan_mutation::MutationIdentity::new(
            &mutation_id,
            scope,
            feature,
        );
        let physical = crate::addons::shared_vulkan_mutation::PhysicalParticipants::new(
            roots,
            composed,
            Some(registry),
        );
        let projection =
            crate::addons::shared_vulkan_mutation::CatalogProjection::new(shared_artifact);
        crate::addons::shared_vulkan_mutation::execute(
            crate::addons::shared_vulkan_mutation::Request::new(
                context, identity, physical, projection,
            ),
        )?;
        if source_last_modified.is_some() || source_mtime.is_some() {
            crate::fs::stamp_mtime_best_effort(
                Path::new(record.addon_file().as_str()),
                source_last_modified,
                source_mtime,
            );
        }
        Ok(record)
    })
}
