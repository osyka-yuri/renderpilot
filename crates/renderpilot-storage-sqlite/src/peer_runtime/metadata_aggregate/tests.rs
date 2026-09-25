use super::{
    MetadataAggregatePreparation, MetadataAggregateTransition,
    PreparedMetadataAggregateCommitPermit,
};
use crate::peer_runtime::{
    AggregateBefore, GameAggregateMutation, PeerStorageRuntime, PlannedAggregateAfter,
};
use crate::repositories::peer_aggregate_reservations::{
    PeerAggregateKind, PeerAggregateReservationState, read_within_transaction,
};
use crate::{
    BeginFileMutationPreparation, BeginSharedVulkanMutation, SharedVulkanMutationScope,
    SqliteStorage,
};
use renderpilot_application::{GameRepository, InstalledAddonRepository, ProxyTopologyRepository};
use renderpilot_domain::{
    AddonKind, FileReceipt, GameId, GameIdentity, GameInstallation, GameProxyTopology, GameRuntime,
    InstalledAddon, Launcher, ManagedAddonFile, PathRef, PeerFileImage, PeerReadGuardEvidence,
    Platform, ProxyImplementation, ProxyLink, ProxyRootPrestate, Sha256Hash,
};

fn game_id(value: &str) -> GameId {
    GameId::new(format!("test:metadata-aggregate-{value}")).expect("game id")
}

fn path(value: &str) -> PathRef {
    PathRef::new(value).expect("path")
}

fn hash(byte: char) -> Sha256Hash {
    Sha256Hash::new(byte.to_string().repeat(64)).expect("hash")
}

fn seed_game(storage: &SqliteStorage, game_id: &GameId) {
    storage
        .upsert_game(&GameInstallation::new(
            GameIdentity::new(game_id.clone(), "Metadata aggregate", Launcher::Steam)
                .expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            path("C:/game"),
        ))
        .expect("game");
}

fn topology(game_id: &GameId) -> GameProxyTopology {
    let root_slot = path("C:/game/dxgi.dll");
    GameProxyTopology {
        id: "topology:metadata-aggregate".to_owned(),
        game_id: game_id.clone(),
        root_slot: root_slot.clone(),
        outer: ProxyLink {
            implementation: ProxyImplementation::OptiScaler,
            path: root_slot,
            receipt: FileReceipt::owned("outer", hash('a')).expect("receipt"),
        },
        downstream: None,
        downstream_origin: None,
        root_prestate: ProxyRootPrestate::Absent,
    }
}

fn mutation(
    operation_id: &str,
    before: AggregateBefore,
    after_peer: InstalledAddon,
    topology: Option<GameProxyTopology>,
) -> GameAggregateMutation {
    let after = PlannedAggregateAfter::new(
        before.game_id().clone(),
        before.state().cloned(),
        topology.map(renderpilot_domain::PlannedGameProxyTopology::Exact),
        Some(after_peer),
    )
    .expect("planned after");
    GameAggregateMutation::new(operation_id, before, after).expect("mutation")
}

fn read_reservation(
    storage: &SqliteStorage,
    game_id: &GameId,
    operation_id: &str,
) -> Option<crate::repositories::peer_aggregate_reservations::PeerAggregateReservation> {
    storage
        .with_transaction(|transaction| read_within_transaction(transaction, game_id, operation_id))
        .expect("reservation read")
}

fn revision(storage: &SqliteStorage, game_id: &GameId) -> i64 {
    storage
        .with_connection(|connection| {
            connection
                .query_row(
                    "SELECT peer_aggregate_revision FROM games WHERE id = ?1",
                    [game_id.as_str()],
                    |row| row.get(0),
                )
                .map_err(crate::error::storage_error)
        })
        .expect("revision")
}

fn refresh_fixture(
    suffix: &str,
    operation_id: &str,
) -> (
    PeerStorageRuntime,
    GameId,
    InstalledAddon,
    InstalledAddon,
    PreparedMetadataAggregateCommitPermit,
) {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = game_id(suffix);
    seed_game(&storage, &game_id);
    let before = InstalledAddon::new(
        game_id.clone(),
        AddonKind::Luma,
        path("C:/game/Luma.addon64"),
    );
    storage.upsert_installed_addon(&before).expect("peer");
    let before = storage
        .get_installed_addon(&game_id)
        .expect("load peer")
        .expect("peer exists");
    let after = before.clone().with_addon_version("new");
    let aggregate_before =
        AggregateBefore::new(game_id.clone(), None, None, Some(before.clone())).expect("before");
    let mutation = mutation(operation_id, aggregate_before, after.clone(), None);
    let runtime = PeerStorageRuntime::new(storage);
    let permit = runtime
        .prepare_metadata_aggregate(MetadataAggregatePreparation::new(
            mutation,
            MetadataAggregateTransition::PeerMetadataRefresh,
        ))
        .expect("prepare");
    (runtime, game_id, before, after, permit)
}

