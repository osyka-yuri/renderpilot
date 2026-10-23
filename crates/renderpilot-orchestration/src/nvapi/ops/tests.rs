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

use super::write::{
    GamePreparationScope, SettingPreparationDispatch, dispatch_setting_preparation,
};
use rusqlite::OptionalExtension;

const GLOBAL_PROFILE: &str = "Base Profile";
const TEST_SETTING_ID: u32 = 0x10E41E11;

struct TestDbFixture {
    _dir: tempfile::TempDir,
    db_path: std::path::PathBuf,
    storage: SqliteStorage,
}

impl TestDbFixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("catalog.db");
        let storage = SqliteStorage::open(&db_path).expect("open storage");
        Self {
            _dir: dir,
            db_path,
            storage,
        }
    }

    fn read_claim(
        &self,
        target_id: &str,
        setting_id: u32,
    ) -> Option<(bool, Option<u32>, bool, Option<u32>)> {
        let conn = rusqlite::Connection::open_with_flags(
            &self.db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("open read-only sqlite connection");
        conn.query_row(
            "SELECT original_present, original_value, expected_present, expected_value
               FROM nvapi_target_setting_claims
              WHERE target_id = ?1 AND setting_id = ?2",
            rusqlite::params![target_id, setting_id],
            |row| {
                Ok((
                    row.get::<_, i32>(0)? != 0,
                    row.get::<_, Option<u32>>(1)?,
                    row.get::<_, i32>(2)? != 0,
                    row.get::<_, Option<u32>>(3)?,
                ))
            },
        )
        .optional()
        .expect("query claim")
    }

    fn count_game_claim_refs(&self) -> usize {
        let conn = rusqlite::Connection::open_with_flags(
            &self.db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("open read-only sqlite connection");
        conn.query_row("SELECT COUNT(*) FROM nvapi_game_claim_refs", [], |row| {
            row.get::<_, i64>(0).map(|n| n as usize)
        })
        .expect("count game claim refs")
    }

    fn read_target_identity_json(&self, target_id: &str) -> Option<String> {
        let conn = rusqlite::Connection::open_with_flags(
            &self.db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("open read-only sqlite connection");
        conn.query_row(
            "SELECT identity_json FROM nvapi_drs_targets WHERE target_id = ?1",
            [target_id],
            |row| row.get(0),
        )
        .optional()
        .expect("query target identity")
    }
}

fn global_identity_json(
    name: &str,
    app_count: u32,
    setting_count: u32,
    is_predefined: bool,
) -> String {
    serde_json::json!({
        "name": name,
        "is_predefined": is_predefined,
        "application_count": app_count,
        "setting_count": setting_count,
    })
    .to_string()
}

fn seed_test_global_claim(
    storage: &SqliteStorage,
    identity_json: &str,
    setting_id: u32,
    original: NvapiSettingState,
    expected: NvapiSettingState,
) {
    storage
        .prepare_nvapi_global_setting_operation(
            renderpilot_storage_sqlite::NvapiGlobalSettingPreparation {
                op_id: "op-seed-global",
                profile_name: GLOBAL_PROFILE,
                profile_is_predefined: true,
                profile_identity_json: identity_json,
                setting_id,
                original,
                before: original,
                after: expected,
            },
        )
        .expect("seed prepare global setting");
    storage
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: "op-seed-global",
            scope: NvapiSettingOperationScope::Global,
            target_id: GLOBAL_PROFILE,
            setting_id,
            expected,
            profile_identity_json: identity_json,
            composition_json: None,
        })
        .expect("seed finish global setting");
}

