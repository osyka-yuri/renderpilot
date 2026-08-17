//! Write and validate NVAPI setting overrides.

use renderpilot_nvapi::setting::{CatalogReadiness, NvapiSetting, SettingContext};

use super::assemble::known_preset_set;
use super::live::read_pre_state;
use super::session::{map_nvapi_write_error, open_drs_session, warning_to_service_error};
use super::target::{SettingTarget, WriteOp};
use crate::ServiceError;
use crate::dlss::preset_manifest::{bundled_manifest, supported_presets_for};

/// Resolves the [`WriteOp`] for a revert request against the recorded baseline.
///
/// `target` is `"predefined"` (always delete the override, restoring the driver
/// default) or `"baseline"` (restore the first value seen for this setting,
/// which may itself have been the predefined default).
pub fn resolve_revert_op(
    context: &crate::Context,
    target: &SettingTarget<'_>,
    setting: &dyn NvapiSetting,
    revert_target: &str,
) -> Result<WriteOp, ServiceError> {
    match revert_target {
        "predefined" => Ok(WriteOp::Delete),
        "baseline" => {
            let game_id = target.game_id().ok_or_else(|| {
                ServiceError::command_failed("baseline revert is not available for global settings")
            })?;
            let baseline = context
                .storage()
                .get_nvapi_baseline(game_id, setting.key())?
                .ok_or_else(|| {
                    ServiceError::command_failed(
                        "no baseline recorded yet -- set a value at least once first",
                    )
                })?;
            if baseline.baseline_was_predefined {
                Ok(WriteOp::Delete)
            } else {
                Ok(WriteOp::Set(baseline.baseline_dword))
            }
        }
        other => Err(ServiceError::command_failed(format!(
            "invalid revert target `{other}`; expected 'predefined' or 'baseline'"
        ))),
    }
}

/// Validates that `dword` is an allowed value for `setting` given the current DLL version.
///
/// Returns `Err` only when the preset manifest explicitly manages this value
/// and marks it as unsupported for the detected DLL version.
pub fn validate_value_supported(
    setting: &dyn NvapiSetting,
    dword: u32,
    ctx: &SettingContext,
) -> Result<(), ServiceError> {
    ensure_dll_setting_catalog_ready(setting, ctx)?;
    let Some(kind) = setting.dll_kind() else {
        return Ok(());
    };
    let Some(info) = ctx.dlls.get(&kind) else {
        return Ok(());
    };
    let Some(version) = info.version else {
        return Ok(());
    };

    let supported = supported_presets_for(bundled_manifest(kind), &version);
    if supported.is_empty() {
        return Ok(());
    }
    // The manifest only constrains the presets it explicitly lists. Values it
    // does not manage -- the "recommended" sentinel, or a preset letter beyond
    // its table -- are always allowed.
    if known_preset_set(setting, ctx).contains(&dword) && !supported.contains(&dword) {
        return Err(ServiceError::command_failed(format!(
            "value `{}` is not supported for DLL version {} (kind={:?})",
            setting
                .format_wire(dword)
                .unwrap_or_else(|| dword.to_string()),
            version,
            kind
        )));
    }
    Ok(())
}

/// Writes a new value (or deletes the override) for `setting` in the game's NVIDIA driver profile.
///
/// Also captures a baseline snapshot the first time this setting is modified.
pub fn write_setting_value(
    context: &crate::Context,
    target: &SettingTarget<'_>,
    setting: &dyn NvapiSetting,
    ctx: &SettingContext,
    op: WriteOp,
) -> Result<(), ServiceError> {
    ensure_dll_setting_catalog_ready(setting, ctx)?;
    let session = open_drs_session().map_err(warning_to_service_error)?;

    let profile = target.resolve_profile_for_write(&session, ctx)?;

    // The pre-write snapshot only feeds the per-game baseline; the global base
    // profile has none, so skip the read entirely there (a transient read
    // failure must not block a valid global write).
    let pre = match target.game_id() {
        Some(_) => Some(read_pre_state(setting, &profile)?),
        None => None,
    };

    match op {
        WriteOp::Set(dword) => {
            profile
                .set_dword(setting.nvapi_id(), dword)
                .map_err(|e| map_nvapi_write_error(e, "set failed"))?;
        }
        WriteOp::Delete => {
            profile
                .delete_setting(setting.nvapi_id())
                .map_err(|e| map_nvapi_write_error(e, "delete failed"))?;
        }
    }
    session
        .save()
        .map_err(|e| map_nvapi_write_error(e, "save failed"))?;

    // Baseline snapshots are only meaningful for a real game; the global base
    // profile has no row in the baseline table to key against (and `pre` is
    // `None` there).
    if let (Some(game_id), Some(pre)) = (target.game_id(), pre) {
        let exe = ctx.effective_exe.as_deref().unwrap_or_default();
        context.storage().capture_nvapi_baseline_if_missing(
            game_id,
            setting.key(),
            pre.current,
            pre.is_current_predefined,
            pre.predefined,
            exe,
        )?;
    }
    Ok(())
}