type MembershipFixture = (
    PeerStorageRuntime,
    GameId,
    InstalledAddon,
    GameProxyTopology,
    PeerReadGuardEvidence,
    Vec<String>,
    PreparedMetadataAggregateCommitPermit,
);

#[derive(Clone, Copy)]
enum MembershipGuardCase {
    Correct,
    Missing,
    Extra,
    Duplicate,
    Wrong,
}

fn membership_fixture(
    suffix: &str,
    operation_id: &str,
    guard_case: MembershipGuardCase,
) -> renderpilot_application::AppResult<MembershipFixture> {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = game_id(suffix);
    seed_game(&storage, &game_id);
    let managed_path = path("C:/game/nvngx_dlss.dll");
    let before = InstalledAddon::new(
        game_id.clone(),
        AddonKind::Luma,
        path("C:/game/Luma.addon64"),
    )
    .try_with_managed_files(vec![ManagedAddonFile::reused(
        managed_path.clone(),
        hash('d'),
    )])
    .expect("peer");
    let after = InstalledAddon::new(
        game_id.clone(),
        AddonKind::Luma,
        path("C:/game/Luma.addon64"),
    );
    let topology = topology(&game_id);
    storage.upsert_installed_addon(&before).expect("peer");
    storage
        .with_transaction(|transaction| {
            crate::repositories::proxy_topologies::upsert_within_transaction(transaction, &topology)
        })
        .expect("topology");
    let before = storage
        .get_installed_addon(&game_id)
        .expect("load peer")
        .expect("peer exists");
    let aggregate_before = AggregateBefore::new(
        game_id.clone(),
        None,
        Some(topology.clone()),
        Some(before.clone()),
    )
    .expect("before");
    let mutation = mutation(
        operation_id,
        aggregate_before,
        after,
        Some(topology.clone()),
    );
    let guard = PeerReadGuardEvidence::new(
        managed_path,
        Some(PeerFileImage::new("live", hash('d'), 4).expect("image")),
    );
    let initial_read_guards = match guard_case {
        MembershipGuardCase::Correct => vec![guard.clone()],
        MembershipGuardCase::Missing => Vec::new(),
        MembershipGuardCase::Extra | MembershipGuardCase::Duplicate => {
            vec![guard.clone(), guard.clone()]
        }
        MembershipGuardCase::Wrong => vec![PeerReadGuardEvidence::new(
            path("C:/game/other.dll"),
            Some(PeerFileImage::new("wrong", hash('c'), 5).expect("image")),
        )],
    };
    let roots = vec!["C:/game".to_owned()];
    let runtime = PeerStorageRuntime::new(storage);
    let permit = runtime.prepare_metadata_aggregate(MetadataAggregatePreparation::new(
        mutation,
        MetadataAggregateTransition::ReusedClaimMembership {
            canonical_game_root: "C:/game",
            sealed_roots: &roots,
            initial_read_guards: &initial_read_guards,
        },
    ))?;
    Ok((runtime, game_id, before, topology, guard, roots, permit))
}

