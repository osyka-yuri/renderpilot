use std::collections::HashSet;

use renderpilot_nvapi::{
    setting::{NvapiSetting, NvapiValueOption, SettingContext},
    DrsSession, DwordSettingState, Nvapi, NvapiError, Profile, NVAPI_SETTING_NOT_FOUND,
};

use super::dto::{
    category_for_family, value_type_str, BaselineDto, DllInfoDto, NvapiWarningDto,
    SettingStateResponse, ValueDescriptorDto, ValueOptionDto,
};
use crate::dlss::preset_manifest::{
    bundled_manifest, resolve_entry, supported_presets_for, VersionSupportEntry,
};
use crate::ServiceError;

fn map_nvapi_write_error(error: NvapiError, label: &'static str) -> ServiceError {
    match error {
        NvapiError::InvalidUserPrivilege => ServiceError::NvapiRequiresElevation,
        other => ServiceError::CommandFailed(format!("{label}: {other}")),
    }
}

/// Opens an NVAPI DRS session, classifying each failure step as the
/// [`NvapiWarningDto`] the UI surfaces. `Nvapi::get()` returns a `&'static`
/// handle, so the borrowed session is itself `'static`.
///
/// Read paths match on the warning directly; the write path maps it to a
/// [`ServiceError`] via [`warning_to_service_error`]. This is the single place
/// the `get → initialize → create_session` sequence lives.
fn open_drs_session() -> Result<DrsSession<'static>, NvapiWarningDto> {
    let nvapi = Nvapi::get().ok_or(NvapiWarningDto::NvapiUnavailable)?;
    nvapi
        .initialize()
        .map_err(|_| NvapiWarningDto::NvapiInitFailed)?;
    nvapi
        .create_session()
        .map_err(|_| NvapiWarningDto::DrsFailed)
}

/// Maps a session-open warning to the user-facing [`ServiceError`] used on the
/// write path, where an unopenable session is a hard failure.
fn warning_to_service_error(warning: NvapiWarningDto) -> ServiceError {
    let message = match warning {
        NvapiWarningDto::NvapiUnavailable => "NVAPI unavailable (non-NVIDIA driver or missing dll)",
        NvapiWarningDto::NvapiInitFailed => "NVAPI initialize failed",
        NvapiWarningDto::DrsFailed => "DRS session failed",
        // Not produced by `open_drs_session`, but keep the mapping total.
        other => return ServiceError::CommandFailed(format!("DRS session failed: {other:?}")),
    };
    ServiceError::CommandFailed(message.to_owned())
}

/// Which NVIDIA DRS profile an NVAPI setting operation targets.
///
/// The read/write/assembly logic is identical for both variants; this enum
/// captures the only three differences between them: which profile is
/// resolved, whether a local baseline is tracked, and whether an effective
/// executable exists.
#[derive(Debug, Clone, Copy)]
pub enum SettingTarget<'a> {
    /// A specific game's profile, resolved by executable. Baselines are
    /// persisted keyed by `game_id`.
    Game {
        /// Catalog id of the game whose profile (and baselines) this targets.
        game_id: &'a str,
    },
    /// The global/base driver profile (`_GLOBAL_DRIVER_PROFILE_`), which
    /// applies to every game without its own override. No baseline tracking
    /// (the baseline table is keyed by a real `game_id`).
    Global,
}

impl SettingTarget<'_> {
    /// The game this target tracks state for, or `None` for the global profile.
    fn game_id(&self) -> Option<&str> {
        match self {
            Self::Game { game_id } => Some(game_id),
            Self::Global => None,
        }
    }

    /// Whether reads/writes are scoped to an executable's profile (`true`) or
    /// the global base profile (`false`, which needs no executable).
    fn requires_exe(&self) -> bool {
        matches!(self, Self::Game { .. })
    }

    /// Resolves the DRS profile within an open session for a *read*. Returns
    /// the profile when resolved, plus an optional warning to surface. A
    /// missing per-game profile is benign (no warning); a missing global base
    /// profile is reported.
    fn resolve_profile_for_read<'s>(
        &self,
        session: &'s DrsSession<'s>,
        exe: Option<&str>,
    ) -> (Option<Profile<'s>>, Option<NvapiWarningDto>) {
        match self {
            Self::Game { .. } => match exe {
                Some(exe) => (session.find_profile_by_exe(exe).ok(), None),
                None => (None, None),
            },
            Self::Global => match session.base_profile() {
                Ok(profile) => (Some(profile), None),
                Err(_) => (None, Some(NvapiWarningDto::DrsFailed)),
            },
        }
    }

    /// Resolves the DRS profile for a *write*, where a missing profile is a
    /// hard error.
    fn resolve_profile_for_write<'s>(
        &self,
        session: &'s DrsSession<'s>,
        ctx: &SettingContext,
    ) -> Result<Profile<'s>, ServiceError> {
        match self {
            Self::Game { .. } => {
                let exe = ctx.effective_exe.as_deref().ok_or_else(|| {
                    ServiceError::CommandFailed("no executable detected for game".to_owned())
                })?;
                session.find_profile_by_exe(exe).map_err(|_| {
                    ServiceError::CommandFailed(format!("NVIDIA profile for {exe} not found"))
                })
            }
            Self::Global => session.base_profile().map_err(|e| {
                ServiceError::CommandFailed(format!("global driver profile unavailable: {e}"))
            }),
        }
    }
}

