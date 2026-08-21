//! Installs Luma Framework from upstream.
//!
//! Network prepare (ZIP + optional host + optional dgVoodoo) runs **outside**
//! the per-game `game_mutation_lock` so peer availability/install is not blocked
//! for the entire multi-download window. The lock is held only for snapshot /
//! fail-fast gates and for revalidation + disk apply (mirrors Luma update).

use std::path::{Path, PathBuf};

use renderpilot_domain::{AddonKind, Architecture, GameId, InstalledAddon};

use crate::addons::exclusivity;
use crate::addons::game_analysis::install_target_dir;
use crate::addons::install_guard;
use crate::addons::luma::dgvoodoo;
use crate::addons::luma::errors;
use crate::addons::luma::fetch::prepare::prepare_install;
use crate::addons::luma::game_context::{analyze_and_resolve, executable_override, require_game};
use crate::addons::luma::install::install as install_files;
use crate::addons::luma::matcher::{LumaResolution, ResolvedLumaInstall};
use crate::addons::luma::mutation_targets;
use crate::addons::luma::types::LumaManifest;
use crate::addons::progress::emit_tool_finalizing;
use crate::addons::records;
use crate::addons::reshade::host_policy;
use crate::addons::reshade::types::ReshadeSourceCatalog;
use crate::game_mutation_lock;
use crate::net::ProgressObserver;
use crate::paths::same_path;
use crate::{Context, ServiceError};

/// Decision inputs captured under the lock before network prepare. Revalidated
/// after re-lock so a concurrent install/update cannot race exclusivity or
/// swap the target while downloads run unlocked.
#[derive(Debug, Clone)]
struct InstallSnapshot {
    target_dir: PathBuf,
    asset: String,
    addon_file: String,
    arch: Architecture,
    proxy_dll_name: String,
    writes_host: bool,
    /// Discriminator only -- full preparation is re-derived after re-lock.
    dgvoodoo_kind: DgVoodooPrepKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DgVoodooPrepKind {
    None,
    Managed,
    Reused,
    Adopted,
}

impl DgVoodooPrepKind {
    fn from_preparation(prep: Option<&dgvoodoo::DgVoodooPreparation<'_>>) -> Self {
        match prep {
            None => Self::None,
            Some(dgvoodoo::DgVoodooPreparation::Managed(_)) => Self::Managed,
            Some(dgvoodoo::DgVoodooPreparation::Reused(_)) => Self::Reused,
            Some(dgvoodoo::DgVoodooPreparation::Adopted(_)) => Self::Adopted,
        }
    }
}

fn state_changed_retry() -> ServiceError {
    errors::state_changed_retry_install()
}

/// Shared parameters for a Luma install operation.
pub struct InstallRequest<'a> {
    /// Backend context (game repository, addon repository, settings).
    pub context: &'a Context,
    /// The resolved Luma tool catalogue.
    pub manifest: &'a LumaManifest,
    /// Independently resolved ReShade source catalogue.
    pub reshade_sources: &'a ReshadeSourceCatalog,
    /// The game to install Luma for.
    pub game_id: &'a GameId,
    /// Typed authority for the final game-file commit.
    pub safety: crate::GameSafetyPermit,
    /// Optional download progress observer.
    pub progress: Option<&'a ProgressObserver<'a>>,
}

