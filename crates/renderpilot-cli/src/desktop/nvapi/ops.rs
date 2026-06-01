use std::collections::HashSet;

use renderpilot_nvapi::{
    setting::{NvapiSetting, NvapiValueOption, SettingContext},
    DwordSettingState, Nvapi, NvapiError, NVAPI_SETTING_NOT_FOUND,
};

use crate::{
    catalog::open_catalog_storage,
    desktop::dlss::preset_manifest::{
        bundled_manifest, resolve_entry, supported_presets_for, VersionSupportEntry,
    },
    desktop::nvapi::dto::{
        category_for_family, value_type_str, BaselineDto, DllInfoDto, SettingStateResponse,
        ValueDescriptorDto, ValueOptionDto,
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

    let supported = supported_presets_for(bundled_manifest(kind), &info.version);
    if supported.is_empty() {
        return Ok(());
    }
    // The manifest only constrains the presets it explicitly lists. Values it
    // does not manage — the "recommended" sentinel, or a preset letter beyond
    // its table — are always allowed.
    if known_preset_set(setting, ctx).contains(&dword) && !supported.contains(&dword) {
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
    let effective_exe = ctx.effective_exe.clone();
    let effective_exe_source = resolve_effective_exe_source(&storage, game_id, &effective_exe)?;

    let live = read_live_or_default(setting, ctx);
    assemble_response(
        setting,
        ctx,
        &storage,
        game_id,
        live,
        effective_exe,
        effective_exe_source,
    )
}

/// Reads the live state of **every** supplied setting through a single DRS
/// session + profile lookup, instead of one session per setting. The session
/// and profile are resolved once up front; if any step fails, each setting
/// reports the same diagnostic warning and falls back to default values —
/// mirroring [`read_live_or_default`] but without re-opening the driver.
pub fn read_all_setting_states(
    game_id: &str,
    settings: &[Box<dyn NvapiSetting>],
    ctx: &SettingContext,
) -> Result<Vec<SettingStateResponse>, CliError> {
    let storage = open_catalog_storage()?;
    let effective_exe = ctx.effective_exe.clone();
    let effective_exe_source = resolve_effective_exe_source(&storage, game_id, &effective_exe)?;

    let exe = ctx.effective_exe.as_deref();
    let mut session_warning: Option<String> = None;
    let session = match exe {
        None => {
            session_warning = Some("no executable resolved for this game".to_owned());
            None
        }
        Some(_) => match Nvapi::get() {
            None => {
                session_warning = Some("NVAPI unavailable".to_owned());
                None
            }
            Some(nvapi) if nvapi.initialize().is_err() => {
                session_warning = Some("NVAPI initialize failed".to_owned());
                None
            }
            Some(nvapi) => match nvapi.create_session() {
                Ok(session) => Some(session),
                Err(_) => {
                    session_warning = Some("DRS session could not be created".to_owned());
                    None
                }
            },
        },
    };
    let profile = match (session.as_ref(), exe) {
        (Some(session), Some(exe)) => match session.find_profile_by_exe(exe) {
            Ok(profile) => Some(profile),
            Err(_) => {
                session_warning = Some(format!(
                    "no NVIDIA profile for {exe} (launch the game once)"
                ));
                None
            }
        },
        _ => None,
    };

    let mut responses = Vec::with_capacity(settings.len());
    for setting in settings {
        let setting = setting.as_ref();
        let live = match &profile {
            Some(profile) => read_dword_or_default(profile, setting),
            None => LiveRead::unavailable(setting.default_dword(), session_warning.clone()),
        };
        responses.push(assemble_response(
            setting,
            ctx,
            &storage,
            game_id,
            live,
            effective_exe.clone(),
            effective_exe_source.clone(),
        )?);
    }
    Ok(responses)
}

/// Outcome of a single live NVAPI read, decoupled from how the DRS session was
/// obtained so the single-setting and batch paths can share response assembly.
struct LiveRead {
    current: u32,
    predefined: Option<u32>,
    is_current_predefined: bool,
    has_profile_for_exe: bool,
    /// Set when the live value could not be read; surfaced as a UI warning.
    warning: Option<String>,
}

impl LiveRead {
    /// The setting is absent from the profile (no override): the current value
    /// is the setting's declared default and it counts as "at the driver
    /// default".
    fn unset(default: u32) -> Self {
        Self {
            current: default,
            predefined: None,
            is_current_predefined: true,
            has_profile_for_exe: true,
            warning: None,
        }
    }

    /// The driver/profile could not be read at all: show the declared default
    /// and surface the reason.
    fn unavailable(default: u32, warning: Option<String>) -> Self {
        Self {
            current: default,
            predefined: None,
            is_current_predefined: false,
            has_profile_for_exe: false,
            warning,
        }
    }
}

/// Builds the full [`SettingStateResponse`] from a live read + storage, without
/// touching the driver itself (the caller supplies the [`LiveRead`]).
fn assemble_response(
    setting: &dyn NvapiSetting,
    ctx: &SettingContext,
    storage: &renderpilot_storage_sqlite::SqliteStorage,
    game_id: &str,
    live: LiveRead,
    effective_exe: Option<String>,
    effective_exe_source: Option<String>,
) -> Result<SettingStateResponse, CliError> {
    let baseline_row = storage.get_nvapi_baseline(game_id, setting.key())?;

    let dll_info = build_dll_info(setting, ctx);
    let supported_set = supported_preset_set(setting, ctx);
    let known_set = known_preset_set(setting, ctx);

    let mut warnings: Vec<String> = Vec::new();
    if dll_info.is_none() && setting.dll_kind().is_some() {
        warnings.push("no DLSS DLL detected in the install directory".to_owned());
    }
    if supported_set.is_empty() && setting.dll_kind().is_some() {
        warnings.push("manifest has no entry for this DLL version".to_owned());
    }
    if let Some(warning) = live.warning {
        warnings.push(warning);
    }

    // "Modified outside RenderPilot": our baseline — captured the first time we
    // touched this setting — already differed from the driver's predefined
    // default, i.e. another tool had overridden it before us.
    let is_modified_outside = match (baseline_row.as_ref(), live.predefined) {
        (Some(row), Some(predefined)) => {
            row.baseline_was_predefined && row.baseline_dword != predefined
        }
        _ => false,
    };

    Ok(SettingStateResponse {
        setting_key: setting.key().to_owned(),
        setting_label: setting.label().to_owned(),
        value_type: value_type_str(setting.value_type()).to_owned(),
        family: setting.family().map(str::to_owned),
        category: setting.family().and_then(category_for_family),
        description: setting.description().map(str::to_owned),
        min_driver: setting.min_driver().map(str::to_owned),
        current: value_descriptor(setting, live.current),
        predefined: live
            .predefined
            .map(|dword| value_descriptor(setting, dword)),
        baseline: baseline_row.as_ref().map(|row| {
            build_baseline_dto(
                setting,
                row.baseline_dword,
                row.baseline_was_predefined,
                row.captured_at,
                row.captured_exe.clone(),
            )
        }),
        is_current_predefined: live.is_current_predefined,
        is_modified_outside_renderpilot: is_modified_outside,
        effective_exe,
        effective_exe_source,
        has_profile_for_exe: live.has_profile_for_exe,
        // Session-level: identical on every row. Drives UI gating of the NVIDIA
        // driver-profile affordances. `Nvapi::get()` is cached, so this is cheap.
        nvapi_available: Nvapi::get().is_some(),
        available_values: build_available_values(setting, ctx, &supported_set, &known_set),
        dll_info,
        warnings,
    })
}

fn read_pre_state(
    setting: &dyn NvapiSetting,
    profile: &renderpilot_nvapi::Profile<'_>,
) -> Result<DwordSettingState, CliError> {
    match profile.get_dword_full(setting.nvapi_id()) {
        Ok(state) => Ok(state),
        Err(NvapiError::GetSettingFailed(code)) if code == NVAPI_SETTING_NOT_FOUND => {
            Ok(DwordSettingState {
                current: setting.default_dword(),
                predefined: None,
                is_current_predefined: true,
            })
        }
        Err(e) => Err(CliError::CommandFailed(format!(
            "could not read setting: {e}"
        ))),
    }
}

/// Builds a value descriptor (wire + label + dword) for a setting's dword,
/// falling back to a raw representation for values outside the setting's table.
fn value_descriptor(setting: &dyn NvapiSetting, dword: u32) -> ValueDescriptorDto {
    ValueDescriptorDto {
        wire: setting
            .format_wire(dword)
            .unwrap_or_else(|| dword.to_string()),
        label: setting
            .label_for_dword(dword)
            .unwrap_or_else(|| format!("dword {dword}")),
        dword,
    }
}

// ---------------------------------------------------------------------------
// Private helpers for read_setting_state
// ---------------------------------------------------------------------------

/// Builds the DLL info DTO for the family a setting belongs to, if a DLL was detected.
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

/// DWORD values the current DLL version officially supports per the preset manifest.
/// Empty when the setting has no DLL dependency, the DLL is absent, or the manifest
/// has no entry for this version.
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

/// DWORD values the preset manifest explicitly manages (the keys of its preset
/// table). Values outside this set — e.g. the "recommended" sentinel, or a
/// preset letter beyond the table — are not constrained by version support and
/// are always offered. Empty when the setting has no DLL dependency or the DLL
/// is absent.
fn known_preset_set(setting: &dyn NvapiSetting, ctx: &SettingContext) -> HashSet<u32> {
    let Some(kind) = setting.dll_kind() else {
        return HashSet::new();
    };
    if !ctx.dlls.contains_key(&kind) {
        return HashSet::new();
    }
    bundled_manifest(kind)
        .presets
        .keys()
        .filter_map(|key| key.parse::<u32>().ok())
        .collect()
}

/// Compiles a list of available configuration values, flagging each option as
/// supported or not. With no manifest constraints (`supported_set` empty) every
/// option is offered. Otherwise an option is offered when the manifest does not
/// manage it (e.g. the "recommended" sentinel) or it is in the supported set;
/// only manifest-managed values absent from the supported set are greyed out.
fn build_available_values(
    setting: &dyn NvapiSetting,
    ctx: &SettingContext,
    supported_set: &HashSet<u32>,
    known_set: &HashSet<u32>,
) -> Vec<ValueOptionDto> {
    setting
        .enumerate_values(ctx)
        .into_iter()
        .map(|opt: NvapiValueOption| {
            let supported = if supported_set.is_empty() {
                opt.supported_by_context
            } else if !known_set.contains(&opt.dword) {
                true
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

/// Returns `Some("override")` or `Some("auto")` when an exe is resolved, or `None`.
/// Uses the already-open `storage` connection to avoid reopening it.
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

/// Reads the live state of a single setting, opening its own DRS session.
/// Used by the single-setting read path; the batch path opens one session and
/// calls [`read_dword_or_default`] directly.
fn read_live_or_default(setting: &dyn NvapiSetting, ctx: &SettingContext) -> LiveRead {
    let unavailable =
        |warning: &str| LiveRead::unavailable(setting.default_dword(), Some(warning.to_owned()));

    let Some(exe) = ctx.effective_exe.as_deref() else {
        return unavailable("no executable resolved for this game");
    };
    let Some(nvapi) = Nvapi::get() else {
        return unavailable("NVAPI unavailable");
    };
    if nvapi.initialize().is_err() {
        return unavailable("NVAPI initialize failed");
    }
    let session = match nvapi.create_session() {
        Ok(session) => session,
        Err(_) => return unavailable("DRS session could not be created"),
    };
    let profile = match session.find_profile_by_exe(exe) {
        Ok(profile) => profile,
        Err(_) => {
            return unavailable(&format!(
                "no NVIDIA profile for {exe} (launch the game once)"
            ))
        }
    };
    read_dword_or_default(&profile, setting)
}

/// Reads a DWORD from an already-resolved profile. A missing setting (or any
/// read failure) is treated as the setting's default with no warning — absence
/// is the expected "no override" state.
fn read_dword_or_default(
    profile: &renderpilot_nvapi::Profile<'_>,
    setting: &dyn NvapiSetting,
) -> LiveRead {
    match profile.get_dword_full(setting.nvapi_id()) {
        Ok(state) => LiveRead {
            current: state.current,
            predefined: state.predefined,
            is_current_predefined: state.is_current_predefined,
            has_profile_for_exe: true,
            warning: None,
        },
        Err(_) => LiveRead::unset(setting.default_dword()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::dlss::settings_catalog::{self, CatalogSetting};
    use renderpilot_nvapi::{setting::DllInfo, DlssDllKind, DlssVersion, SettingContext};
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn ctx_with_sr_dll(version: DlssVersion) -> SettingContext {
        let mut dlls = HashMap::new();
        dlls.insert(
            DlssDllKind::Sr,
            DllInfo {
                path: PathBuf::from("nvngx_dlss.dll"),
                version,
            },
        );
        SettingContext {
            game_install_dir: PathBuf::from("/tmp"),
            dlls,
            effective_exe: Some("game.exe".to_owned()),
        }
    }

    /// On DLSS 4 the SR manifest only supports presets {0, 6, 10, 11}, but the
    /// "recommended"/Latest sentinel and preset letters beyond the manifest's
    /// table must stay selectable — only manifest-managed-but-unsupported
    /// presets get greyed out.
    #[test]
    fn sr_render_preset_keeps_sentinel_and_unknown_presets_selectable() {
        let def = settings_catalog::find("dlss_sr_render_preset").expect("catalog has SR preset");
        let setting = CatalogSetting::new(def);
        let ctx = ctx_with_sr_dll(DlssVersion::new(310, 1, 0, 0));

        let supported_set = supported_preset_set(&setting, &ctx);
        let known_set = known_preset_set(&setting, &ctx);
        assert!(
            !supported_set.is_empty(),
            "DLSS 4 version should match a manifest entry"
        );

        let values = build_available_values(&setting, &ctx, &supported_set, &known_set);
        let supported = |wire: &str| {
            values
                .iter()
                .find(|v| v.wire == wire)
                .unwrap_or_else(|| panic!("missing option {wire}"))
                .supported
        };

        // Manifest-managed and supported on DLSS 4.
        assert!(supported("default")); // 0
        assert!(supported("f")); // preset F = 6
                                 // Manifest-managed but not supported on DLSS 4 → greyed.
        assert!(!supported("a")); // preset A = 1
                                  // Not managed by the manifest → always selectable.
        assert!(supported("recommended")); // 0x00FFFFFF sentinel
        assert!(supported("o")); // preset O = 15, beyond the manifest table

        // Writing the sentinel must be allowed even though it is not in the
        // supported set.
        let recommended = def
            .values
            .iter()
            .find(|v| v.wire == "recommended")
            .unwrap()
            .dword;
        assert!(validate_value_supported(&setting, recommended, &ctx).is_ok());
        // Writing a managed-but-unsupported preset must be rejected.
        let preset_a = def.values.iter().find(|v| v.wire == "a").unwrap().dword;
        assert!(validate_value_supported(&setting, preset_a, &ctx).is_err());
    }
}
