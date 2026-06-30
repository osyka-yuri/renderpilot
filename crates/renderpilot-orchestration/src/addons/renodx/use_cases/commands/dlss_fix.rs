//! Commands for the DLSS-Fix companion RenoDX add-on.

use std::path::Path;

use renderpilot_application::InstalledAddonRepository;
use renderpilot_domain::{AddonKind, GameId, RenoDxInstallState, TrackedSource, TrackedSourceRole};

use crate::addons::engine::{self, FileOp, InstallPlan};
use crate::addons::renodx::arch_from_addon_file;
use crate::addons::renodx::dlss_fix::resolve_dlss_fix;
use crate::addons::renodx::errors;
use crate::addons::renodx::fetch;
use crate::addons::renodx::install::{dlss_fix_file_name, dlss_fix_file_path};
use crate::addons::renodx::operation_lock;
use crate::addons::renodx::progress::emit_finalizing;
use crate::addons::renodx::reshade;
use crate::addons::renodx::reshade_ini;
use crate::addons::renodx::source;
use crate::addons::renodx::tracking;
use crate::addons::renodx::types::{DlssFixIniTweaks, ReshadeIniTweaks};
use crate::net::ProgressObserver;
use crate::{Context, ServiceError};

/// Installs the DLSS-Fix companion add-on for a game that already has RenoDX.
pub async fn install_dlss_fix(
    context: &Context,
    game_id: &GameId,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<RenoDxInstallState, ServiceError> {
    let _guard = operation_lock::lock(game_id).await;
    let record = context
        .storage()
        .get_installed_addon(game_id)?
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
        .ok_or_else(|| errors::invalid("installed add-on has no parent directory".to_owned()))?;

    let arch = arch_from_addon_file(record.addon_file().as_str()).ok_or_else(|| {
        errors::invalid("cannot determine architecture from add-on file name".to_owned())
    })?;
    let file_name = dlss_fix_file_name(arch);

    let download = fetch::fetch_dlss_fix(arch, progress).await?;
    let download_last_modified = download.last_modified.clone();

    let ini_tweaks = ReshadeIniTweaks {
        disabled_addons: Vec::new(),
        addon_path: None,
        dlss_fix: Some(DlssFixIniTweaks {
            addon_file_name: file_name.clone(),
            dlss_path: request.dlss_path,
            streamline_path: request.streamline_path,
        }),
    };
    let strategy = reshade_ini::ini_merge_strategy(&ini_tweaks);

    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![
            FileOp::Create {
                name: file_name.clone(),
                bytes: download.bytes,
            },
            FileOp::UpdateText {
                name: reshade::RESHADE_INI_FILE_NAME.to_owned(),
                default: String::new(),
                strategy,
            },
        ],
    };

    emit_finalizing(progress);
    let receipt = engine::install(game_dir, &plan)?;
    crate::fs::stamp_mtime_best_effort(
        &game_dir.join(&file_name),
        download_last_modified.as_deref(),
        None,
    );

    let source = TrackedSource::new(
        TrackedSourceRole::DlssFix,
        source::dlss_fix_url(arch),
        download.etag,
        download.digest,
    )
    .with_last_modified(download.last_modified);
    let updated =
        tracking::rebuild_after_receipt(&record, &receipt, None, Some(source), "DLSS-Fix rebuild")?;
    context.storage().upsert_installed_addon(&updated)?;

    Ok(tracking::install_state_from_record(&updated))
}

/// Removes the DLSS-Fix companion add-on, leaving the main RenoDX install intact.
///
/// Deletes the `renodx-dlssfix.addon*` file and merges `ReShade.ini` to remove
/// `LoadFromDllMain` from `[ADDON]` and the entire `[RENODX-DLSSFIX]` section.
pub fn uninstall_dlss_fix(
    context: &Context,
    game_id: &GameId,
) -> Result<RenoDxInstallState, ServiceError> {
    let _guard = operation_lock::blocking_lock(game_id);
    let record = context
        .storage()
        .get_installed_addon(game_id)?
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

    let receipt = engine::install(game_dir, &plan)?;

    let updated = tracking::rebuild_after_receipt(
        &record,
        &receipt,
        Some((&dll_path, TrackedSourceRole::DlssFix)),
        None,
        "DLSS-Fix rebuild",
    )?;
    context.storage().upsert_installed_addon(&updated)?;

    Ok(tracking::install_state_from_record(&updated))
}