/// Installs Luma into `game`, fetching the release asset + ReShade host (always
/// nightly) from upstream and persisting the record needed to reverse it.
///
/// `safety` must come from a fresh assessment for the target game. Refuses outright
/// when Luma is already installed for this game,
/// when RenoDX is installed (or unmanaged files belonging to it are on disk --
/// see the shared add-on exclusivity policy), or when Luma-shaped files are present on
/// disk with no tracked record (adoption is not implemented in v1).
///
/// Returns the persisted [`InstalledAddon`] record.
pub async fn install(request: InstallRequest<'_>) -> Result<InstalledAddon, ServiceError> {
    let InstallRequest {
        context,
        manifest,
        reshade_sources,
        game_id,
        safety,
        progress,
    } = request;

    // Phase 1: snapshot under the per-game lock (fail-fast gates + plan), then
    // release for multi-artifact network work.
    let (snapshot, plan, dgvoodoo_preparation_kind) = {
        let _guard =
            game_mutation_lock::enter_game_mutation_boundary_async(context, game_id).await?;
        let prepared = resolve_install_snapshot(context, manifest, game_id)?;
        (
            prepared.snapshot,
            prepared.plan,
            prepared.dgvoodoo_preparation_kind,
        )
    };

    // Phase 2: downloads and validation only -- no game-folder mutation.
    // Re-derive preparation from the plan for the fetch layer (requirement
    // lives on the plan; kind was snapshotted for phase-3 revalidation).
    let dgvoodoo_preparation =
        dgvoodoo_preparation_for_plan(&plan, &snapshot.target_dir, dgvoodoo_preparation_kind)?;
    let mut prepared = prepare_install(
        &plan,
        reshade_sources,
        game_id.clone(),
        snapshot.writes_host,
        dgvoodoo_preparation,
        progress,
    )
    .await?;

    // Phase 3: re-lock, revalidate, apply under exclusivity.
    let guard = game_mutation_lock::enter_game_mutation_boundary_async(context, game_id).await?;
    let revalidated = resolve_install_snapshot(context, manifest, game_id)?;
    ensure_install_snapshot_still_matches(&snapshot, &revalidated.snapshot)?;
    // Adopted ownership paths were frozen during unlocked prepare -- rebuild them
    // from disk under the lock so config presence / stack membership match apply.
    refresh_adopted_dgvoodoo(
        &mut prepared,
        &revalidated.snapshot.target_dir,
        &revalidated.plan,
    )?;

    emit_tool_finalizing(progress, AddonKind::Luma);
    let min_version = manifest.min_reshade_version_parsed()?;
    let targets = mutation_targets::install_targets(
        &revalidated.snapshot.target_dir,
        &prepared,
        &min_version,
    )?;
    crate::FileSafetyAuthority::new().authorize_game_commit(
        context,
        crate::addons::mutation_features::LUMA_INSTALL,
        &guard,
        &safety,
        || {
            crate::addons::durable::run_install_mutation(
                context,
                &guard,
                targets,
                crate::addons::mutation_features::LUMA_INSTALL,
                game_id,
                || {
                    let source_last_modified = prepared.source_last_modified.clone();
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
                },
            )
        },
    )
}

struct ResolvedInstallSnapshot {
    snapshot: InstallSnapshot,
    plan: ResolvedLumaInstall,
    dgvoodoo_preparation_kind: DgVoodooPrepKind,
}

/// Resolve plan, run exclusivity/torn/unmanaged gates, assess host/dgVoodoo.
/// Must be called while holding the per-game `game_mutation_lock`.
fn resolve_install_snapshot(
    context: &Context,
    manifest: &LumaManifest,
    game_id: &GameId,
) -> Result<ResolvedInstallSnapshot, ServiceError> {
    let game = require_game(context, game_id)?;
    let override_path = executable_override(context, game_id);
    let (analysis, resolution) = analyze_and_resolve(&game, manifest, override_path.as_deref());
    let target_dir = install_target_dir(&analysis)?;
    let roots = install_guard::resolve_install_scan_roots(&analysis)?;

    ensure_not_already_installed(context, game_id)?;
    // Peer exclusivity + torn recovery share install scan roots with availability.
    install_guard::guard_exclusivity_and_torn(context, game_id, AddonKind::Luma, &roots)?;
    // A crash mid-install left debris with no DB record -- peer check already
    // confirmed no other tool's claim; `ensure_not_unmanaged` is the real gate
    // if recover could not clean up all Luma-shaped files.
    let scan_dirs = roots.scan_dir_paths();
    ensure_not_unmanaged(scan_dirs.as_slice())?;

    let plan = match resolution {
        LumaResolution::Installable(plan) => *plan,
        LumaResolution::Incompatible { reason } => {
            return Err(errors::invalid(format!(
                "Luma is not compatible with this game: {reason:?}"
            )));
        }
        LumaResolution::Blacklisted { message } => {
            return Err(errors::invalid(format!(
                "Luma is not supported for this game: {}",
                message.fallback_text
            )));
        }
        LumaResolution::NoMatch => {
            return Err(errors::invalid(
                "Luma has no profile for this game".to_owned(),
            ));
        }
    };

    let min_version = manifest.min_reshade_version_parsed()?;
    let host = host_policy::assess_for_tool(
        &target_dir,
        &plan.proxy_dll_name,
        "Luma",
        Some(&min_version),
    );
    host.ensure_initial_installable(&plan.proxy_dll_name)?;
    let writes_host = host.initial_writes_host();
    let dgvoodoo_preparation = assess_dgvoodoo_preparation(&plan, &target_dir, host.lifecycle)?;
    let dgvoodoo_preparation_kind =
        DgVoodooPrepKind::from_preparation(dgvoodoo_preparation.as_ref());

    let snapshot = InstallSnapshot {
        target_dir,
        asset: plan.asset.clone(),
        addon_file: plan.addon_file.clone(),
        arch: plan.arch,
        proxy_dll_name: plan.proxy_dll_name.clone(),
        writes_host,
        dgvoodoo_kind: dgvoodoo_preparation_kind,
    };

    Ok(ResolvedInstallSnapshot {
        snapshot,
        plan,
        dgvoodoo_preparation_kind,
    })
}

