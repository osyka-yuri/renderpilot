//! Installs RenoDX from upstream or from a user-selected add-on file.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use renderpilot_domain::{
    AddonKind, Architecture, GameId, InstalledAddon, InstalledAddonHostKind, PathRef,
};

use crate::addons::game_analysis::{analyze_game, install_target_dir};
use crate::addons::install_guard;
use crate::addons::progress::emit_tool_finalizing;
use crate::addons::renodx::arch_from_addon_file;
use crate::addons::renodx::errors;
use crate::addons::renodx::fetch::{LocalAddonSource, prepare_install, prepare_install_from_file};
use crate::addons::renodx::game_context::{analyze_and_resolve, executable_override, require_game};
use crate::addons::renodx::install::install as install_files;
use crate::addons::renodx::matcher::{
    RenoDxResolution, ResolvedInstall, generic_file_install_plan, resolve_external_install,
};
use crate::addons::renodx::types::RenoDxManifest;
use crate::addons::renodx::use_cases::commands::shared_vulkan_layer;
use crate::addons::reshade::host_policy;
use crate::addons::reshade::proxy::HostKind;
use crate::addons::reshade::types::{ReshadeChannel, ReshadeSourceCatalog};
use crate::net::ProgressObserver;
use crate::{Context, ServiceError};

/// Upper bound on a user-selected add-on file, so a stray pick cannot exhaust
/// memory. A RenoDX add-on DLL is a few MB; this is a generous ceiling.
const MAX_ADDON_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Shared parameters for a RenoDX install operation.
pub struct InstallRequest<'a> {
    /// Backend context (game repository, addon repository, settings).
    pub context: &'a Context,
    /// The resolved RenoDX tool catalogue.
    pub manifest: &'a RenoDxManifest,
    /// Independently resolved ReShade source catalogue.
    pub reshade_sources: &'a ReshadeSourceCatalog,
    /// The game to install RenoDX for.
    pub game_id: &'a GameId,
    /// The ReShade host channel to install (stable or nightly).
    pub requested_channel: ReshadeChannel,
    /// Typed game authority and optional shared-Vulkan authority.
    pub safety: crate::GameMutationSafetyPermits,
    /// Whether this caller permits installing the shared Vulkan layer when needed.
    pub allow_shared_vulkan_layer_install: bool,
    /// Optional download progress observer.
    pub progress: Option<&'a ProgressObserver<'a>>,
}

fn authorize_install_commit<T>(
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
struct CombinedVulkanInstallRequest<'a> {
    context: &'a Context,
    feature: &'static str,
    guards: crate::mutation_boundary::GameMutationBoundary,
    safety: &'a crate::GameMutationSafetyPermits,
    game_id: &'a GameId,
    game_dir: &'a Path,
    prepared: &'a crate::addons::renodx::install::PreparedInstall,
    registered_exe_path: Option<&'a Path>,
    shared_change: shared_vulkan_layer::PreparedInstallChange,
    source_last_modified: Option<&'a str>,
    source_mtime: Option<SystemTime>,
    targets: crate::addons::mutation_targets::MutationTargets,
}