/// Rejects a DLL-dependent mutation until the game has a current complete
/// catalog projection. This must run before DRS access or baseline writes.
pub(crate) fn ensure_dll_setting_catalog_ready(
    setting: &dyn NvapiSetting,
    ctx: &SettingContext,
) -> Result<(), ServiceError> {
    if setting.dll_kind().is_some() && ctx.catalog_readiness == CatalogReadiness::NotReady {
        return Err(ServiceError::NvapiCatalogNotReady);
    }
    Ok(())
}

/// Restores every setting baseline for one game in a single DRS session and
/// publishes the driver changes with exactly one save.
///
/// Baseline rows are deleted only after the driver save succeeds. A later
/// SQLite failure therefore leaves an idempotent, retryable restore record.
pub(crate) fn restore_game_baselines(
    context: &crate::Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    game_id: &str,
) -> Result<(), ServiceError> {
    if guard.game_id().as_str() != game_id {
        return Err(ServiceError::command_failed(
            "NVAPI baseline restore requires the matching game mutation boundary",
        ));
    }
    let baselines = context
        .storage()
        .list_nvapi_setting_baselines_for_game(game_id)?;
    if baselines.is_empty() {
        return Ok(());
    }

    let settings = baselines
        .iter()
        .map(|baseline| {
            let setting = crate::nvapi::registry::lookup_setting(&baseline.setting_key)
                .ok_or_else(|| {
                    ServiceError::command_failed(format!(
                        "NVAPI baseline uses an unsupported setting key: {}",
                        baseline.setting_key
                    ))
                })?;
            if baseline.captured_exe.trim().is_empty() {
                return Err(ServiceError::command_failed(format!(
                    "NVAPI baseline {} has no captured executable",
                    baseline.setting_key
                )));
            }
            Ok((baseline, setting))
        })
        .collect::<Result<Vec<_>, ServiceError>>()?;

    let session = open_drs_session().map_err(warning_to_service_error)?;
    let executables = baselines
        .iter()
        .map(|baseline| baseline.captured_exe.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for executable in executables {
        session.find_profile_by_exe(executable).map_err(|_| {
            ServiceError::command_failed(format!(
                "NVIDIA profile for captured executable {executable} was not found"
            ))
        })?;
    }

    for (baseline, setting) in &settings {
        let profile = session
            .find_profile_by_exe(&baseline.captured_exe)
            .map_err(|_| {
                ServiceError::command_failed(format!(
                    "NVIDIA profile for captured executable {} was not found",
                    baseline.captured_exe
                ))
            })?;
        if baseline.baseline_was_predefined {
            profile
                .delete_setting(setting.nvapi_id())
                .map_err(|error| map_nvapi_write_error(error, "baseline delete failed"))?;
        } else {
            profile
                .set_dword(setting.nvapi_id(), baseline.baseline_dword)
                .map_err(|error| map_nvapi_write_error(error, "baseline restore failed"))?;
        }
    }
    session
        .save()
        .map_err(|error| map_nvapi_write_error(error, "baseline save failed"))?;

    for baseline in baselines {
        context
            .storage()
            .delete_nvapi_baseline(game_id, &baseline.setting_key)?;
    }
    Ok(())
}
