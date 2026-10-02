use super::*;
use renderpilot_application::GameRepository;
use renderpilot_domain::{
    EngineConfigJournal, EngineConfigReceipt, GameIdentity, GameInstallation, GameProxyTopology,
    GameRuntime, Launcher, Platform, ProxyImplementation, ProxyLink, ProxyRootPrestate,
    RenoDxConfigReceipt, RenoDxSetPathBaseline, RenoDxSetPathValue, Sha256Hash,
};

fn game_id() -> GameId {
    GameId::new("steam:1091500").expect("id")
}

fn path(value: &str) -> PathRef {
    PathRef::new(value).expect("path")
}

fn recorded_host_addon() -> InstalledAddon {
    InstalledAddon::new(
        game_id(),
        AddonKind::RenoDx,
        path("C:/Games/CP2077/renodx-cp2077.addon64"),
    )
    .with_addon_version("snapshot-2026.06")
    .with_created_file(path("C:/Games/CP2077/dxgi.dll"))
    .with_created_file(path("C:/Games/CP2077/ReShade.ini"))
    .with_tracked_source(
        TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            "https://clshortfuse.github.io/renodx/renodx-cp2077.addon64",
            Some("\"etag-1\"".to_owned()),
            "addon-digest",
        )
        .with_last_modified(Some("Wed, 18 Jun 2026 12:00:00 GMT".to_owned())),
    )
    .with_tracked_source(TrackedSource::new(
        TrackedSourceRole::HostBinary,
        "https://nightly.link/x64.zip",
        None,
        "host-digest",
    ))
}

fn active_optiscaler_topology() -> GameProxyTopology {
    let root = path("C:/Games/CP2077/dxgi.dll");
    GameProxyTopology {
        id: "optiscaler:steam:1091500".to_owned(),
        game_id: game_id(),
        root_slot: root.clone(),
        outer: ProxyLink {
            implementation: ProxyImplementation::OptiScaler,
            path: root,
            receipt: renderpilot_domain::FileReceipt::owned(
                "test:optiscaler-outer",
                Sha256Hash::new("a".repeat(64)).expect("hash"),
            )
            .expect("receipt"),
        },
        downstream: None,
        downstream_origin: None,
        root_prestate: ProxyRootPrestate::Absent,
    }
}

fn store_active_optiscaler_topology(storage: &SqliteStorage) {
    storage
        .upsert_game(&GameInstallation::new(
            GameIdentity::new(game_id(), "Cyberpunk 2077", Launcher::Steam).expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            path("C:/Games/CP2077"),
        ))
        .expect("game");
    let topology = active_optiscaler_topology();
    storage
        .with_transaction(|transaction| {
            crate::repositories::proxy_topologies::upsert_within_transaction(transaction, &topology)
        })
        .expect("topology");
}

#[test]
fn corrupt_reused_managed_binding_is_rejected_on_rehydrate() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let record = InstalledAddon::new(
        game_id(),
        AddonKind::Luma,
        path("C:/Games/CP2077/Luma-Test.addon64"),
    );
    storage.upsert_installed_addon(&record).expect("record");
    storage
        .connection
        .lock()
        .expect("connection")
        .execute(
            "UPDATE installed_addons SET managed_files_json = ?1 WHERE game_id = ?2",
            rusqlite::params![
                r#"[{"path":"C:/Games/CP2077/nvngx_dlss.dll","mode":"reused","baseline":{"state":"absent"},"installed_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}]"#,
                game_id().as_str()
            ],
        )
        .expect("corrupt row");

    assert!(storage.get_installed_addon(&game_id()).is_err());
}

#[test]
fn upsert_then_get_round_trips_a_recorded_host_install() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let addon = recorded_host_addon();
    storage.upsert_installed_addon(&addon).expect("upsert");

    let loaded = storage
        .get_installed_addon(&game_id())
        .expect("get")
        .expect("present");

    assert_eq!(loaded.addon_version(), Some("snapshot-2026.06"));
    assert!(loaded.has_host_binary_provenance());
    assert_eq!(loaded.addon_file().as_str(), addon.addon_file().as_str());
    assert_eq!(loaded.created_files(), addon.created_files());
    assert_eq!(loaded.backed_up_files(), addon.backed_up_files());
    assert_eq!(loaded.tracked_sources(), addon.tracked_sources());
    // The persisted upstream date + install/update timestamps surface on read.
    assert_eq!(loaded.addon_dated(), Some("Wed, 18 Jun 2026 12:00:00 GMT"));
    assert!(loaded.installed_at().is_some());
    assert!(loaded.updated_at().is_some());
}

