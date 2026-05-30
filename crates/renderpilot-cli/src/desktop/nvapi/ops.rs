use std::collections::HashSet;

use renderpilot_nvapi::{
    setting::{BaselineSnapshot, NvapiSetting, NvapiValueOption, SettingContext, SettingState},
    DwordSettingState, Nvapi, NvapiError, NVAPI_SETTING_NOT_FOUND,
};

use crate::{
    catalog::open_catalog_storage,
    desktop::dlss::preset_manifest::{
        bundled_manifest, resolve_entry, supported_presets_for, VersionSupportEntry,
    },
    desktop::nvapi::dto::{
        value_type_str, BaselineDto, DllInfoDto, SettingStateResponse, ValueDescriptorDto,
        ValueOptionDto,
    },
    error::CliError,
};

fn map_nvapi_write_error(error: NvapiError, label: &'static str) -> CliError {
    match error {
        NvapiError::InvalidUserPrivilege => CliError::NvapiRequiresElevation,
        other => CliError::CommandFailed(format!("{label}: {other}")),
    }
}

pub enum WriteOp {
    Set(u32),
    Delete,
}

pub fn validate_value_supported(
    setting: &dyn NvapiSetting,
    dword: u32,
    ctx: &SettingContext,
) -> Result<(), CliError> {
    let Some(kind) = setting.dll_kind() else {
        return Ok(());
    };
    let Some(info) = ctx.dlls.get(&kind) else {
        return Ok(());
    };

    let manifest = bundled_manifest(kind);
    let supported = supported_presets_for(manifest, &info.version);
    if supported.is_empty() {
        return Ok(());
    }
    if !supported.contains(&dword) {
        return Err(CliError::CommandFailed(format!(
            "value `{}` is not supported for DLL version {} (kind={:?})",
            setting
                .format_wire(dword)
                .unwrap_or_else(|| dword.to_string()),
            info.version,
            kind
        )));
    }
    Ok(())
}

pub fn write_setting_value(
    game_id: &str,
    setting: &dyn NvapiSetting,
    ctx: &SettingContext,
    op: WriteOp,
) -> Result<(), CliError> {
    let exe = ctx
        .effective_exe
        .as_deref()
        .ok_or_else(|| CliError::CommandFailed("no executable detected for game".to_owned()))?;

    let nvapi = Nvapi::get().ok_or_else(|| {
        CliError::CommandFailed("NVAPI unavailable (non-NVIDIA driver or missing dll)".to_owned())
    })?;
    nvapi
        .initialize()
        .map_err(|e| CliError::CommandFailed(format!("NVAPI initialize failed: {e}")))?;
    let session = nvapi
        .create_session()
        .map_err(|e| CliError::CommandFailed(format!("DRS session failed: {e}")))?;

    let profile = session.find_profile_by_exe(exe).map_err(|_| {
        CliError::CommandFailed(format!(
            "no NVIDIA profile for {exe} - launch the game once so NVIDIA creates one"
        ))
    })?;

    let pre = read_pre_state(setting, &profile)?;

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

    let storage = open_catalog_storage()?;
    storage.capture_nvapi_baseline_if_missing(
        game_id,
        setting.key(),
        pre.current,
        pre.is_current_predefined,
        pre.predefined,
        exe,
    )?;
    Ok(())
}