#[test]
fn orchestration_dispatch_global_set_succeeds_after_driver_reset() {
    let fixture = TestDbFixture::new();
    let storage = &fixture.storage;
    let identity = global_identity_json(GLOBAL_PROFILE, 50, 120, true);
    let original = NvapiSettingState {
        present: false,
        value: None,
    };
    let expected = NvapiSettingState {
        present: true,
        value: Some(1),
    };
    seed_test_global_claim(storage, &identity, TEST_SETTING_ID, original, expected);

    assert_eq!(
        fixture.read_claim(GLOBAL_PROFILE, TEST_SETTING_ID),
        Some((false, None, true, Some(1))),
        "seeded global claim must record original NotExplicit and expected Explicit(1)"
    );

    let before = NvapiSettingState {
        present: false,
        value: None,
    };

    dispatch_setting_preparation(
        storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Global,
            op_id: "op-auth-reset",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before,
            op: WriteOp::Set(2),
            game_scope: None,
        },
    )
    .expect("dispatch global Set after driver reset");

    let pending = storage.list_pending_nvapi_operations().expect("pending");
    assert_eq!(
        pending.len(),
        1,
        "exactly one pending DRS operation must exist"
    );
    let pending_op = &pending[0];
    assert_eq!(pending_op.op_id, "op-auth-reset");
    assert_eq!(
        pending_op.kind, "setting",
        "operation kind must be 'setting'"
    );
    assert!(
        pending_op.game_id.is_none(),
        "game_id must be absent for global operation"
    );
    assert_eq!(
        pending_op.target_id.as_deref(),
        Some(GLOBAL_PROFILE),
        "target_id must match trusted Base Profile name"
    );
    assert_eq!(
        pending_op.phase, "intent",
        "operation must be in 'intent' phase"
    );
    assert!(
        pending_op.observed_json.is_none(),
        "observed_json must be none in intent phase"
    );

    let before_obj: serde_json::Value =
        serde_json::from_str(&pending_op.before_json).expect("valid before json");
    assert_eq!(before_obj["setting_id"], TEST_SETTING_ID);
    assert_eq!(
        before_obj["present"], false,
        "before_json must represent NotExplicit"
    );
    assert_eq!(before_obj["value"], serde_json::Value::Null);
    assert_eq!(
        before_obj["claim_was_new"], false,
        "claim_was_new must be false"
    );
    assert_eq!(before_obj["reference_was_new"], false);

    let after_obj: serde_json::Value =
        serde_json::from_str(&pending_op.after_json).expect("valid after json");
    assert_eq!(after_obj["setting_id"], TEST_SETTING_ID);
    assert_eq!(
        after_obj["present"], true,
        "after_json must represent Explicit(2)"
    );
    assert_eq!(after_obj["value"], 2);

    assert_eq!(
        fixture.count_game_claim_refs(),
        0,
        "no game references must be inserted"
    );

    let claim_during_prep = fixture
        .read_claim(GLOBAL_PROFILE, TEST_SETTING_ID)
        .expect("claim must exist");
    assert_eq!(
        claim_during_prep,
        (false, None, true, Some(1)),
        "original must remain NotExplicit and expected must remain Explicit(1) before finalization"
    );
    assert_ne!(
        claim_during_prep.3,
        Some(2),
        "expected value MUST NOT become Explicit(2) before verified finalization"
    );

    storage
        .cancel_nvapi_setting_operation("op-auth-reset")
        .expect("cancel operation");

    assert_eq!(
        fixture.read_claim(GLOBAL_PROFILE, TEST_SETTING_ID),
        Some((false, None, true, Some(1))),
        "claim state must be preserved after cancellation"
    );

    dispatch_setting_preparation(
        storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Global,
            op_id: "op-verify-expected-1",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: true,
                value: Some(1),
            },
            op: WriteOp::Delete,
            game_scope: None,
        },
    )
    .expect("strict CAS succeeds with prior expected 1");
    storage
        .cancel_nvapi_setting_operation("op-verify-expected-1")
        .expect("cancel verify");

    dispatch_setting_preparation(
        storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Global,
            op_id: "op-auth-reset-2",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before,
            op: WriteOp::Set(2),
            game_scope: None,
        },
    )
    .expect("prep authorized set 2");

    storage
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: "op-auth-reset-2",
            scope: NvapiSettingOperationScope::Global,
            target_id: GLOBAL_PROFILE,
            setting_id: TEST_SETTING_ID,
            expected: NvapiSettingState {
                present: true,
                value: Some(2),
            },
            profile_identity_json: &identity,
            composition_json: None,
        })
        .expect("finish operation");

    assert_eq!(
        fixture.read_claim(GLOBAL_PROFILE, TEST_SETTING_ID),
        Some((false, None, true, Some(2))),
        "after verified finalization, expected must be Explicit(2)"
    );

    dispatch_setting_preparation(
        storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Global,
            op_id: "op-verify-expected-2",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: true,
                value: Some(2),
            },
            op: WriteOp::Delete,
            game_scope: None,
        },
    )
    .expect("strict CAS succeeds with updated expected 2");
}