fn persistent_prepared_fixture(
    label: &str,
    operation_id: &str,
) -> (
    std::path::PathBuf,
    PeerStorageRuntime,
    PeerStorageRuntime,
    GameId,
    PreparedMetadataAggregateCommitPermit,
) {
    let db_path = std::env::temp_dir().join(format!(
        "renderpilot-metadata-aggregate-{label}-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let storage = SqliteStorage::open(&db_path).expect("storage");
    let game_id = game_id(label);
    seed_game(&storage, &game_id);
    let before_peer = InstalledAddon::new(
        game_id.clone(),
        AddonKind::Luma,
        path("C:/game/Luma.addon64"),
    );
    storage.upsert_installed_addon(&before_peer).expect("peer");
    let before_peer = storage
        .get_installed_addon(&game_id)
        .expect("load peer")
        .expect("peer");
    let aggregate_before =
        AggregateBefore::new(game_id.clone(), None, None, Some(before_peer)).expect("before");
    let mutation = mutation(
        operation_id,
        aggregate_before,
        InstalledAddon::new(
            game_id.clone(),
            AddonKind::Luma,
            path("C:/game/Luma.addon64"),
        )
        .with_addon_version("new"),
        None,
    );
    let runtime = PeerStorageRuntime::new(storage);
    let permit = runtime
        .prepare_metadata_aggregate(MetadataAggregatePreparation::new(
            mutation,
            MetadataAggregateTransition::PeerMetadataRefresh,
        ))
        .expect("prepare");
    let other = PeerStorageRuntime::new(SqliteStorage::open(&db_path).expect("other storage"));
    (db_path, runtime, other, game_id, permit)
}

#[test]
fn refresh_reserves_commits_and_cleans_up_one_scalar_generation() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = game_id("refresh");
    seed_game(&storage, &game_id);
    let before_peer = InstalledAddon::new(
        game_id.clone(),
        AddonKind::Luma,
        path("C:/game/Luma.addon64"),
    );
    let after_peer = before_peer.clone().with_addon_version("new");
    storage.upsert_installed_addon(&before_peer).expect("peer");
    let before_peer = storage
        .get_installed_addon(&game_id)
        .expect("load peer")
        .expect("peer exists");
    let before = AggregateBefore::new(game_id.clone(), None, None, Some(before_peer.clone()))
        .expect("before");
    let mutation = mutation("refresh-operation", before, after_peer, None);
    let runtime = PeerStorageRuntime::new(storage);

    let permit = runtime
        .prepare_metadata_aggregate(MetadataAggregatePreparation::new(
            mutation,
            MetadataAggregateTransition::PeerMetadataRefresh,
        ))
        .expect("prepare");
    let prepared = read_reservation(runtime.repositories(), &game_id, "refresh-operation")
        .expect("prepared row");
    assert_eq!(prepared.kind(), PeerAggregateKind::Metadata);
    assert_eq!(prepared.state(), PeerAggregateReservationState::Prepared);
    assert_eq!(prepared.expected_revision().as_i64(), 0);

    let committed = runtime
        .commit_metadata_aggregate(permit, Vec::new())
        .expect("commit");
    assert_eq!(committed.aggregate().generation().as_i64(), 1);
    assert_eq!(
        runtime
            .repositories()
            .get_installed_addon(&game_id)
            .expect("load committed peer")
            .expect("peer")
            .addon_version(),
        Some("new")
    );
    assert_eq!(
        read_reservation(runtime.repositories(), &game_id, "refresh-operation")
            .expect("committed row")
            .state(),
        PeerAggregateReservationState::Committed
    );
    assert!(
        runtime
            .repositories()
            .upsert_installed_addon(&before_peer)
            .is_err()
    );
    assert!(
        runtime
            .repositories()
            .pending_file_mutations_for_game(&game_id)
            .expect("pending")
            .is_empty()
    );
    assert!(runtime.cleanup_metadata_aggregate(committed).is_ok());
    assert!(read_reservation(runtime.repositories(), &game_id, "refresh-operation").is_none());
}

#[test]
fn membership_route_uses_exact_guards_and_writes_no_topology() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = game_id("membership");
    seed_game(&storage, &game_id);
    let managed_path = path("C:/game/nvngx_dlss.dll");
    let before_peer = InstalledAddon::new(
        game_id.clone(),
        AddonKind::Luma,
        path("C:/game/Luma.addon64"),
    )
    .try_with_managed_files(vec![ManagedAddonFile::reused(
        managed_path.clone(),
        hash('d'),
    )])
    .expect("peer");
    let after_peer = InstalledAddon::new(
        game_id.clone(),
        AddonKind::Luma,
        path("C:/game/Luma.addon64"),
    );
    let topology = topology(&game_id);
    storage.upsert_installed_addon(&before_peer).expect("peer");
    storage
        .with_transaction(|transaction| {
            crate::repositories::proxy_topologies::upsert_within_transaction(transaction, &topology)
        })
        .expect("topology");
    let before_peer = storage
        .get_installed_addon(&game_id)
        .expect("load peer")
        .expect("peer exists");
    let before = AggregateBefore::new(
        game_id.clone(),
        None,
        Some(topology.clone()),
        Some(before_peer),
    )
    .expect("before");
    let mutation = mutation(
        "membership-operation",
        before,
        after_peer,
        Some(topology.clone()),
    );
    let guard = PeerReadGuardEvidence::new(
        managed_path,
        Some(PeerFileImage::new("live", hash('d'), 4).expect("image")),
    );
    let roots = vec!["C:/game".to_owned()];
    let runtime = PeerStorageRuntime::new(storage);
    let permit = runtime
        .prepare_metadata_aggregate(MetadataAggregatePreparation::new(
            mutation,
            MetadataAggregateTransition::ReusedClaimMembership {
                canonical_game_root: "C:/game",
                sealed_roots: &roots,
                initial_read_guards: std::slice::from_ref(&guard),
            },
        ))
        .expect("prepare membership");
    let committed = runtime
        .commit_metadata_aggregate(permit, vec![guard])
        .expect("commit membership");
    assert_eq!(
        runtime
            .repositories()
            .get_proxy_topology(&game_id)
            .expect("topology"),
        Some(topology)
    );
    assert!(runtime.cleanup_metadata_aggregate(committed).is_ok());
}

