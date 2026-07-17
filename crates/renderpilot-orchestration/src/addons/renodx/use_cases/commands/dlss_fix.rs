//! Commands for the DLSS-Fix companion RenoDX add-on.
//!
//! Install uses the same 3-phase contract as RenoDX/Luma update: snapshot under
//! the game lock, network fetch unlocked, revalidate + durable apply under lock.

use std::path::{Path, PathBuf};

use renderpilot_domain::{
    AddonKind, Architecture, GameId, InstalledAddon, RenoDxInstallState, TrackedSource,
    TrackedSourceRole,
};

use crate::addons::engine::{self, FileOp, InstallPlan, InstallReceipt};
use crate::addons::file_update::{OriginalFile, restore_originals_best_effort};
use crate::addons::progress::emit_tool_finalizing;
use crate::addons::records;
use crate::addons::renodx::arch_from_addon_file;
use crate::addons::renodx::dlss_fix::{DlssFixRequest, resolve_dlss_fix};
use crate::addons::renodx::errors;
use crate::addons::renodx::fetch;
use crate::addons::renodx::install::{dlss_fix_file_name, dlss_fix_file_path};
use crate::addons::renodx::reshade_ini;
use crate::addons::renodx::source;
use crate::addons::renodx::tracking;
use crate::addons::reshade::ini_schema::ini_merge_strategy;
use crate::addons::reshade::scan as reshade;
use crate::addons::reshade::types::{DlssFixIniTweaks, ReshadeIniTweaks};
use crate::game_mutation_lock;
use crate::net::ProgressObserver;
use crate::{Context, ServiceError};

struct DlssFixInstallSnapshot {
    record: InstalledAddon,
    request: DlssFixRequest,
    game_dir: PathBuf,
    arch: Architecture,
    file_name: String,
}

/// Installs the DLSS-Fix companion add-on for a game that already has RenoDX.
pub async fn install_dlss_fix(
    context: &Context,
    game_id: &GameId,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<RenoDxInstallState, ServiceError> {
    // Phase 1: snapshot under the per-game lock, then release for network work.
    let snapshot = {
        let _guard =
            game_mutation_lock::enter_game_mutation_boundary_async(context, game_id).await?;
        resolve_dlss_fix_install_snapshot(context, game_id)?
    };

    // Phase 2: download only — no game-folder mutation.
    let download = fetch::fetch_dlss_fix(snapshot.arch, progress).await?;
    let download_last_modified = download.last_modified.clone();

    // Phase 3: re-lock, revalidate, durable apply.
    let guard = game_mutation_lock::enter_game_mutation_boundary_async(context, game_id).await?;
    let current = resolve_dlss_fix_install_snapshot(context, game_id)?;
    ensure_dlss_fix_install_snapshot_matches(&snapshot, &current)?;

    let ini_tweaks = ReshadeIniTweaks {
        disabled_addons: Vec::new(),
        addon_path: None,
        dlss_fix: Some(DlssFixIniTweaks {
            addon_file_name: current.file_name.clone(),
            dlss_path: current.request.dlss_path.clone(),
            streamline_path: current.request.streamline_path.clone(),
        }),
    };
    let strategy = ini_merge_strategy(&ini_tweaks);

    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![
            FileOp::Replace {
                name: current.file_name.clone(),
                bytes: download.bytes,
            },
            FileOp::UpdateText {
                name: reshade::RESHADE_INI_FILE_NAME.to_owned(),
                default: String::new(),
                strategy,
            },
        ],
    };

    // The `UpdateText` merge into `ReShade.ini` (an existing file — the main
    // RenoDX install already created one) never appears in the engine's own
    // receipt (see `FileOp::UpdateText`'s docs), so it isn't something
    // `engine::uninstall` can undo after the fact. Capture its pre-merge bytes
    // ourselves so a failure further down (DB rebuild/persist) can restore both
    // the ini and the fresh add-on file, not just the latter.
    let ini_path = reshade::reshade_ini_path(&current.game_dir)
        .unwrap_or_else(|| current.game_dir.join(reshade::RESHADE_INI_FILE_NAME));
    let ini_original = OriginalFile {
        path: ini_path.clone(),
        bytes: ini_path
            .is_file()
            .then(|| crate::fs::read_file(&ini_path))
            .transpose()?,
    };

    let dlss_fix_source = TrackedSource::new(
        TrackedSourceRole::DlssFix,
        source::dlss_fix_url(current.arch),
        download.etag,
        download.digest,
    )
    .with_last_modified(download.last_modified);

    let targets = crate::addons::renodx::mutation_targets::dlss_fix_targets(
        &current.game_dir,
        &current.file_name,
    );

    emit_tool_finalizing(progress, AddonKind::RenoDx);
    crate::addons::durable::run_targets_mutation(
        crate::addons::durable::TargetsMutation {
            context,
            guard: &guard,
            targets,
            feature: crate::addons::mutation_features::RENODX_DLSS_FIX_INSTALL,
            game_id,
        },
        |mutation_id| -> Result<RenoDxInstallState, ServiceError> {
            let receipt = engine::install(&current.game_dir, &plan)?;
            crate::fs::stamp_mtime_best_effort(
                &current.game_dir.join(&current.file_name),
                download_last_modified.as_deref(),
                None,
            );
            let updated = match tracking::rebuild_after_receipt(
                &current.record,
                &receipt,
                None,
                Some(dlss_fix_source),
                "DLSS-Fix rebuild",
            ) {
                Ok(updated) => updated,
                Err(error) => {
                    restore_dlss_fix_install_best_effort(&receipt, &ini_original);
                    return Err(error);
                }
            };
            context.storage().commit_game_mutation(
                renderpilot_storage_sqlite::GameMutationCommit {
                    game_id,
                    component_set: None,
                    baseline_inserts: &[],
                    baseline_deletes: &[],
                    addon: renderpilot_storage_sqlite::InstalledAddonMutation::Upsert(&updated),
                    mutation_id: Some(mutation_id),
                },
            )?;
            Ok(tracking::install_state_from_record(&updated))
        },
        |_| {},
        || {},
    )
}

