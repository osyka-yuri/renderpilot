use super::*;

fn exact_reused_receipt(path: &Path, digest: Sha256Hash) -> FileReceipt {
    let (parent, leaf) = crate::fs::verified_parent(path).expect("verified parent");
    let observation = parent
        .observe_leaf(&leaf)
        .expect("exact observation")
        .expect("present file");
    assert_eq!(observation.kind, crate::fs::EntryKind::File);
    assert_eq!(observation.digest.as_deref(), Some(digest.as_str()));
    FileReceipt::reused(observation.identity, digest).expect("reused receipt")
}

fn adopt_active_optiscaler_topology(context: &Context, game_id: &GameId, root: &Path) {
    let root_ref = |path: &Path| {
        PathRef::new(path.to_string_lossy().replace('\\', "/")).expect("path reference")
    };
    let proxy = root.join("dxgi.dll");
    let config = root.join("OptiScaler.ini");
    fs::write(&proxy, b"active-optiscaler-proxy").expect("proxy");
    fs::write(&config, b"[OptiScaler]\nEnabled=true\n").expect("config");
    let proxy_hash = renderpilot_detection::sha256_file(&proxy).expect("proxy hash");
    let config_hash = renderpilot_detection::sha256_file(&config).expect("config hash");
    let proxy_receipt = exact_reused_receipt(&proxy, proxy_hash);
    let config_receipt = exact_reused_receipt(&config, config_hash);
    let topology_id = format!("optiscaler:{}", game_id.as_str());
    let topology = GameProxyTopology {
        id: topology_id.clone(),
        game_id: game_id.clone(),
        root_slot: root_ref(&proxy),
        outer: ProxyLink {
            implementation: ProxyImplementation::OptiScaler,
            path: root_ref(&proxy),
            receipt: proxy_receipt,
        },
        downstream: None,
        downstream_origin: None,
        root_prestate: ProxyRootPrestate::Absent,
    };
    let state = renderpilot_domain::from_persisted(
        renderpilot_domain::OptiScalerInstallStateParts {
            game_id: game_id.clone(),
            release_id: "test-release".to_owned(),
            manifest_revision: "test-manifest".to_owned(),
            archive_sha256: None,
            source: None,
            target_exe_path: root_ref(&root.join("Game.exe")),
            target_dir: root_ref(root),
            modules: vec!["core".to_owned()],
            release_files: vec![OptiScalerFileReceipt {
                path: root_ref(&config),
                installed: config_receipt.clone(),
                role: OptiScalerFileRole::Configuration,
                cleanup: OptiScalerFileCleanup::PreserveUnchanged,
                baseline: renderpilot_domain::OptiScalerReleaseFileBaseline::Absent,
            }],
            runtime_bindings: Vec::new(),
            directory_receipts: Vec::new(),
            proxy_topology_id: Some(topology_id),
            config_schema: 1,
            config_base_release: "test-release".to_owned(),
            adoption_state: OptiScalerAdoptionState::AdoptedExact,
            prerequisite_binding: renderpilot_domain::OptiScalerPrerequisiteBinding::None,
            created_at: None,
            updated_at: None,
        },
        renderpilot_domain::OptiScalerConfigurationBaseline::present(
            config_receipt,
            b"[OptiScaler]\nEnabled=true\n".to_vec(),
        )
        .expect("configuration baseline"),
    )
    .expect("state");
    context
        .storage()
        .commit_game_mutation(GameMutationCommit {
            game_id,
            component_set: None,
            baseline_mutations: &[],
            addon: InstalledAddonMutation::OptiScaler(
                OptiScalerAggregateMutation::AdoptExactMetadata {
                    state: &state,
                    topology: &topology,
                },
            ),
            mutation_id: None,
        })
        .expect("adopt active OptiScaler topology");
}