#[test]
fn observed_downstream_and_nonempty_refresh_guards_fail_before_reservation() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = game_id("reject");
    seed_game(&storage, &game_id);
    let before_peer = InstalledAddon::new(
        game_id.clone(),
        AddonKind::Luma,
        path("C:/game/Luma.addon64"),
    );
    storage.upsert_installed_addon(&before_peer).expect("peer");
    let before = AggregateBefore::new(game_id.clone(), None, None, Some(before_peer.clone()))
        .expect("before");
    let planned = PlannedAggregateAfter::new(
        game_id.clone(),
        None,
        Some(
            renderpilot_domain::PlannedGameProxyTopology::ObservedOwnedDownstream {
                id: "observed".to_owned(),
                game_id: game_id.clone(),
                root_slot: path("C:/game/dxgi.dll"),
                outer: ProxyLink {
                    implementation: ProxyImplementation::OptiScaler,
                    path: path("C:/game/dxgi.dll"),
                    receipt: FileReceipt::owned("outer", hash('a')).expect("receipt"),
                },
                implementation: ProxyImplementation::ReShade,
                downstream_path: path("C:/game/d3d11.dll"),
                downstream_origin: path("C:/game/dxgi.dll"),
                root_prestate: ProxyRootPrestate::Absent,
                planned_sha256: hash('b'),
                planned_length: 4,
            },
        ),
        Some(before_peer.with_addon_version("new")),
    )
    .expect("planned observed");
    let mutation =
        GameAggregateMutation::new("reject-operation", before, planned).expect("mutation");
    let runtime = PeerStorageRuntime::new(storage);
    assert!(
        runtime
            .prepare_metadata_aggregate(MetadataAggregatePreparation::new(
                mutation,
                MetadataAggregateTransition::PeerMetadataRefresh,
            ))
            .is_err()
    );
    assert!(read_reservation(runtime.repositories(), &game_id, "reject-operation").is_none());

    let (runtime, game_id, _before, _after, permit) =
        refresh_fixture("refresh-final-guard", "refresh-final-guard-operation");
    let unexpected_guard = PeerReadGuardEvidence::new(path("C:/game/unexpected.dll"), None);
    assert!(
        runtime
            .commit_metadata_aggregate(permit, vec![unexpected_guard])
            .is_err()
    );
    assert_eq!(revision(runtime.repositories(), &game_id), 0);
    assert_eq!(
        read_reservation(
            runtime.repositories(),
            &game_id,
            "refresh-final-guard-operation",
        )
        .expect("prepared")
        .state(),
        PeerAggregateReservationState::Prepared
    );
}