pub fn read_setting_state(
    game_id: &str,
    setting: &dyn NvapiSetting,
    ctx: &SettingContext,
) -> Result<SettingStateResponse, CliError> {
    let storage = open_catalog_storage()?;
    let baseline_row = storage.get_nvapi_baseline(game_id, setting.key())?;

    let baseline = baseline_row.as_ref().map(|row| BaselineSnapshot {
        dword: row.baseline_dword,
        was_predefined_when_captured: row.baseline_was_predefined,
        captured_at_unix_secs: row.captured_at / 1000,
    });

    let dll_info = build_dll_info(setting, ctx);
    let supported_set = supported_preset_set(setting, ctx);

    let mut warnings: Vec<String> = Vec::new();
    if dll_info.is_none() && setting.dll_kind().is_some() {
        warnings.push("no DLSS DLL detected in the install directory".to_owned());
    }
    if supported_set.is_empty() && setting.dll_kind().is_some() {
        warnings.push("manifest has no entry for this DLL version".to_owned());
    }

    let (current, predefined, is_current_predefined, has_profile_for_exe) =
        read_live_or_default(setting, ctx, &mut warnings);
    let state = SettingState {
        live: DwordSettingState {
            current,
            predefined,
            is_current_predefined,
        },
        baseline,
    };

    let available_values = build_available_values(setting, ctx, &supported_set);
    let baseline_dto = baseline_row.as_ref().map(|row| {
        build_baseline_dto(
            setting,
            row.baseline_dword,
            row.baseline_was_predefined,
            row.captured_at,
            row.captured_exe.clone(),
        )
    });
    let effective_exe = ctx.effective_exe.clone();
    let effective_exe_source = resolve_effective_exe_source(&storage, game_id, &effective_exe)?;

    Ok(build_setting_state_response(
        setting,
        &state,
        baseline_dto,
        effective_exe,
        effective_exe_source,
        has_profile_for_exe,
        available_values,
        dll_info,
        warnings,
    ))
}