#[test]
fn active_optiscaler_topology_blocks_peer_torn_recovery_before_live_writes() {
    let db_root = tempfile::tempdir().expect("database root");
    let game_root = tempfile::tempdir().expect("game root");
    let context = Context::open_at(db_root.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:peer-torn-fence").expect("game id");
    store_game(&context, game_id.clone(), game_root.path());
    adopt_active_optiscaler_topology(&context, &game_id, game_root.path());

    let debris = game_root.path().join("torn.addon64");
    fs::write(&debris, b"peer debris must remain byte-for-byte").expect("debris");
    let marker = crate::addons::engine::sentinel_path(game_root.path(), AddonKind::RenoDx);
    crate::addons::engine::write_sentinel(&marker).expect("sentinel");
    let roots = crate::addons::reshade::InstallRoots::resolve_from_ini(game_root.path());

    let error = crate::addons::install_guard::guard_exclusivity_and_torn_with_external_owner(
        &context,
        &game_id,
        AddonKind::RenoDx,
        &roots,
        None,
    )
    .expect_err("active topology must fence before recovery");

    assert_eq!(
        error,
        ServiceError::PeerTopologyConflict {
            peer_kind: AddonKind::RenoDx,
        }
    );
    assert_eq!(
        fs::read(&debris).expect("debris survives"),
        b"peer debris must remain byte-for-byte"
    );
    assert!(marker.is_file());
}

#[test]
fn prepare_rejects_paths_outside_scope() {
    let root = tempfile::tempdir().expect("game");
    let outside = tempfile::tempdir().expect("outside");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:scope").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let foreign = outside.path().join("foreign.dll");
    fs::write(&foreign, b"x").expect("seed");

    let Err(error) = DurableFileTransaction::prepare(
        &context,
        &guard,
        &scope(root.path()),
        "test",
        None,
        [foreign],
    ) else {
        panic!("outside scope should fail");
    };
    assert!(error.to_string().contains("outside authorized roots"));
}

#[test]
fn prepare_rejects_non_file_paths() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:non-file").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let nested = root.path().join("subdir");
    fs::create_dir_all(&nested).expect("dir");

    let Err(error) = DurableFileTransaction::prepare(
        &context,
        &guard,
        &scope(root.path()),
        "test",
        None,
        [nested],
    ) else {
        panic!("directory should fail");
    };
    assert!(error.to_string().contains("non-file path"));
}

#[test]
fn recover_pending_retains_unknown_transaction_directory() {
    let root = tempfile::tempdir().expect("root");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:unknown-transaction-directory").expect("id");
    let guard = crate::game_mutation_lock::blocking_lock(&game_id);
    let foreign = context.file_mutation_root().join("foreign-tool-state");
    let sentry = foreign.join("do-not-delete");
    fs::create_dir_all(&foreign).expect("foreign directory");
    fs::write(&sentry, b"foreign state").expect("foreign sentry");

    recover_pending(&context, &guard).expect("recovery");

    assert!(foreign.is_dir(), "unknown directory must be retained");
    assert_eq!(
        fs::read(&sentry).expect("foreign sentry survives"),
        b"foreign state"
    );
}

#[test]
fn v2_prepared_recovery_never_touches_a_foreign_file_created_after_crash() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:v2-crash").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let target = root.path().join("renodx-dlssfix.addon64");
    let mutation = RetryableFileMutationV2::prepare(
        &context,
        &guard,
        &scope(root.path()),
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_INSTALL,
        None,
        &RetryableFilePlan {
            operations: vec![RetryableFileOperation::Write {
                path: target.clone(),
                bytes: b"renderpilot".to_vec(),
                expected: V2DiskObservation::Absent,
            }],
        },
    )
    .expect("prepared");
    drop(mutation); // crash before first target write
    fs::write(&target, b"foreign").expect("foreign create during downtime");

    recover_pending(&context, &guard).expect("cleanup-only recovery");
    assert_eq!(fs::read(&target).expect("foreign target"), b"foreign");
}

#[test]
fn peer_prepare_paths_fail_before_reserving_or_writing_under_optiscaler_topology() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:peer-fence").expect("id");
    store_game(&context, game_id.clone(), root.path());
    adopt_active_optiscaler_topology(&context, &game_id, root.path());
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let target = root.path().join("peer.addon64");

    let v1_error = DurableFileTransaction::prepare(
        &context,
        &guard,
        &scope(root.path()),
        renderpilot_domain::mutation_features::LUMA_INSTALL,
        None,
        [target.clone()],
    )
    .err()
    .expect("Luma prepare must be fenced");
    assert_eq!(
        v1_error,
        ServiceError::peer_topology_conflict(renderpilot_domain::AddonKind::Luma,)
    );

    let v2_error = RetryableFileMutationV2::prepare(
        &context,
        &guard,
        &scope(root.path()),
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_INSTALL,
        None,
        &RetryableFilePlan {
            operations: vec![RetryableFileOperation::Write {
                path: target.clone(),
                bytes: b"peer".to_vec(),
                expected: V2DiskObservation::Absent,
            }],
        },
    )
    .err()
    .expect("RenoDX prepare must be fenced");
    assert_eq!(
        v2_error,
        ServiceError::peer_topology_conflict(renderpilot_domain::AddonKind::RenoDx,)
    );

    assert!(!target.exists(), "peer target must remain untouched");
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(&game_id)
            .expect("pending rows")
            .is_empty(),
        "peer fence must run before reserving a durable row"
    );
}
