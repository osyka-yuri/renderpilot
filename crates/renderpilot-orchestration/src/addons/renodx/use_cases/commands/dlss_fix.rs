//! Dedicated DLSS-Fix lifecycle commands.
//!
//! This deliberately does not share the RenoDX add-on/host update path. The
//! companion has its own ownership projection and can be repaired after partial
//! record loss without touching ReShade host or shared Vulkan policy.

use std::path::{Path, PathBuf};

use renderpilot_domain::{
    AddonKind, GameId, InstalledAddon, RenoDxInstallState, TrackedSource, TrackedSourceRole,
};

use crate::addons::game_context::require_game;
use crate::addons::progress::emit_tool_finalizing;
use crate::addons::records;
use crate::addons::renodx::dlss_fix::{DlssFixRequest, resolve_dlss_fix};
use crate::addons::renodx::dlss_fix_binding::{self, DlssFixBinding, DlssFixBindingState};
use crate::addons::renodx::{errors, fetch, reshade_ini, source, tracking};
use crate::addons::reshade::ini_schema::ini_merge_strategy;
use crate::addons::reshade::scan as reshade;
use crate::addons::reshade::types::{DlssFixIniTweaks, ReshadeIniTweaks};
use crate::file_mutation::{
    MutationScope, RetryableFileMutationV2, RetryableFileOperation, RetryableFilePlan,
    V2DiskObservation, observe,
};
use crate::game_mutation_lock;
use crate::net::ProgressObserver;
use crate::{Context, ServiceError};

#[cfg_attr(test, derive(Clone))]
struct DlssSnapshot {
    record: InstalledAddon,
    binding: DlssFixBinding,
    request: Option<DlssFixRequest>,
    ini_path: PathBuf,
    ini_observation: V2DiskObservation,
}

/// Complete projection change to commit alongside a prepared DLSS-Fix durable
/// file plan. The canonical path always comes from the snapshot binding, so a
/// caller cannot separately select an unrelated record path.
struct DlssProjectionCommit {
    intent: DlssProjectionIntent,
    operations: Vec<RetryableFileOperation>,
    feature: &'static str,
    label: &'static str,
}

enum DlssProjectionIntent {
    Bind(TrackedSource),
    Clear,
}

