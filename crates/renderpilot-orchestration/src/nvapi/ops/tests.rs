use std::assert_matches;
use std::collections::HashMap;
use std::path::PathBuf;

use renderpilot_detection::NVNGX_DLSS_FILE_NAME;
use renderpilot_nvapi::{
    CatalogReadiness, DlssDllKind, DlssVersion, NvapiSetting, SettingContext, setting::DllInfo,
};

use super::assemble::{
    assemble_response, build_available_values, known_preset_set, supported_preset_set,
};
use super::live::LiveRead;
use super::target::{SettingTarget, WriteOp};
use super::write::{ensure_dll_setting_catalog_ready, resolve_revert_op, validate_value_supported};
use crate::dlss::settings_catalog::{self, CatalogSetting};
use crate::nvapi::dto::{CatalogReadinessDto, NvapiWarningDto};

#[test]
fn sr_dll_file_name_matches_detection_constant() {
    // nvapi cannot depend on detection; keep the SR name in lockstep.
    assert_eq!(DlssDllKind::Sr.file_name(), NVNGX_DLSS_FILE_NAME);
}

fn ctx_with_sr_dll(version: DlssVersion) -> SettingContext {
    let mut dlls = HashMap::new();
    dlls.insert(
        DlssDllKind::Sr,
        DllInfo {
            path: PathBuf::from("nvngx_dlss.dll"),
            version: Some(version),
        },
    );
    SettingContext {
        game_install_dir: PathBuf::from("/tmp"),
        dlls,
        catalog_readiness: CatalogReadiness::Ready,
        effective_exe: Some("game.exe".to_owned()),
    }
}

#[test]
fn unknown_dll_version_leaves_version_specific_validation_permissive() {
    let def = settings_catalog::find("dlss_sr_render_preset").expect("catalog has SR preset");
    let setting = CatalogSetting::new(def);
    let mut ctx = ctx_with_sr_dll(DlssVersion::new(310, 1, 0, 0));
    ctx.dlls.get_mut(&DlssDllKind::Sr).expect("SR DLL").version = None;
    let preset_a = def
        .values
        .iter()
        .find(|value| value.wire == "a")
        .unwrap()
        .dword;

    assert!(validate_value_supported(&setting, preset_a, &ctx).is_ok());
}

#[test]
fn unready_catalog_rejects_dll_dependent_mutations() {
    let def = settings_catalog::find("dlss_sr_render_preset").expect("catalog has SR preset");
    let setting = CatalogSetting::new(def);
    let mut ctx = ctx_with_sr_dll(DlssVersion::new(310, 1, 0, 0));
    ctx.catalog_readiness = CatalogReadiness::NotReady;

    assert_eq!(
        ensure_dll_setting_catalog_ready(&setting, &ctx),
        Err(crate::ServiceError::NvapiCatalogNotReady)
    );
}

#[test]
fn unready_catalog_response_never_projects_stale_dll_state() {
    let def = settings_catalog::find("dlss_sr_render_preset").expect("catalog has SR preset");
    let setting = CatalogSetting::new(def);
    let mut ctx = ctx_with_sr_dll(DlssVersion::new(310, 1, 0, 0));
    ctx.catalog_readiness = CatalogReadiness::NotReady;
    ctx.dlls.clear();
    let storage = renderpilot_storage_sqlite::SqliteStorage::in_memory().expect("sqlite storage");

    let response = assemble_response(
        &setting,
        &ctx,
        &storage,
        &SettingTarget::Game {
            game_id: "steam:unready",
        },
        LiveRead::unset(setting.default_dword()),
    )
    .expect("response");

    assert_eq!(response.catalog_readiness, CatalogReadinessDto::NotReady);
    assert!(response.dll_info.is_none());
    assert_eq!(response.warnings, vec![NvapiWarningDto::CatalogNotReady]);
}

#[test]
fn unready_catalog_suppresses_a_secondary_live_warning() {
    let def = settings_catalog::find("dlss_sr_render_preset").expect("catalog has SR preset");
    let setting = CatalogSetting::new(def);
    let mut ctx = ctx_with_sr_dll(DlssVersion::new(310, 1, 0, 0));
    ctx.catalog_readiness = CatalogReadiness::NotReady;
    let storage = renderpilot_storage_sqlite::SqliteStorage::in_memory().expect("sqlite storage");

    let response = assemble_response(
        &setting,
        &ctx,
        &storage,
        &SettingTarget::Game {
            game_id: "steam:unready-live-warning",
        },
        LiveRead::unavailable(setting.default_dword(), Some(NvapiWarningDto::NoExecutable)),
    )
    .expect("response");

    assert_eq!(response.warnings, vec![NvapiWarningDto::CatalogNotReady]);
}

#[test]
fn ready_catalog_without_a_detected_dll_reports_only_no_dll() {
    let def = settings_catalog::find("dlss_sr_render_preset").expect("catalog has SR preset");
    let setting = CatalogSetting::new(def);
    let mut ctx = ctx_with_sr_dll(DlssVersion::new(310, 1, 0, 0));
    ctx.dlls.clear();
    let storage = renderpilot_storage_sqlite::SqliteStorage::in_memory().expect("sqlite storage");

    let response = assemble_response(
        &setting,
        &ctx,
        &storage,
        &SettingTarget::Game {
            game_id: "steam:ready-no-dll",
        },
        LiveRead::unset(setting.default_dword()),
    )
    .expect("response");

    assert_eq!(response.catalog_readiness, CatalogReadinessDto::Ready);
    assert!(response.dll_info.is_none());
    assert_eq!(response.warnings, vec![NvapiWarningDto::NoDll]);
}

#[test]
fn ready_unknown_dll_version_is_present_and_warned_without_no_manifest() {
    let def = settings_catalog::find("dlss_sr_render_preset").expect("catalog has SR preset");
    let setting = CatalogSetting::new(def);
    let mut ctx = ctx_with_sr_dll(DlssVersion::new(310, 1, 0, 0));
    ctx.dlls.get_mut(&DlssDllKind::Sr).expect("SR DLL").version = None;
    let storage = renderpilot_storage_sqlite::SqliteStorage::in_memory().expect("sqlite storage");

    let response = assemble_response(
        &setting,
        &ctx,
        &storage,
        &SettingTarget::Game {
            game_id: "steam:unknown-version",
        },
        LiveRead::unset(setting.default_dword()),
    )
    .expect("response");

    assert_eq!(response.catalog_readiness, CatalogReadinessDto::Ready);
    assert_eq!(response.dll_info.expect("present DLL").version, None);
    assert_eq!(response.warnings, vec![NvapiWarningDto::DllVersionUnknown]);
}

/// On DLSS 4 the SR manifest only supports presets {0, 6, 10, 11}, but the
/// "recommended"/Latest sentinel and preset letters beyond the manifest's
/// table must stay selectable -- only manifest-managed-but-unsupported
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
    // Manifest-managed but not supported on DLSS 4 -> greyed.
    assert!(!supported("a")); // preset A = 1
    // Not managed by the manifest -> always selectable.
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
    assert_matches!(op, WriteOp::Delete);
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
    // presets like "a") is allowed -- a global setting is not tied to one
    // game's DLL version.
    let def = settings_catalog::find("dlss_sr_render_preset").expect("catalog has SR preset");
    let setting = CatalogSetting::new(def);
    let ctx = crate::nvapi::resolve::global_setting_context();
    let preset_a = def.values.iter().find(|v| v.wire == "a").unwrap().dword;
    assert!(validate_value_supported(&setting, preset_a, &ctx).is_ok());
}
