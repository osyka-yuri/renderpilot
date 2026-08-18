//! Build [`SettingStateResponse`] from a live read without touching the driver.

use std::collections::HashSet;

use renderpilot_nvapi::Nvapi;
use renderpilot_nvapi::setting::{
    CatalogReadiness as NvapiCatalogReadiness, NvapiSetting, NvapiValueOption, SettingContext,
};

use super::super::dto::{
    BaselineDto, CatalogReadinessDto, DllInfoDto, NvapiWarningDto, SettingStateResponse,
    ValueDescriptorDto, ValueOptionDto, category_for_family, value_type_str,
};
use super::live::LiveRead;
use super::target::SettingTarget;
use crate::ServiceError;
use crate::dlss::preset_manifest::{
    VersionSupportEntry, bundled_manifest, resolve_entry, supported_presets_for,
};

/// Builds the full [`SettingStateResponse`] from a live read + storage, without
/// touching the driver itself (the caller supplies the [`LiveRead`]).
pub(super) fn assemble_response(
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
        match ctx.catalog_readiness {
            NvapiCatalogReadiness::NotReady => warnings.push(NvapiWarningDto::CatalogNotReady),
            NvapiCatalogReadiness::Ready => match dll_info.as_ref() {
                None => warnings.push(NvapiWarningDto::NoDll),
                Some(info) if info.version.is_none() => {
                    warnings.push(NvapiWarningDto::DllVersionUnknown);
                }
                Some(_) if supported_set.is_empty() => warnings.push(NvapiWarningDto::NoManifest),
                Some(_) => {}
            },
            NvapiCatalogReadiness::NotApplicable => {}
        }
    }
    // An unready catalog is terminal for DLL-dependent settings. Do not
    // append a live-driver warning: consumers must receive the single,
    // actionable CatalogNotReady fact and wait for an initial scan.
    if !matches!(ctx.catalog_readiness, NvapiCatalogReadiness::NotReady)
        && let Some(warning) = live.warning
    {
        warnings.push(warning);
    }

    // "Modified outside RenderPilot": our baseline -- captured the first time we
    // touched this setting -- already differed from the driver's predefined
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
        dll_kind: setting
            .dll_kind()
            .map(|kind| kind.manifest_tag().to_owned()),
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
        catalog_readiness: catalog_readiness_dto(ctx.catalog_readiness),
        available_values: build_available_values(setting, ctx, &supported_set, &known_set),
        dll_info,
        warnings,
    })
}

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

fn build_dll_info(setting: &dyn NvapiSetting, ctx: &SettingContext) -> Option<DllInfoDto> {
    let kind = setting.dll_kind()?;
    let info = ctx.dlls.get(&kind)?;
    let manifest = bundled_manifest(kind);
    let label = info.version.as_ref().and_then(|version| {
        resolve_entry(manifest, version).map(|e: &VersionSupportEntry| e.label.clone())
    });
    Some(DllInfoDto {
        kind: kind.manifest_tag().to_owned(),
        version: info.version.map(|version| version.to_string()),
        path: info.path.to_string_lossy().replace('\\', "/"),
        manifest_label: label,
    })
}

/// DWORD values the current DLL version officially supports per the preset manifest.
pub(super) fn supported_preset_set(
    setting: &dyn NvapiSetting,
    ctx: &SettingContext,
) -> HashSet<u32> {
    let Some(kind) = setting.dll_kind() else {
        return HashSet::new();
    };
    let Some(info) = ctx.dlls.get(&kind) else {
        return HashSet::new();
    };
    let Some(version) = info.version else {
        return HashSet::new();
    };
    let manifest = bundled_manifest(kind);
    supported_presets_for(manifest, &version)
        .iter()
        .copied()
        .collect()
}

fn catalog_readiness_dto(readiness: NvapiCatalogReadiness) -> CatalogReadinessDto {
    match readiness {
        NvapiCatalogReadiness::NotApplicable => CatalogReadinessDto::NotApplicable,
        NvapiCatalogReadiness::Ready => CatalogReadinessDto::Ready,
        NvapiCatalogReadiness::NotReady => CatalogReadinessDto::NotReady,
    }
}

/// DWORD values the preset manifest explicitly manages.
pub(super) fn known_preset_set(setting: &dyn NvapiSetting, ctx: &SettingContext) -> HashSet<u32> {
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

pub(super) fn build_available_values(
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