/// Explicitly installs/claims DLSS-Fix. An active row with no evidence never
/// auto-claims a physical file; this command is the affirmative user action
/// allowed to replace the exact regular target or create it when absent.
pub async fn install_dlss_fix(
    context: &Context,
    game_id: &GameId,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<RenoDxInstallState, ServiceError> {
    let snapshot = {
        let _guard =
            game_mutation_lock::enter_game_mutation_boundary_async(context, game_id).await?;
        resolve_snapshot(context, game_id, true)?
    };
    match snapshot.binding.state {
        DlssFixBindingState::Invalid => return Err(invalid_binding()),
        DlssFixBindingState::SourceOnly | DlssFixBindingState::OwnedOnly
            if matches!(
                snapshot.binding.observation,
                V2DiskObservation::Regular { .. }
            ) =>
        {
            let _guard =
                game_mutation_lock::enter_game_mutation_boundary_async(context, game_id).await?;
            let current = resolve_snapshot(context, game_id, true)?;
            ensure_snapshot_matches(&snapshot, &current)?;
            return reconcile_regular_partial(context, game_id, &current, None);
        }
        DlssFixBindingState::Bound => {
            return Err(errors::invalid(
                "DLSS-Fix is already installed; use its update action".to_owned(),
            ));
        }
        DlssFixBindingState::SourceOnly | DlssFixBindingState::OwnedOnly => {
            return Err(errors::invalid(
                "DLSS-Fix is missing; use its repair action".to_owned(),
            ));
        }
        DlssFixBindingState::None => {}
    }

    let arch = snapshot.binding.arch.ok_or_else(invalid_binding)?;
    let download = fetch::fetch_dlss_fix(arch, progress).await?;
    let guard = game_mutation_lock::enter_game_mutation_boundary_async(context, game_id).await?;
    let current = resolve_snapshot(context, game_id, true)?;
    ensure_snapshot_matches(&snapshot, &current)?;
    if current.binding.state != DlssFixBindingState::None {
        return Err(errors::state_changed_retry_update());
    }
    let request = current.request.as_ref().ok_or_else(|| {
        errors::invalid("DLSS-Fix is no longer available for this game".to_owned())
    })?;
    let source = source_from_download(arch, &download);
    let mut operations = vec![RetryableFileOperation::Write {
        path: current.binding.target.clone(),
        bytes: download.bytes,
        expected: current.binding.observation.clone(),
    }];
    operations.extend(install_ini_operation(&current, request)?);
    emit_tool_finalizing(progress, AddonKind::RenoDx);
    commit_projection(
        context,
        &guard,
        game_id,
        &current,
        DlssProjectionCommit {
            intent: DlssProjectionIntent::Bind(source),
            operations,
            feature: renderpilot_domain::mutation_features::RENODX_DLSS_FIX_INSTALL,
            label: "DLSS-Fix install projection",
        },
    )
}

/// Updates the companion or repairs a source/ownership projection. Repair is
/// payload-only: it never rewrites the active ReShade.ini or touches host policy.
pub async fn update_dlss_fix(
    context: &Context,
    game_id: &GameId,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<RenoDxInstallState, ServiceError> {
    let snapshot = {
        let _guard =
            game_mutation_lock::enter_game_mutation_boundary_async(context, game_id).await?;
        resolve_snapshot(context, game_id, false)?
    };
    match snapshot.binding.state {
        DlssFixBindingState::Invalid => return Err(invalid_binding()),
        DlssFixBindingState::None => {
            return Err(errors::invalid(
                "DLSS-Fix is not installed; use its install action".to_owned(),
            ));
        }
        DlssFixBindingState::SourceOnly | DlssFixBindingState::OwnedOnly
            if matches!(
                snapshot.binding.observation,
                V2DiskObservation::Regular { .. }
            ) =>
        {
            let _guard =
                game_mutation_lock::enter_game_mutation_boundary_async(context, game_id).await?;
            let current = resolve_snapshot(context, game_id, false)?;
            ensure_snapshot_matches(&snapshot, &current)?;
            return reconcile_regular_partial(context, game_id, &current, None);
        }
        DlssFixBindingState::SourceOnly
        | DlssFixBindingState::OwnedOnly
        | DlssFixBindingState::Bound => {}
    }

    let arch = snapshot.binding.arch.ok_or_else(invalid_binding)?;
    let download = fetch::fetch_dlss_fix(arch, progress).await?;
    let guard = game_mutation_lock::enter_game_mutation_boundary_async(context, game_id).await?;
    let current = resolve_snapshot(context, game_id, false)?;
    ensure_snapshot_matches(&snapshot, &current)?;
    if current.binding.state == DlssFixBindingState::Invalid {
        return Err(invalid_binding());
    }
    let source = source_from_download(arch, &download);
    // Bound regular updates are normal content refreshes. Missing partial/bound
    // rows recreate the exact target, still without touching the INI.
    let operations = vec![RetryableFileOperation::Write {
        path: current.binding.target.clone(),
        bytes: download.bytes,
        expected: current.binding.observation.clone(),
    }];
    emit_tool_finalizing(progress, AddonKind::RenoDx);
    commit_projection(
        context,
        &guard,
        game_id,
        &current,
        DlssProjectionCommit {
            intent: DlssProjectionIntent::Bind(source),
            operations,
            feature: renderpilot_domain::mutation_features::RENODX_DLSS_FIX_UPDATE,
            label: "DLSS-Fix update projection",
        },
    )
}

/// Retries only pending durable-file recovery for this game, then reports the
/// current RenoDX state. This is deliberately a narrow mutation boundary:
/// it neither fetches a payload nor runs ReShade/host reconciliation policy.
pub fn retry_dlss_fix_recovery(
    context: &Context,
    game_id: &GameId,
) -> Result<RenoDxInstallState, ServiceError> {
    let guard = game_mutation_lock::blocking_lock(game_id);
    let recovered = crate::file_mutation::recover_pending_matching(context, &guard, |row| {
        renderpilot_domain::mutation_features::is_renodx_dlss_fix_feature(&row.feature)
    })?;
    if recovered == 0 {
        return Err(errors::invalid(
            "no pending DLSS-Fix recovery exists for this game".to_owned(),
        ));
    }
    crate::addons::renodx::use_cases::queries::status::status(context, game_id)
}

/// Removes only the companion's exact recorded target and its active game-root
/// INI entries. A source-only row never grants deletion authority over a merely
/// physical exact file, so it clears metadata/INI only.
pub fn uninstall_dlss_fix(
    context: &Context,
    game_id: &GameId,
) -> Result<RenoDxInstallState, ServiceError> {
    let guard = game_mutation_lock::enter_game_mutation_boundary(context, game_id)?;
    let snapshot = resolve_snapshot(context, game_id, false)?;
    if snapshot.binding.state == DlssFixBindingState::Invalid {
        return Err(invalid_binding());
    }
    if snapshot.binding.state == DlssFixBindingState::None {
        return Err(errors::invalid(
            "DLSS-Fix is not installed for this game".to_owned(),
        ));
    }
    let mut operations = remove_ini_operation(&snapshot)?;
    if matches!(
        snapshot.binding.state,
        DlssFixBindingState::OwnedOnly | DlssFixBindingState::Bound
    ) && matches!(
        snapshot.binding.observation,
        V2DiskObservation::Regular { .. }
    ) {
        operations.insert(
            0,
            RetryableFileOperation::Delete {
                path: snapshot.binding.target.clone(),
                expected: snapshot.binding.observation.clone(),
            },
        );
    }
    commit_projection(
        context,
        &guard,
        game_id,
        &snapshot,
        DlssProjectionCommit {
            intent: DlssProjectionIntent::Clear,
            operations,
            feature: renderpilot_domain::mutation_features::RENODX_DLSS_FIX_UNINSTALL,
            label: "DLSS-Fix removal projection",
        },
    )
}

fn resolve_snapshot(
    context: &Context,
    game_id: &GameId,
    need_request: bool,
) -> Result<DlssSnapshot, ServiceError> {
    let record = records::record_of_kind(context, game_id, AddonKind::RenoDx)?
        .ok_or_else(errors::not_installed)?;
    let binding = dlss_fix_binding::resolve(&record);
    let game = require_game(context, game_id)?;
    let game_root = PathBuf::from(game.install_path().as_str());
    let host_path = crate::addons::tracking::host_proxy_path(&record);
    let paths = reshade::resolve_paths(&game_root, host_path.as_deref());
    let ini_path = paths
        .ini_path
        .unwrap_or_else(|| game_root.join(reshade::RESHADE_INI_FILE_NAME));
    let request = need_request
        .then(|| resolve_dlss_fix(context.storage(), game_id))
        .transpose()?
        .flatten();
    Ok(DlssSnapshot {
        record,
        binding,
        request,
        ini_observation: observe(&ini_path),
        ini_path,
    })
}

fn ensure_snapshot_matches(
    before: &DlssSnapshot,
    current: &DlssSnapshot,
) -> Result<(), ServiceError> {
    if before.record != current.record
        || before.binding.state != current.binding.state
        || before.binding.target != current.binding.target
        || before.binding.observation != current.binding.observation
        || before.ini_path != current.ini_path
        || before.ini_observation != current.ini_observation
        || before.request != current.request
    {
        return Err(errors::state_changed_retry_update());
    }
    Ok(())
}

fn reconcile_regular_partial(
    context: &Context,
    game_id: &GameId,
    snapshot: &DlssSnapshot,
    mutation_id: Option<&str>,
) -> Result<RenoDxInstallState, ServiceError> {
    let source = match snapshot.binding.state {
        DlssFixBindingState::SourceOnly => snapshot.binding.source.clone(),
        DlssFixBindingState::OwnedOnly => Some(advisory_source_from_live(&snapshot.binding)?),
        _ => {
            return Err(errors::invalid(
                "DLSS-Fix projection is not partial".to_owned(),
            ));
        }
    };
    let updated = tracking::rebuild_with_dlss_projection(
        &snapshot.record,
        Some(&snapshot.binding.target),
        source,
        "DLSS-Fix partial reconciliation",
    )?;
    persist_projection(context, game_id, &updated, mutation_id)?;
    Ok(tracking::install_state_from_record(&updated))
}

fn advisory_source_from_live(binding: &DlssFixBinding) -> Result<TrackedSource, ServiceError> {
    let arch = binding.arch.ok_or_else(invalid_binding)?;
    let V2DiskObservation::Regular { digest } = &binding.observation else {
        return Err(errors::invalid(
            "DLSS-Fix companion is not a readable regular file".to_owned(),
        ));
    };
    Ok(TrackedSource::new(
        TrackedSourceRole::DlssFix,
        source::dlss_fix_url(arch),
        None,
        digest.clone(),
    )
    .with_advisory())
}

fn source_from_download(
    arch: renderpilot_domain::Architecture,
    download: &crate::addons::reshade::fetch::Download,
) -> TrackedSource {
    TrackedSource::new(
        TrackedSourceRole::DlssFix,
        source::dlss_fix_url(arch),
        download.etag.clone(),
        download.digest.clone(),
    )
    .with_last_modified(download.last_modified.clone())
}

fn install_ini_operation(
    snapshot: &DlssSnapshot,
    request: &DlssFixRequest,
) -> Result<Vec<RetryableFileOperation>, ServiceError> {
    let strategy = ini_merge_strategy(&ReshadeIniTweaks {
        disabled_addons: Vec::new(),
        addon_path: None,
        dlss_fix: Some(DlssFixIniTweaks {
            addon_file_name: snapshot
                .binding
                .target
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(invalid_binding)?
                .to_owned(),
            dlss_path: request.dlss_path.clone(),
            streamline_path: request.streamline_path.clone(),
        }),
    });
    ini_write_operation(&snapshot.ini_path, &snapshot.ini_observation, &strategy)
}

fn remove_ini_operation(
    snapshot: &DlssSnapshot,
) -> Result<Vec<RetryableFileOperation>, ServiceError> {
    ini_write_operation(
        &snapshot.ini_path,
        &snapshot.ini_observation,
        &reshade_ini::ini_remove_dlss_fix_strategy(),
    )
}

fn ini_write_operation(
    path: &Path,
    expected: &V2DiskObservation,
    strategy: &crate::addons::engine::MergeStrategy,
) -> Result<Vec<RetryableFileOperation>, ServiceError> {
    let base = match expected {
        V2DiskObservation::Absent => String::new(),
        V2DiskObservation::Regular { .. } => {
            String::from_utf8(std::fs::read(path).map_err(|error| {
                crate::failed(format!(
                    "failed to read active ReShade.ini {}: {error}",
                    path.display()
                ))
            })?)
            .map_err(|_| {
                errors::invalid(format!(
                    "active ReShade.ini is not UTF-8: {}",
                    path.display()
                ))
            })?
        }
        V2DiskObservation::NonRegular | V2DiskObservation::Unreadable => {
            return Err(errors::invalid(format!(
                "active ReShade.ini is unsafe to modify: {}",
                path.display()
            )));
        }
    };
    let next = strategy.apply(&base);
    if next == base {
        return Ok(Vec::new());
    }
    Ok(vec![RetryableFileOperation::Write {
        path: path.to_path_buf(),
        bytes: next.into_bytes(),
        expected: expected.clone(),
    }])
}

fn commit_projection(
    context: &Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    game_id: &GameId,
    snapshot: &DlssSnapshot,
    commit: DlssProjectionCommit,
) -> Result<RenoDxInstallState, ServiceError> {
    let updated = match commit.intent {
        DlssProjectionIntent::Bind(source) => tracking::rebuild_with_dlss_projection(
            &snapshot.record,
            Some(&snapshot.binding.target),
            Some(source),
            commit.label,
        )?,
        DlssProjectionIntent::Clear => {
            tracking::rebuild_with_dlss_projection(&snapshot.record, None, None, commit.label)?
        }
    };
    if commit.operations.is_empty() {
        persist_projection(context, game_id, &updated, None)?;
        return Ok(tracking::install_state_from_record(&updated));
    }
    let roots = commit
        .operations
        .iter()
        .filter_map(|operation| operation.path().parent().map(Path::to_path_buf));
    let scope = MutationScope::new(roots)?;
    let mutation = RetryableFileMutationV2::prepare(
        context,
        guard,
        &scope,
        commit.feature,
        Some(game_id.as_str()),
        &RetryableFilePlan {
            operations: commit.operations,
        },
    )?;
    mutation.commit_or_rollback(context, |mutation_id| {
        persist_projection(context, game_id, &updated, Some(mutation_id))?;
        Ok(tracking::install_state_from_record(&updated))
    })
}

fn persist_projection(
    context: &Context,
    game_id: &GameId,
    record: &InstalledAddon,
    mutation_id: Option<&str>,
) -> Result<(), ServiceError> {
    context
        .storage()
        .commit_game_mutation(renderpilot_storage_sqlite::GameMutationCommit {
            game_id,
            component_set: None,
            baseline_mutations: &[],
            addon: renderpilot_storage_sqlite::InstalledAddonMutation::Upsert(record),
            mutation_id,
        })?;
    Ok(())
}

fn invalid_binding() -> ServiceError {
    errors::invalid(
        "DLSS-Fix record or disk binding requires validation before targeted mutation".to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use renderpilot_application::InstalledAddonRepository;
    use renderpilot_domain::PathRef;
    use renderpilot_storage_sqlite::BeginFileMutationPreparation;
    use tempfile::tempdir;

    use super::*;

    fn seed_preparing_v2_row(
        context: &Context,
        game_id: &GameId,
        id: &str,
        feature: &str,
        root: &Path,
    ) -> PathBuf {
        let transaction_dir = context.file_mutation_root().join(id);
        fs::create_dir_all(&transaction_dir).expect("transaction dir");
        let manifest = serde_json::json!({
            "format_version": 2,
            "roots": [root.to_string_lossy().into_owned()],
            "transaction_dir": transaction_dir.to_string_lossy().into_owned(),
            "operations": [],
            "snapshots": [],
        })
        .to_string();
        context
            .storage()
            .begin_file_mutation_preparation(&BeginFileMutationPreparation {
                id: id.to_owned(),
                game_id: game_id.clone(),
                feature: feature.to_owned(),
                subject_id: Some(game_id.as_str().to_owned()),
                initial_manifest_json: manifest,
            })
            .expect("preparing v2 row");
        transaction_dir
    }

    fn snapshot(addon_dir: &Path, game_root: &Path) -> DlssSnapshot {
        let record = InstalledAddon::new(
            GameId::new("manual:dlss-snapshot").expect("game id"),
            AddonKind::RenoDx,
            PathRef::new(
                addon_dir
                    .join("renodx-game.addon64")
                    .to_string_lossy()
                    .into_owned(),
            )
            .expect("add-on path"),
        );
        let binding = dlss_fix_binding::resolve(&record);
        let ini_path = game_root.join(reshade::RESHADE_INI_FILE_NAME);
        DlssSnapshot {
            record,
            binding,
            request: None,
            ini_observation: observe(&ini_path),
            ini_path,
        }
    }

    #[test]
    fn phase_three_revalidation_rejects_record_target_and_ini_drift() {
        let addon_dir = tempdir().expect("add-on dir");
        let game_root = tempdir().expect("game root");
        let before = snapshot(addon_dir.path(), game_root.path());

        let mut record_changed = before.clone();
        record_changed.record = record_changed.record.with_addon_version("foreign");
        assert!(ensure_snapshot_matches(&before, &record_changed).is_err());

        let mut target_changed = before.clone();
        target_changed.binding.observation = V2DiskObservation::Regular {
            digest: "foreign".to_owned(),
        };
        assert!(ensure_snapshot_matches(&before, &target_changed).is_err());

        let mut ini_changed = before.clone();
        ini_changed.ini_observation = V2DiskObservation::Regular {
            digest: "foreign".to_owned(),
        };
        assert!(ensure_snapshot_matches(&before, &ini_changed).is_err());
    }

    #[test]
    fn install_ini_targets_the_active_game_root_not_the_addon_parent() {
        let addon_dir = tempdir().expect("split add-on dir");
        let game_root = tempdir().expect("game root");
        let active_ini = game_root.path().join(reshade::RESHADE_INI_FILE_NAME);
        fs::write(&active_ini, "[ADDON]\nDisabledAddons=Example\n").expect("active ini");
        let snapshot = snapshot(addon_dir.path(), game_root.path());
        let operations = install_ini_operation(
            &snapshot,
            &DlssFixRequest {
                dlss_path: r"C:\\Game\\nvngx_dlss.dll".to_owned(),
                streamline_path: r"C:\\Game\\sl.interposer.dll".to_owned(),
            },
        )
        .expect("ini operation");

        let [RetryableFileOperation::Write { path, .. }] = operations.as_slice() else {
            panic!("expected exactly one active INI write")
        };
        assert_eq!(path, &active_ini);
        assert!(
            !addon_dir
                .path()
                .join(reshade::RESHADE_INI_FILE_NAME)
                .exists()
        );
    }

    #[test]
    fn retry_recovery_cleans_pending_v2_without_host_or_network_work() {
        let db_root = tempdir().expect("db root");
        let game_root = tempdir().expect("game root");
        let context = Context::open_at(db_root.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("manual:dlss-retry-recovery").expect("game id");
        let addon = game_root.path().join("renodx-game.addon64");
        fs::write(&addon, b"addon").expect("addon");
        let record = InstalledAddon::new(
            game_id.clone(),
            AddonKind::RenoDx,
            PathRef::new(addon.to_string_lossy().into_owned()).expect("add-on path"),
        );
        context
            .storage()
            .upsert_installed_addon(&record)
            .expect("record");

        let guard = game_mutation_lock::try_lock(&game_id).expect("test lock");
        let target = game_root.path().join("renodx-dlssfix.addon64");
        let scope = MutationScope::new([game_root.path().to_path_buf()]).expect("scope");
        let _pending = RetryableFileMutationV2::prepare(
            &context,
            &guard,
            &scope,
            renderpilot_domain::mutation_features::RENODX_DLSS_FIX_INSTALL,
            Some(game_id.as_str()),
            &RetryableFilePlan {
                operations: vec![RetryableFileOperation::Write {
                    path: target.clone(),
                    bytes: b"payload".to_vec(),
                    expected: V2DiskObservation::Absent,
                }],
            },
        )
        .expect("prepared v2 row");
        drop(guard);

        let state = retry_dlss_fix_recovery(&context, &game_id).expect("recovery retry");
        assert!(matches!(state, RenoDxInstallState::Installed { .. }));
        assert!(
            context
                .storage()
                .pending_file_mutations_for_game(&game_id)
                .expect("pending rows")
                .is_empty()
        );
        assert!(
            !target.exists(),
            "recovery must not apply the pending payload"
        );
    }

    #[test]
    fn retry_recovery_rejects_an_unrelated_pending_mutation() {
        let db_root = tempdir().expect("db root");
        let game_root = tempdir().expect("game root");
        let context = Context::open_at(db_root.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("manual:dlss-retry-unrelated").expect("game id");
        let guard = game_mutation_lock::try_lock(&game_id).expect("test lock");
        let scope = MutationScope::new([game_root.path().to_path_buf()]).expect("scope");
        let _pending = RetryableFileMutationV2::prepare(
            &context,
            &guard,
            &scope,
            renderpilot_domain::mutation_features::RENODX_UPDATE,
            Some(game_id.as_str()),
            &RetryableFilePlan {
                operations: vec![RetryableFileOperation::Write {
                    path: game_root.path().join("pending-generic.addon64"),
                    bytes: b"payload".to_vec(),
                    expected: V2DiskObservation::Absent,
                }],
            },
        )
        .expect("prepared generic row");
        drop(guard);

        assert!(retry_dlss_fix_recovery(&context, &game_id).is_err());
        assert_eq!(
            context
                .storage()
                .pending_file_mutations_for_game(&game_id)
                .expect("pending rows")
                .len(),
            1,
            "an unrelated recovery row must remain untouched"
        );
    }

    #[test]
    fn retry_recovery_handles_only_dlss_rows_when_unrelated_work_is_pending() {
        let db_root = tempdir().expect("db root");
        let game_root = tempdir().expect("game root");
        let context = Context::open_at(db_root.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("manual:dlss-retry-mixed").expect("game id");
        let dlss_id = "dlss-retry-mixed-exact";
        let unrelated_id = "dlss-retry-mixed-unrelated";
        let dlss_dir = seed_preparing_v2_row(
            &context,
            &game_id,
            dlss_id,
            renderpilot_domain::mutation_features::RENODX_DLSS_FIX_UPDATE,
            game_root.path(),
        );
        let unrelated_dir = seed_preparing_v2_row(
            &context,
            &game_id,
            unrelated_id,
            renderpilot_domain::mutation_features::RENODX_UPDATE,
            game_root.path(),
        );
        let unrelated_orphan_dir = context.file_mutation_root().join("unrelated-orphan");
        fs::create_dir_all(&unrelated_orphan_dir).expect("unrelated orphan dir");

        let state = retry_dlss_fix_recovery(&context, &game_id).expect("DLSS recovery retry");
        assert!(matches!(state, RenoDxInstallState::NotInstalled));
        let rows = context
            .storage()
            .pending_file_mutations_for_game(&game_id)
            .expect("pending rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, unrelated_id);
        assert_eq!(
            rows[0].feature,
            renderpilot_domain::mutation_features::RENODX_UPDATE
        );
        assert!(!dlss_dir.exists(), "selected recovery must be cleaned");
        assert!(
            unrelated_dir.exists(),
            "unrelated recovery artifacts must remain untouched"
        );
        assert!(
            unrelated_orphan_dir.exists(),
            "feature-scoped recovery must not run the global orphan sweep"
        );
    }
}