#[test]
fn orchestration_dispatch_global_set_cancellation_preserves_claim_and_target_identity() {
    let fixture = TestDbFixture::new();
    let storage = &fixture.storage;
    let initial_identity = global_identity_json(GLOBAL_PROFILE, 50, 120, true);
    let original = NvapiSettingState {
        present: false,
        value: None,
    };
    let expected = NvapiSettingState {
        present: true,
        value: Some(1),
    };
    seed_test_global_claim(
        storage,
        &initial_identity,
        TEST_SETTING_ID,
        original,
        expected,
    );

    let before = NvapiSettingState {
        present: false,
        value: None,
    };

    dispatch_setting_preparation(
        storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Global,
            op_id: "op-cancel-test",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &initial_identity,
            setting_id: TEST_SETTING_ID,
            before,
            op: WriteOp::Set(2),
            game_scope: None,
        },
    )
    .expect("dispatch preparation");

    storage
        .cancel_nvapi_setting_operation("op-cancel-test")
        .expect("cancel operation");

    assert_eq!(
        fixture.read_claim(GLOBAL_PROFILE, TEST_SETTING_ID),
        Some((false, None, true, Some(1))),
        "cancellation must preserve old original and expected values"
    );

    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending")
            .is_empty(),
        "pending operation row must be removed upon cancellation"
    );

    assert_eq!(
        fixture.read_target_identity_json(GLOBAL_PROFILE),
        Some(initial_identity),
        "target identity receipt must be unchanged upon cancellation"
    );

    assert_eq!(
        fixture.count_game_claim_refs(),
        0,
        "no game claim references must exist"
    );
}

#[test]
fn orchestration_dispatch_global_set_finalization_updates_expected_and_target_identity() {
    let fixture = TestDbFixture::new();
    let storage = &fixture.storage;
    let initial_identity = global_identity_json(GLOBAL_PROFILE, 50, 120, true);
    let updated_identity = global_identity_json(GLOBAL_PROFILE, 55, 125, true);
    let original = NvapiSettingState {
        present: false,
        value: None,
    };
    let expected = NvapiSettingState {
        present: true,
        value: Some(1),
    };
    seed_test_global_claim(
        storage,
        &initial_identity,
        TEST_SETTING_ID,
        original,
        expected,
    );

    let before = NvapiSettingState {
        present: false,
        value: None,
    };

    dispatch_setting_preparation(
        storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Global,
            op_id: "op-finalize-test",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &updated_identity,
            setting_id: TEST_SETTING_ID,
            before,
            op: WriteOp::Set(2),
            game_scope: None,
        },
    )
    .expect("dispatch preparation");

    storage
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: "op-finalize-test",
            scope: NvapiSettingOperationScope::Global,
            target_id: GLOBAL_PROFILE,
            setting_id: TEST_SETTING_ID,
            expected: NvapiSettingState {
                present: true,
                value: Some(2),
            },
            profile_identity_json: &updated_identity,
            composition_json: None,
        })
        .expect("finish operation");

    let claim = fixture
        .read_claim(GLOBAL_PROFILE, TEST_SETTING_ID)
        .expect("claim must exist");
    assert_eq!(
        claim,
        (false, None, true, Some(2)),
        "original must remain NotExplicit and expected must become Explicit(2)"
    );

    assert_eq!(
        fixture.read_target_identity_json(GLOBAL_PROFILE),
        Some(updated_identity),
        "target identity receipt must be updated to verified after-state"
    );

    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending")
            .is_empty(),
        "pending operation row must be removed upon finalization"
    );

    assert_eq!(fixture.count_game_claim_refs(), 0);
}

#[test]
fn orchestration_dispatch_global_set_succeeds_with_count_drift() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let initial_identity = global_identity_json(GLOBAL_PROFILE, 50, 120, true);
    let original = NvapiSettingState {
        present: false,
        value: None,
    };
    let expected = NvapiSettingState {
        present: true,
        value: Some(1),
    };
    seed_test_global_claim(
        &storage,
        &initial_identity,
        TEST_SETTING_ID,
        original,
        expected,
    );

    let drifted_identity = global_identity_json(GLOBAL_PROFILE, 55, 125, true);
    let before = NvapiSettingState {
        present: true,
        value: Some(1),
    };

    dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Global,
            op_id: "op-auth-count-drift",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &drifted_identity,
            setting_id: TEST_SETTING_ID,
            before,
            op: WriteOp::Set(3),
            game_scope: None,
        },
    )
    .expect("dispatch global Set with count drift");

    let pending = storage.list_pending_nvapi_operations().expect("pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].op_id, "op-auth-count-drift");
}