fn read_pre_state(
    setting: &dyn NvapiSetting,
    profile: &renderpilot_nvapi::Profile<'_>,
) -> Result<DwordSettingState, CliError> {
    match profile.get_dword_full(setting.nvapi_id()) {
        Ok(state) => Ok(state),
        Err(NvapiError::GetSettingFailed(code)) if code == NVAPI_SETTING_NOT_FOUND => {
            Ok(DwordSettingState {
                current: 0,
                predefined: None,
                is_current_predefined: true,
            })
        }
        Err(e) => Err(CliError::CommandFailed(format!("could not read setting: {e}"))),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_setting_state_response(
    setting: &dyn NvapiSetting,
    state: &SettingState,
    baseline: Option<BaselineDto>,
    effective_exe: Option<String>,
    effective_exe_source: Option<String>,
    has_profile_for_exe: bool,
    available_values: Vec<ValueOptionDto>,
    dll_info: Option<DllInfoDto>,
    warnings: Vec<String>,
) -> SettingStateResponse {
    let current_descriptor = ValueDescriptorDto {
        wire: setting
            .format_wire(state.live.current)
            .unwrap_or_else(|| state.live.current.to_string()),
        label: setting
            .label_for_dword(state.live.current)
            .unwrap_or_else(|| format!("dword {}", state.live.current)),
        dword: state.live.current,
    };
    let predefined_descriptor = state.live.predefined.map(|dw| ValueDescriptorDto {
        wire: setting.format_wire(dw).unwrap_or_else(|| dw.to_string()),
        label: setting
            .label_for_dword(dw)
            .unwrap_or_else(|| format!("dword {dw}")),
        dword: dw,
    });

    let is_modified_outside = match (&state.baseline, state.live.predefined) {
        (Some(b), Some(predef)) => b.dword != predef && b.was_predefined_when_captured,
        _ => false,
    };

    SettingStateResponse {
        setting_key: setting.key().to_owned(),
        setting_label: setting.label().to_owned(),
        value_type: value_type_str(setting.value_type()).to_owned(),
        current: current_descriptor,
        predefined: predefined_descriptor,
        baseline,
        is_current_predefined: state.live.is_current_predefined,
        is_modified_outside_renderpilot: is_modified_outside,
        effective_exe,
        effective_exe_source,
        has_profile_for_exe,
        available_values,
        dll_info,
        warnings,
    }
}

// ---------------------------------------------------------------------------
// Private helpers for read_setting_state
// ---------------------------------------------------------------------------

/// Constructs a DLL information Data Transfer Object (DTO) for the DLL family
/// associated with a specific setting, provided a DLL was detected within the
/// game installation directory.
fn build_dll_info(setting: &dyn NvapiSetting, ctx: &SettingContext) -> Option<DllInfoDto> {
    let kind = setting.dll_kind()?;
    let info = ctx.dlls.get(&kind)?;
    let manifest = bundled_manifest(kind);
    let label =
        resolve_entry(manifest, &info.version).map(|e: &VersionSupportEntry| e.label.clone());
    Some(DllInfoDto {
        kind: kind.manifest_tag().to_owned(),
        version: info.version.to_string(),
        path: info.path.to_string_lossy().replace('\\', "/"),
        manifest_label: label,
    })
}

/// Retrieves a set of DWORD values that the current DLL version officially
/// supports. This returns an empty set if the setting lacks a DLL dependency,
/// the DLL could not be located, or the preset manifest contains no entries
/// for this specific version.
fn supported_preset_set(setting: &dyn NvapiSetting, ctx: &SettingContext) -> HashSet<u32> {
    let Some(kind) = setting.dll_kind() else {
        return HashSet::new();
    };
    let Some(info) = ctx.dlls.get(&kind) else {
        return HashSet::new();
    };
    let manifest = bundled_manifest(kind);
    supported_presets_for(manifest, &info.version)
        .iter()
        .copied()
        .collect()
}

/// Compiles a list of available configuration values, flagging each option as
/// either supported or unsupported based on the provided `supported_set`. If
/// `supported_set` is empty, it implies there are no manifest constraints,
/// and all options are treated as supported.
fn build_available_values(
    setting: &dyn NvapiSetting,
    ctx: &SettingContext,
    supported_set: &HashSet<u32>,
) -> Vec<ValueOptionDto> {
    setting
        .enumerate_values(ctx)
        .into_iter()
        .map(|opt: NvapiValueOption| {
            let supported = if supported_set.is_empty() {
                opt.supported_by_context
            } else {
                supported_set.contains(&opt.dword)
            };
            ValueOptionDto {
                wire: opt.wire,
                label: opt.label,
                supported,
            }
        })
        .collect()
}

/// Transforms the raw database baseline row (if it exists) into the corresponding wire DTO.
/// This function accepts row fields directly to avoid a dependency on the internal
/// `NvapiSettingBaselineRow` type defined within the storage crate.
fn build_baseline_dto(
    setting: &dyn NvapiSetting,
    baseline_dword: u32,
    baseline_was_predefined: bool,
    captured_at: i64,
    captured_exe: String,
) -> BaselineDto {
    BaselineDto {
        wire: setting.format_wire(baseline_dword),
        label: setting.label_for_dword(baseline_dword),
        dword: baseline_dword,
        was_predefined: baseline_was_predefined,
        captured_at: captured_at / 1000,
        captured_exe,
    }
}

/// Returns `Some("override")` or `Some("auto")` when a target executable is
/// successfully resolved, or `None` if no executable was identified.
/// This method leverages an existing `storage` connection to prevent a redundant
/// call to `open_catalog_storage()` during the `read_setting_state` process.
fn resolve_effective_exe_source(
    storage: &renderpilot_storage_sqlite::SqliteStorage,
    game_id: &str,
    effective_exe: &Option<String>,
) -> Result<Option<String>, CliError> {
    if effective_exe.is_none() {
        return Ok(None);
    }
    let row = storage.get_nvapi_executable_override(game_id)?;
    let source = match row {
        Some(_) => "override",
        None => "auto",
    };
    Ok(Some(source.to_owned()))
}

fn read_live_or_default(
    setting: &dyn NvapiSetting,
    ctx: &SettingContext,
    warnings: &mut Vec<String>,
) -> (u32, Option<u32>, bool, bool) {
    let Some(exe) = ctx.effective_exe.as_deref() else {
        warnings.push("no executable resolved for this game".to_owned());
        return (0, None, false, false);
    };
    let Some(nvapi) = Nvapi::get() else {
        warnings.push("NVAPI unavailable".to_owned());
        return (0, None, false, false);
    };
    if nvapi.initialize().is_err() {
        warnings.push("NVAPI initialize failed".to_owned());
        return (0, None, false, false);
    }
    let session = match nvapi.create_session() {
        Ok(s) => s,
        Err(_) => {
            warnings.push("DRS session could not be created".to_owned());
            return (0, None, false, false);
        }
    };
    let profile = match session.find_profile_by_exe(exe) {
        Ok(p) => p,
        Err(_) => {
            warnings.push(format!(
                "no NVIDIA profile for {exe} (launch the game once)"
            ));
            return (0, None, false, false);
        }
    };
    match profile.get_dword_full(setting.nvapi_id()) {
        Ok(state) => (
            state.current,
            state.predefined,
            state.is_current_predefined,
            true,
        ),
        Err(_) => (0, None, true, true),
    }
}