fn resolve_dlss_fix_install_snapshot(
    context: &Context,
    game_id: &GameId,
) -> Result<DlssFixInstallSnapshot, ServiceError> {
    let record = records::record_of_kind(context, game_id, AddonKind::RenoDx)?
        .ok_or_else(errors::not_installed)?;

    if record.has_dlss_fix() {
        return Err(errors::invalid(
            "DLSS-Fix is already installed for this game".to_owned(),
        ));
    }

    let request = resolve_dlss_fix(context.storage(), game_id)?.ok_or_else(|| {
        errors::invalid(
            "this game does not have NVIDIA Frame Generation + DLSS + Streamline; \
             DLSS-Fix is not available"
                .to_owned(),
        )
    })?;

    let game_dir = Path::new(record.addon_file().as_str())
        .parent()
        .ok_or_else(|| errors::invalid("installed add-on has no parent directory".to_owned()))?
        .to_path_buf();

    let arch = arch_from_addon_file(record.addon_file().as_str()).ok_or_else(|| {
        errors::invalid("cannot determine architecture from add-on file name".to_owned())
    })?;
    let file_name = dlss_fix_file_name(arch);

    Ok(DlssFixInstallSnapshot {
        record,
        request,
        game_dir,
        arch,
        file_name,
    })
}

fn ensure_dlss_fix_install_snapshot_matches(
    snapshot: &DlssFixInstallSnapshot,
    current: &DlssFixInstallSnapshot,
) -> Result<(), ServiceError> {
    if snapshot.record != current.record
        || snapshot.request != current.request
        || snapshot.game_dir != current.game_dir
        || snapshot.arch != current.arch
        || snapshot.file_name != current.file_name
    {
        return Err(errors::state_changed_retry_update());
    }
    Ok(())
}

/// Reverses a DLSS-Fix install that failed after its files were already
/// written: deletes the fresh add-on file (via the engine's own generic
/// reversal — safe here since `receipt.created_files` holds nothing but that
/// one file, freshly written with no backup) and restores `ReShade.ini` to its
/// pre-merge bytes (captured before the merge ran, since the merge itself never
/// entered the receipt). Best-effort: this only runs once something has
/// already failed, so a further failure here is logged, not layered into a new
/// error.
fn restore_dlss_fix_install_best_effort(receipt: &InstallReceipt, ini_original: &OriginalFile) {
    if let Err(error) = engine::uninstall(&receipt.created_files, &receipt.backed_up_files) {
        log::warn!("DLSS-Fix install rollback failed to remove its add-on file: {error}");
    }
    restore_originals_best_effort(std::slice::from_ref(ini_original));
}