#[test]
fn orchestration_dispatch_global_set_succeeds_with_combined_drift() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let initial_identity = global_identity_json(GLOBAL_PROFILE, 50, 120, true);
    let original = NvapiSettingState {
        present: false,
        value: None,
    };
    let expected = NvapiSettingState {
        present: true,
        value: Some(1),
    };
    seed_test_global_claim(
        &storage,
        &initial_identity,
        TEST_SETTING_ID,
        original,
        expected,
    );

    let drifted_identity = global_identity_json(GLOBAL_PROFILE, 55, 125, true);
    let before = NvapiSettingState {
        present: false,
        value: None,
    };

    dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Global,
            op_id: "op-auth-combined-drift",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &drifted_identity,
            setting_id: TEST_SETTING_ID,
            before,
            op: WriteOp::Set(5),
            game_scope: None,
        },
    )
    .expect("dispatch global Set with combined drift");

    let pending = storage.list_pending_nvapi_operations().expect("pending");
    assert_eq!(pending.len(), 1);
    assert!(pending[0].after_json.contains("\"value\":5"));
}

#[test]
fn orchestration_dispatch_global_set_zero_remains_distinct_from_delete() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let identity = global_identity_json(GLOBAL_PROFILE, 50, 120, true);
    let original = NvapiSettingState {
        present: false,
        value: None,
    };
    let expected = NvapiSettingState {
        present: true,
        value: Some(1),
    };
    seed_test_global_claim(&storage, &identity, TEST_SETTING_ID, original, expected);

    let before = NvapiSettingState {
        present: true,
        value: Some(1),
    };

    dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Global,
            op_id: "op-auth-zero",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before,
            op: WriteOp::Set(0),
            game_scope: None,
        },
    )
    .expect("dispatch global Set(0)");

    let pending = storage.list_pending_nvapi_operations().expect("pending");
    assert_eq!(pending.len(), 1);
    let after_obj: serde_json::Value =
        serde_json::from_str(&pending[0].after_json).expect("valid after json");
    assert_eq!(after_obj["present"], true);
    assert_eq!(after_obj["value"], 0);
}

#[test]
fn orchestration_dispatch_global_delete_preserves_strict_preparation() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let identity = global_identity_json(GLOBAL_PROFILE, 50, 120, true);
    let original = NvapiSettingState {
        present: false,
        value: None,
    };
    let expected = NvapiSettingState {
        present: true,
        value: Some(1),
    };
    seed_test_global_claim(&storage, &identity, TEST_SETTING_ID, original, expected);

    let stale_before = NvapiSettingState {
        present: false,
        value: None,
    };

    let error = dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Global,
            op_id: "op-delete-stale",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before: stale_before,
            op: WriteOp::Delete,
            game_scope: None,
        },
    )
    .expect_err("global Delete with stale before state must be rejected");

    assert!(error.to_string().contains("conflict"));
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending")
            .is_empty(),
        "failed global Delete must not create a pending intent"
    );

    let matching_before = NvapiSettingState {
        present: true,
        value: Some(1),
    };
    dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Global,
            op_id: "op-delete-matching",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before: matching_before,
            op: WriteOp::Delete,
            game_scope: None,
        },
    )
    .expect("global Delete with matching before state must succeed");

    let pending = storage.list_pending_nvapi_operations().expect("pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].op_id, "op-delete-matching");
    assert!(pending[0].after_json.contains("\"present\":false"));
}

#[test]
fn orchestration_dispatch_game_setting_preserves_strict_preparation() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = "manual:game-non-regression";
    let def = settings_catalog::find("dlss_sr_render_preset").expect("catalog setting");
    let setting = CatalogSetting::new(def);

    seed_nvapi_setting_claim(&storage, game_id, &setting, (false, None), (true, Some(1)));

    let stale_before = NvapiSettingState {
        present: false,
        value: None,
    };

    let game_scope = GamePreparationScope {
        game_id,
        target_kind: "external",
        executable_path: "/tmp/game.exe",
        application_witness_json: r#"{"app_name":"/tmp/game.exe"}"#,
    };

    // Game-scoped setting preparation requires strict CAS against expected state, disallowing authorized global reapply.
    let err = dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Game { game_id },
            op_id: "op-game-stale",
            profile_name: "NVIDIA Claim Fixture",
            profile_is_predefined: false,
            profile_identity_json: r#"{"name":"NVIDIA Claim Fixture"}"#,
            setting_id: setting.nvapi_id(),
            before: stale_before,
            op: WriteOp::Set(2),
            game_scope: Some(game_scope),
        },
    )
    .expect_err("game Set with stale before state must fail CAS check");

    assert!(err.to_string().contains("outside RenderPilot"));
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending")
            .is_empty(),
        "failed game Set must not create a pending intent"
    );
}