#[test]
fn invalid_routes_and_membership_guard_shapes_never_open_a_reservation() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = game_id("noop");
    seed_game(&storage, &game_id);
    let before_peer = InstalledAddon::new(
        game_id.clone(),
        AddonKind::Luma,
        path("C:/game/Luma.addon64"),
    );
    storage.upsert_installed_addon(&before_peer).expect("peer");
    let before_peer = storage
        .get_installed_addon(&game_id)
        .expect("load peer")
        .expect("peer");
    let before = AggregateBefore::new(game_id.clone(), None, None, Some(before_peer.clone()))
        .expect("before");
    let noop = mutation("noop-operation", before.clone(), before_peer.clone(), None);
    let runtime = PeerStorageRuntime::new(storage);
    assert!(
        runtime
            .prepare_metadata_aggregate(MetadataAggregatePreparation::new(
                noop,
                MetadataAggregateTransition::PeerMetadataRefresh,
            ))
            .is_err()
    );
    assert!(read_reservation(runtime.repositories(), &game_id, "noop-operation").is_none());

    let physical = mutation(
        "physical-through-membership",
        before,
        before_peer.with_addon_version("new"),
        None,
    );
    assert!(
        runtime
            .prepare_metadata_aggregate(MetadataAggregatePreparation::new(
                physical,
                MetadataAggregateTransition::ReusedClaimMembership {
                    canonical_game_root: "C:/game",
                    sealed_roots: &["C:/game".to_owned()],
                    initial_read_guards: &[],
                },
            ))
            .is_err()
    );
    assert!(
        read_reservation(
            runtime.repositories(),
            &game_id,
            "physical-through-membership"
        )
        .is_none()
    );

    for (suffix, case) in [
        ("missing-guard", MembershipGuardCase::Missing),
        ("extra-guard", MembershipGuardCase::Extra),
        ("duplicate-guard", MembershipGuardCase::Duplicate),
        ("wrong-guard", MembershipGuardCase::Wrong),
    ] {
        assert!(
            membership_fixture(suffix, &format!("{suffix}-operation"), case).is_err(),
            "{suffix} must be rejected before reservation"
        );
    }
}

#[test]
fn wrong_final_guard_and_participant_drift_leave_prepared_state_unchanged() {
    let (runtime, game_id, before, _, _, _roots, permit) = membership_fixture(
        "wrong-final",
        "wrong-final-operation",
        MembershipGuardCase::Correct,
    )
    .expect("fixture");
    let wrong_guard = PeerReadGuardEvidence::new(
        path("C:/game/other.dll"),
        Some(PeerFileImage::new("wrong", hash('c'), 5).expect("image")),
    );
    assert!(
        runtime
            .commit_metadata_aggregate(permit, vec![wrong_guard])
            .is_err()
    );
    assert_eq!(revision(runtime.repositories(), &game_id), 0);
    assert_eq!(
        runtime
            .repositories()
            .get_installed_addon(&game_id)
            .expect("peer")
            .expect("peer exists"),
        before
    );
    assert_eq!(
        read_reservation(runtime.repositories(), &game_id, "wrong-final-operation")
            .expect("prepared")
            .state(),
        PeerAggregateReservationState::Prepared
    );

    for (suffix, drift_peer, drift_topology) in
        [("peer-drift", true, false), ("topology-drift", false, true)]
    {
        let (runtime, game_id, before, topology, guard, _roots, permit) = membership_fixture(
            suffix,
            &format!("{suffix}-operation"),
            MembershipGuardCase::Correct,
        )
        .expect("fixture");
        if drift_peer {
            let changed = before.clone().with_addon_version("drift");
            runtime
                .repositories()
                .with_transaction(|transaction| {
                    crate::repositories::installed_addons::upsert_within_transaction(
                        transaction,
                        &changed,
                    )
                })
                .expect("peer drift");
        }
        if drift_topology {
            let mut changed = topology.clone();
            changed.id.push_str(":drift");
            runtime
                .repositories()
                .with_transaction(|transaction| {
                    crate::repositories::proxy_topologies::upsert_within_transaction(
                        transaction,
                        &changed,
                    )
                })
                .expect("topology drift");
        }
        assert!(
            runtime
                .commit_metadata_aggregate(permit, vec![guard])
                .is_err()
        );
        assert_eq!(revision(runtime.repositories(), &game_id), 0);
        assert_eq!(
            read_reservation(
                runtime.repositories(),
                &game_id,
                &format!("{suffix}-operation"),
            )
            .expect("prepared")
            .state(),
            PeerAggregateReservationState::Prepared
        );
    }
}