fn assess_dgvoodoo_preparation<'a>(
    plan: &'a ResolvedLumaInstall,
    target_dir: &Path,
    host_lifecycle: host_policy::HostLifecycle,
) -> Result<Option<dgvoodoo::DgVoodooPreparation<'a>>, ServiceError> {
    match dgvoodoo::requirement(plan.external_requirement.as_ref()) {
        None => Ok(None),
        Some(requirement) => match dgvoodoo::assess_existing(target_dir, requirement) {
            dgvoodoo::ExistingDgVoodoo::Absent => {
                Ok(Some(dgvoodoo::DgVoodooPreparation::Managed(requirement)))
            }
            dgvoodoo::ExistingDgVoodoo::CompatibleReusable => Ok(Some(
                dgvoodoo::DgVoodooPreparation::Reused(dgvoodoo::reused_config(requirement)),
            )),
            dgvoodoo::ExistingDgVoodoo::CompatibleAdoptable
                if host_lifecycle == host_policy::HostLifecycle::AdoptEmpty =>
            {
                Ok(Some(dgvoodoo::DgVoodooPreparation::Adopted(
                    dgvoodoo::adopted_existing(requirement, target_dir),
                )))
            }
            dgvoodoo::ExistingDgVoodoo::CompatibleAdoptable => Ok(Some(
                dgvoodoo::DgVoodooPreparation::Reused(dgvoodoo::reused_config(requirement)),
            )),
            dgvoodoo::ExistingDgVoodoo::Conflict(reason) => Err(errors::invalid(format!(
                "the existing dgVoodoo runtime is incompatible with Luma: {reason}"
            ))),
        },
    }
}

/// Rebuild preparation for `prepare_install` from the owned plan + snapshotted kind.
fn dgvoodoo_preparation_for_plan<'a>(
    plan: &'a ResolvedLumaInstall,
    target_dir: &Path,
    kind: DgVoodooPrepKind,
) -> Result<Option<dgvoodoo::DgVoodooPreparation<'a>>, ServiceError> {
    let Some(requirement) = dgvoodoo::requirement(plan.external_requirement.as_ref()) else {
        return Ok(None);
    };
    Ok(Some(match kind {
        DgVoodooPrepKind::None => return Ok(None),
        DgVoodooPrepKind::Managed => dgvoodoo::DgVoodooPreparation::Managed(requirement),
        DgVoodooPrepKind::Reused => {
            dgvoodoo::DgVoodooPreparation::Reused(dgvoodoo::reused_config(requirement))
        }
        DgVoodooPrepKind::Adopted => dgvoodoo::DgVoodooPreparation::Adopted(
            dgvoodoo::adopted_existing(requirement, target_dir),
        ),
    }))
}

/// Re-scan Adopted dgVoodoo ownership under the per-game lock immediately before
/// apply. Prepare-time paths can drift (config appears/disappears) while the
/// network window is unlocked; coarse kind revalidation alone is not enough.
fn refresh_adopted_dgvoodoo(
    prepared: &mut crate::addons::luma::install::PreparedInstall,
    target_dir: &Path,
    plan: &ResolvedLumaInstall,
) -> Result<(), ServiceError> {
    use crate::addons::luma::dgvoodoo::DgVoodooInstall;

    let Some(DgVoodooInstall::Adopted(_)) = prepared.dgvoodoo.as_ref() else {
        return Ok(());
    };
    let Some(requirement) = dgvoodoo::requirement(plan.external_requirement.as_ref()) else {
        return Err(state_changed_retry());
    };
    match dgvoodoo::assess_existing(target_dir, requirement) {
        dgvoodoo::ExistingDgVoodoo::CompatibleAdoptable => {
            prepared.dgvoodoo = Some(DgVoodooInstall::Adopted(dgvoodoo::adopted_existing(
                requirement,
                target_dir,
            )));
            Ok(())
        }
        _ => Err(state_changed_retry()),
    }
}