fn seed_nvapi_predefined_game_setting_claim(
    storage: &SqliteStorage,
    game_id: &str,
    profile_name: &str,
    setting_id: u32,
) {
    let gid = GameId::new(game_id.to_owned()).expect("game id");
    let identity =
        GameIdentity::new(gid, "Predefined Game", Launcher::Manual).expect("game identity");
    let game = GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new("/tmp".to_owned()).expect("install path"),
    );
    storage.upsert_game(&game).expect("persist game");

    let profile_identity_json = serde_json::json!({
        "name": profile_name,
        "is_predefined": true,
        "application_count": 1,
        "setting_count": 10,
    })
    .to_string();
    let op_id = format!("op-{}", game_id.replace(':', "-"));

    storage
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id: &op_id,
            game_id,
            profile_name,
            target_kind: "external",
            profile_is_predefined: true,
            profile_identity_json: &profile_identity_json,
            executable_path: "/tmp/game.exe",
            application_witness_json: r#"{"app_name":"/tmp/game.exe"}"#,
            setting_id,
            original: NvapiSettingState {
                present: false,
                value: None,
            },
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            after: NvapiSettingState {
                present: true,
                value: Some(1),
            },
        })
        .expect("prep game setting");

    storage
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: &op_id,
            scope: NvapiSettingOperationScope::Game { game_id },
            target_id: profile_name,
            setting_id,
            expected: NvapiSettingState {
                present: true,
                value: Some(1),
            },
            profile_identity_json: &profile_identity_json,
            composition_json: None,
        })
        .expect("finish game setting");
}

#[test]
fn orchestration_dispatch_safety_guards_reject_game_targets_pending_overlap_and_non_predefined() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let identity = global_identity_json(GLOBAL_PROFILE, 50, 120, true);
    let original = NvapiSettingState {
        present: false,
        value: None,
    };
    let expected = NvapiSettingState {
        present: true,
        value: Some(1),
    };
    seed_test_global_claim(&storage, &identity, TEST_SETTING_ID, original, expected);

    let game_target_profile = "Predefined Game Profile";
    let game_setting_id = 0x10E41E22;
    seed_nvapi_predefined_game_setting_claim(
        &storage,
        "manual:game-associated",
        game_target_profile,
        game_setting_id,
    );

    let game_target_identity = serde_json::json!({
        "name": game_target_profile,
        "is_predefined": true,
        "application_count": 1,
        "setting_count": 10,
    })
    .to_string();

    let err_game_ref = dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Global,
            op_id: "op-safety-1",
            profile_name: game_target_profile,
            profile_is_predefined: true,
            profile_identity_json: &game_target_identity,
            setting_id: game_setting_id,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            op: WriteOp::Set(2),
            game_scope: None,
        },
    )
    .expect_err("game-associated target must be rejected");
    assert!(
        err_game_ref
            .to_string()
            .contains("associated with game setting claims")
    );
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending")
            .is_empty(),
        "failed preparation on game-associated target must not leave a pending intent"
    );

    let err_not_predef = dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Global,
            op_id: "op-safety-2",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: false,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            op: WriteOp::Set(2),
            game_scope: None,
        },
    )
    .expect_err("non-predefined global profile must be rejected");
    assert!(err_not_predef.to_string().contains("predefined"));
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending")
            .is_empty(),
        "failed preparation on non-predefined global profile must not leave a pending intent"
    );

    dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Global,
            op_id: "op-safety-in-flight",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            op: WriteOp::Set(2),
            game_scope: None,
        },
    )
    .expect("first preparation succeeds");

    let err_overlap = dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Global,
            op_id: "op-safety-overlapping",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            op: WriteOp::Set(3),
            game_scope: None,
        },
    )
    .expect_err("overlapping preparation must be rejected");
    assert!(err_overlap.to_string().contains("overlaps this profile"));
    let pending = storage.list_pending_nvapi_operations().expect("pending");
    assert_eq!(
        pending.len(),
        1,
        "failed overlapping preparation must not create an additional pending row"
    );
    assert_eq!(pending[0].op_id, "op-safety-in-flight");

    storage
        .mark_nvapi_operation_conflict("op-safety-in-flight", r#"{"diverged":true}"#)
        .expect("mark conflict");
    let pending_conflict = storage.list_pending_nvapi_operations().expect("pending");
    assert_eq!(pending_conflict[0].phase, "conflict");

    let err_conflict = dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Global,
            op_id: "op-safety-conflict-blocked",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            op: WriteOp::Set(4),
            game_scope: None,
        },
    )
    .expect_err("target in conflict must reject global Set");
    assert!(err_conflict.to_string().contains("overlaps this profile"));
    let pending_after_conflict = storage.list_pending_nvapi_operations().expect("pending");
    assert_eq!(
        pending_after_conflict.len(),
        1,
        "failed preparation under conflict must not insert a pending row"
    );
    assert_eq!(pending_after_conflict[0].op_id, "op-safety-in-flight");
}