/// Operation to perform when writing an NVAPI setting value.
#[derive(Debug, Clone, Copy)]
pub enum WriteOp {
    /// Set the setting to the given DWORD value.
    Set(u32),
    /// Delete the setting override, restoring the driver predefined default.
    Delete,
}

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
                ServiceError::CommandFailed(
                    "baseline revert is not available for global settings".to_owned(),
                )
            })?;
            let baseline = context
                .storage()
                .get_nvapi_baseline(game_id, setting.key())?
                .ok_or_else(|| {
                    ServiceError::CommandFailed(
                        "no baseline recorded yet — set a value at least once first".to_owned(),
                    )
                })?;
            if baseline.baseline_was_predefined {
                Ok(WriteOp::Delete)
            } else {
                Ok(WriteOp::Set(baseline.baseline_dword))
            }
        }
        other => Err(ServiceError::CommandFailed(format!(
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
        return Err(ServiceError::CommandFailed(format!(
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

/// Reads the live state of a single NVAPI `setting` for `game_id`.
pub fn read_setting_state(
    context: &crate::Context,
    target: &SettingTarget<'_>,
    setting: &dyn NvapiSetting,
    ctx: &SettingContext,
) -> Result<SettingStateResponse, ServiceError> {
    let live = read_live_or_default(target, setting, ctx);
    assemble_response(setting, ctx, context.storage(), target, live)
}

/// Reads the live state of **every** supplied setting through a single DRS
/// session + profile lookup, instead of one session per setting. The session
/// and profile are resolved once up front; if any step fails, each setting
/// reports the same diagnostic warning and falls back to default values —
/// mirroring `read_live_or_default` but without re-opening the driver.
pub fn read_all_setting_states(
    context: &crate::Context,
    target: &SettingTarget<'_>,
    settings: &[Box<dyn NvapiSetting>],
    ctx: &SettingContext,
) -> Result<Vec<SettingStateResponse>, ServiceError> {
    let storage = context.storage();
    let exe = ctx.effective_exe.as_deref();

    let session_result = if target.requires_exe() && exe.is_none() {
        Err(NvapiWarningDto::NoExecutable)
    } else {
        open_drs_session()
    };
    let (session, session_warning) =
        session_result.map_or_else(|w| (None, Some(w)), |s| (Some(s), None));

    let (profile, profile_warning) = match session.as_ref() {
        Some(session) => target.resolve_profile_for_read(session, exe),
        None => (None, None),
    };
    let unavailable_warning = session_warning.or(profile_warning);

    let mut responses = Vec::with_capacity(settings.len());
    for setting in settings {
        let setting = setting.as_ref();
        let live = match &profile {
            Some(profile) => read_dword_or_default(profile, setting),
            None => LiveRead::unavailable(setting.default_dword(), unavailable_warning),
        };
        responses.push(assemble_response(setting, ctx, storage, target, live)?);
    }
    Ok(responses)
}

/// Outcome of a single live NVAPI read, decoupled from how the DRS session was
/// obtained so the single-setting and batch paths can share response assembly.
#[derive(Clone, Copy)]
struct LiveRead {
    current: u32,
    predefined: Option<u32>,
    is_current_predefined: bool,
    has_profile_for_exe: bool,
    /// Set when the live value could not be read; surfaced as a UI warning.
    warning: Option<NvapiWarningDto>,
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
    fn unavailable(default: u32, warning: Option<NvapiWarningDto>) -> Self {
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
    target: &SettingTarget<'_>,
    live: LiveRead,
) -> Result<SettingStateResponse, ServiceError> {
    // Baseline tracking and executable resolution only apply to a real game;
    // the global base profile has neither.
    let (baseline_row, effective_exe, effective_exe_source) = match target.game_id() {
        Some(game_id) => {
            let baseline_row = storage.get_nvapi_baseline(game_id, setting.key())?;
            let effective_exe = ctx.effective_exe.clone();
            let effective_exe_source =
                resolve_effective_exe_source(storage, game_id, &effective_exe)?;
            (baseline_row, effective_exe, effective_exe_source)
        }
        None => (None, None, None),
    };

    let dll_info = build_dll_info(setting, ctx);
    let supported_set = supported_preset_set(setting, ctx);
    let known_set = known_preset_set(setting, ctx);

    let mut warnings: Vec<NvapiWarningDto> = Vec::new();
    // DLL-version warnings only make sense for a specific game's install. The
    // global base profile is intentionally DLL-independent, so suppress them.
    if target.game_id().is_some() && setting.dll_kind().is_some() {
        if dll_info.is_none() {
            warnings.push(NvapiWarningDto::NoDll);
        } else if supported_set.is_empty() {
            warnings.push(NvapiWarningDto::NoManifest);
        }
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
) -> Result<DwordSettingState, ServiceError> {
    match profile.get_dword_full(setting.nvapi_id()) {
        Ok(state) => Ok(state),
        Err(NvapiError::GetSettingFailed(code)) if code == NVAPI_SETTING_NOT_FOUND => {
            Ok(DwordSettingState {
                current: setting.default_dword(),
                predefined: None,
                is_current_predefined: true,
            })
        }
        Err(e) => Err(ServiceError::CommandFailed(format!(
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
) -> Result<Option<String>, ServiceError> {
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
fn read_live_or_default(
    target: &SettingTarget<'_>,
    setting: &dyn NvapiSetting,
    ctx: &SettingContext,
) -> LiveRead {
    let unavailable =
        |warning: NvapiWarningDto| LiveRead::unavailable(setting.default_dword(), Some(warning));

    let exe = ctx.effective_exe.as_deref();
    if target.requires_exe() && exe.is_none() {
        return unavailable(NvapiWarningDto::NoExecutable);
    }
    let session = match open_drs_session() {
        Ok(session) => session,
        Err(warning) => return unavailable(warning),
    };
    match target.resolve_profile_for_read(&session, exe) {
        (Some(profile), _) => read_dword_or_default(&profile, setting),
        (None, warning) => LiveRead::unavailable(setting.default_dword(), warning),
    }
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
    use crate::dlss::settings_catalog::{self, CatalogSetting};
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

    #[test]
    fn target_exe_requirement_distinguishes_scope() {
        assert!(SettingTarget::Game { game_id: "g1" }.requires_exe());
        assert!(!SettingTarget::Global.requires_exe());
        assert_eq!(SettingTarget::Game { game_id: "g1" }.game_id(), Some("g1"));
        assert_eq!(SettingTarget::Global.game_id(), None);
    }

    #[test]
    fn global_revert_to_default_is_delete() {
        let context = crate::Context::from_storage(
            renderpilot_storage_sqlite::SqliteStorage::in_memory().expect("sqlite should open"),
        );
        let def = settings_catalog::find("dlss_sr_render_preset").expect("catalog has SR preset");
        let setting = CatalogSetting::new(def);
        let op = resolve_revert_op(&context, &SettingTarget::Global, &setting, "predefined")
            .expect("predefined revert is always valid");
        assert!(matches!(op, WriteOp::Delete));
    }

    #[test]
    fn global_revert_to_baseline_is_rejected() {
        let context = crate::Context::from_storage(
            renderpilot_storage_sqlite::SqliteStorage::in_memory().expect("sqlite should open"),
        );
        let def = settings_catalog::find("dlss_sr_render_preset").expect("catalog has SR preset");
        let setting = CatalogSetting::new(def);
        // There is no per-game baseline table for the global profile, so a
        // baseline revert must be refused rather than silently no-op.
        assert!(resolve_revert_op(&context, &SettingTarget::Global, &setting, "baseline").is_err());
    }

    #[test]
    fn global_context_imposes_no_dll_version_constraints() {
        // With no detected DLL, every catalog value (including manifest-managed
        // presets like "a") is allowed — a global setting is not tied to one
        // game's DLL version.
        let def = settings_catalog::find("dlss_sr_render_preset").expect("catalog has SR preset");
        let setting = CatalogSetting::new(def);
        let ctx = crate::nvapi::resolve::global_setting_context();
        let preset_a = def.values.iter().find(|v| v.wire == "a").unwrap().dword;
        assert!(validate_value_supported(&setting, preset_a, &ctx).is_ok());
    }
}