/// Removes the DLSS-Fix companion add-on, leaving the main RenoDX install intact.
///
/// Deletes the `renodx-dlssfix.addon*` file and merges `ReShade.ini` to remove
/// `LoadFromDllMain` from `[ADDON]` and the entire `[RENODX-DLSSFIX]` section.
pub fn uninstall_dlss_fix(
    context: &Context,
    game_id: &GameId,
) -> Result<RenoDxInstallState, ServiceError> {
    let guard = game_mutation_lock::enter_game_mutation_boundary(context, game_id)?;
    let record = records::record_of_kind(context, game_id, AddonKind::RenoDx)?
        .ok_or_else(errors::not_installed)?;

    if !record.has_dlss_fix() {
        return Err(errors::invalid(
            "DLSS-Fix is not installed for this game".to_owned(),
        ));
    }

    let dll_path = dlss_fix_file_path(&record)
        .ok_or_else(|| errors::invalid("DLSS-Fix file not found in install record".to_owned()))?;
    let game_dir = dll_path
        .parent()
        .ok_or_else(|| errors::invalid("dlss-fix path has no parent".to_owned()))?;
    let file_name = dll_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| errors::invalid("dlss-fix path has no file name".to_owned()))?
        .to_owned();

    let strategy = reshade_ini::ini_remove_dlss_fix_strategy();
    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![
            FileOp::Remove {
                name: dll_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            },
            FileOp::UpdateText {
                name: reshade::RESHADE_INI_FILE_NAME.to_owned(),
                default: String::new(),
                strategy,
            },
        ],
    };

    let targets = crate::addons::renodx::mutation_targets::dlss_fix_targets(game_dir, &file_name);

    crate::addons::durable::run_targets_mutation(
        crate::addons::durable::TargetsMutation {
            context,
            guard: &guard,
            targets,
            feature: crate::addons::mutation_features::RENODX_DLSS_FIX_UNINSTALL,
            game_id,
        },
        |mutation_id| -> Result<RenoDxInstallState, ServiceError> {
            let receipt = engine::install(game_dir, &plan)?;
            let updated = tracking::rebuild_after_receipt(
                &record,
                &receipt,
                Some((&dll_path, TrackedSourceRole::DlssFix)),
                None,
                "DLSS-Fix rebuild",
            )?;
            context.storage().commit_game_mutation(
                renderpilot_storage_sqlite::GameMutationCommit {
                    game_id,
                    component_set: None,
                    baseline_inserts: &[],
                    baseline_deletes: &[],
                    addon: renderpilot_storage_sqlite::InstalledAddonMutation::Upsert(&updated),
                    mutation_id: Some(mutation_id),
                },
            )?;
            Ok(tracking::install_state_from_record(&updated))
        },
        |_| {},
        || {},
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderpilot_domain::Architecture;
    use tempfile::tempdir;

    fn read(path: &Path) -> Vec<u8> {
        std::fs::read(path).expect("file should exist")
    }

    #[test]
    fn restore_dlss_fix_install_best_effort_reverts_the_file_and_the_ini_merge() {
        let dir = tempdir().expect("tempdir");
        let game_dir = dir.path();
        // The main RenoDX install's existing ini, as it was before DLSS-Fix.
        let original_ini = "[GENERAL]\r\nPreset=mine.ini\r\n\r\n\
             [ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\n";
        std::fs::write(game_dir.join("ReShade.ini"), original_ini).expect("write ini");

        let file_name = dlss_fix_file_name(Architecture::X64);
        let ini_tweaks = ReshadeIniTweaks {
            disabled_addons: Vec::new(),
            addon_path: None,
            dlss_fix: Some(DlssFixIniTweaks {
                addon_file_name: file_name.clone(),
                dlss_path: r"C:\Game\nvngx_dlss.dll".to_owned(),
                streamline_path: r"C:\Game\sl.interposer.dll".to_owned(),
            }),
        };
        let strategy = ini_merge_strategy(&ini_tweaks);

        // Captured before the merge runs, exactly as `install_dlss_fix` does.
        let ini_path = reshade::reshade_ini_path(game_dir).expect("ini exists");
        let ini_original = OriginalFile {
            path: ini_path.clone(),
            bytes: Some(read(&ini_path)),
        };

        let plan = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![
                FileOp::Replace {
                    name: file_name.clone(),
                    bytes: b"dlssfix-bytes".to_vec(),
                },
                FileOp::UpdateText {
                    name: reshade::RESHADE_INI_FILE_NAME.to_owned(),
                    default: String::new(),
                    strategy,
                },
            ],
        };
        let receipt = engine::install(game_dir, &plan).expect("install");
        // Sanity: the merge actually happened and the file actually landed —
        // otherwise this test would trivially pass without exercising anything.
        assert!(game_dir.join(&file_name).is_file());
        assert!(
            String::from_utf8(read(&ini_path))
                .unwrap()
                .contains("LoadFromDllMain")
        );

        restore_dlss_fix_install_best_effort(&receipt, &ini_original);

        assert!(!game_dir.join(&file_name).exists());
        assert_eq!(String::from_utf8(read(&ini_path)).unwrap(), original_ini);
    }
}