#[test]
fn upsert_then_get_round_trips_the_renodx_config_receipt() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let ini_path = path("C:/Games/CP2077/ReShade.ini");
    let receipt = RenoDxConfigReceipt::new(
        ini_path,
        RenoDxSetPathBaseline::Present {
            value: "arbitrary".to_owned(),
        },
        true,
        RenoDxSetPathValue::One,
    );
    let addon = recorded_host_addon()
        .with_renodx_config_receipt(Some(receipt.clone()))
        .expect("receipt");
    storage.upsert_installed_addon(&addon).expect("upsert");

    let loaded = storage
        .get_installed_addon(&game_id())
        .expect("get")
        .expect("present");
    assert_eq!(loaded.renodx_config_receipt(), Some(&receipt));
}

#[test]
fn engine_config_journal_uses_null_safe_cas_and_survives_regular_upsert() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let addon = recorded_host_addon();
    storage.upsert_installed_addon(&addon).expect("upsert");
    assert_eq!(
        storage
            .engine_config_journal_token(&game_id(), AddonKind::RenoDx)
            .expect("read"),
        None
    );

    let journal = EngineConfigJournal::default();
    assert!(
        storage
            .compare_and_swap_engine_config_journal(
                &game_id(),
                AddonKind::RenoDx,
                None,
                Some(&journal),
            )
            .expect("cas")
    );
    assert_eq!(
        storage
            .engine_config_journal_token(&game_id(), AddonKind::RenoDx)
            .expect("read"),
        None
    );

    let updated = addon.with_addon_version("new-version");
    storage.upsert_installed_addon(&updated).expect("upsert");
    assert_eq!(
        storage
            .engine_config_journal_token(&game_id(), AddonKind::RenoDx)
            .expect("read"),
        None
    );
    assert!(
        storage
            .compare_and_swap_engine_config_journal(&game_id(), AddonKind::RenoDx, None, None,)
            .expect("cas")
    );
    assert_eq!(
        storage
            .engine_config_journal_token(&game_id(), AddonKind::RenoDx)
            .expect("read"),
        None
    );
}

#[test]
fn nonempty_engine_config_journal_survives_upsert_and_exact_cas_delete() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let addon = recorded_host_addon();
    storage.upsert_installed_addon(&addon).expect("upsert");
    let receipt = EngineConfigReceipt {
        schema_version: 1,
        path: "C:/Games/CP2077/Engine.ini".to_owned(),
        file_created: false,
        encoding: "utf8".to_owned(),
        before_digest: "0".repeat(64),
        after_digest: "1".repeat(64),
        recipe_fingerprint: "2".repeat(64),
        contributions: vec![EngineConfigContribution {
            section: "SystemSettings".to_owned(),
            key: "r.AllowHDR".to_owned(),
            value: "1".to_owned(),
            line: b"r.AllowHDR=1\n".to_vec(),
            left_anchor: "3".repeat(64),
            right_anchor: "4".repeat(64),
            introduced_prefix: Vec::new(),
            group: "systemsettings".to_owned(),
            ordinal: 0,
        }],
        created_headers: Vec::new(),
        created_header_prefixes: Vec::new(),
        created_header_groups: Vec::new(),
        created_header_ordinals: Vec::new(),
    };
    let journal = EngineConfigJournal {
        stable: Some(receipt.clone()),
        pending: Some(EngineConfigTransition {
            operation_id: "op-1".to_owned(),
            stage_name: ".renderpilot-engine-op-1.stage".to_owned(),
            prior: Some(receipt),
            after: None,
            before_digest: "0".repeat(64),
            after_digest: "1".repeat(64),
        }),
    };
    assert!(
        storage
            .compare_and_swap_engine_config_journal(
                &game_id(),
                AddonKind::RenoDx,
                None,
                Some(&journal),
            )
            .expect("store journal")
    );
    let token = storage
        .engine_config_journal_token(&game_id(), AddonKind::RenoDx)
        .expect("read journal")
        .expect("nonempty token");

    storage
        .upsert_installed_addon(&addon.with_addon_version("new-version"))
        .expect("upsert must preserve journal");
    assert_eq!(
        storage
            .get_installed_addon(&game_id())
            .expect("hydrate owner")
            .expect("addon")
            .engine_config_journal(),
        Some(&journal),
        "hydration reads the matching standalone owner"
    );
    assert_eq!(
        storage
            .engine_config_journal_token(&game_id(), AddonKind::RenoDx)
            .expect("read journal")
            .as_deref(),
        Some(token.as_str())
    );
    assert!(
        !storage
            .compare_and_swap_engine_config_journal(
                &game_id(),
                AddonKind::RenoDx,
                Some("stale-token"),
                None,
            )
            .expect("stale delete")
    );
    assert!(
        storage
            .compare_and_swap_engine_config_journal(
                &game_id(),
                AddonKind::RenoDx,
                Some(&token),
                None,
            )
            .expect("delete journal")
    );
    assert_eq!(
        storage
            .engine_config_journal_token(&game_id(), AddonKind::RenoDx)
            .expect("read cleared journal"),
        None
    );
}

