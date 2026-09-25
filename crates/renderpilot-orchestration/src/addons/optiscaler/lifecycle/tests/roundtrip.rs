use super::*;

/// Exercises the production-shaped RenoDX proxy receipt before OptiScaler
/// takes over its root slot.  The conversion must carry the real RenoDX
/// record from its installer, not a hand-written peer fixture: `dxgi.dll`
/// moves from RenoDX's generic created claim to the one Owned/Absent managed
/// ReShade host claim at `ReShade64.dll` in the same OptiScaler aggregate.
#[test]
fn renodx_proxy_install_then_optiscaler_chain_converts_the_actual_peer_receipt() {
    let db_root = tempdir().expect("database root");
    let game_root = tempdir().expect("game root");
    let context = Context::open_at(db_root.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:renodx-then-optiscaler").expect("game id");
    let exe = game_root.path().join("Game.exe");
    std::fs::write(&exe, b"exe").expect("exe");
    let game = GameInstallation::new(
        GameIdentity::new(game_id.clone(), "RenoDX then OptiScaler", Launcher::Manual)
            .expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(game_root.path().to_string_lossy()).expect("game path"),
    );
    context.storage().upsert_game(&game).expect("game");

    let reshade = crate::addons::test_support::build_pe_with_exports(
        crate::addons::test_support::MACHINE_AMD64,
        crate::addons::test_support::PE32_PLUS_MAGIC,
        &[
            "ReShadeVersion",
            "ReShadeRegisterAddon",
            "ReShadeUnregisterAddon",
            "ReShadeRegisterEvent",
            "ReShadeUnregisterEvent",
        ],
    );
    let prepared_renodx = crate::addons::renodx::install::PreparedInstall {
        game_id: game_id.clone(),
        host_kind: crate::addons::reshade::proxy::HostKind::Proxy,
        processing_path: crate::addons::renodx::types::RenoDxProcessingPath::Unmanaged,
        renodx_config: None,
        proxy_dll_name: "dxgi.dll".to_owned(),
        addon_file_name: "renodx.addon64".to_owned(),
        addon_source_url: "https://example.invalid/renodx.addon64".to_owned(),
        source_digest: archive::sha256_hex(b"renodx-addon"),
        source_etag: None,
        source_last_modified: None,
        addon_bytes: b"renodx-addon".to_vec(),
        reshade_dll_bytes: reshade.clone(),
        reshade_source_url: "https://example.invalid/reshade.zip".to_owned(),
        reshade_source_etag: None,
        reshade_last_modified: None,
        reshade_digest: archive::sha256_hex(&reshade),
        reshade_channel: Some(crate::addons::reshade::types::ReshadeChannel::Stable),
        // A real RenoDX proxy install normally leaves an unrelated generic
        // `ReShade.ini` claim beside its host.  The OptiScaler handoff must
        // move only `dxgi.dll`; this claim stays byte-for-byte intact.
        ini_tweaks: crate::addons::reshade::types::ReshadeIniTweaks {
            disabled_addons: vec!["Generic Depth".to_owned()],
            addon_path: None,
            dlss_fix: None,
        },
    };
    let (renodx_before, pending) =
        crate::addons::renodx::install::install(game_root.path(), &prepared_renodx)
            .expect("RenoDX proxy install");
    context
        .storage()
        .upsert_installed_addon(&renodx_before)
        .expect("persist RenoDX record");
    pending.finish_committed();
    let persisted_renodx = context
        .storage()
        .get_installed_addon(&game_id)
        .expect("read persisted RenoDX record")
        .expect("persisted RenoDX record");

    let proxy_path = game_root.path().join("dxgi.dll");
    let downstream_path = game_root.path().join("ReShade64.dll");
    let reshade_ini_path = game_root.path().join("ReShade.ini");
    let proxy_ref = PathRef::new(proxy_path.to_string_lossy()).expect("proxy path");
    let downstream_ref = PathRef::new(downstream_path.to_string_lossy()).expect("downstream path");
    let reshade_ini_ref =
        PathRef::new(reshade_ini_path.to_string_lossy()).expect("ReShade.ini path");
    let reshade_ini_bytes = std::fs::read(&reshade_ini_path).expect("RenoDX ReShade.ini");
    assert!(
        persisted_renodx
            .created_files()
            .iter()
            .any(|path| path == &reshade_ini_ref)
    );
    let expected_peer = super::super::adoption::plan_peer_host_transition(
        super::super::adoption::PeerTopologyDirection::IntoOptiTopology,
        &context,
        &game_id,
        &proxy_ref,
        &downstream_ref,
        None,
    )
    .expect("strict RenoDX peer conversion planning")
    .expect("RenoDX owns the proxy host");
    assert_eq!(expected_peer.receipt.before, persisted_renodx);
    assert!(
        expected_peer
            .receipt
            .after
            .created_files()
            .iter()
            .all(|path| path != &proxy_ref)
    );
    assert!(
        expected_peer
            .receipt
            .after
            .created_files()
            .iter()
            .any(|path| path == &reshade_ini_ref)
    );
    assert!(matches!(
        expected_peer.receipt.after.managed_files(),
        [managed]
            if managed.path() == &downstream_ref
                && managed.mode() == renderpilot_domain::ManagedFileMode::Owned
                && matches!(managed.baseline(), renderpilot_domain::ManagedFileBaseline::Absent)
    ));

    let manifest = manifest_store::parse_manifest(include_bytes!(
        "../../../../../assets/optiscaler-fallback.json"
    ))
    .expect("bundled manifest");
    let mut source = manifest.releases[0].source.clone();
    source.tag = "v1.0.0".to_owned();
    source.asset = "Optiscaler_1.0.0.7z".to_owned();
    let optiscaler_proxy = b"optiscaler-proxy".to_vec();
    let optiscaler_config = b"[OptiScaler]\n".to_vec();
    let release = OptiScalerRelease {
        id: "v1.0.0".to_owned(),
        source,
        archive_sha256: "a".repeat(64),
        archive_size: 10_000,
        config_schema: 1,
        members: vec![
            OptiScalerArchiveMember {
                archive_path: "proxy.dll".to_owned(),
                target: "$proxy".to_owned(),
                sha256: archive::sha256_hex(&optiscaler_proxy),
                size: optiscaler_proxy.len() as u64,
                module: "core".to_owned(),
                pe_x64: false,
            },
            OptiScalerArchiveMember {
                archive_path: "OptiScaler.ini".to_owned(),
                target: "OptiScaler.ini".to_owned(),
                sha256: archive::sha256_hex(&optiscaler_config),
                size: optiscaler_config.len() as u64,
                module: "core".to_owned(),
                pe_x64: false,
            },
        ],
    };
    let archive = archive::PreparedArchive::from_files(HashMap::from([
        ("proxy.dll".to_owned(), optiscaler_proxy),
        ("OptiScaler.ini".to_owned(), optiscaler_config),
    ]));
    let authority = FileSafetyAuthority::new();
    let assessment = authority
        .issue_game_assessment(&context, &game_id)
        .expect("safety assessment");
    let safety = authority
        .game_permit(game_id.clone(), Some(&assessment.context_token))
        .expect("safety permit");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let plan = ApplyPlan {
        intent: ApplyIntent::Install {
            modules: Some(vec!["core".to_owned()]),
        },
        context: &context,
        guard,
        manifest,
        game_id: game_id.clone(),
        old_state: None,
        old_managed_files: Vec::new(),
        old_config_base: None,
        release,
        modules: HashSet::from(["core".to_owned()]),
        artifacts: ArtifactSet {
            archive,
            module_artifacts: Vec::new(),
            native_artifacts: Vec::new(),
        },
        target: ApplyTarget {
            exe,
            dir: game_root.path().to_path_buf(),
            proxy: EvaluatedProxyPlan {
                slot: proxy_path.clone(),
                chain_reshade: true,
                downstream_path: Some(downstream_path.clone()),
                conflict: None,
                reshade_source_path: Some(proxy_path.clone()),
                reshade_source_sha256: Some(
                    renderpilot_detection::sha256_file(&proxy_path).expect("RenoDX host digest"),
                ),
            },
        },
        safety,
        compatibility_invariants: Vec::new(),
        accepted_prerequisite_binding: renderpilot_domain::OptiScalerPrerequisiteBinding::None,
    };

    apply_release(&plan).expect("OptiScaler chain over the persisted RenoDX record");

    let persisted_after = context
        .storage()
        .get_installed_addon(&game_id)
        .expect("persisted peer")
        .expect("RenoDX peer");
    assert!(persisted_after.eq_ignoring_persistence_timestamps(&expected_peer.receipt.after));
    let topology = context
        .storage()
        .get_proxy_topology(&game_id)
        .expect("topology")
        .expect("OptiScaler topology");
    assert_eq!(topology.outer.path, proxy_ref);
    assert_eq!(
        topology
            .downstream
            .as_ref()
            .expect("ReShade downstream")
            .path,
        downstream_ref
    );
    assert_eq!(
        std::fs::read(&downstream_path).expect("relocated host"),
        reshade
    );
    assert_eq!(
        std::fs::read(&reshade_ini_path).expect("preserved RenoDX ReShade.ini"),
        reshade_ini_bytes,
        "OptiScaler host adoption must not rewrite RenoDX's unrelated configuration claim"
    );

    // The inverse handoff is just as important as adoption: after OptiScaler
    // removes its outer topology, the canonical RenoDX record owns the host
    // again as an Owned/Absent managed file.  Its ordinary inactive uninstall
    // must remove that host together with the add-on payload rather than
    // retiring only metadata and leaving a stale ReShade slot behind.
    uninstall_locked(&context, &game_id, &plan.guard).expect("OptiScaler uninstall");
    let renodx_after_outer_release = context
        .storage()
        .get_installed_addon(&game_id)
        .expect("RenoDX record after outer release")
        .expect("RenoDX record remains installed");
    assert!(matches!(
        renodx_after_outer_release.managed_files(),
        [managed]
            if managed.path() == &proxy_ref
                && managed.mode() == renderpilot_domain::ManagedFileMode::Owned
                && matches!(managed.baseline(), renderpilot_domain::ManagedFileBaseline::Absent)
    ));
    crate::addons::renodx::install::uninstall(&renodx_after_outer_release, Some(game_root.path()))
        .expect("RenoDX inactive uninstall after OptiScaler release");
    assert!(
        !proxy_path.exists(),
        "RenoDX host returned by OptiScaler must not remain as a stale ReShade slot"
    );
    assert!(
        !game_root.path().join("renodx.addon64").exists(),
        "RenoDX add-on payload must be removed with its host"
    );
}

/// Exercises the reverse handoff against the same native shape emitted by a
/// Luma inactive install: the real proxy and payload are generic created-file
/// claims before OptiScaler, become one managed downstream host while the
/// OptiScaler outer is installed, and return to the native claims on removal.
#[test]
fn luma_native_receipt_round_trip_through_optiscaler_restores_then_cleans_up() {
    let db_root = tempdir().expect("database root");
    let game_root = tempdir().expect("game root");
    let context = Context::open_at(db_root.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:luma-native-optiscaler-roundtrip").expect("game id");
    let exe = game_root.path().join("Game.exe");
    std::fs::write(&exe, b"exe").expect("exe");
    let game = GameInstallation::new(
        GameIdentity::new(
            game_id.clone(),
            "Luma native OptiScaler roundtrip",
            Launcher::Manual,
        )
        .expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(game_root.path().to_string_lossy()).expect("game path"),
    );
    context.storage().upsert_game(&game).expect("game");

    let reshade = crate::addons::test_support::build_pe_with_exports(
        crate::addons::test_support::MACHINE_AMD64,
        crate::addons::test_support::PE32_PLUS_MAGIC,
        &[
            "ReShadeVersion",
            "ReShadeRegisterAddon",
            "ReShadeUnregisterAddon",
            "ReShadeRegisterEvent",
            "ReShadeUnregisterEvent",
        ],
    );
    let addon_path = game_root.path().join("Luma-Test.addon");
    let payload_path = game_root
        .path()
        .join("Luma")
        .join("Global")
        .join("test.hlsl");
    let proxy_path = game_root.path().join("dxgi.dll");
    std::fs::create_dir_all(payload_path.parent().expect("payload parent"))
        .expect("payload directory");
    std::fs::write(&addon_path, b"luma addon").expect("Luma addon");
    std::fs::write(&payload_path, b"luma payload").expect("Luma payload");
    std::fs::write(&proxy_path, &reshade).expect("Luma ReShade host");
    let addon_ref = PathRef::new(addon_path.to_string_lossy()).expect("addon path");
    let payload_ref = PathRef::new(payload_path.to_string_lossy()).expect("payload path");
    let proxy_ref = PathRef::new(proxy_path.to_string_lossy()).expect("proxy path");
    let luma_record = InstalledAddon::new(game_id.clone(), AddonKind::Luma, addon_ref)
        .with_created_file(payload_ref)
        .with_created_file(proxy_ref)
        .with_host_kind(renderpilot_domain::InstalledAddonHostKind::Proxy)
        .with_reshade_channel("nightly");
    context
        .storage()
        .upsert_installed_addon(&luma_record)
        .expect("persist native Luma record");

    let manifest = manifest_store::parse_manifest(include_bytes!(
        "../../../../../assets/optiscaler-fallback.json"
    ))
    .expect("bundled manifest");
    let mut source = manifest.releases[0].source.clone();
    source.tag = "v1.0.0".to_owned();
    source.asset = "Optiscaler_1.0.0.7z".to_owned();
    let optiscaler_proxy = b"optiscaler-proxy".to_vec();
    let optiscaler_config = b"[OptiScaler]\n".to_vec();
    let release = OptiScalerRelease {
        id: "v1.0.0".to_owned(),
        source,
        archive_sha256: "a".repeat(64),
        archive_size: 10_000,
        config_schema: 1,
        members: vec![
            OptiScalerArchiveMember {
                archive_path: "proxy.dll".to_owned(),
                target: "$proxy".to_owned(),
                sha256: archive::sha256_hex(&optiscaler_proxy),
                size: optiscaler_proxy.len() as u64,
                module: "core".to_owned(),
                pe_x64: false,
            },
            OptiScalerArchiveMember {
                archive_path: "OptiScaler.ini".to_owned(),
                target: "OptiScaler.ini".to_owned(),
                sha256: archive::sha256_hex(&optiscaler_config),
                size: optiscaler_config.len() as u64,
                module: "core".to_owned(),
                pe_x64: false,
            },
        ],
    };
    let archive = archive::PreparedArchive::from_files(HashMap::from([
        ("proxy.dll".to_owned(), optiscaler_proxy),
        ("OptiScaler.ini".to_owned(), optiscaler_config),
    ]));
    let authority = FileSafetyAuthority::new();
    let assessment = authority
        .issue_game_assessment(&context, &game_id)
        .expect("safety assessment");
    let safety = authority
        .game_permit(game_id.clone(), Some(&assessment.context_token))
        .expect("safety permit");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let plan = ApplyPlan {
        intent: ApplyIntent::Install { modules: None },
        context: &context,
        guard,
        manifest,
        game_id: game_id.clone(),
        old_state: None,
        old_managed_files: Vec::new(),
        old_config_base: None,
        release,
        modules: HashSet::from(["core".to_owned()]),
        artifacts: ArtifactSet {
            archive,
            module_artifacts: Vec::new(),
            native_artifacts: Vec::new(),
        },
        target: ApplyTarget {
            exe,
            dir: game_root.path().to_path_buf(),
            proxy: EvaluatedProxyPlan {
                slot: proxy_path.clone(),
                chain_reshade: true,
                downstream_path: Some(game_root.path().join("ReShade64.dll")),
                conflict: None,
                reshade_source_path: Some(proxy_path.clone()),
                reshade_source_sha256: Some(
                    renderpilot_detection::sha256_file(&proxy_path).expect("Luma host digest"),
                ),
            },
        },
        safety,
        compatibility_invariants: Vec::new(),
        accepted_prerequisite_binding: renderpilot_domain::OptiScalerPrerequisiteBinding::None,
    };
    apply_release(&plan).expect("OptiScaler install over native Luma");
    let luma_managed = context
        .storage()
        .get_installed_addon(&game_id)
        .expect("Luma after OptiScaler install")
        .expect("Luma record");
    assert!(
        luma_managed
            .created_files()
            .iter()
            .all(|path| { !crate::paths::same_path(Path::new(path.as_str()), &proxy_path) })
    );
    assert!(matches!(
        luma_managed.managed_files(),
        [managed]
            if managed.path().as_str().ends_with("ReShade64.dll")
                && managed.mode() == renderpilot_domain::ManagedFileMode::Owned
                && matches!(managed.baseline(), renderpilot_domain::ManagedFileBaseline::Absent)
    ));

    uninstall_locked(&context, &game_id, &plan.guard).expect("OptiScaler uninstall");
    let luma_native = context
        .storage()
        .get_installed_addon(&game_id)
        .expect("Luma after OptiScaler uninstall")
        .expect("Luma record");
    assert!(luma_native.managed_files().is_empty());
    assert!(
        luma_native
            .created_files()
            .iter()
            .any(|path| { crate::paths::same_path(Path::new(path.as_str()), &proxy_path) })
    );
    crate::addons::luma::use_cases::commands::uninstall::uninstall_locked(
        &context,
        &plan.guard,
        &game_id,
    )
    .expect("Luma inactive cleanup");
    assert!(
        context
            .storage()
            .get_installed_addon(&game_id)
            .expect("Luma record after cleanup")
            .is_none(),
        "production Luma uninstall must delete the durable record"
    );
    assert!(!addon_path.exists());
    assert!(!payload_path.exists());
    assert!(!proxy_path.exists());
}

#[test]
fn first_install_and_exact_uninstall_round_trip_owned_files_and_topology() {
    let db_root = tempdir().expect("database root");
    let game_root = tempdir().expect("game root");
    let context = Context::open_at(db_root.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:optiscaler-install").expect("game id");
    let exe = game_root.path().join("Game.exe");
    std::fs::write(&exe, b"exe").expect("exe");
    let proxy_path = game_root.path().join("dxgi.dll");
    let downstream_path = game_root.path().join("ReShade64.dll");
    let reshade = crate::addons::test_support::build_pe_with_exports(
        crate::addons::test_support::MACHINE_AMD64,
        crate::addons::test_support::PE32_PLUS_MAGIC,
        &[
            "ReShadeVersion",
            "ReShadeRegisterAddon",
            "ReShadeUnregisterAddon",
            "ReShadeRegisterEvent",
            "ReShadeUnregisterEvent",
        ],
    );
    std::fs::write(&proxy_path, &reshade).expect("pre-existing ReShade");
    let reshade_sha256 = renderpilot_detection::sha256_file(&proxy_path).expect("ReShade digest");
    let game = GameInstallation::new(
        GameIdentity::new(game_id.clone(), "OptiScaler", Launcher::Manual).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(game_root.path().to_string_lossy()).expect("game path"),
    );
    context.storage().upsert_game(&game).expect("game");

    let manifest = manifest_store::parse_manifest(include_bytes!(
        "../../../../../assets/optiscaler-fallback.json"
    ))
    .expect("bundled manifest");
    let mut source = manifest.releases[0].source.clone();
    source.tag = "v1.0.0".to_owned();
    source.asset = "Optiscaler_1.0.0.7z".to_owned();
    let proxy = b"optiscaler-proxy".to_vec();
    let config = b"[OptiScaler]\n".to_vec();
    let preexisting_config_path = game_root.path().join("OptiScaler.ini");
    std::fs::write(&preexisting_config_path, &config).expect("pre-existing exact config");
    let runtime = b"private-runtime".to_vec();
    let obsolete_runtime = b"obsolete-runtime".to_vec();
    let release = OptiScalerRelease {
        id: "v1.0.0".to_owned(),
        source,
        archive_sha256: "a".repeat(64),
        archive_size: 10_000,
        config_schema: 1,
        members: vec![
            OptiScalerArchiveMember {
                archive_path: "proxy.dll".to_owned(),
                target: "$proxy".to_owned(),
                sha256: archive::sha256_hex(&proxy),
                size: proxy.len() as u64,
                module: "core".to_owned(),
                pe_x64: false,
            },
            OptiScalerArchiveMember {
                archive_path: "OptiScaler.ini".to_owned(),
                target: "OptiScaler.ini".to_owned(),
                sha256: archive::sha256_hex(&config),
                size: config.len() as u64,
                module: "core".to_owned(),
                pe_x64: false,
            },
            OptiScalerArchiveMember {
                archive_path: "runtime.dll".to_owned(),
                target: "OptiScaler/runtime.dll".to_owned(),
                sha256: archive::sha256_hex(&runtime),
                size: runtime.len() as u64,
                module: "core".to_owned(),
                pe_x64: false,
            },
            OptiScalerArchiveMember {
                archive_path: "obsolete.dll".to_owned(),
                target: "OptiScaler/obsolete.dll".to_owned(),
                sha256: archive::sha256_hex(&obsolete_runtime),
                size: obsolete_runtime.len() as u64,
                module: "core".to_owned(),
                pe_x64: false,
            },
        ],
    };
    let installed_release = release.clone();
    let archive = archive::PreparedArchive::from_files(HashMap::from([
        ("proxy.dll".to_owned(), proxy.clone()),
        ("OptiScaler.ini".to_owned(), config.clone()),
        ("runtime.dll".to_owned(), runtime),
        ("obsolete.dll".to_owned(), obsolete_runtime),
    ]));
    let external_runtime = b"pre-existing exact module".to_vec();
    let external_runtime_path = game_root.path().join("plugins").join("OptiPatcher.asi");
    std::fs::create_dir(external_runtime_path.parent().expect("module parent"))
        .expect("module directory");
    std::fs::write(&external_runtime_path, &external_runtime).expect("pre-existing module");
    let external_runtime_sha256 =
        Sha256Hash::new(archive::sha256_hex(&external_runtime)).expect("pre-existing module hash");
    let external_module = PreparedModuleArtifact {
        module_id: "core".to_owned(),
        manifest: super::super::super::types::OptiScalerModuleArtifact {
            id: "pre-existing-module".to_owned(),
            source: release.source.clone(),
            sha256: external_runtime_sha256.to_string(),
            size: external_runtime.len() as u64,
            target: "plugins/OptiPatcher.asi".to_owned(),
            pe_x64: false,
        },
        bytes: external_runtime,
        sha256: external_runtime_sha256.clone(),
    };
    let authority = FileSafetyAuthority::new();
    let assessment = authority
        .issue_game_assessment(&context, &game_id)
        .expect("safety assessment");
    let safety = authority
        .game_permit(game_id.clone(), Some(&assessment.context_token))
        .expect("safety permit");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let target_dir = game_root.path().to_path_buf();
    let plan = ApplyPlan {
        intent: ApplyIntent::Install {
            modules: Some(vec!["core".to_owned()]),
        },
        context: &context,
        guard,
        manifest,
        game_id: game_id.clone(),
        old_state: None,
        old_managed_files: Vec::new(),
        old_config_base: None,
        release,
        modules: HashSet::from(["core".to_owned()]),
        artifacts: ArtifactSet {
            archive,
            module_artifacts: vec![external_module],
            native_artifacts: Vec::new(),
        },
        target: ApplyTarget {
            exe: exe.clone(),
            dir: target_dir.clone(),
            proxy: EvaluatedProxyPlan {
                slot: proxy_path.clone(),
                chain_reshade: true,
                downstream_path: Some(downstream_path.clone()),
                conflict: None,
                reshade_source_path: Some(proxy_path.clone()),
                reshade_source_sha256: Some(reshade_sha256),
            },
        },
        safety,
        compatibility_invariants: Vec::new(),
        accepted_prerequisite_binding: renderpilot_domain::OptiScalerPrerequisiteBinding::None,
    };

    apply_release(&plan).expect("first install");
    assert_eq!(
        std::fs::read(&downstream_path).expect("relocated ReShade"),
        reshade
    );
    assert!(
        game_root
            .path()
            .join("OptiScaler")
            .join("runtime.dll")
            .is_file()
    );
    let state = context
        .storage()
        .get_optiscaler_install_state(&game_id)
        .expect("state")
        .expect("installed state");
    let configuration = state
        .release_files
        .iter()
        .find(|receipt| receipt.role == OptiScalerFileRole::Configuration)
        .expect("configuration receipt");
    assert_eq!(configuration.installed.ownership(), FileOwnership::Owned);
    assert_eq!(
        configuration.cleanup,
        OptiScalerFileCleanup::PreserveCurrentThenRestoreBaseline
    );
    let external_binding = state
        .runtime_bindings
        .iter()
        .find(|binding| {
            crate::paths::same_path(
                Path::new(binding.path.as_str()),
                external_runtime_path.as_path(),
            )
        })
        .expect("pre-existing runtime binding");
    assert_eq!(
        external_binding.installed.ownership(),
        FileOwnership::Reused
    );
    assert!(matches!(
        &external_binding.baseline,
        OptiScalerFileBaseline::Present { receipt }
            if receipt.ownership() == FileOwnership::Reused
                && receipt.digest() == &external_runtime_sha256
    ));
    assert!(
        state
            .directory_receipts
            .iter()
            .any(|receipt| crate::paths::same_path(
                Path::new(receipt.path.as_str()),
                &game_root.path().join("OptiScaler")
            ))
    );
    assert_eq!(
        context
            .storage()
            .get_proxy_topology(&game_id)
            .expect("topology")
            .expect("topology")
            .id,
        format!("optiscaler:{}", game_id.as_str())
    );
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(&game_id)
            .expect("pending rows")
            .is_empty()
    );

    let next_proxy = b"optiscaler-proxy-next".to_vec();
    // The update changes managed content while retaining the exact
    // pre-first-write user baseline for the final uninstall.
    let next_config = b"[OptiScaler]\nManagedDefault=true\n".to_vec();
    let next_runtime = b"private-runtime-next".to_vec();
    let mut next_source = installed_release.source.clone();
    next_source.tag = "v1.0.1".to_owned();
    next_source.asset = "Optiscaler_1.0.1.7z".to_owned();
    let next_release = OptiScalerRelease {
        id: "v1.0.1".to_owned(),
        source: next_source,
        archive_sha256: "b".repeat(64),
        archive_size: 10_000,
        config_schema: 1,
        members: vec![
            OptiScalerArchiveMember {
                archive_path: "proxy.dll".to_owned(),
                target: "$proxy".to_owned(),
                sha256: archive::sha256_hex(&next_proxy),
                size: next_proxy.len() as u64,
                module: "core".to_owned(),
                pe_x64: false,
            },
            OptiScalerArchiveMember {
                archive_path: "OptiScaler.ini".to_owned(),
                target: "OptiScaler.ini".to_owned(),
                sha256: archive::sha256_hex(&next_config),
                size: next_config.len() as u64,
                module: "core".to_owned(),
                pe_x64: false,
            },
            OptiScalerArchiveMember {
                archive_path: "runtime.dll".to_owned(),
                target: "OptiScaler/runtime.dll".to_owned(),
                sha256: archive::sha256_hex(&next_runtime),
                size: next_runtime.len() as u64,
                module: "core".to_owned(),
                pe_x64: false,
            },
        ],
    };
    let mut update_wire = plan.manifest.wire_clone();
    update_wire.current_release = next_release.id.clone();
    update_wire.releases = vec![installed_release, next_release.clone()];
    update_wire.config_migrations.clear();
    let update_manifest = OptiScalerManifest::try_from(update_wire).expect("update manifest");
    let update_archive = archive::PreparedArchive::from_files(HashMap::from([
        ("proxy.dll".to_owned(), next_proxy),
        ("OptiScaler.ini".to_owned(), next_config),
        ("runtime.dll".to_owned(), next_runtime.clone()),
    ]));
    let update_assessment = authority
        .issue_game_assessment(&context, &game_id)
        .expect("update assessment");
    let update_safety = authority
        .game_permit(game_id.clone(), Some(&update_assessment.context_token))
        .expect("update permit");
    let mut update_plan = ApplyPlan {
        intent: ApplyIntent::Update,
        context: &context,
        guard: plan.guard,
        manifest: update_manifest,
        game_id: game_id.clone(),
        old_state: Some(state.clone()),
        old_managed_files: managed_bindings_from_state(Some(&state)),
        old_config_base: None,
        release: next_release,
        modules: HashSet::from(["core".to_owned()]),
        artifacts: ArtifactSet {
            archive: update_archive,
            module_artifacts: Vec::new(),
            native_artifacts: Vec::new(),
        },
        target: ApplyTarget {
            exe,
            dir: target_dir.clone(),
            proxy: EvaluatedProxyPlan {
                slot: proxy_path.clone(),
                chain_reshade: true,
                downstream_path: Some(downstream_path.clone()),
                conflict: None,
                reshade_source_path: None,
                reshade_source_sha256: None,
            },
        },
        safety: update_safety,
        compatibility_invariants: Vec::new(),
        accepted_prerequisite_binding: renderpilot_domain::OptiScalerPrerequisiteBinding::None,
    };

    std::fs::write(&proxy_path, b"foreign proxy drift").expect("proxy drift");
    apply_release(&update_plan).expect_err("update must reject a drifted owned proxy");
    assert_eq!(
        std::fs::read(&proxy_path).expect("foreign proxy remains"),
        b"foreign proxy drift",
        "the failed update must not overwrite externally changed owned bytes"
    );
    assert_eq!(
        context
            .storage()
            .get_optiscaler_install_state(&game_id)
            .expect("state after rejected update")
            .expect("installed state after rejected update")
            .release_id,
        "v1.0.0"
    );
    assert_eq!(
        context
            .storage()
            .get_optiscaler_install_state(&game_id)
            .expect("state after install")
            .expect("installed state")
            .release_files
            .iter()
            .find(|receipt| receipt.role == OptiScalerFileRole::Configuration)
            .expect("config receipt")
            .installed
            .ownership(),
        FileOwnership::Owned
    );
    std::fs::write(&proxy_path, &proxy).expect("restore committed proxy");
    let runtime_path = target_dir.join("OptiScaler").join("runtime.dll");
    std::fs::write(&runtime_path, b"foreign runtime drift").expect("runtime drift");
    apply_release(&update_plan).expect_err("update must reject a drifted owned runtime");
    assert_eq!(
        std::fs::read(&runtime_path).expect("foreign runtime remains"),
        b"foreign runtime drift"
    );
    std::fs::write(&runtime_path, b"private-runtime").expect("restore committed runtime");
    std::fs::remove_file(&external_runtime_path)
        .expect("manually remove exact-adopted deselected runtime");
    let runtime_directory = target_dir.join("OptiScaler");
    std::fs::remove_dir_all(&runtime_directory)
        .expect("manually remove the nested managed runtime directory");
    let missing_runtime_assessment = authority
        .issue_game_assessment(&context, &game_id)
        .expect("assessment after manual runtime removal");
    update_plan.safety = authority
        .game_permit(
            game_id.clone(),
            Some(&missing_runtime_assessment.context_token),
        )
        .expect("permit after manual runtime removal");
    apply_release(&update_plan).expect("update removes obsolete release member");
    assert!(
        !target_dir.join("OptiScaler").join("obsolete.dll").exists(),
        "obsolete exact release member must be deleted by the custody operation"
    );
    assert_eq!(
        std::fs::read(&runtime_path).expect("updated runtime"),
        next_runtime
    );
    assert!(
        runtime_directory.is_dir(),
        "publication recreates the missing managed runtime directory"
    );
    let updated_state = context
        .storage()
        .get_optiscaler_install_state(&game_id)
        .expect("updated state")
        .expect("updated state");
    assert_eq!(updated_state.release_id, "v1.0.1");
    let updated_configuration = updated_state
        .release_files
        .iter()
        .find(|receipt| receipt.role == OptiScalerFileRole::Configuration)
        .expect("updated configuration receipt");
    assert_eq!(
        updated_configuration.installed.ownership(),
        FileOwnership::Owned
    );
    assert_eq!(
        updated_configuration.cleanup,
        OptiScalerFileCleanup::PreserveCurrentThenRestoreBaseline
    );
    assert_eq!(
        updated_state.configuration_baseline().bytes(),
        Some(config.as_slice())
    );

    std::fs::remove_file(&proxy_path).expect("manually remove exact OptiScaler outer");
    let result = uninstall_locked(&context, &game_id, &update_plan.guard).expect("exact uninstall");
    assert!(result.state.is_none());
    assert_eq!(
        std::fs::read(&proxy_path).expect("restored ReShade root"),
        reshade,
        "uninstall must restore the exact relocated downstream"
    );
    assert!(!downstream_path.exists());
    assert_eq!(
        std::fs::read(&preexisting_config_path).expect("retained pre-existing config"),
        config,
        "an exact pre-existing release config must survive uninstall"
    );
    assert!(!target_dir.join("OptiScaler").join("runtime.dll").exists());
    assert!(
        !external_runtime_path.exists(),
        "a manually missing exact-adopted module remains absent through update and uninstall"
    );
    assert!(
        context
            .storage()
            .get_optiscaler_install_state(&game_id)
            .expect("state after uninstall")
            .is_none()
    );
    assert!(
        context
            .storage()
            .get_proxy_topology(&game_id)
            .expect("topology after uninstall")
            .is_none()
    );
}

#[test]
fn reused_configuration_first_write_seals_the_current_user_baseline() {
    let db_root = tempdir().expect("database root");
    let game_root = tempdir().expect("game root");
    let context = Context::open_at(db_root.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:edited-config-preservation").expect("game id");
    let game = GameInstallation::new(
        GameIdentity::new(game_id.clone(), "OptiScaler", Launcher::Manual).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(game_root.path().to_string_lossy()).expect("game path"),
    );
    context.storage().upsert_game(&game).expect("game");
    let source = game_root.path().join("OptiScaler.ini");
    let proxy = game_root.path().join("dxgi.dll");
    let exe = game_root.path().join("Game.exe");
    let original = b"[OptiScaler]\nold=1\n";
    let edited = b"[OptiScaler]\nold=2\n";
    let managed = b"[OptiScaler]\nmanaged=1\n".to_vec();
    let proxy_bytes = b"OptiScaler proxy".to_vec();
    std::fs::write(&exe, b"exe").expect("exe");
    std::fs::write(&proxy, &proxy_bytes).expect("proxy");
    std::fs::write(&source, original).expect("original config");
    let manifest_seed = manifest_store::parse_manifest(include_bytes!(
        "../../../../../assets/optiscaler-fallback.json"
    ))
    .expect("bundled manifest");
    let mut release_source = manifest_seed.releases[0].source.clone();
    release_source.tag = "v1.0.0".to_owned();
    release_source.asset = "OptiScaler_1.0.0.7z".to_owned();
    let release = OptiScalerRelease {
        id: "v1.0.0".to_owned(),
        source: release_source,
        archive_sha256: "a".repeat(64),
        archive_size: 1_024,
        config_schema: 1,
        members: vec![
            OptiScalerArchiveMember {
                archive_path: "proxy.dll".to_owned(),
                target: "$proxy".to_owned(),
                sha256: archive::sha256_hex(&proxy_bytes),
                size: proxy_bytes.len() as u64,
                module: "core".to_owned(),
                pe_x64: false,
            },
            OptiScalerArchiveMember {
                archive_path: "OptiScaler.ini".to_owned(),
                target: "OptiScaler.ini".to_owned(),
                sha256: archive::sha256_hex(&managed),
                size: managed.len() as u64,
                module: "core".to_owned(),
                pe_x64: false,
            },
        ],
    };
    let mut manifest_wire = manifest_seed.wire_clone();
    manifest_wire.current_release = release.id.clone();
    manifest_wire.releases = vec![release.clone()];
    manifest_wire.config_migrations.clear();
    let manifest = OptiScalerManifest::try_from(manifest_wire).expect("test manifest");
    let topology_id = format!("optiscaler:{}", game_id.as_str());
    let topology_receipt = exact_receipt_from_live(&proxy, FileOwnership::Reused).expect("proxy");
    let topology = GameProxyTopology {
        id: topology_id.clone(),
        game_id: game_id.clone(),
        root_slot: path_ref(&proxy).expect("proxy path"),
        outer: ProxyLink {
            implementation: ProxyImplementation::OptiScaler,
            path: path_ref(&proxy).expect("proxy path"),
            receipt: topology_receipt,
        },
        downstream: None,
        downstream_origin: None,
        root_prestate: ProxyRootPrestate::Absent,
    };
    let adopted_config =
        exact_receipt_from_live(&source, FileOwnership::Reused).expect("adopted configuration");
    let state = renderpilot_domain::from_new_adoption(
        renderpilot_domain::OptiScalerInstallStateParts {
            game_id: game_id.clone(),
            release_id: "v1.0.0".to_owned(),
            manifest_revision: manifest.revision.clone(),
            archive_sha256: Some(Sha256Hash::new("a".repeat(64)).expect("archive hash")),
            source: Some("https://example.invalid/OptiScaler_test.7z".to_owned()),
            target_exe_path: path_ref(&exe).expect("exe path"),
            target_dir: path_ref(game_root.path()).expect("target path"),
            modules: vec!["core".to_owned()],
            release_files: vec![renderpilot_domain::OptiScalerFileReceipt {
                path: path_ref(&source).expect("config path"),
                installed: adopted_config.clone(),
                role: renderpilot_domain::OptiScalerFileRole::Configuration,
                cleanup: renderpilot_domain::OptiScalerFileCleanup::PreserveUnchanged,
                baseline: renderpilot_domain::OptiScalerReleaseFileBaseline::Absent,
            }],
            runtime_bindings: Vec::new(),
            directory_receipts: Vec::new(),
            proxy_topology_id: Some(topology_id),
            config_schema: 1,
            config_base_release: "v1.0.0".to_owned(),
            adoption_state: renderpilot_domain::OptiScalerAdoptionState::AdoptedExact,
            prerequisite_binding: renderpilot_domain::OptiScalerPrerequisiteBinding::None,
            created_at: None,
            updated_at: None,
        },
        renderpilot_domain::OptiScalerConfigurationBaseline::present(
            adopted_config,
            original.to_vec(),
        )
        .expect("configuration baseline"),
    )
    .expect("state");
    context
        .storage()
        .commit_game_mutation(renderpilot_storage_sqlite::GameMutationCommit {
            game_id: &game_id,
            component_set: None,
            baseline_mutations: &[],
            addon: renderpilot_storage_sqlite::InstalledAddonMutation::OptiScaler(
                renderpilot_storage_sqlite::OptiScalerAggregateMutation::AdoptExactMetadata {
                    state: &state,
                    topology: &topology,
                },
            ),
            mutation_id: None,
        })
        .expect("adopt aggregate");
    let state = context
        .storage()
        .get_optiscaler_install_state(&game_id)
        .expect("persisted adopted state")
        .expect("adopted state");
    // Model the ordinary editor atomic-save path rather than overwriting the
    // old file in place: replace the pathname with a distinct file identity.
    let saved_original = game_root.path().join("OptiScaler.user-save");
    std::fs::rename(&source, &saved_original).expect("move original config aside");
    std::fs::write(&source, edited).expect("atomic-save replacement config");
    let current =
        exact_receipt_from_live(&source, FileOwnership::Reused).expect("current user config");
    assert_ne!(
        current.identity(),
        state
            .configuration_receipt()
            .expect("adopted configuration")
            .installed
            .identity(),
        "the user save must replace the file generation"
    );
    let archive = archive::PreparedArchive::from_files(HashMap::from([
        ("proxy.dll".to_owned(), proxy_bytes),
        ("OptiScaler.ini".to_owned(), managed),
    ]));
    let authority = FileSafetyAuthority::new();
    let assessment = authority
        .issue_game_assessment(&context, &game_id)
        .expect("safety assessment");
    let safety = authority
        .game_permit(game_id.clone(), Some(&assessment.context_token))
        .expect("safety permit");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let plan = ApplyPlan {
        intent: ApplyIntent::Update,
        context: &context,
        guard,
        manifest,
        game_id: game_id.clone(),
        old_state: Some(state),
        old_managed_files: Vec::new(),
        old_config_base: Some(original.to_vec()),
        release,
        modules: HashSet::from(["core".to_owned()]),
        artifacts: ArtifactSet {
            archive,
            module_artifacts: Vec::new(),
            native_artifacts: Vec::new(),
        },
        target: ApplyTarget {
            exe,
            dir: game_root.path().to_path_buf(),
            proxy: EvaluatedProxyPlan {
                slot: proxy,
                chain_reshade: false,
                downstream_path: None,
                conflict: None,
                reshade_source_path: None,
                reshade_source_sha256: None,
            },
        },
        safety,
        compatibility_invariants: Vec::new(),
        accepted_prerequisite_binding: renderpilot_domain::OptiScalerPrerequisiteBinding::None,
    };
    apply_release(&plan).expect("production update acquires replaced configuration");
    let persisted = context
        .storage()
        .get_optiscaler_install_state(&game_id)
        .expect("persisted state")
        .expect("managed state");
    assert_eq!(
        persisted.configuration_baseline().bytes(),
        Some(edited.as_slice()),
        "the immutable baseline is the exact replacement bytes observed by apply planning"
    );
    assert_eq!(
        persisted
            .configuration_receipt()
            .expect("managed configuration")
            .installed
            .ownership(),
        FileOwnership::Owned
    );
}

#[test]
fn reused_configuration_retarget_preserves_the_old_exact_file_and_commits_an_absent_baseline() {
    let db_root = tempdir().expect("database root");
    let game_root = tempdir().expect("game root");
    let old_dir = game_root.path().join("old");
    let new_dir = game_root.path().join("new");
    std::fs::create_dir_all(&old_dir).expect("old directory");
    std::fs::create_dir_all(&new_dir).expect("new directory");
    let context = Context::open_at(db_root.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:retarget-reused-config").expect("game id");
    let game = GameInstallation::new(
        GameIdentity::new(game_id.clone(), "OptiScaler", Launcher::Manual).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(game_root.path().to_string_lossy()).expect("game path"),
    );
    context.storage().upsert_game(&game).expect("game");

    let old_exe = old_dir.join("Game.exe");
    let new_exe = new_dir.join("Game.exe");
    let proxy = old_dir.join("dxgi.dll");
    let old_config = old_dir.join("OptiScaler.ini");
    let new_config = new_dir.join("OptiScaler.ini");
    let proxy_bytes = b"OptiScaler proxy".to_vec();
    let config_bytes = b"[OptiScaler]\nmanaged=1\n".to_vec();
    std::fs::write(&old_exe, b"old exe").expect("old exe");
    std::fs::write(&new_exe, b"new exe").expect("new exe");
    std::fs::write(&proxy, &proxy_bytes).expect("proxy");
    std::fs::write(&old_config, &config_bytes).expect("old config");

    let manifest_seed = manifest_store::parse_manifest(include_bytes!(
        "../../../../../assets/optiscaler-fallback.json"
    ))
    .expect("bundled manifest");
    let mut release_source = manifest_seed.releases[0].source.clone();
    release_source.tag = "v1.0.0".to_owned();
    release_source.asset = "OptiScaler_1.0.0.7z".to_owned();
    let release = OptiScalerRelease {
        id: "v1.0.0".to_owned(),
        source: release_source,
        archive_sha256: "b".repeat(64),
        archive_size: 1_024,
        config_schema: 1,
        members: vec![
            OptiScalerArchiveMember {
                archive_path: "proxy.dll".to_owned(),
                target: "$proxy".to_owned(),
                sha256: archive::sha256_hex(&proxy_bytes),
                size: proxy_bytes.len() as u64,
                module: "core".to_owned(),
                pe_x64: false,
            },
            OptiScalerArchiveMember {
                archive_path: "OptiScaler.ini".to_owned(),
                target: "OptiScaler.ini".to_owned(),
                sha256: archive::sha256_hex(&config_bytes),
                size: config_bytes.len() as u64,
                module: "core".to_owned(),
                pe_x64: false,
            },
        ],
    };
    let mut manifest_wire = manifest_seed.wire_clone();
    manifest_wire.current_release = release.id.clone();
    manifest_wire.releases = vec![release.clone()];
    manifest_wire.config_migrations.clear();
    let manifest = OptiScalerManifest::try_from(manifest_wire).expect("test manifest");

    let topology_id = format!("optiscaler:{}", game_id.as_str());
    let topology = GameProxyTopology {
        id: topology_id.clone(),
        game_id: game_id.clone(),
        root_slot: path_ref(&proxy).expect("root slot"),
        outer: ProxyLink {
            implementation: ProxyImplementation::OptiScaler,
            path: path_ref(&proxy).expect("outer path"),
            receipt: exact_receipt_from_live(&proxy, FileOwnership::Reused).expect("outer receipt"),
        },
        downstream: None,
        downstream_origin: None,
        root_prestate: ProxyRootPrestate::Absent,
    };
    let adopted_config =
        exact_receipt_from_live(&old_config, FileOwnership::Reused).expect("config receipt");
    let state = renderpilot_domain::from_new_adoption(
        renderpilot_domain::OptiScalerInstallStateParts {
            game_id: game_id.clone(),
            release_id: release.id.clone(),
            manifest_revision: manifest.revision.clone(),
            archive_sha256: Some(Sha256Hash::new("b".repeat(64)).expect("archive hash")),
            source: Some("https://example.invalid/OptiScaler_1.0.0.7z".to_owned()),
            target_exe_path: path_ref(&old_exe).expect("old exe path"),
            target_dir: path_ref(&old_dir).expect("old target path"),
            modules: vec!["core".to_owned()],
            release_files: vec![renderpilot_domain::OptiScalerFileReceipt {
                path: path_ref(&old_config).expect("config path"),
                installed: adopted_config.clone(),
                role: renderpilot_domain::OptiScalerFileRole::Configuration,
                cleanup: renderpilot_domain::OptiScalerFileCleanup::PreserveUnchanged,
                baseline: renderpilot_domain::OptiScalerReleaseFileBaseline::Absent,
            }],
            runtime_bindings: Vec::new(),
            directory_receipts: Vec::new(),
            proxy_topology_id: Some(topology_id),
            config_schema: 1,
            config_base_release: release.id.clone(),
            adoption_state: renderpilot_domain::OptiScalerAdoptionState::AdoptedExact,
            prerequisite_binding: renderpilot_domain::OptiScalerPrerequisiteBinding::None,
            created_at: None,
            updated_at: None,
        },
        renderpilot_domain::OptiScalerConfigurationBaseline::present(
            adopted_config,
            config_bytes.clone(),
        )
        .expect("configuration baseline"),
    )
    .expect("adopted state");
    context
        .storage()
        .commit_game_mutation(renderpilot_storage_sqlite::GameMutationCommit {
            game_id: &game_id,
            component_set: None,
            baseline_mutations: &[],
            addon: renderpilot_storage_sqlite::InstalledAddonMutation::OptiScaler(
                renderpilot_storage_sqlite::OptiScalerAggregateMutation::AdoptExactMetadata {
                    state: &state,
                    topology: &topology,
                },
            ),
            mutation_id: None,
        })
        .expect("adopt aggregate");
    let state = context
        .storage()
        .get_optiscaler_install_state(&game_id)
        .expect("persisted adopted state")
        .expect("adopted state");
    let authority = FileSafetyAuthority::new();
    let assessment = authority
        .issue_game_assessment(&context, &game_id)
        .expect("safety assessment");
    let safety = authority
        .game_permit(game_id.clone(), Some(&assessment.context_token))
        .expect("safety permit");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let plan = ApplyPlan {
        intent: ApplyIntent::Relocate {
            target_exe: new_exe.clone(),
        },
        context: &context,
        guard,
        manifest,
        game_id: game_id.clone(),
        old_state: Some(state),
        old_managed_files: Vec::new(),
        old_config_base: Some(config_bytes.clone()),
        release,
        modules: HashSet::from(["core".to_owned()]),
        artifacts: ArtifactSet {
            archive: archive::PreparedArchive::from_files(HashMap::from([
                ("proxy.dll".to_owned(), proxy_bytes),
                ("OptiScaler.ini".to_owned(), config_bytes.clone()),
            ])),
            module_artifacts: Vec::new(),
            native_artifacts: Vec::new(),
        },
        target: ApplyTarget {
            exe: new_exe,
            dir: new_dir.clone(),
            proxy: EvaluatedProxyPlan {
                slot: new_dir.join("dxgi.dll"),
                chain_reshade: false,
                downstream_path: None,
                conflict: None,
                reshade_source_path: None,
                reshade_source_sha256: None,
            },
        },
        safety,
        compatibility_invariants: Vec::new(),
        accepted_prerequisite_binding: renderpilot_domain::OptiScalerPrerequisiteBinding::None,
    };
    apply_release(&plan).expect("retarget transaction commits");

    assert_eq!(
        std::fs::read(&old_config).expect("old Reused configuration"),
        config_bytes,
        "the exact old Reused configuration is a Verify-only handoff"
    );
    assert!(
        old_config.is_file(),
        "the old configuration remains present"
    );
    assert!(
        std::fs::read(&new_config)
            .expect("new configuration")
            .starts_with(&config_bytes),
        "the new configuration contains the selected release content before managed invariants"
    );
    let new_proxy = new_dir.join("dxgi.dll");
    assert!(
        !proxy.exists(),
        "the exact-adopted old outer is released before the new slot is published"
    );
    assert_eq!(
        std::fs::read(&new_proxy).expect("new outer"),
        b"OptiScaler proxy"
    );
    let persisted = context
        .storage()
        .get_optiscaler_install_state(&game_id)
        .expect("persisted retarget state")
        .expect("retarget state");
    let configuration = persisted
        .configuration_receipt()
        .expect("retarget configuration");
    assert!(crate::paths::same_path(
        Path::new(configuration.path.as_str()),
        &new_config
    ));
    assert_eq!(configuration.installed.ownership(), FileOwnership::Owned);
    assert!(matches!(
        persisted.configuration_baseline(),
        renderpilot_domain::OptiScalerConfigurationBaseline::Absent
    ));
    let topology = context
        .storage()
        .get_proxy_topology(&game_id)
        .expect("retarget topology")
        .expect("retarget topology");
    assert!(crate::paths::same_path(
        Path::new(topology.root_slot.as_str()),
        &new_proxy
    ));
    assert_eq!(topology.outer.receipt.ownership(), FileOwnership::Owned);
}