#[test]
fn orchestration_dispatch_noop_preserves_claim_and_pending_intent() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let identity = global_identity_json(GLOBAL_PROFILE, 50, 120, true);
    let original = NvapiSettingState {
        present: false,
        value: None,
    };
    let expected = NvapiSettingState {
        present: true,
        value: Some(2),
    };
    seed_test_global_claim(&storage, &identity, TEST_SETTING_ID, original, expected);

    let before = NvapiSettingState {
        present: true,
        value: Some(2),
    };

    dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Global,
            op_id: "op-auth-noop",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before,
            op: WriteOp::Set(2),
            game_scope: None,
        },
    )
    .expect("dispatch global Set when before == after");

    let pending = storage.list_pending_nvapi_operations().expect("pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].op_id, "op-auth-noop");
    assert_eq!(pending[0].phase, "intent");
    assert!(pending[0].before_json.contains("\"value\":2"));
    assert!(pending[0].after_json.contains("\"value\":2"));
}

#[test]
fn game_setting_preparation_delete_creates_pending_intent_with_correct_after_state() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = "manual:game-char-delete";
    let def = settings_catalog::find("dlss_sr_render_preset").expect("catalog setting");
    let setting = CatalogSetting::new(def);

    seed_nvapi_setting_claim(&storage, game_id, &setting, (false, None), (true, Some(1)));

    let before = NvapiSettingState {
        present: true,
        value: Some(1),
    };
    let game_scope = GamePreparationScope {
        game_id,
        target_kind: "external",
        executable_path: "/tmp/game.exe",
        application_witness_json: r#"{"app_name":"/tmp/game.exe"}"#,
    };

    dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Game { game_id },
            op_id: "op-game-char-delete",
            profile_name: "NVIDIA Claim Fixture",
            profile_is_predefined: false,
            profile_identity_json: r#"{"name":"NVIDIA Claim Fixture"}"#,
            setting_id: setting.nvapi_id(),
            before,
            op: WriteOp::Delete,
            game_scope: Some(game_scope),
        },
    )
    .expect("dispatch game Delete");

    let pending = storage.list_pending_nvapi_operations().expect("pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].op_id, "op-game-char-delete");
    assert_eq!(pending[0].game_id.as_deref(), Some(game_id));
    assert_eq!(
        pending[0].target_id.as_deref(),
        Some("NVIDIA Claim Fixture")
    );
    assert_eq!(pending[0].kind, "setting");
    assert_eq!(pending[0].phase, "intent");

    let after: serde_json::Value =
        serde_json::from_str(&pending[0].after_json).expect("valid after_json");
    assert_eq!(after["present"], false);
    assert_eq!(after["value"], serde_json::Value::Null);

    let claim = storage
        .get_nvapi_target_setting_claim(game_id, "NVIDIA Claim Fixture", setting.nvapi_id())
        .expect("query claim")
        .expect("claim must exist");
    assert!(!claim.original_present);
    assert_eq!(claim.original_value, None);
    assert!(claim.expected_present);
    assert_eq!(claim.expected_value, Some(1));
}