#[test]
fn standalone_engine_journal_owner_supports_kind_scoped_null_safe_cas_without_addon_row() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let journal = EngineConfigJournal {
        stable: Some(EngineConfigReceipt {
            schema_version: 1,
            path: "C:/Games/CP2077/Engine.ini".to_owned(),
            file_created: false,
            encoding: "utf8".to_owned(),
            before_digest: "0".repeat(64),
            after_digest: "1".repeat(64),
            recipe_fingerprint: "2".repeat(64),
            contributions: Vec::new(),
            created_headers: Vec::new(),
            created_header_prefixes: Vec::new(),
            created_header_groups: Vec::new(),
            created_header_ordinals: Vec::new(),
        }),
        pending: None,
    };
    assert!(
        storage
            .compare_and_swap_engine_config_journal(
                &game_id(),
                AddonKind::RenoDx,
                None,
                Some(&journal)
            )
            .expect("create standalone owner")
    );
    let owner = storage
        .engine_config_journal_owner(&game_id())
        .expect("owner read")
        .expect("standalone owner exists");
    assert_eq!(owner.kind, AddonKind::RenoDx);
    assert_eq!(owner.journal, journal);
    assert_eq!(
        storage
            .engine_config_journal_token(&game_id(), AddonKind::RenoDx)
            .expect("kind-scoped token")
            .as_deref(),
        Some(owner.raw_token.as_str())
    );
    assert_eq!(
        storage
            .engine_config_journal_token(&game_id(), AddonKind::Luma)
            .expect("other-kind lookup"),
        None
    );
    assert_eq!(
        storage.list_installed_addons().expect("empty local list"),
        []
    );
    assert!(
        !storage
            .compare_and_swap_engine_config_journal(
                &game_id(),
                AddonKind::RenoDx,
                Some("stale"),
                None,
            )
            .expect("stale token")
    );
    assert!(
        storage
            .compare_and_swap_engine_config_journal(
                &game_id(),
                AddonKind::RenoDx,
                Some(&owner.raw_token),
                None,
            )
            .expect("exact clear")
    );
    assert_eq!(
        storage
            .engine_config_journal_owner(&game_id())
            .expect("empty owner"),
        None
    );
}

#[test]
fn non_canonical_engine_config_journal_is_rejected_on_read() {
    let storage = SqliteStorage::in_memory().expect("storage");
    storage
        .upsert_installed_addon(&recorded_host_addon())
        .expect("upsert");
    storage
        .connection
        .lock()
        .expect("connection")
        .execute(
            "INSERT INTO game_engine_config_journals
                (game_id, addon_kind, journal_json, created_at, updated_at)
             VALUES (?1, 'renodx', '{ \"stable\": null, \"pending\": null }', 1, 1)",
            rusqlite::params![game_id().as_str()],
        )
        .expect("non-canonical fixture");

    assert!(storage.get_installed_addon(&game_id()).is_err());
}