fn authorize_combined_vulkan_install(
    request: CombinedVulkanInstallRequest<'_>,
) -> Result<InstalledAddon, ServiceError> {
    let CombinedVulkanInstallRequest {
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
    } = request;
    let crate::mutation_boundary::GameMutationBoundary::GameShared(guards) = guards else {
        return Err(ServiceError::command_failed(
            "a shared Vulkan install requires the combined mutation boundary",
        ));
    };
    let locked_plan = shared_change.resolve_locked_plan()?.ok_or_else(|| {
        ServiceError::command_failed("combined Vulkan install has no shared-layer plan")
    })?;
    let authority = crate::FileSafetyAuthority::new();
    if locked_plan.is_noop() {
        return authority.authorize_game_commit(
            context,
            feature,
            guards.game(),
            safety.game(),
            || {
                crate::addons::durable::run_install_mutation(
                    context,
                    guards.game(),
                    targets,
                    feature,
                    game_id,
                    || {
                        let (record, commit) = install_files(game_dir, prepared)?;
                        let record = annotate_install_record(
                            record,
                            HostKind::Vulkan,
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
                    },
                )
            },
        );
    }
    let input = shared_change
        .into_transaction_input(locked_plan)
        .ok_or_else(|| {
            ServiceError::command_failed("combined Vulkan install has no shared-layer plan")
        })?;
    authority.authorize_game_shared_commit(context, feature, &guards, safety, || {
        let shared_vulkan_layer::SharedLayerTransactionInput {
            plan: shared_plan,
            layer_dir,
            source,
        } = input;
        let participants =
            crate::addons::renodx::install::build_vulkan_game_participants(prepared, game_dir)?;
        let record =
            crate::addons::renodx::install::build_vulkan_record(prepared, game_dir, &participants)?;
        let record = annotate_install_record(
            record,
            HostKind::Vulkan,
            prepared.reshade_channel.unwrap_or(ReshadeChannel::Stable),
            registered_exe_path,
        )?;
        let game_scope = crate::file_mutation::MutationScope::single(game_dir)?;
        let roots = crate::addons::shared_vulkan_mutation::TrustedRoots::game_shared(
            &game_scope,
            &layer_dir,
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
        let composed =
            crate::addons::shared_vulkan_mutation::compose(Some(participants), Some(shared_plan))?;
        let shared_artifact = match shared_record.as_ref() {
            Some(record) => renderpilot_storage_sqlite::SharedArtifactMutation::Upsert(record),
            None => renderpilot_storage_sqlite::SharedArtifactMutation::Keep,
        };
        let registry = crate::addons::renodx::platform::vulkan::native_registry()
            .ok_or_else(errors::vulkan_unsupported_platform)?;
        let mutation_id = ulid::Ulid::generate().to_string();
        let identity = crate::addons::shared_vulkan_mutation::MutationIdentity::new(
            &mutation_id,
            crate::addons::shared_vulkan_mutation::ScopeSpec::game_upsert(game_id, &record),
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

/// Installs RenoDX into `game`, fetching the add-on + ReShade from upstream and
/// persisting the record needed to reverse it.
///
/// The game permit must be fresh for the target game. A shared-Vulkan permit is
/// required when a Vulkan install will mutate
/// the shared layer. `allow_shared_vulkan_layer_install` must be `true` for a Vulkan game when
/// no ReShade Vulkan layer is present yet. The ReShade host (when one must be
/// installed) uses the requested channel. An unavailable explicit channel is
/// rejected rather than silently remapped.
///
/// Returns the `managed_app_record` (the per-game `InstalledAddon`).
///
/// Network prepare runs **outside** the per-game `game_mutation_lock` (same 3-phase
/// contract as Luma install) so a slow download does not block peer availability.
pub async fn install(request: InstallRequest<'_>) -> Result<InstalledAddon, ServiceError> {
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
        shared_vulkan_layer::prepare_for_install(shared_vulkan_layer::PrepareInstallRequest {
            plan: &snapshot.plan,
            reshade_config: reshade_sources,
            channel: snapshot.channel,
            allow_shared_vulkan_layer_install,
            exe_path: snapshot.registered_exe_path.as_deref(),
            progress,
        })
        .await?;

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
    let targets = crate::addons::renodx::mutation_targets::install_targets(
        &revalidated.target_dir,
        &prepared,
    )?;
    let source_last_modified = prepared.source_last_modified.as_deref();
    if shared_change.mutates_shared_resource() {
        if !matches!(revalidated.plan.host_kind, HostKind::Vulkan) {
            return Err(ServiceError::command_failed(
                "a shared Vulkan plan was produced for a non-Vulkan install",
            ));
        }
        return authorize_combined_vulkan_install(CombinedVulkanInstallRequest {
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
        });
    }
    authorize_install_commit(
        context,
        crate::addons::mutation_features::RENODX_INSTALL,
        guards,
        &safety,
        |guard| {
            crate::addons::durable::run_install_mutation(
                context,
                guard,
                targets,
                crate::addons::mutation_features::RENODX_INSTALL,
                game_id,
                || {
                    let (record, commit) = install_files(&revalidated.target_dir, &prepared)?;
                    let record = annotate_install_record(
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
                },
            )
        },
    )
}

/// Installs RenoDX from a user-downloaded add-on file — the manual path for any
/// DirectX game, whether or not the catalogue knows it.
///
/// Same engine and reversibility as [`install`]; the add-on bytes come from
/// `file_path` (validated as a PE) instead of an upstream download, and the record
/// tracks no upstream source. A curated *External* title is no longer a special
/// case: it just yields a richer plan, while any DirectX game falls back to a
/// generic "ReShade host + your add-on" plan. The renderer must be able to load a
/// proxy DLL (a confirmed Vulkan/OpenGL game is refused), and the add-on's
/// architecture must match the game's.
pub async fn install_from_file(
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
    let file_arch = arch_from_addon_file(file_path).ok_or_else(|| {
        errors::invalid("the selected file is not a RenoDX add-on (.addon64 / .addon32)".to_owned())
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
        shared_vulkan_layer::prepare_for_install(shared_vulkan_layer::PrepareInstallRequest {
            plan: &snapshot.plan,
            reshade_config: reshade_sources,
            channel: snapshot.channel,
            allow_shared_vulkan_layer_install,
            exe_path: snapshot.registered_exe_path.as_deref(),
            progress,
        })
        .await?;

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
    let targets = crate::addons::renodx::mutation_targets::install_targets(
        &revalidated.target_dir,
        &prepared,
    )?;
    if shared_change.mutates_shared_resource() {
        if !matches!(revalidated.plan.host_kind, HostKind::Vulkan) {
            return Err(ServiceError::command_failed(
                "a shared Vulkan plan was produced for a non-Vulkan install",
            ));
        }
        return authorize_combined_vulkan_install(CombinedVulkanInstallRequest {
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
        });
    }
    authorize_install_commit(
        context,
        crate::addons::mutation_features::RENODX_INSTALL_FROM_FILE,
        guards,
        &safety,
        |guard| {
            crate::addons::durable::run_install_mutation(
                context,
                guard,
                targets,
                crate::addons::mutation_features::RENODX_INSTALL_FROM_FILE,
                game_id,
                || {
                    let (record, commit) = install_files(&revalidated.target_dir, &prepared)?;
                    let record = annotate_install_record(
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
                },
            )
        },
    )
}

/// Owned install plan snapshot used across the unlocked network prepare window.
struct CatalogInstallSnapshot {
    plan: ResolvedInstall,
    target_dir: std::path::PathBuf,
    channel: ReshadeChannel,
    writes_host: bool,
    registered_exe_path: Option<std::path::PathBuf>,
}

fn resolve_catalog_install_snapshot(
    context: &Context,
    manifest: &RenoDxManifest,
    game_id: &GameId,
    requested_channel: ReshadeChannel,
) -> Result<CatalogInstallSnapshot, ServiceError> {
    let game = require_game(context, game_id)?;
    let override_path = executable_override(context, game_id);
    let (analysis, resolution) = analyze_and_resolve(&game, manifest, override_path.as_deref());
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

fn resolve_file_install_snapshot(
    context: &Context,
    manifest: &RenoDxManifest,
    game_id: &GameId,
    requested_channel: ReshadeChannel,
    file_arch: Architecture,
) -> Result<CatalogInstallSnapshot, ServiceError> {
    let game = require_game(context, game_id)?;
    let analysis = analyze_game(&game, executable_override(context, game_id).as_deref());
    let target_dir = install_target_dir(&analysis)?;
    let roots = install_guard::resolve_install_scan_roots(&analysis)?;
    install_guard::guard_exclusivity_and_torn(context, game_id, AddonKind::RenoDx, &roots)?;

    if let Some(game_arch) = analysis.facts.graphics.architecture()
        && game_arch != file_arch
    {
        return Err(errors::invalid(format!(
            "this add-on is {} but the game is {} — download the matching add-on",
            arch_label(file_arch),
            arch_label(game_arch),
        )));
    }

    let plan = resolve_external_install(manifest, &analysis.facts)
        .or_else(|| generic_file_install_plan(&analysis.facts, file_arch))
        .ok_or_else(|| {
            errors::invalid(
                "RenoDX cannot be installed for this game: its renderer is not Direct3D".to_owned(),
            )
        })?;
    ensure_addon_arch(file_arch, plan.arch)?;

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

fn ensure_catalog_install_snapshot_matches(
    snapshot: &CatalogInstallSnapshot,
    current: &CatalogInstallSnapshot,
) -> Result<(), ServiceError> {
    use crate::paths::same_path;

    if snapshot.plan.slug != current.plan.slug
        || snapshot.plan.addon_url != current.plan.addon_url
        || snapshot.plan.arch != current.plan.arch
        || snapshot.plan.host_kind != current.plan.host_kind
        || snapshot.plan.proxy_dll_name != current.plan.proxy_dll_name
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

fn annotate_install_record(
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

/// Reads a user-selected add-on file, rejecting non-files and anything larger than
/// a sane add-on so a stray pick cannot exhaust memory. PE validation happens in
/// the fetch layer alongside the download path.
fn read_addon_file(file_path: &str) -> Result<(Vec<u8>, Option<SystemTime>), ServiceError> {
    let path = Path::new(file_path);
    let metadata =
        std::fs::metadata(path).map_err(|error| errors::io("read add-on file", path, &error))?;
    if !metadata.is_file() {
        return Err(errors::invalid(
            "the selected add-on path is not a file".to_owned(),
        ));
    }
    if metadata.len() > MAX_ADDON_FILE_BYTES {
        return Err(errors::invalid(format!(
            "add-on file is too large (maximum {} MB)",
            MAX_ADDON_FILE_BYTES / (1024 * 1024)
        )));
    }
    let source_mtime = metadata.modified().ok();
    let bytes =
        std::fs::read(path).map_err(|error| errors::io("read add-on file", path, &error))?;
    Ok((bytes, source_mtime))
}

/// Human-readable bitness label for an add-on/game architecture-mismatch message.
fn arch_label(arch: Architecture) -> &'static str {
    match arch {
        Architecture::X64 => "64-bit",
        Architecture::X86 => "32-bit",
    }
}

/// Enforces the add-on ↔ host bitness invariant: a picked add-on must match the
/// architecture of the resolved install plan (the ReShade host it installs beside),
/// so a 32-bit add-on can never be paired with a 64-bit host or vice-versa.
fn ensure_addon_arch(file_arch: Architecture, plan_arch: Architecture) -> Result<(), ServiceError> {
    if file_arch != plan_arch {
        return Err(errors::invalid(format!(
            "this add-on is {} but RenoDX for this game needs the {} build — download the matching add-on",
            arch_label(file_arch),
            arch_label(plan_arch),
        )));
    }
    Ok(())
}

fn ensure_requested_channel(
    reshade_sources: &ReshadeSourceCatalog,
    requested_channel: ReshadeChannel,
) -> Result<(), ServiceError> {
    if reshade_sources.supports_channel(requested_channel) {
        Ok(())
    } else {
        Err(errors::channel_unavailable(requested_channel))
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;
    use std::fs;

    use renderpilot_application::GameRepository;
    use renderpilot_domain::{
        GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
    };
    use tempfile::{TempDir, tempdir};

    use super::*;

    #[test]
    fn addon_arch_invariant_rejects_a_bitness_mismatch() {
        assert!(ensure_addon_arch(Architecture::X64, Architecture::X64).is_ok());
        let error = ensure_addon_arch(Architecture::X86, Architecture::X64)
            .expect_err("a 32-bit add-on for a 64-bit host must be rejected");
        assert_matches!(error, ServiceError::InvalidInput(_));
    }

    #[test]
    fn every_install_path_rejects_an_explicit_unavailable_stable_channel() {
        let mut reshade_sources = crate::addons::renodx::test_support::reshade_sources();
        reshade_sources.stable = None;

        let error = ensure_requested_channel(&reshade_sources, ReshadeChannel::Stable)
            .expect_err("Stable must not silently remap to Nightly");

        assert_matches!(error, ServiceError::InvalidInput(_));
    }

    struct SafetyFixture {
        _db_dir: TempDir,
        game_dir: TempDir,
        context: Context,
        game_id: GameId,
    }

    fn safety_fixture(suffix: &str) -> SafetyFixture {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new(format!("manual:install-safety-{suffix}")).expect("game id");
        let game = GameInstallation::new(
            GameIdentity::new(game_id.clone(), "Safety Test Game", Launcher::Manual)
                .expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(game_dir.path().to_string_lossy()).expect("game path"),
        );
        context.storage().upsert_game(&game).expect("game");

        SafetyFixture {
            _db_dir: db_dir,
            game_dir,
            context,
            game_id,
        }
    }

    async fn assert_install_barrier_rejects(
        fixture: &SafetyFixture,
        safety: crate::GameMutationSafetyPermits,
        expected: fn(&ServiceError) -> bool,
    ) {
        let guards = crate::mutation_boundary::enter_mutation_boundary_async(
            &fixture.context,
            &fixture.game_id,
            false,
        )
        .await
        .expect("game boundary");
        let mut commit_called = false;
        let error = authorize_install_commit(
            &fixture.context,
            crate::addons::mutation_features::RENODX_INSTALL,
            guards,
            &safety,
            |_| {
                commit_called = true;
                Ok(())
            },
        )
        .expect_err("invalid safety must reject the install commit");

        assert!(expected(&error), "unexpected error: {error:?}");
        assert!(!commit_called, "safety rejection must precede first write");
        assert!(
            fixture
                .context
                .storage()
                .pending_file_mutations_for_game(&fixture.game_id)
                .expect("pending mutations")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn install_commit_barrier_rejects_stale_game_context_before_first_write() {
        let fixture = safety_fixture("stale");
        let authority = crate::FileSafetyAuthority::new();
        let assessment = authority
            .issue_game_assessment(&fixture.context, &fixture.game_id)
            .expect("assessment");
        let safety = authority
            .game_mutation_permits(
                fixture.game_id.clone(),
                Some(&assessment.context_token),
                None,
            )
            .expect("permits");
        fs::create_dir(fixture.game_dir.path().join("EasyAntiCheat")).expect("anti-cheat marker");

        assert_install_barrier_rejects(&fixture, safety, |error| {
            matches!(error, ServiceError::SafetyContextStale { .. })
        })
        .await;
    }

    #[tokio::test]
    async fn install_commit_barrier_rejects_another_game_scope_before_first_write() {
        let fixture = safety_fixture("scope");
        let other = safety_fixture("other");
        fixture
            .context
            .storage()
            .upsert_game(
                &other
                    .context
                    .storage()
                    .require_game(&other.game_id)
                    .expect("other game"),
            )
            .expect("copy other game");
        let authority = crate::FileSafetyAuthority::new();
        let assessment = authority
            .issue_game_assessment(&fixture.context, &other.game_id)
            .expect("assessment");
        let safety = authority
            .game_mutation_permits(
                fixture.game_id.clone(),
                Some(&assessment.context_token),
                None,
            )
            .expect("well-formed permits");

        assert_install_barrier_rejects(&fixture, safety, |error| {
            matches!(error, ServiceError::SafetyContextScopeMismatch { .. })
        })
        .await;
    }
}