#[test]
fn revision_drift_happens_before_peer_write_and_ordinary_first_fences_prepare() {
    let (runtime, game_id, before, _, permit) =
        refresh_fixture("revision-drift", "revision-drift-operation");
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE games SET peer_aggregate_revision = 7 WHERE id = ?1",
                    [game_id.as_str()],
                )
                .map_err(crate::error::storage_error)?;
            Ok(())
        })
        .expect("revision drift");
    assert!(
        runtime
            .commit_metadata_aggregate(permit, Vec::new())
            .is_err()
    );
    assert_eq!(revision(runtime.repositories(), &game_id), 7);
    assert_eq!(
        runtime
            .repositories()
            .get_installed_addon(&game_id)
            .expect("peer")
            .expect("peer exists"),
        before
    );
    assert!(
        read_reservation(runtime.repositories(), &game_id, "revision-drift-operation").is_some()
    );

    for shared in [false, true] {
        let storage = SqliteStorage::in_memory().expect("storage");
        let blocked_game_id = self::game_id(if shared { "shared-first" } else { "file-first" });
        seed_game(&storage, &blocked_game_id);
        let peer = InstalledAddon::new(
            blocked_game_id.clone(),
            AddonKind::Luma,
            path("C:/game/Luma.addon64"),
        );
        storage.upsert_installed_addon(&peer).expect("peer");
        if shared {
            storage
                .try_begin_shared_vulkan_mutation(&BeginSharedVulkanMutation {
                    id: "ordinary-shared".to_owned(),
                    scope: SharedVulkanMutationScope::GameShared,
                    game_id: Some(blocked_game_id.clone()),
                    feature: "metadata-test".to_owned(),
                    initial_manifest_json: "{}".to_owned(),
                    root_capabilities_json: "{}".to_owned(),
                })
                .expect("shared begin");
        } else {
            storage
                .begin_file_mutation_preparation(&BeginFileMutationPreparation {
                    id: "ordinary-file".to_owned(),
                    game_id: blocked_game_id.clone(),
                    feature: "metadata-test".to_owned(),
                    subject_id: None,
                    initial_manifest_json: "{}".to_owned(),
                })
                .expect("file begin");
        }
        let before = AggregateBefore::new(blocked_game_id.clone(), None, None, Some(peer.clone()))
            .expect("before");
        let aggregate = mutation(
            if shared {
                "shared-blocked"
            } else {
                "file-blocked"
            },
            before,
            peer.with_addon_version("new"),
            None,
        );
        let runtime = PeerStorageRuntime::new(storage);
        assert!(
            runtime
                .prepare_metadata_aggregate(MetadataAggregatePreparation::new(
                    aggregate,
                    MetadataAggregateTransition::PeerMetadataRefresh,
                ))
                .is_err()
        );
        assert!(
            read_reservation(
                runtime.repositories(),
                &blocked_game_id,
                if shared {
                    "shared-blocked"
                } else {
                    "file-blocked"
                },
            )
            .is_none()
        );
    }
}

#[test]
fn runtime_identity_binds_commit_and_cleanup_to_the_minting_runtime() {
    let (db_path, runtime, other, game_id, permit) =
        persistent_prepared_fixture("cross-commit", "cross-commit-operation");
    assert!(other.commit_metadata_aggregate(permit, Vec::new()).is_err());
    assert_eq!(revision(runtime.repositories(), &game_id), 0);
    assert_eq!(
        read_reservation(runtime.repositories(), &game_id, "cross-commit-operation")
            .expect("prepared")
            .state(),
        PeerAggregateReservationState::Prepared
    );
    drop(other);
    drop(runtime);
    let _ = std::fs::remove_file(db_path);

    let (db_path, runtime, other, game_id, permit) =
        persistent_prepared_fixture("cross-cleanup", "cross-cleanup-operation");
    let committed = runtime
        .commit_metadata_aggregate(permit, Vec::new())
        .expect("commit");
    assert!(other.cleanup_metadata_aggregate(committed).is_err());
    assert_eq!(revision(runtime.repositories(), &game_id), 1);
    assert_eq!(
        read_reservation(runtime.repositories(), &game_id, "cross-cleanup-operation")
            .expect("committed")
            .state(),
        PeerAggregateReservationState::Committed
    );
    drop(other);
    drop(runtime);
    let _ = std::fs::remove_file(db_path);
}

#[test]
fn cleanup_revision_drift_preserves_the_committed_fence() {
    let (runtime, game_id, _before, _after, permit) =
        refresh_fixture("cleanup-drift", "cleanup-drift-operation");
    let committed = runtime
        .commit_metadata_aggregate(permit, Vec::new())
        .expect("commit");
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE games SET peer_aggregate_revision = 9 WHERE id = ?1",
                    [game_id.as_str()],
                )
                .map_err(crate::error::storage_error)?;
            Ok(())
        })
        .expect("revision drift");
    assert!(runtime.cleanup_metadata_aggregate(committed).is_err());
    assert_eq!(revision(runtime.repositories(), &game_id), 9);
    assert_eq!(
        read_reservation(runtime.repositories(), &game_id, "cleanup-drift-operation")
            .expect("committed")
            .state(),
        PeerAggregateReservationState::Committed
    );
}