#[test]
fn empty_engine_config_journal_is_rejected_instead_of_becoming_some_empty_state() {
    let storage = SqliteStorage::in_memory().expect("storage");
    storage
        .upsert_installed_addon(&recorded_host_addon())
        .expect("upsert");
    storage
        .connection
        .lock()
        .expect("connection")
        .execute(
            "INSERT INTO game_engine_config_journals
                (game_id, addon_kind, journal_json, created_at, updated_at)
             VALUES (?1, 'renodx', '{}', 1, 1)",
            rusqlite::params![game_id().as_str()],
        )
        .expect("empty fixture");

    assert!(storage.get_installed_addon(&game_id()).is_err());
    assert!(
        storage
            .engine_config_journal_token(&game_id(), AddonKind::RenoDx)
            .is_err()
    );
}

#[test]
fn upsert_then_get_round_trips_host_metadata() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let addon = recorded_host_addon()
        .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer)
        .with_reshade_channel("nightly")
        .with_registered_exe_path(path("C:/Games/CP2077/bin/x64/Cyberpunk2077.exe"));

    storage.upsert_installed_addon(&addon).expect("upsert");
    let loaded = storage
        .get_installed_addon(&game_id())
        .expect("get")
        .expect("present");

    assert_eq!(
        loaded.host_kind(),
        Some(InstalledAddonHostKind::SharedVulkanLayer)
    );
    assert_eq!(loaded.reshade_channel(), Some("nightly"));
    assert_eq!(
        loaded.registered_exe_path().map(PathRef::as_str),
        Some("C:/Games/CP2077/bin/x64/Cyberpunk2077.exe")
    );
}

/// E.17: a Luma record's `created_files` are deep, multi-component paths
/// (the `Luma/**` shader tree) rather than RenoDX's flat, single-file
/// layout — the JSON round-trip must preserve them exactly.
#[test]
fn upsert_then_get_round_trips_a_luma_record_with_nested_created_files() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let addon = InstalledAddon::new(
        game_id(),
        AddonKind::Luma,
        path("C:/Games/Dishonored2/Luma-Dishonored_2.addon"),
    )
    .with_addon_version("Build 515")
    .with_created_file(path("C:/Games/Dishonored2/dxgi.dll"))
    .with_created_file(path("C:/Games/Dishonored2/Luma/Global/Copy_PS.hlsl"))
    .with_created_file(path("C:/Games/Dishonored2/Luma/Includes/Common.hlsl"))
    .with_created_file(path("C:/Games/Dishonored2/Luma/Dishonored 2/Fog_PS.hlsl"))
    .with_tracked_source(TrackedSource::new(
        TrackedSourceRole::AddonPayload,
        "https://github.com/Filoppi/Luma-Framework/releases/latest/download/Luma-Dishonored_2.zip",
        None,
        "zip-digest",
    ));

    storage.upsert_installed_addon(&addon).expect("upsert");
    let loaded = storage
        .get_installed_addon(&game_id())
        .expect("get")
        .expect("present");

    assert_eq!(loaded.kind(), AddonKind::Luma);
    assert_eq!(loaded.created_files(), addon.created_files());
    assert!(
        loaded
            .created_files()
            .iter()
            .any(|p| p.as_str().ends_with("Luma/Dishonored 2/Fog_PS.hlsl")),
        "a deeply nested path with a space in a component must round-trip"
    );
}

#[test]
fn upsert_replaces_an_existing_record() {
    let storage = SqliteStorage::in_memory().expect("storage");
    storage
        .upsert_installed_addon(&recorded_host_addon())
        .expect("first");

    // A local-file install records no HostBinary entry.
    let local_file = InstalledAddon::new(
        game_id(),
        AddonKind::RenoDx,
        path("C:/Games/CP2077/renodx-cp2077.addon64"),
    )
    .with_backed_up_file(path("C:/Games/CP2077/ReShade.ini"));
    storage.upsert_installed_addon(&local_file).expect("second");

    let loaded = storage
        .get_installed_addon(&game_id())
        .expect("get")
        .expect("present");
    assert!(!loaded.has_host_binary_provenance());
    assert!(loaded.tracked_sources().is_empty());
    assert_eq!(loaded.backed_up_files().len(), 1);
}