#[test]
fn game_setting_preparation_set_creates_pending_intent_with_strict_cas() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = "manual:game-char-set";
    let def = settings_catalog::find("dlss_sr_render_preset").expect("catalog setting");
    let setting = CatalogSetting::new(def);

    seed_nvapi_setting_claim(&storage, game_id, &setting, (false, None), (true, Some(1)));

    let before = NvapiSettingState {
        present: true,
        value: Some(1),
    };
    let game_scope = GamePreparationScope {
        game_id,
        target_kind: "external",
        executable_path: "/tmp/game.exe",
        application_witness_json: r#"{"app_name":"/tmp/game.exe"}"#,
    };

    dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Game { game_id },
            op_id: "op-game-char-set-zero",
            profile_name: "NVIDIA Claim Fixture",
            profile_is_predefined: false,
            profile_identity_json: r#"{"name":"NVIDIA Claim Fixture"}"#,
            setting_id: setting.nvapi_id(),
            before,
            op: WriteOp::Set(0),
            game_scope: Some(game_scope),
        },
    )
    .expect("dispatch game Set(0)");

    let pending = storage.list_pending_nvapi_operations().expect("pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].op_id, "op-game-char-set-zero");
    let after: serde_json::Value =
        serde_json::from_str(&pending[0].after_json).expect("valid after_json");
    assert_eq!(after["present"], true);
    assert_eq!(after["value"], 0);

    let storage_cas = SqliteStorage::in_memory().expect("storage cas");
    seed_nvapi_setting_claim(
        &storage_cas,
        game_id,
        &setting,
        (false, None),
        (true, Some(1)),
    );
    let stale_before = NvapiSettingState {
        present: false,
        value: None,
    };
    let err = dispatch_setting_preparation(
        &storage_cas,
        SettingPreparationDispatch {
            target: &SettingTarget::Game { game_id },
            op_id: "op-game-char-set-stale",
            profile_name: "NVIDIA Claim Fixture",
            profile_is_predefined: false,
            profile_identity_json: r#"{"name":"NVIDIA Claim Fixture"}"#,
            setting_id: setting.nvapi_id(),
            before: stale_before,
            op: WriteOp::Set(0),
            game_scope: Some(game_scope),
        },
    )
    .expect_err("stale before state must fail CAS check");
    assert!(err.to_string().contains("outside RenderPilot"));
    assert!(
        storage_cas
            .list_pending_nvapi_operations()
            .expect("pending")
            .is_empty(),
        "failed CAS check must not persist a pending intent"
    );
}

#[test]
fn game_setting_preparation_rejects_missing_game_scope_for_set_and_delete() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = "manual:game-char-missing-scope";
    let def = settings_catalog::find("dlss_sr_render_preset").expect("catalog setting");
    let setting = CatalogSetting::new(def);

    let before = NvapiSettingState {
        present: false,
        value: None,
    };

    let err_set = dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Game { game_id },
            op_id: "op-missing-scope-set",
            profile_name: "NVIDIA Claim Fixture",
            profile_is_predefined: false,
            profile_identity_json: r#"{"name":"NVIDIA Claim Fixture"}"#,
            setting_id: setting.nvapi_id(),
            before,
            op: WriteOp::Set(1),
            game_scope: None,
        },
    )
    .expect_err("game Set without game scope must fail");
    assert_eq!(
        err_set.to_string(),
        "game setting preparation requires game scope details"
    );

    let err_del = dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Game { game_id },
            op_id: "op-missing-scope-delete",
            profile_name: "NVIDIA Claim Fixture",
            profile_is_predefined: false,
            profile_identity_json: r#"{"name":"NVIDIA Claim Fixture"}"#,
            setting_id: setting.nvapi_id(),
            before,
            op: WriteOp::Delete,
            game_scope: None,
        },
    )
    .expect_err("game Delete without game scope must fail");
    assert_eq!(
        err_del.to_string(),
        "game setting preparation requires game scope details"
    );

    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending")
            .is_empty(),
        "missing game scope must not create a pending operation"
    );
}

