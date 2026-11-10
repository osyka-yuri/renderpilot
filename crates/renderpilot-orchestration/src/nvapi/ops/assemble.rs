//! Build [`SettingStateResponse`] from a live read without touching the driver.

use std::collections::HashSet;

use renderpilot_nvapi::Nvapi;
use renderpilot_nvapi::setting::{
    CatalogReadiness as NvapiCatalogReadiness, NvapiSetting, NvapiValueOption, SettingContext,
};
use renderpilot_storage_sqlite::SqliteStorage;

use super::super::dto::{
    CatalogReadinessDto, DllInfoDto, NvapiWarningDto, OriginalStateDto, SettingStateResponse,
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
    storage: &SqliteStorage,
    target: &SettingTarget<'_>,
    live: &LiveRead,
) -> Result<SettingStateResponse, ServiceError> {
    let (original, effective_exe, effective_exe_source) = match target.game_id() {
        Some(game_id) => (
            original_state(
                storage,
                game_id,
                setting,
                ctx.effective_exe_path.as_deref(),
                live,
            )?,
            ctx.effective_exe.clone(),
            resolve_effective_exe_source(storage, game_id, ctx.effective_exe_path.as_deref())?,
        ),
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
        current_is_explicit: live.current_is_explicit,
        predefined: live
            .predefined
            .map(|dword| value_descriptor(setting, dword)),
        original,
        is_current_predefined: live.is_current_predefined,
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

fn original_state(
    storage: &SqliteStorage,
    game_id: &str,
    setting: &dyn NvapiSetting,
    executable_path: Option<&str>,
    live: &LiveRead,
) -> Result<Option<OriginalStateDto>, ServiceError> {
    let (Some(name), Some(predefined), Some(application), Some(path)) = (
        live.profile_name.as_deref(),
        live.profile_is_predefined,
        live.matched_application.as_ref(),
        executable_path,
    ) else {
        return Ok(None);
    };
    if !live.has_profile_for_exe || live.current_is_explicit.is_none() {
        return Ok(None);
    }
    let witness = serde_json::to_string(application).map_err(|error| {
        ServiceError::command_failed(format!(
            "could not encode live NVIDIA application identity: {error}"
        ))
    })?;
    let original = storage.get_nvapi_bound_setting_original(
        game_id,
        name,
        setting.nvapi_id(),
        path,
        predefined,
        &witness,
    )?;
    Ok(original.and_then(|state| {
        if state.present {
            state.value.map(|dword| OriginalStateDto {
                present: true,
                value: Some(value_descriptor(setting, dword)),
            })
        } else {
            Some(OriginalStateDto {
                present: false,
                value: None,
            })
        }
    }))
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

fn resolve_effective_exe_source(
    storage: &renderpilot_storage_sqlite::SqliteStorage,
    game_id: &str,
    effective_exe: Option<&str>,
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