#[test]
fn get_returns_none_when_absent() {
    let storage = SqliteStorage::in_memory().expect("storage");
    assert!(
        storage
            .get_installed_addon(&game_id())
            .expect("get")
            .is_none()
    );
}

#[test]
fn delete_removes_the_record() {
    let storage = SqliteStorage::in_memory().expect("storage");
    storage
        .upsert_installed_addon(&recorded_host_addon())
        .expect("upsert");
    storage
        .delete_installed_addon(&game_id(), AddonKind::RenoDx)
        .expect("delete");
    assert!(
        storage
            .get_installed_addon(&game_id())
            .expect("get")
            .is_none()
    );
}

#[test]
fn public_peer_upsert_reports_peer_topology_conflict() {
    let storage = SqliteStorage::in_memory().expect("storage");
    store_active_optiscaler_topology(&storage);

    let error = storage
        .upsert_installed_addon(&recorded_host_addon())
        .expect_err("public peer upsert must be fenced");

    assert_eq!(
        error.kind(),
        &renderpilot_application::AppErrorKind::PeerTopologyConflict {
            peer_kind: AddonKind::RenoDx,
        }
    );
    assert!(storage.get_installed_addon(&game_id()).unwrap().is_none());
}

#[test]
fn public_peer_delete_reports_peer_topology_conflict() {
    let storage = SqliteStorage::in_memory().expect("storage");
    storage
        .upsert_installed_addon(&recorded_host_addon())
        .expect("seed peer");
    store_active_optiscaler_topology(&storage);

    let error = storage
        .delete_installed_addon(&game_id(), AddonKind::RenoDx)
        .expect_err("public peer delete must be fenced");

    assert_eq!(
        error.kind(),
        &renderpilot_application::AppErrorKind::PeerTopologyConflict {
            peer_kind: AddonKind::RenoDx,
        }
    );
    assert!(storage.get_installed_addon(&game_id()).unwrap().is_some());
}

#[test]
fn delete_with_wrong_kind_is_rejected() {
    let storage = SqliteStorage::in_memory().expect("storage");
    storage
        .upsert_installed_addon(&recorded_host_addon())
        .expect("upsert");
    let error = storage
        .delete_installed_addon(&game_id(), AddonKind::Luma)
        .expect_err("wrong-kind delete must be refused");
    assert!(error.message().contains("renodx"));
    assert!(error.message().contains("luma"));

    // The original RenoDX record must be untouched.
    let loaded = storage
        .get_installed_addon(&game_id())
        .expect("get")
        .expect("renodx row must survive");
    assert_eq!(loaded.kind(), AddonKind::RenoDx);
}

#[test]
fn list_returns_all_records() {
    let storage = SqliteStorage::in_memory().expect("storage");
    storage
        .upsert_installed_addon(&recorded_host_addon())
        .expect("upsert");
    assert_eq!(storage.list_installed_addons().expect("list").len(), 1);
}

#[test]
fn upsert_replaces_a_same_kind_record_without_a_guard_error() {
    let storage = SqliteStorage::in_memory().expect("storage");
    storage
        .upsert_installed_addon(&recorded_host_addon())
        .expect("first insert");

    let updated = InstalledAddon::new(
        game_id(),
        AddonKind::RenoDx,
        path("C:/Games/CP2077/renodx-cp2077.addon64"),
    )
    .with_addon_version("snapshot-2026.07");
    storage
        .upsert_installed_addon(&updated)
        .expect("same-kind replace");

    let loaded = storage
        .get_installed_addon(&game_id())
        .expect("get")
        .expect("present");
    assert_eq!(loaded.addon_version(), Some("snapshot-2026.07"));
}