fn ensure_install_snapshot_still_matches(
    snapshot: &InstallSnapshot,
    current: &InstallSnapshot,
) -> Result<(), ServiceError> {
    if !same_path(&snapshot.target_dir, &current.target_dir)
        || snapshot.asset != current.asset
        || snapshot.addon_file != current.addon_file
        || snapshot.arch != current.arch
        || snapshot.proxy_dll_name != current.proxy_dll_name
        || snapshot.writes_host != current.writes_host
        || snapshot.dgvoodoo_kind != current.dgvoodoo_kind
    {
        return Err(state_changed_retry());
    }
    Ok(())
}

/// Refuses when a Luma record already exists for this game -- unlike RenoDX's
/// flat, no-backup proxy install (safely idempotent to re-run), Luma's
/// tree-shaped payload uses backup-on-collision ops, so a blind re-run would
/// litter `.bak` siblings across an already-installed tree instead of cleanly
/// overwriting it.
fn ensure_not_already_installed(context: &Context, game_id: &GameId) -> Result<(), ServiceError> {
    records::ensure_no_record(
        context,
        game_id,
        AddonKind::Luma,
        "Luma is already installed for this game; uninstall before reinstalling",
    )
}

/// Refuses to install when Luma-shaped files are already present on disk with
/// no tracked database record. Availability attempts adoption (see
/// `AvailabilityOutcome::UnmanagedPresent` fallback); a direct install path
/// still refuses to avoid `.bak` litter from the tree-shaped CreateNested
/// strategy. The caller (UI) normally sees an adopted record instead.
fn ensure_not_unmanaged(scan_dirs: &[&Path]) -> Result<(), ServiceError> {
    exclusivity::ensure_not_unmanaged(
        scan_dirs,
        AddonKind::Luma,
        "an existing Luma install was found on disk with no tracked record; remove it manually before installing",
    )
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::*;
    #[cfg(windows)]
    use crate::addons::engine;

    // -----------------------------------------------------------------
    // E.19: DB/disk-level refusals happen before any network is touched --
    // `ensure_not_already_installed`/`ensure_not_blocked`/`ensure_not_unmanaged`
    // all run before `prepare_install`'s fetches, so these are safe to exercise
    // without a mock HTTP layer: the call must fail before reaching one.
    //
    // Primary-exe resolution is Windows-only; these integration tests are
    // `cfg(windows)` (same pattern as RenoDX install/availability tests).
    // -----------------------------------------------------------------

    #[cfg(windows)]
    use crate::addons::luma::test_support::{
        MACHINE_AMD64, PE32_PLUS_MAGIC, build_pe_with_exports, manifest,
    };
    #[cfg(windows)]
    use renderpilot_application::{GameRepository, InstalledAddonRepository};
    #[cfg(windows)]
    use renderpilot_domain::{
        GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
    };
    #[cfg(windows)]
    use tempfile::tempdir;

    #[cfg(windows)]
    fn seed_game(
        context: &Context,
        game_id: &GameId,
        appid: &str,
        game_dir: &Path,
        exe_path: &Path,
    ) {
        let identity = GameIdentity::new(game_id.clone(), "Dishonored 2", Launcher::Steam)
            .expect("identity")
            .with_external_id(appid)
            .expect("external id");
        let game = GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(game_dir.to_string_lossy().replace('\\', "/")).expect("install path"),
        )
        .with_executable_candidate(
            PathRef::new(exe_path.to_string_lossy().replace('\\', "/")).expect("exe path"),
        );
        context.storage().upsert_game(&game).expect("seed game");
    }

    #[cfg(windows)]
    fn write_stub_exe(path: &Path) {
        std::fs::write(
            path,
            build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
        )
        .expect("write exe");
    }

    #[cfg(windows)]
    fn game_safety(context: &Context, game_id: &GameId) -> crate::GameSafetyPermit {
        let authority = crate::FileSafetyAuthority::new();
        let assessment = authority
            .issue_game_assessment(context, game_id)
            .expect("assessment");
        authority
            .game_permit(game_id.clone(), Some(&assessment.context_token))
            .expect("permit")
    }

    #[test]
    #[cfg(windows)]
    fn install_safety_boundary_rejects_missing_stale_and_scope_mismatched_permits_before_writes() {
        let db_dir = tempdir().expect("db root");
        let game_dir = tempdir().expect("game root");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:luma-install-safety").expect("game id");
        let exe_path = game_dir.path().join("Dishonored2.exe");
        write_stub_exe(&exe_path);
        seed_game(
            &context,
            &game_id,
            "luma-install-safety",
            game_dir.path(),
            &exe_path,
        );
        let target = game_dir.path().join("Luma-Game.addon");

        let authority = crate::FileSafetyAuthority::new();
        let missing = authority
            .game_permit(game_id.clone(), None)
            .expect_err("missing permit must reject before install writes");
        assert!(matches!(missing, ServiceError::SafetyContextMissing { .. }));
        assert!(!target.exists());
        assert!(
            context
                .storage()
                .pending_file_mutations_for_game(&game_id)
                .expect("pending rows")
                .is_empty()
        );
        let assessment = authority
            .issue_game_assessment(&context, &game_id)
            .expect("assessment");
        let permit = authority
            .game_permit(game_id.clone(), Some(&assessment.context_token))
            .expect("permit");
        std::fs::write(game_dir.path().join("EasyAntiCheat"), b"detected marker").expect("marker");
        let guard = game_mutation_lock::try_lock(&game_id).expect("lock");
        let stale = authority
            .authorize_game_commit(
                &context,
                crate::addons::mutation_features::LUMA_INSTALL,
                &guard,
                &permit,
                || -> Result<(), ServiceError> { panic!("stale permit entered commit") },
            )
            .expect_err("stale permit must reject before install writes");
        assert!(matches!(stale, ServiceError::SafetyContextStale { .. }));
        assert!(!target.exists());
        assert!(
            context
                .storage()
                .pending_file_mutations_for_game(&game_id)
                .expect("pending rows")
                .is_empty()
        );
        drop(guard);

        let other_db_game = tempdir().expect("other game root");
        let other_id = GameId::new("steam:luma-install-other-safety").expect("other game id");
        let other_exe = other_db_game.path().join("Dishonored2.exe");
        write_stub_exe(&other_exe);
        seed_game(
            &context,
            &other_id,
            "luma-install-other-safety",
            other_db_game.path(),
            &other_exe,
        );
        let other_assessment = authority
            .issue_game_assessment(&context, &other_id)
            .expect("other assessment");
        let mismatched = authority
            .game_permit(game_id.clone(), Some(&other_assessment.context_token))
            .expect("well-formed permit");
        let guard = game_mutation_lock::try_lock(&game_id).expect("lock");
        let scope = authority
            .authorize_game_commit(
                &context,
                crate::addons::mutation_features::LUMA_INSTALL,
                &guard,
                &mismatched,
                || -> Result<(), ServiceError> { panic!("mismatched permit entered commit") },
            )
            .expect_err("scope-mismatched permit must reject before install writes");
        assert!(matches!(
            scope,
            ServiceError::SafetyContextScopeMismatch { .. }
        ));
        assert!(!target.exists());
        assert!(
            context
                .storage()
                .pending_file_mutations_for_game(&game_id)
                .expect("pending rows")
                .is_empty()
        );
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn install_refuses_before_any_network_when_renodx_is_installed() {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:403640").expect("game id");
        let exe_path = game_dir.path().join("Dishonored2.exe");
        write_stub_exe(&exe_path);
        seed_game(&context, &game_id, "403640", game_dir.path(), &exe_path);

        let renodx_record = InstalledAddon::new(
            game_id.clone(),
            AddonKind::RenoDx,
            PathRef::new(
                game_dir
                    .path()
                    .join("renodx-test.addon64")
                    .to_string_lossy()
                    .replace('\\', "/"),
            )
            .expect("path"),
        );
        context
            .storage()
            .upsert_installed_addon(&renodx_record)
            .expect("seed renodx record");

        let error = install(InstallRequest {
            context: &context,
            manifest: &manifest(Vec::new()),
            reshade_sources: &crate::addons::luma::test_support::reshade_sources(),
            game_id: &game_id,
            safety: game_safety(&context, &game_id),
            progress: None,
        })
        .await
        .expect_err("must refuse while RenoDX is installed");

        assert!(matches!(error, ServiceError::InvalidInput(_)));
        // No Luma record must have been created either.
        assert!(
            records::record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("query")
                .is_none()
        );
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn install_refuses_before_any_network_when_luma_files_are_unmanaged_on_disk() {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:403641").expect("game id");
        let exe_path = game_dir.path().join("Dishonored2.exe");
        write_stub_exe(&exe_path);
        seed_game(&context, &game_id, "403641", game_dir.path(), &exe_path);
        // No DB record for either tool -- just an unmanaged Luma add-on on disk.
        std::fs::write(game_dir.path().join("Luma-Dishonored_2.addon"), b"x")
            .expect("write unmanaged addon");

        let error = install(InstallRequest {
            context: &context,
            manifest: &manifest(Vec::new()),
            reshade_sources: &crate::addons::luma::test_support::reshade_sources(),
            game_id: &game_id,
            safety: game_safety(&context, &game_id),
            progress: None,
        })
        .await
        .expect_err("must refuse over unmanaged Luma files");

        assert!(matches!(error, ServiceError::InvalidInput(_)));
        assert!(
            records::record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("query")
                .is_none(),
            "unmanaged presence must never be silently adopted by install"
        );
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn install_recovers_from_a_torn_install_and_proceeds_past_the_unmanaged_gate() {
        // P1.2: a crash mid-install left tool-owned debris and the crash-safety
        // sentinel behind with no database record. Recovery must run before
        // `ensure_not_unmanaged`, so the call fails later (no matching profile
        // in an empty manifest -- still network-free) rather than being blocked
        // by the debris it just cleaned up.
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:403642").expect("game id");
        let exe_path = game_dir.path().join("Dishonored2.exe");
        write_stub_exe(&exe_path);
        seed_game(&context, &game_id, "403642", game_dir.path(), &exe_path);
        std::fs::write(
            game_dir.path().join("Luma-Dishonored_2.addon"),
            b"half-written",
        )
        .expect("write torn debris");
        std::fs::write(game_dir.path().join("renderpilot-luma-install.lock"), b"")
            .expect("write sentinel");

        let error = install(InstallRequest {
            context: &context,
            manifest: &manifest(Vec::new()),
            reshade_sources: &crate::addons::luma::test_support::reshade_sources(),
            game_id: &game_id,
            safety: game_safety(&context, &game_id),
            progress: None,
        })
        .await
        .expect_err("empty manifest has no matching profile");

        match error {
            ServiceError::InvalidInput(message) => assert!(
                !message.contains("found on disk"),
                "must not still be blocked by the unmanaged-files gate: {message}"
            ),
            other => panic!("expected InvalidInput, got {other:?}"),
        }
        assert!(!game_dir.path().join("Luma-Dishonored_2.addon").exists());
        assert!(!engine::is_install_torn(game_dir.path(), AddonKind::Luma));
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn install_still_refuses_when_renodx_blocks_a_torn_install() {
        // Exclusivity is checked before torn-recovery: another tool's claim on
        // this game must win even while this game's own folder also happens to
        // carry a stale Luma sentinel.
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:403643").expect("game id");
        let exe_path = game_dir.path().join("Dishonored2.exe");
        write_stub_exe(&exe_path);
        seed_game(&context, &game_id, "403643", game_dir.path(), &exe_path);
        std::fs::write(game_dir.path().join("renderpilot-luma-install.lock"), b"")
            .expect("write sentinel");

        let renodx_record = InstalledAddon::new(
            game_id.clone(),
            AddonKind::RenoDx,
            PathRef::new(
                game_dir
                    .path()
                    .join("renodx-test.addon64")
                    .to_string_lossy()
                    .replace('\\', "/"),
            )
            .expect("path"),
        );
        context
            .storage()
            .upsert_installed_addon(&renodx_record)
            .expect("seed renodx record");

        let error = install(InstallRequest {
            context: &context,
            manifest: &manifest(Vec::new()),
            reshade_sources: &crate::addons::luma::test_support::reshade_sources(),
            game_id: &game_id,
            safety: game_safety(&context, &game_id),
            progress: None,
        })
        .await
        .expect_err("must refuse while RenoDX is installed");

        assert!(matches!(error, ServiceError::InvalidInput(_)));
        // The sentinel is untouched -- recovery never ran.
        assert!(engine::is_install_torn(game_dir.path(), AddonKind::Luma));
    }
}
