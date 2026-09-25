use super::super::*;
use super::{PeerTopologyDirection, plan_peer_host_transition};
use renderpilot_application::{GameRepository, InstalledAddonRepository};
use renderpilot_domain::{
    AddonKind, FileOwnership, GameIdentity, GameInstallation, GameRuntime, Launcher,
    ManagedAddonFile, ManagedFileBaseline, PathRef, Platform,
};
use tempfile::tempdir;

struct PeerFixture {
    _database: tempfile::TempDir,
    root: tempfile::TempDir,
    context: Context,
    game_id: GameId,
    from: PathRef,
    to: PathRef,
    addon: PathRef,
    host_bytes: Vec<u8>,
}

fn peer_fixture() -> PeerFixture {
    let database = tempdir().expect("database root");
    let root = tempdir().expect("game root");
    let context = Context::open_at(database.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:peer-host-transition").expect("game id");
    let game = GameInstallation::new(
        GameIdentity::new(game_id.clone(), "Peer host transition", Launcher::Manual)
            .expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(root.path().to_string_lossy().into_owned()).expect("game path"),
    );
    context.storage().upsert_game(&game).expect("game");
    let host_bytes = crate::addons::test_support::build_pe_with_exports(
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
    let from_path = root.path().join("d3d12.dll");
    let to = PathRef::new(
        root.path()
            .join("ReShade64.dll")
            .to_string_lossy()
            .into_owned(),
    )
    .expect("destination path");
    let addon = PathRef::new(
        root.path()
            .join("renodx-peer.addon64")
            .to_string_lossy()
            .into_owned(),
    )
    .expect("addon path");
    std::fs::write(&from_path, &host_bytes).expect("source");
    PeerFixture {
        _database: database,
        root,
        context,
        game_id,
        from: PathRef::new(from_path.to_string_lossy().into_owned()).expect("source path"),
        to,
        addon,
        host_bytes,
    }
}

fn persist_peer(fixture: &PeerFixture, peer: &InstalledAddon) {
    fixture
        .context
        .storage()
        .upsert_installed_addon(peer)
        .expect("peer receipt persisted");
}

fn owned_host(fixture: &PeerFixture, baseline: ManagedFileBaseline) -> ManagedAddonFile {
    ManagedAddonFile::owned(
        fixture.from.clone(),
        baseline,
        crate::fs::sha256_of_non_empty_file(Path::new(fixture.from.as_str())).expect("hash"),
    )
}

#[test]
fn peer_transition_allows_created_host() {
    let fixture = peer_fixture();
    let peer = InstalledAddon::new(
        fixture.game_id.clone(),
        AddonKind::RenoDx,
        fixture.addon.clone(),
    )
    .with_created_file(fixture.from.clone());
    persist_peer(&fixture, &peer);
    let plan = plan_peer_host_transition(
        PeerTopologyDirection::IntoOptiTopology,
        &fixture.context,
        &fixture.game_id,
        &fixture.from,
        &fixture.to,
        None,
    )
    .expect("created host plan")
    .expect("owner");
    assert!(plan.sidecar.is_none());
    assert!(!plan.receipt.after.created_files().contains(&fixture.from));
    assert!(!plan.receipt.after.created_files().contains(&fixture.to));
    let managed = plan
        .receipt
        .after
        .managed_files()
        .iter()
        .find(|file| file.path() == &fixture.to)
        .expect("canonical managed host");
    assert_eq!(managed.mode(), renderpilot_domain::ManagedFileMode::Owned);
    assert_eq!(managed.baseline(), &ManagedFileBaseline::Absent);
    assert_eq!(managed.installed_sha256(), &plan.live_sha256);
    assert_eq!(plan.destination_ownership(), FileOwnership::Owned);
}

#[test]
fn peer_transition_allows_owned_absent_host() {
    let fixture = peer_fixture();
    let peer = InstalledAddon::new(
        fixture.game_id.clone(),
        AddonKind::RenoDx,
        fixture.addon.clone(),
    )
    .try_with_managed_files(vec![owned_host(&fixture, ManagedFileBaseline::Absent)])
    .expect("managed receipt");
    persist_peer(&fixture, &peer);
    let plan = plan_peer_host_transition(
        PeerTopologyDirection::IntoOptiTopology,
        &fixture.context,
        &fixture.game_id,
        &fixture.from,
        &fixture.to,
        None,
    )
    .expect("owned absent plan")
    .expect("owner");
    assert!(plan.sidecar.is_none());
    assert_eq!(plan.receipt.after.managed_files()[0].path(), &fixture.to);
}

#[test]
fn peer_transition_moves_owned_present_sidecar_through_relocate() {
    let fixture = peer_fixture();
    let source_sidecar =
        crate::fs::backup_path(Path::new(fixture.from.as_str())).expect("source sidecar");
    let baseline_bytes = b"peer baseline";
    std::fs::write(&source_sidecar, baseline_bytes).expect("source sidecar");
    let baseline = crate::fs::sha256_of_non_empty_file(&source_sidecar).expect("baseline hash");
    let peer = InstalledAddon::new(
        fixture.game_id.clone(),
        AddonKind::RenoDx,
        fixture.addon.clone(),
    )
    .try_with_managed_files(vec![owned_host(
        &fixture,
        ManagedFileBaseline::Present { sha256: baseline },
    )])
    .expect("managed receipt");
    persist_peer(&fixture, &peer);
    std::fs::write(fixture.to.as_str(), b"outer").expect("outer");
    let outer_sha256 =
        crate::fs::sha256_of_non_empty_file(Path::new(fixture.to.as_str())).expect("outer hash");
    let plan = plan_peer_host_transition(
        PeerTopologyDirection::IntoOptiTopology,
        &fixture.context,
        &fixture.game_id,
        &fixture.from,
        &fixture.to,
        Some(&outer_sha256),
    )
    .expect("owned present plan")
    .expect("owner");
    let sidecar = plan.sidecar.as_ref().expect("sidecar plan");
    let config_path = fixture.root.path().join("OptiScaler.ini");
    std::fs::write(&config_path, b"config").expect("config");
    let topology_id = format!("optiscaler:{}", fixture.game_id.as_str());
    let topology = GameProxyTopology {
        id: topology_id.clone(),
        game_id: fixture.game_id.clone(),
        root_slot: fixture.to.clone(),
        outer: ProxyLink {
            implementation: ProxyImplementation::OptiScaler,
            path: fixture.to.clone(),
            receipt: exact_receipt_from_live(
                Path::new(fixture.to.as_str()),
                renderpilot_domain::FileOwnership::Reused,
            )
            .expect("outer receipt"),
        },
        downstream: Some(ProxyLink {
            implementation: ProxyImplementation::ReShade,
            path: fixture.from.clone(),
            receipt: exact_receipt_from_live(
                Path::new(fixture.from.as_str()),
                renderpilot_domain::FileOwnership::Owned,
            )
            .expect("downstream receipt"),
        }),
        downstream_origin: Some(fixture.to.clone()),
        root_prestate: ProxyRootPrestate::RelocatedDownstream,
    };
    let configuration_receipt = exact_receipt_from_live(&config_path, FileOwnership::Reused)
        .expect("configuration receipt");
    let state = renderpilot_domain::from_new_adoption(
        renderpilot_domain::OptiScalerInstallStateParts {
            game_id: fixture.game_id.clone(),
            release_id: "peer-sidecar".to_owned(),
            manifest_revision: "test".to_owned(),
            archive_sha256: None,
            source: None,
            target_exe_path: path_ref(&fixture.root.path().join("Game.exe")).expect("target exe"),
            target_dir: path_ref(fixture.root.path()).expect("target dir"),
            modules: vec!["core".to_owned()],
            release_files: vec![OptiScalerFileReceipt {
                path: path_ref(&config_path).expect("config path"),
                installed: configuration_receipt.clone(),
                role: OptiScalerFileRole::Configuration,
                cleanup: OptiScalerFileCleanup::PreserveUnchanged,
                baseline: renderpilot_domain::OptiScalerReleaseFileBaseline::Absent,
            }],
            runtime_bindings: Vec::new(),
            directory_receipts: Vec::new(),
            proxy_topology_id: Some(topology_id),
            config_schema: 1,
            config_base_release: "peer-sidecar".to_owned(),
            adoption_state: OptiScalerAdoptionState::AdoptedExact,
            prerequisite_binding: renderpilot_domain::OptiScalerPrerequisiteBinding::None,
            created_at: None,
            updated_at: None,
        },
        renderpilot_domain::OptiScalerConfigurationBaseline::present(
            configuration_receipt,
            b"config".to_vec(),
        )
        .expect("configuration baseline"),
    )
    .expect("state");
    fixture
        .context
        .storage()
        .commit_game_mutation(renderpilot_storage_sqlite::GameMutationCommit {
            game_id: &fixture.game_id,
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
        .expect("initial OptiScaler aggregate");
    let persisted_state = fixture
        .context
        .storage()
        .get_optiscaler_install_state(&fixture.game_id)
        .expect("state")
        .expect("persisted state");
    let guard = crate::game_mutation_lock::try_lock(&fixture.game_id).expect("guard");
    let scope = MutationScope::single(fixture.root.path()).expect("scope");
    let targets = apply_snapshot_overrides(
        [
            PathBuf::from(fixture.from.as_str()),
            PathBuf::from(fixture.to.as_str()),
            sidecar.source.clone(),
            sidecar.destination.clone(),
            config_path.clone(),
        ],
        plan.mutation_targets(),
        std::iter::empty(),
        scope.roots(),
    )
    .expect("snapshot targets");
    let mut exact_preimages = HashMap::new();
    exact_preimages.insert(
        crate::paths::normalized_key(Path::new(fixture.from.as_str())),
        crate::file_mutation::optiscaler::PlannedPreimage::Exact {
            current: topology
                .downstream
                .as_ref()
                .expect("downstream")
                .receipt
                .clone(),
            prior_owned: None,
        },
    );
    exact_preimages.insert(
        crate::paths::normalized_key(Path::new(fixture.to.as_str())),
        crate::file_mutation::optiscaler::PlannedPreimage::ExactReused {
            current: topology.outer.receipt.clone(),
            authority:
                crate::file_mutation::optiscaler::ReusedMutationAuthority::OptiScalerArtifact,
        },
    );
    exact_preimages.insert(
        crate::paths::normalized_key(&sidecar.source),
        match sidecar.source_receipt.ownership() {
            FileOwnership::Owned => crate::file_mutation::optiscaler::PlannedPreimage::Exact {
                current: sidecar.source_receipt.clone(),
                prior_owned: None,
            },
            FileOwnership::Reused => {
                crate::file_mutation::optiscaler::PlannedPreimage::ExactReused {
                    current: sidecar.source_receipt.clone(),
                    authority:
                        crate::file_mutation::optiscaler::ReusedMutationAuthority::RelocationSource,
                }
            }
        },
    );
    exact_preimages.insert(
        crate::paths::normalized_key(&config_path),
        crate::file_mutation::optiscaler::PlannedPreimage::ExactReused {
            current: exact_receipt_from_live(&config_path, FileOwnership::Reused)
                .expect("configuration receipt"),
            authority: crate::file_mutation::optiscaler::ReusedMutationAuthority::ObservationOnly,
        },
    );
    let delete_paths = HashSet::from([
        // The exact outer delete is the producer before restoring the
        // peer host into this same root slot.
        crate::paths::normalized_key(Path::new(fixture.to.as_str())),
    ]);
    let relocations = plan.relocations();
    let operations = super::super::build_operations(
        &targets,
        &HashSet::new(),
        &delete_paths,
        &relocations,
        &exact_preimages,
        &[],
        std::iter::empty::<&std::path::Path>(),
    )
    .expect("canonical operation plan");
    let subject_id = format!("optiscaler:{}", fixture.game_id.as_str());
    std::fs::write(&sidecar.destination, b"foreign sidecar").expect("sidecar destination race");
    let error = crate::file_mutation::optiscaler::run_optiscaler_mutation(
        &crate::file_mutation::optiscaler::OptiScalerMutation {
            context: &fixture.context,
            guard: &guard,
            scope: &scope,
            feature: renderpilot_domain::mutation_features::OPTISCALER_UNINSTALL,
            subject_id: Some(&subject_id),
            operations: operations.clone(),
            managed_endpoint_roots: Vec::new(),
            threat_model: crate::file_mutation::optiscaler::ThreatModel::CooperativeSameUid,
        },
        |_| Ok::<_, ServiceError>(()),
        |_, ()| Ok::<_, ServiceError>(()),
        |()| {},
        || {},
    )
    .expect_err("a sidecar appearing after peer planning must be fenced");
    // The planner now reports the common endpoint-drift category for a
    // destination that became occupied after planning.  The guarantee is
    // the fail-closed error class plus retention of the foreign object;
    // its diagnostic wording is intentionally not part of the contract.
    assert!(matches!(error, ServiceError::CommandFailed(_)));
    assert_eq!(
        std::fs::read(&sidecar.destination).expect("foreign sidecar"),
        b"foreign sidecar"
    );
    assert_eq!(
        std::fs::read(&sidecar.source).expect("source sidecar"),
        baseline_bytes
    );
    assert_eq!(
        std::fs::read(fixture.to.as_str()).expect("unchanged outer"),
        b"outer"
    );
    assert!(
        fixture
            .context
            .storage()
            .pending_file_mutations_for_game(&fixture.game_id)
            .expect("pending rows")
            .is_empty()
    );
    std::fs::remove_file(&sidecar.destination).expect("remove raced sidecar");

    let mut changed = Vec::new();
    crate::file_mutation::optiscaler::run_optiscaler_mutation(
        &crate::file_mutation::optiscaler::OptiScalerMutation {
            context: &fixture.context,
            guard: &guard,
            scope: &scope,
            feature: renderpilot_domain::mutation_features::OPTISCALER_UNINSTALL,
            subject_id: Some(&subject_id),
            operations,
            managed_endpoint_roots: Vec::new(),
            threat_model: crate::file_mutation::optiscaler::ThreatModel::CooperativeSameUid,
        },
        |mutation| {
            mutation.delete_file_exact(Path::new(fixture.to.as_str()), &topology.outer.receipt)?;
            changed.push(fixture.to.as_str().to_owned());
            let downstream = topology.downstream.as_ref().expect("downstream");
            let restored = mutation.relocate_file_exact(
                Path::new(fixture.from.as_str()),
                Path::new(fixture.to.as_str()),
                &downstream.receipt,
                downstream.receipt.ownership(),
            )?;
            assert_eq!(restored.identity(), downstream.receipt.identity());
            assert_eq!(restored.digest(), downstream.receipt.digest());
            assert_eq!(restored.ownership(), downstream.receipt.ownership());
            changed.push(fixture.to.as_str().to_owned());
            changed.push(fixture.from.as_str().to_owned());
            plan.execute_sidecar(mutation, &mut changed)?;
            mutation.verify_unchanged(&config_path)?;
            Ok::<_, ServiceError>(())
        },
        |mutation, ()| {
            let before = renderpilot_storage_sqlite::AggregateBefore::new(
                fixture.game_id.clone(),
                Some(persisted_state.clone()),
                Some(topology.clone()),
                fixture
                    .context
                    .storage()
                    .get_installed_addon(&fixture.game_id)
                    .map_err(ServiceError::from)?,
            )
            .map_err(ServiceError::from)?;
            let mutation_id = mutation.id().to_owned();
            mutation.commit_journal_aggregate(
                before,
                renderpilot_storage_sqlite::OptiScalerAggregateMutation::FilesystemWithAuxiliary {
                    before_state: Some(&persisted_state),
                    after_state: None,
                    before_topology: Some(&topology),
                    after_topology: None,
                    peer: renderpilot_storage_sqlite::OptiScalerPeerMutation::Replace {
                        before: &plan.receipt.before,
                        after: &plan.receipt.after,
                    },
                    auxiliary_preservations: &[],
                    retained_claims: &[],
                    mutation_id: &mutation_id,
                },
            )
        },
        |()| {},
        || {},
    )
    .expect("host and sidecar move");
    assert!(!Path::new(fixture.from.as_str()).exists());
    assert_eq!(
        std::fs::read(fixture.to.as_str()).expect("destination"),
        fixture.host_bytes
    );
    assert!(!sidecar.source.exists());
    assert_eq!(
        std::fs::read(&sidecar.destination).expect("destination sidecar"),
        baseline_bytes
    );
    assert_eq!(
        fixture
            .context
            .storage()
            .get_installed_addon(&fixture.game_id)
            .expect("peer receipt")
            .expect("relocated peer")
            .managed_files()[0]
            .path(),
        &fixture.to
    );
    assert!(
        fixture
            .context
            .storage()
            .get_optiscaler_install_state(&fixture.game_id)
            .expect("OptiScaler state")
            .is_none()
    );
    assert!(
        fixture
            .context
            .storage()
            .get_proxy_topology(&fixture.game_id)
            .expect("proxy topology")
            .is_none()
    );
}

#[test]
fn peer_transition_blocks_generic_backing_and_ambiguous_claims() {
    let fixture = peer_fixture();
    let backed = InstalledAddon::new(
        fixture.game_id.clone(),
        AddonKind::RenoDx,
        fixture.addon.clone(),
    )
    .with_backed_up_file(fixture.from.clone());
    persist_peer(&fixture, &backed);
    assert!(
        plan_peer_host_transition(
            PeerTopologyDirection::IntoOptiTopology,
            &fixture.context,
            &fixture.game_id,
            &fixture.from,
            &fixture.to,
            None,
        )
        .is_err()
    );

    let fixture = peer_fixture();
    let peer = InstalledAddon::new(
        fixture.game_id.clone(),
        AddonKind::RenoDx,
        fixture.addon.clone(),
    )
    .try_with_managed_files(vec![owned_host(&fixture, ManagedFileBaseline::Absent)])
    .expect("managed receipt")
    .with_registered_exe_path(fixture.from.clone());
    persist_peer(&fixture, &peer);
    assert!(
        plan_peer_host_transition(
            PeerTopologyDirection::IntoOptiTopology,
            &fixture.context,
            &fixture.game_id,
            &fixture.from,
            &fixture.to,
            None,
        )
        .is_err()
    );
}

#[test]
fn peer_transition_blocks_drifted_sidecar_and_allows_reused_without_touching_it() {
    let fixture = peer_fixture();
    let source_sidecar =
        crate::fs::backup_path(Path::new(fixture.from.as_str())).expect("source sidecar");
    let expected =
        Sha256Hash::new("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            .expect("expected hash");
    std::fs::write(&source_sidecar, b"drifted").expect("drifted sidecar");
    let peer = InstalledAddon::new(
        fixture.game_id.clone(),
        AddonKind::RenoDx,
        fixture.addon.clone(),
    )
    .try_with_managed_files(vec![owned_host(
        &fixture,
        ManagedFileBaseline::Present { sha256: expected },
    )])
    .expect("managed receipt");
    persist_peer(&fixture, &peer);
    assert!(
        plan_peer_host_transition(
            PeerTopologyDirection::IntoOptiTopology,
            &fixture.context,
            &fixture.game_id,
            &fixture.from,
            &fixture.to,
            None,
        )
        .is_err()
    );

    let fixture = peer_fixture();
    let destination_sidecar =
        crate::fs::backup_path(Path::new(fixture.to.as_str())).expect("destination sidecar");
    std::fs::write(&destination_sidecar, b"unrelated").expect("unrelated sidecar");
    let hash =
        crate::fs::sha256_of_non_empty_file(Path::new(fixture.from.as_str())).expect("host hash");
    let peer = InstalledAddon::new(
        fixture.game_id.clone(),
        AddonKind::RenoDx,
        fixture.addon.clone(),
    )
    .try_with_managed_files(vec![ManagedAddonFile::reused(fixture.from.clone(), hash)])
    .expect("reused receipt");
    persist_peer(&fixture, &peer);
    let plan = plan_peer_host_transition(
        PeerTopologyDirection::IntoOptiTopology,
        &fixture.context,
        &fixture.game_id,
        &fixture.from,
        &fixture.to,
        None,
    )
    .expect("reused plan")
    .expect("owner");
    assert!(plan.sidecar.is_none());
    assert_eq!(plan.destination_ownership(), FileOwnership::Reused);
    assert_eq!(
        std::fs::read(destination_sidecar).expect("sidecar"),
        b"unrelated"
    );
}

#[test]
fn peer_transition_reverse_accepts_exact_existing_destinations() {
    let fixture = peer_fixture();
    let old_sidecar =
        crate::fs::backup_path(Path::new(fixture.from.as_str())).expect("old sidecar");
    let baseline_bytes = b"reverse baseline";
    std::fs::write(&old_sidecar, baseline_bytes).expect("old sidecar");
    let baseline = crate::fs::sha256_of_non_empty_file(&old_sidecar).expect("baseline");
    let new_path = Path::new(fixture.to.as_str());
    let new_sidecar = crate::fs::backup_path(new_path).expect("new sidecar");
    std::fs::write(new_path, &fixture.host_bytes).expect("new host");
    std::fs::write(&new_sidecar, baseline_bytes).expect("new sidecar");
    std::fs::remove_file(Path::new(fixture.from.as_str())).expect("old host absent");
    std::fs::remove_file(&old_sidecar).expect("old sidecar absent");
    let hash = crate::fs::sha256_of_non_empty_file(new_path).expect("new hash");
    let peer = InstalledAddon::new(
        fixture.game_id.clone(),
        AddonKind::RenoDx,
        fixture.addon.clone(),
    )
    .try_with_managed_files(vec![ManagedAddonFile::owned(
        fixture.to.clone(),
        ManagedFileBaseline::Present { sha256: baseline },
        hash.clone(),
    )])
    .expect("reverse receipt");
    persist_peer(&fixture, &peer);
    let plan = plan_peer_host_transition(
        PeerTopologyDirection::OutOfOptiTopology,
        &fixture.context,
        &fixture.game_id,
        &fixture.to,
        &fixture.from,
        Some(&hash),
    )
    .expect("reverse plan")
    .expect("owner");
    assert_eq!(
        plan.sidecar.as_ref().expect("reverse sidecar").destination,
        old_sidecar
    );
    assert_eq!(plan.receipt.after.managed_files()[0].path(), &fixture.from);
}

#[test]
fn luma_reverse_transition_projects_each_managed_custody_case_to_native_receipt() {
    for (baseline_bytes, expected_backed, expected_ownership) in [
        (None, false, FileOwnership::Owned),
        (
            Some(b"Luma baseline".as_slice()),
            true,
            FileOwnership::Owned,
        ),
        (None, false, FileOwnership::Reused),
    ] {
        let fixture = peer_fixture();
        let source = Path::new(fixture.to.as_str());
        let destination = Path::new(fixture.from.as_str());
        std::fs::rename(destination, source).expect("move host to Opti downstream");
        let baseline = if let Some(bytes) = baseline_bytes {
            let sidecar = crate::fs::backup_path(source).expect("source sidecar");
            std::fs::write(&sidecar, bytes).expect("source sidecar");
            let actual = crate::fs::sha256_of_non_empty_file(&sidecar).expect("sidecar hash");
            ManagedFileBaseline::Present { sha256: actual }
        } else if expected_ownership == FileOwnership::Owned {
            ManagedFileBaseline::Absent
        } else {
            ManagedFileBaseline::Present {
                sha256: crate::fs::sha256_of_non_empty_file(source).expect("host hash"),
            }
        };
        let installed = crate::fs::sha256_of_non_empty_file(source).expect("host hash");
        let peer = InstalledAddon::new(
            fixture.game_id.clone(),
            AddonKind::Luma,
            fixture.addon.clone(),
        )
        .try_with_managed_files(vec![ManagedAddonFile::owned(
            fixture.to.clone(),
            baseline.clone(),
            installed.clone(),
        )])
        .expect("Luma peer");
        let peer = if expected_ownership == FileOwnership::Reused {
            InstalledAddon::new(
                fixture.game_id.clone(),
                AddonKind::Luma,
                fixture.addon.clone(),
            )
            .try_with_managed_files(vec![ManagedAddonFile::reused(
                fixture.to.clone(),
                installed.clone(),
            )])
            .expect("Reused Luma peer")
        } else {
            peer
        };
        persist_peer(&fixture, &peer);
        let plan = plan_peer_host_transition(
            PeerTopologyDirection::OutOfOptiTopology,
            &fixture.context,
            &fixture.game_id,
            &fixture.to,
            &fixture.from,
            None,
        )
        .expect("Luma reverse plan")
        .expect("Luma peer owner");
        assert_eq!(plan.destination_ownership(), expected_ownership);
        assert!(plan.receipt.after.managed_files().is_empty());
        if expected_ownership == FileOwnership::Owned {
            assert_eq!(
                plan.receipt.after.created_files().last(),
                Some(&fixture.from)
            );
        } else {
            assert!(
                plan.receipt.after.created_files().iter().all(|path| {
                    !crate::paths::same_path(Path::new(path.as_str()), destination)
                })
            );
            assert!(
                plan.receipt.after.backed_up_files().iter().all(|path| {
                    !crate::paths::same_path(Path::new(path.as_str()), destination)
                })
            );
        }
        assert_eq!(
            plan.receipt.after.backed_up_files().last(),
            expected_backed.then_some(&fixture.from)
        );
        if expected_backed {
            let sidecar = plan.sidecar.as_ref().expect("owned baseline sidecar");
            assert_eq!(
                sidecar.source,
                crate::fs::backup_path(source).expect("source sidecar")
            );
            assert_eq!(
                sidecar.destination,
                crate::fs::backup_path(destination).expect("destination sidecar")
            );
        } else {
            assert!(plan.sidecar.is_none());
        }
    }
}

#[test]
fn luma_reverse_transition_rejects_generic_created_source() {
    let fixture = peer_fixture();
    let source = Path::new(fixture.to.as_str());
    let destination = Path::new(fixture.from.as_str());
    std::fs::rename(destination, source).expect("move host to Opti downstream");
    let peer = InstalledAddon::new(
        fixture.game_id.clone(),
        AddonKind::Luma,
        fixture.addon.clone(),
    )
    .with_created_file(fixture.to.clone());
    persist_peer(&fixture, &peer);
    assert!(
        plan_peer_host_transition(
            PeerTopologyDirection::OutOfOptiTopology,
            &fixture.context,
            &fixture.game_id,
            &fixture.to,
            &fixture.from,
            None,
        )
        .is_err()
    );
}

#[test]
fn recognized_peer_relocation_transforms_managed_host_receipt() {
    let database = tempdir().expect("database root");
    let game_root = tempdir().expect("game root");
    let context = Context::open_at(database.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:peer-relocation-receipt").expect("game id");
    let game = GameInstallation::new(
        GameIdentity::new(game_id.clone(), "Peer relocation", Launcher::Manual).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(game_root.path().to_string_lossy().into_owned()).expect("game path"),
    );
    context.storage().upsert_game(&game).expect("game");

    let destination = game_root.path().join("ReShade64.dll");
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
    let source = game_root.path().join("d3d12.dll");
    std::fs::write(&source, &reshade).expect("source");
    let destination_ref =
        PathRef::new(destination.to_string_lossy().into_owned()).expect("destination path");
    let from_ref = PathRef::new(source.to_string_lossy().into_owned()).expect("source path");
    let addon_ref = PathRef::new(
        game_root
            .path()
            .join("renodx-peer.addon64")
            .to_string_lossy()
            .into_owned(),
    )
    .expect("addon path");
    let hash = crate::fs::sha256_of_non_empty_file(&source).expect("hash");
    let peer = InstalledAddon::new(game_id.clone(), AddonKind::RenoDx, addon_ref.clone())
        .try_with_managed_files(vec![ManagedAddonFile::owned(
            from_ref.clone(),
            ManagedFileBaseline::Absent,
            hash,
        )])
        .expect("peer receipt");
    context
        .storage()
        .upsert_installed_addon(&peer)
        .expect("peer receipt persisted");

    let moved = plan_peer_host_transition(
        PeerTopologyDirection::IntoOptiTopology,
        &context,
        &game_id,
        &from_ref,
        &destination_ref,
        None,
    )
    .expect("relocation")
    .expect("recognized peer owner");
    assert_eq!(moved.receipt.before.addon_file(), &addon_ref);
    assert_eq!(moved.receipt.after.addon_file(), &addon_ref);
    assert_eq!(moved.receipt.after.managed_files().len(), 1);
    assert_eq!(
        moved.receipt.after.managed_files()[0].path(),
        &destination_ref
    );
}
