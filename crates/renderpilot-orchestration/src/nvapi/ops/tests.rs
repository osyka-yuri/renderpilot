use std::assert_matches;
use std::collections::HashMap;
use std::path::PathBuf;

use renderpilot_application::GameRepository;
use renderpilot_detection::NVNGX_DLSS_FILE_NAME;
use renderpilot_domain::{
    GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
};
use renderpilot_nvapi::{
    CatalogReadiness, DlssDllKind, DlssVersion, NvapiSetting, SettingContext, setting::DllInfo,
};
use renderpilot_storage_sqlite::{
    NvapiGameSettingPreparation, NvapiSettingOperationCompletion, NvapiSettingOperationScope,
    NvapiSettingState, SqliteStorage,
};

use super::assemble::{
    assemble_response, build_available_values, known_preset_set, supported_preset_set,
};
use super::live::LiveRead;
use super::target::{SettingTarget, WriteOp};
use super::write::{
    ensure_dll_setting_catalog_ready, ensure_profile_owner_matches_game, resolve_revert_op,
    validate_value_supported,
};
use crate::dlss::settings_catalog::{self, CatalogSetting};
use crate::nvapi::dto::{CatalogReadinessDto, NvapiWarningDto};

#[test]
fn sr_dll_file_name_matches_detection_constant() {
    // nvapi cannot depend on detection; keep the SR name in lockstep.
    assert_eq!(DlssDllKind::Sr.file_name(), NVNGX_DLSS_FILE_NAME);
}

#[test]
fn setting_write_rejects_a_profile_owned_by_another_game() {
    let owner = renderpilot_storage_sqlite::NvapiOwnedProfileRow {
        game_id: "manual:game-b".to_owned(),
        profile_name: "RenderPilot Shared Profile".to_owned(),
        binding_path: "C:/Games/Shared/Game.exe".to_owned(),
        application_witness_json: "{}".to_owned(),
        composition_json: "{}".to_owned(),
        state: "active".to_owned(),
        updated_at: 0,
    };

    let error = ensure_profile_owner_matches_game("manual:game-a", Some(&owner))
        .expect_err("a different game cannot write an owned NVIDIA profile");

    assert!(error.to_string().contains("manual:game-b"));
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
        effective_exe_path: Some("/tmp/game.exe".to_owned()),
    }
}

fn seed_nvapi_setting_claim(
    storage: &SqliteStorage,
    game_id: &str,
    setting: &dyn NvapiSetting,
    original: (bool, Option<u32>),
    expected: (bool, Option<u32>),
) {
    let game_id = GameId::new(game_id.to_owned()).expect("game id");
    let identity = GameIdentity::new(game_id.clone(), "Claim fixture", Launcher::Manual)
        .expect("game identity");
    let game = GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new("/tmp".to_owned()).expect("install path"),
    );
    storage.upsert_game(&game).expect("persist game");

    let profile_name = "NVIDIA Claim Fixture";
    let profile_identity_json = r#"{"name":"NVIDIA Claim Fixture"}"#;
    let executable_path = "/tmp/game.exe";
    let application_witness_json = r#"{"app_name":"/tmp/game.exe"}"#;
    let op_id = format!("op-{}", game_id.as_str().replace(':', "-"));
    storage
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id: &op_id,
            game_id: game_id.as_str(),
            profile_name,
            target_kind: "external",
            profile_is_predefined: false,
            profile_identity_json,
            executable_path,
            application_witness_json,
            setting_id: setting.nvapi_id(),
            original: NvapiSettingState {
                present: original.0,
                value: original.1,
            },
            before: NvapiSettingState {
                present: original.0,
                value: original.1,
            },
            after: NvapiSettingState {
                present: expected.0,
                value: expected.1,
            },
        })
        .expect("record claim intent");
    storage
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: &op_id,
            scope: NvapiSettingOperationScope::Game {
                game_id: game_id.as_str(),
            },
            target_id: profile_name,
            setting_id: setting.nvapi_id(),
            expected: NvapiSettingState {
                present: expected.0,
                value: expected.1,
            },
            profile_identity_json,
            composition_json: None,
        })
        .expect("confirm claim");
}

fn response_with_claim(
    game_id: &str,
    original: (bool, Option<u32>),
    expected: (bool, Option<u32>),
    current: u32,
    current_is_explicit: bool,
) -> crate::nvapi::dto::SettingStateResponse {
    let def = settings_catalog::find("dlss_sr_render_preset").expect("catalog has SR preset");
    let setting = CatalogSetting::new(def);
    let storage = SqliteStorage::in_memory().expect("storage");
    seed_nvapi_setting_claim(&storage, game_id, &setting, original, expected);
    let ctx = ctx_with_sr_dll(DlssVersion::new(310, 1, 0, 0));
    assemble_response(
        &setting,
        &ctx,
        &storage,
        &SettingTarget::Game { game_id },
        &LiveRead {
            current,
            current_is_explicit: Some(current_is_explicit),
            predefined: None,
            is_current_predefined: current == setting.default_dword(),
            has_profile_for_exe: true,
            profile_name: Some("NVIDIA Claim Fixture".to_owned()),
            warning: None,
        },
    )
    .expect("assemble response")
}

#[test]
fn response_exposes_claimed_explicit_original_for_exact_live_profile_and_path() {
    let response = response_with_claim(
        "manual:nvapi-original-explicit",
        (true, Some(7)),
        (true, Some(5)),
        5,
        true,
    );

    let original = response.original.expect("claimed original");
    assert!(original.present);
    assert_eq!(original.value.expect("explicit original").dword, 7);
    assert_eq!(response.current_is_explicit, Some(true));
}

#[test]
fn response_exposes_absent_original_as_presence_without_a_value() {
    let response = response_with_claim(
        "manual:nvapi-original-absent",
        (false, None),
        (true, Some(5)),
        5,
        true,
    );

    let original = response.original.expect("claimed original");
    assert!(!original.present);
    assert!(original.value.is_none());
}

#[test]
fn response_preserves_presence_difference_when_dword_equals_default() {
    let def = settings_catalog::find("dlss_sr_render_preset").expect("catalog has SR preset");
    let default = CatalogSetting::new(def).default_dword();
    let response = response_with_claim(
        "manual:nvapi-original-presence-differs",
        (false, None),
        (true, Some(default)),
        default,
        true,
    );

    let original = response.original.expect("claimed original");
    assert!(!original.present);
    assert_eq!(response.current.dword, default);
    assert_eq!(response.current_is_explicit, Some(true));
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
        &LiveRead::unset(setting.default_dword()),
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
        &LiveRead::unavailable(setting.default_dword(), Some(NvapiWarningDto::NoExecutable)),
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
        &LiveRead::unset(setting.default_dword()),
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
        &LiveRead::unset(setting.default_dword()),
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
    let op = resolve_revert_op(&SettingTarget::Global, "predefined")
        .expect("predefined revert is always valid");
    assert_matches!(op, WriteOp::Delete);
}

#[test]
fn global_revert_to_original_is_rejected() {
    // Original restore is scoped to a game's exact profile claim, so a
    // global request must be refused rather than silently no-op.
    assert!(resolve_revert_op(&SettingTarget::Global, "original").is_err());
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