#[test]
fn game_setting_preparation_rejects_mismatched_game_id_for_set_and_delete() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = "manual:game-target";
    let def = settings_catalog::find("dlss_sr_render_preset").expect("catalog setting");
    let setting = CatalogSetting::new(def);

    seed_nvapi_setting_claim(&storage, game_id, &setting, (false, None), (true, Some(1)));

    let initial_claim = storage
        .get_nvapi_target_setting_claim(game_id, "NVIDIA Claim Fixture", setting.nvapi_id())
        .expect("query claim")
        .expect("claim must exist");
    let expected_orig_pres = initial_claim.original_present;
    let expected_orig_val = initial_claim.original_value;
    let expected_exp_pres = initial_claim.expected_present;
    let expected_exp_val = initial_claim.expected_value;

    let before = NvapiSettingState {
        present: true,
        value: Some(1),
    };
    let mismatched_scope = GamePreparationScope {
        game_id: "manual:different-game",
        target_kind: "external",
        executable_path: "/tmp/game.exe",
        application_witness_json: r#"{"app_name":"/tmp/game.exe"}"#,
    };

    let err_set = dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Game { game_id },
            op_id: "op-mismatch-scope-set",
            profile_name: "NVIDIA Claim Fixture",
            profile_is_predefined: false,
            profile_identity_json: r#"{"name":"NVIDIA Claim Fixture"}"#,
            setting_id: setting.nvapi_id(),
            before,
            op: WriteOp::Set(1),
            game_scope: Some(mismatched_scope),
        },
    )
    .expect_err("game Set with mismatched game scope must fail");
    assert_eq!(
        err_set.to_string(),
        "game id mismatch in setting preparation"
    );

    let err_del = dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Game { game_id },
            op_id: "op-mismatch-scope-delete",
            profile_name: "NVIDIA Claim Fixture",
            profile_is_predefined: false,
            profile_identity_json: r#"{"name":"NVIDIA Claim Fixture"}"#,
            setting_id: setting.nvapi_id(),
            before,
            op: WriteOp::Delete,
            game_scope: Some(mismatched_scope),
        },
    )
    .expect_err("game Delete with mismatched game scope must fail");
    assert_eq!(
        err_del.to_string(),
        "game id mismatch in setting preparation"
    );

    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending")
            .is_empty(),
        "mismatched game scope must not create a pending operation"
    );

    let final_claim = storage
        .get_nvapi_target_setting_claim(game_id, "NVIDIA Claim Fixture", setting.nvapi_id())
        .expect("query claim")
        .expect("claim must exist");
    assert_eq!(final_claim.original_present, expected_orig_pres);
    assert_eq!(final_claim.original_value, expected_orig_val);
    assert_eq!(final_claim.expected_present, expected_exp_pres);
    assert_eq!(final_claim.expected_value, expected_exp_val);
}

#[test]
fn setting_preparation_rejects_unresolved_restore_original() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = "manual:game-char-restore-orig";
    let def = settings_catalog::find("dlss_sr_render_preset").expect("catalog setting");
    let setting = CatalogSetting::new(def);

    let before = NvapiSettingState {
        present: false,
        value: None,
    };
    let game_scope = GamePreparationScope {
        game_id,
        target_kind: "external",
        executable_path: "/tmp/game.exe",
        application_witness_json: r#"{"app_name":"/tmp/game.exe"}"#,
    };

    let err_game = dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Game { game_id },
            op_id: "op-restore-orig-game",
            profile_name: "NVIDIA Claim Fixture",
            profile_is_predefined: false,
            profile_identity_json: r#"{"name":"NVIDIA Claim Fixture"}"#,
            setting_id: setting.nvapi_id(),
            before,
            op: WriteOp::RestoreOriginal,
            game_scope: Some(game_scope),
        },
    )
    .expect_err("RestoreOriginal on Game target must be rejected before preparation");
    assert_eq!(
        err_game.to_string(),
        "WriteOp::RestoreOriginal must be resolved before preparation"
    );

    let identity = global_identity_json(GLOBAL_PROFILE, 50, 120, true);
    let err_global = dispatch_setting_preparation(
        &storage,
        SettingPreparationDispatch {
            target: &SettingTarget::Global,
            op_id: "op-restore-orig-global",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: setting.nvapi_id(),
            before,
            op: WriteOp::RestoreOriginal,
            game_scope: None,
        },
    )
    .expect_err("RestoreOriginal on Global target must be rejected before preparation");
    assert_eq!(
        err_global.to_string(),
        "WriteOp::RestoreOriginal must be resolved before preparation"
    );

    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending")
            .is_empty(),
        "rejected RestoreOriginal must not create a pending operation"
    );
}