#[test]
fn upsert_refuses_to_overwrite_a_different_kind_record() {
    let storage = SqliteStorage::in_memory().expect("storage");
    storage
        .upsert_installed_addon(&recorded_host_addon())
        .expect("renodx insert");

    let luma_record = InstalledAddon::new(
        game_id(),
        AddonKind::Luma,
        path("C:/Games/CP2077/Luma-Game.addon"),
    );
    let error = storage
        .upsert_installed_addon(&luma_record)
        .expect_err("cross-kind upsert must be refused");
    assert!(error.message().contains("renodx"));
    assert!(error.message().contains("luma"));

    // The original RenoDX record must be untouched.
    let loaded = storage
        .get_installed_addon(&game_id())
        .expect("get")
        .expect("present");
    assert_eq!(loaded.kind(), AddonKind::RenoDx);
}

#[test]
fn upsert_allows_a_different_kind_after_the_prior_record_is_deleted() {
    let storage = SqliteStorage::in_memory().expect("storage");
    storage
        .upsert_installed_addon(&recorded_host_addon())
        .expect("renodx insert");
    storage
        .delete_installed_addon(&game_id(), AddonKind::RenoDx)
        .expect("delete");

    let luma_record = InstalledAddon::new(
        game_id(),
        AddonKind::Luma,
        path("C:/Games/CP2077/Luma-Game.addon"),
    );
    storage
        .upsert_installed_addon(&luma_record)
        .expect("insert after delete");

    let loaded = storage
        .get_installed_addon(&game_id())
        .expect("get")
        .expect("present");
    assert_eq!(loaded.kind(), AddonKind::Luma);
}

fn raw_addon(addon: &InstalledAddon) -> RawInstalledAddon {
    RawInstalledAddon {
        game_id: addon.game_id().as_str().to_owned(),
        kind: mapping::enum_to_text(&addon.kind()).expect("kind"),
        addon_file: addon.addon_file().as_str().to_owned(),
        addon_version: addon.addon_version().map(str::to_owned),
        created_files_json: mapping::serialize_json(addon.created_files()).expect("created"),
        backed_up_files_json: mapping::serialize_json(addon.backed_up_files()).expect("backed up"),
        managed_files_json: mapping::serialize_json(addon.managed_files()).expect("managed"),
        tracked_sources_json: mapping::serialize_json(addon.tracked_sources()).expect("sources"),
        host_kind: addon
            .host_kind()
            .map(|kind| mapping::enum_to_text(&kind).expect("host kind")),
        reshade_channel: addon.reshade_channel().map(str::to_owned),
        registered_exe_path: addon
            .registered_exe_path()
            .map(|path| path.as_str().to_owned()),
        renodx_config_receipt_json: addon
            .renodx_config_receipt()
            .map(|receipt| mapping::serialize_json(receipt).expect("receipt")),
        engine_config_journal_json: addon
            .engine_config_journal()
            .map(|journal| mapping::serialize_json(journal).expect("engine journal")),
        created_at: addon.installed_at().unwrap_or(1),
        updated_at: addon.updated_at().unwrap_or(1),
    }
}

#[test]
fn addon_observation_distinguishes_missing_invalid_and_sql_outcomes() {
    assert!(matches!(
        observe_raw(Ok(None)).expect("missing row"),
        RowObservation::Missing
    ));

    let addon = recorded_host_addon();
    assert!(matches!(
        observe_raw(Ok(Some(raw_addon(&addon)))).expect("valid row"),
        RowObservation::Present(_)
    ));

    let mut malformed = raw_addon(&addon);
    malformed.managed_files_json = "{".to_owned();
    assert!(matches!(
        observe_raw(Ok(Some(malformed))).expect("malformed json"),
        RowObservation::Invalid(_)
    ));

    let mut invalid_domain = raw_addon(&addon);
    invalid_domain.created_files_json = "[]".to_owned();
    assert!(matches!(
        observe_raw(Ok(Some(invalid_domain))).expect("domain invariant"),
        RowObservation::Invalid(_)
    ));

    let mut malformed_receipt = raw_addon(&addon);
    malformed_receipt.renodx_config_receipt_json = Some("{}".to_owned());
    assert!(matches!(
        observe_raw(Ok(Some(malformed_receipt))).expect("malformed receipt"),
        RowObservation::Invalid(_)
    ));

    assert!(observe_raw(Err(rusqlite::Error::InvalidQuery)).is_err());
}
