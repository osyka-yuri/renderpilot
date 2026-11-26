use renderpilot_application::{GameRepository, InstalledAddonRepository};
use renderpilot_domain::mutation_features::RENODX_DLSS_FIX_INSTALL;
use renderpilot_domain::{
    AddonKind, FileReceipt, GameId, GameIdentity, GameInstallation, GameProxyTopology, GameRuntime,
    InstalledAddon, Launcher, PathRef, PeerReadGuardEvidence, PeerReadGuardExpectation,
    PeerTransitionContext, Platform, ProxyImplementation, ProxyLink, ProxyPeerRoute,
    ProxyRootPrestate, RenoDxDlssBeforeImage, RenoDxDlssClaim, RenoDxDlssProjection, Sha256Hash,
    TrackedSource, TrackedSourceRole, required_read_guards_with_renodx_reshade_ini_and_dlss,
};

use super::permit::{PeerCommitPreparation, PeerStorageRuntime};
use crate::{BeginFileMutationPreparation, PendingFileMutationState, SqliteStorage};

const ROOT: &str = "C:/game";
const ADDON: &str = "C:/game/RenoDx.addon64";
const COMPANION: &str = "C:/game/nvngx_dlss.dll";
const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const ABC_DIGEST: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn path(value: &str) -> PathRef {
    PathRef::new(value).expect("path")
}

fn source() -> TrackedSource {
    TrackedSource::new(
        TrackedSourceRole::DlssFix,
        "https://example.test/renodx-dlss-fix",
        None,
        "dlss-source",
    )
}

fn peers(game_id: &GameId) -> (InstalledAddon, InstalledAddon, TrackedSource) {
    let source = source();
    let before = InstalledAddon::new(game_id.clone(), AddonKind::RenoDx, path(ADDON));
    let after = before
        .clone()
        .with_created_file(path(COMPANION))
        .with_tracked_sources(vec![source.clone()]);
    (before, after, source)
}

fn projection(source: TrackedSource, companion_path: &str) -> RenoDxDlssProjection {
    RenoDxDlssProjection::new(
        path(companion_path),
        RenoDxDlssBeforeImage::absent(),
        RenoDxDlssClaim::absent(),
        RenoDxDlssClaim::new(true, Some(source)).expect("claim"),
    )
}

fn claim_refresh_projection(source: TrackedSource, companion_path: &str) -> RenoDxDlssProjection {
    RenoDxDlssProjection::new(
        path(companion_path),
        RenoDxDlssBeforeImage::present(
            "native-dlss",
            Sha256Hash::new(ABC_DIGEST).expect("digest"),
            3,
            b"abc".to_vec(),
        )
        .expect("before image"),
        RenoDxDlssClaim::new(true, None).expect("before claim"),
        RenoDxDlssClaim::new(true, Some(source)).expect("after claim"),
    )
}

fn empty_manifest(id: &str) -> String {
    serde_json::json!({
        "format_version": 1,
        "roots": [ROOT],
        "snapshots": [],
        "peer_program": {
            "format": 1,
            "transaction_owner": id,
            "execution_class": "ordinary",
            "roots": [ROOT],
            "stage": [],
            "custody": [],
            "created_ancestors": [],
            "endpoints": []
        }
    })
    .to_string()
}

fn nonempty_manifest(id: &str) -> String {
    serde_json::json!({
        "format_version": 1,
        "roots": [ROOT],
        "snapshots": [{
            "path": COMPANION,
            "snapshot": null
        }],
        "peer_program": {
            "format": 1,
            "transaction_owner": id,
            "execution_class": "ordinary",
            "roots": [ROOT],
            "stage": [],
            "custody": [],
            "created_ancestors": [],
            "endpoints": [{
                "ordinal": 0,
                "path": COMPANION,
                "role": "disjoint",
                "operation": "create",
                "planned_sha256": DIGEST,
                "planned_length": 3,
                "before": null,
                "read_guards": ["C:/game:nvngx_dlss.dll"],
                "subtree_publishes": []
            }]
        }
    })
    .to_string()
}

fn foreign_replace_manifest(id: &str) -> String {
    serde_json::json!({
        "format_version": 1,
        "roots": [ROOT],
        "snapshots": [{
            "path": COMPANION,
            "snapshot": "C:/transaction/foreign-dlss.snapshot"
        }],
        "peer_program": {
            "format": 1,
            "transaction_owner": id,
            "execution_class": "ordinary",
            "roots": [ROOT],
            "stage": [],
            "custody": ["C:/game:nvngx_dlss.dll"],
            "created_ancestors": [],
            "endpoints": [{
                "ordinal": 0,
                "path": COMPANION,
                "role": "disjoint",
                "operation": "replace",
                "planned_sha256": DIGEST,
                "planned_length": 3,
                "before": {
                    "identity": "native-dlss",
                    "sha256": ABC_DIGEST,
                    "length": 3
                },
                "read_guards": ["C:/game:nvngx_dlss.dll"],
                "subtree_publishes": []
            }]
        }
    })
    .to_string()
}

fn later_companion_manifest(id: &str) -> String {
    serde_json::json!({
        "format_version": 1,
        "roots": [ROOT],
        "snapshots": [
            {"path": "C:/game/other.dll", "snapshot": null},
            {"path": COMPANION, "snapshot": null}
        ],
        "peer_program": {
            "format": 1,
            "transaction_owner": id,
            "execution_class": "ordinary",
            "roots": [ROOT],
            "stage": [],
            "custody": [],
            "created_ancestors": [],
            "endpoints": [
                {
                    "ordinal": 0,
                    "path": "C:/game/other.dll",
                    "role": "disjoint",
                    "operation": "create",
                    "planned_sha256": DIGEST,
                    "planned_length": 3,
                    "before": null,
                    "read_guards": ["C:/game:other.dll"],
                    "subtree_publishes": []
                },
                {
                    "ordinal": 1,
                    "path": COMPANION,
                    "role": "disjoint",
                    "operation": "create",
                    "planned_sha256": DIGEST,
                    "planned_length": 3,
                    "before": null,
                    "read_guards": ["C:/game:nvngx_dlss.dll"],
                    "subtree_publishes": []
                }
            ]
        }
    })
    .to_string()
}

fn recovery_empty_manifest(id: &str) -> String {
    let mut value: serde_json::Value =
        serde_json::from_str(&empty_manifest(id)).expect("empty manifest");
    value["transaction_dir"] = serde_json::json!(format!("C:/transaction/{id}"));
    value.to_string()
}

fn begin(storage: &SqliteStorage, id: &str, game_id: &GameId) {
    storage
        .begin_file_mutation_preparation(&BeginFileMutationPreparation {
            id: id.to_owned(),
            game_id: game_id.clone(),
            feature: RENODX_DLSS_FIX_INSTALL.to_owned(),
            subject_id: None,
            initial_manifest_json: "{}".to_owned(),
        })
        .expect("begin preparation");
}

fn guard_evidence(
    before_peer: Option<&InstalledAddon>,
    after_peer: Option<&InstalledAddon>,
    projection: &RenoDxDlssProjection,
) -> Vec<PeerReadGuardEvidence> {
    let requirements = required_read_guards_with_renodx_reshade_ini_and_dlss(
        PeerTransitionContext::new(
            before_peer,
            after_peer,
            None,
            None,
            ProxyPeerRoute::DurableDisjoint,
        ),
        &[],
        None,
        projection,
    )
    .expect("guard requirements");
    requirements
        .iter()
        .map(|requirement| {
            let observed = match requirement.expectation() {
                PeerReadGuardExpectation::Absent => None,
                PeerReadGuardExpectation::Digest { sha256 }
                | PeerReadGuardExpectation::Receipt { sha256, .. } => Some(
                    renderpilot_domain::PeerFileImage::new(
                        requirement
                            .expectation()
                            .identity()
                            .unwrap_or("dlss-companion"),
                        sha256.clone(),
                        projection.before_image().length().unwrap_or(3),
                    )
                    .expect("guard image"),
                ),
            };
            PeerReadGuardEvidence::new(requirement.path().clone(), observed)
        })
        .collect()
}

struct DlssPreparation<'a> {
    runtime: &'a PeerStorageRuntime,
    id: &'a str,
    game_id: &'a GameId,
    before_peer: Option<&'a InstalledAddon>,
    after_peer: Option<&'a InstalledAddon>,
    manifest: &'a str,
    projection: RenoDxDlssProjection,
    initial_read_guards: &'a [PeerReadGuardEvidence],
}

fn preparation(
    input: DlssPreparation<'_>,
) -> renderpilot_application::AppResult<super::permit::PreparedPeerCommitPermit> {
    input.runtime.finish_file_peer_preparation_with_renodx_dlss(
        PeerCommitPreparation {
            mutation_id: input.id,
            game_id: input.game_id,
            feature: RENODX_DLSS_FIX_INSTALL,
            subject_id: None,
            manifest_json: input.manifest,
            canonical_game_root: ROOT,
            initial_read_guards: input.initial_read_guards,
            before_peer: input.before_peer,
            after_peer: input.after_peer,
            before_topology: None,
            planned_after_topology: None,
            route: ProxyPeerRoute::DurableDisjoint,
            component_set: None,
            baseline_mutations: &[],
            catalog_claim: None,
            renodx_reshade_ini: None,
        },
        input.projection,
    )
}

#[test]
fn specialized_empty_program_prepares_and_commits_claim_only_projection() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = GameId::new("game:renodx-dlss-specialized").expect("game id");
    let id = "renodx-dlss-specialized";
    begin(&storage, id, &game_id);
    let runtime = PeerStorageRuntime::new(storage);
    let (before_seed, _, source) = peers(&game_id);
    runtime
        .repositories()
        .upsert_installed_addon(&before_seed.with_created_file(path(COMPANION)))
        .expect("seed peer");
    let before = runtime
        .repositories()
        .get_installed_addon(&game_id)
        .expect("seeded peer")
        .expect("seeded peer exists");
    let after = before.clone().with_tracked_sources(vec![source.clone()]);
    let projection = claim_refresh_projection(source, COMPANION);
    let manifest = empty_manifest(id);
    let guards = guard_evidence(Some(&before), Some(&after), &projection);
    let permit = preparation(DlssPreparation {
        runtime: &runtime,
        id,
        game_id: &game_id,
        before_peer: Some(&before),
        after_peer: Some(&after),
        manifest: &manifest,
        projection,
        initial_read_guards: &guards,
    })
    .expect("specialized preparation");
    assert!(permit.contract().intents().is_empty());
    assert!(permit.contract().renodx_dlss_projection().is_some());
    runtime
        .seal_and_commit_ordinary_peer(permit, Vec::new(), guards)
        .expect("claim-only commit has no endpoint evidence");
    assert_eq!(
        runtime
            .repositories()
            .get_pending_file_mutation(id)
            .expect("row")
            .expect("row exists")
            .state,
        PendingFileMutationState::Committed
    );
    let persisted = runtime
        .repositories()
        .get_installed_addon(&game_id)
        .expect("peer")
        .expect("peer exists");
    assert_eq!(persisted.game_id(), after.game_id());
    assert_eq!(persisted.kind(), after.kind());
    assert_eq!(persisted.addon_file(), after.addon_file());
    assert_eq!(persisted.created_files(), after.created_files());
    assert_eq!(persisted.tracked_sources(), after.tracked_sources());
}

#[test]
fn generic_empty_program_still_rejects_and_recovery_stays_strict() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = GameId::new("game:renodx-dlss-generic").expect("game id");
    let id = "renodx-dlss-generic";
    begin(&storage, id, &game_id);
    let runtime = PeerStorageRuntime::new(storage);
    let (before, after, _) = peers(&game_id);
    let manifest = empty_manifest(id);
    assert!(
        runtime
            .finish_file_peer_preparation(PeerCommitPreparation {
                mutation_id: id,
                game_id: &game_id,
                feature: RENODX_DLSS_FIX_INSTALL,
                subject_id: None,
                manifest_json: &manifest,
                canonical_game_root: ROOT,
                initial_read_guards: &[],
                before_peer: Some(&before),
                after_peer: Some(&after),
                before_topology: None,
                planned_after_topology: None,
                route: ProxyPeerRoute::DurableDisjoint,
                component_set: None,
                baseline_mutations: &[],
                catalog_claim: None,
                renodx_reshade_ini: None,
            })
            .is_err()
    );
    assert!(crate::validated_file_peer_recovery_program(id, &manifest).is_err());
    assert_eq!(
        runtime
            .repositories()
            .get_pending_file_mutation(id)
            .expect("row")
            .expect("row exists")
            .state,
        PendingFileMutationState::Preparing
    );
}

#[test]
fn typed_preflight_accepts_only_the_specialized_empty_or_physical_shape() {
    let game_id = GameId::new("game:renodx-dlss-preflight").expect("game id");
    let (_, _, initial_source) = peers(&game_id);
    let empty_projection = projection(initial_source, COMPANION);
    assert!(
        crate::validate_file_peer_program_manifest_with_renodx_dlss(
            RENODX_DLSS_FIX_INSTALL,
            ROOT,
            None,
            &empty_projection,
            &empty_manifest("preflight-empty"),
        )
        .is_ok()
    );
    assert!(
        crate::validate_file_peer_program_manifest(&empty_manifest("preflight-empty")).is_err()
    );

    let physical_projection = projection(source(), COMPANION);
    assert!(
        crate::validate_file_peer_program_manifest_with_renodx_dlss(
            RENODX_DLSS_FIX_INSTALL,
            ROOT,
            None,
            &physical_projection,
            &nonempty_manifest("preflight-physical"),
        )
        .is_ok(),
        "nonempty specialized manifests remain ordinary physical programs"
    );
}

#[test]
fn specialized_preflight_accepts_replacing_a_foreign_companion_before_claim_adoption() {
    let projection = RenoDxDlssProjection::new(
        path(COMPANION),
        RenoDxDlssBeforeImage::present(
            "native-dlss",
            Sha256Hash::new(ABC_DIGEST).expect("digest"),
            3,
            b"abc".to_vec(),
        )
        .expect("foreign preimage"),
        RenoDxDlssClaim::absent(),
        RenoDxDlssClaim::new(true, Some(source())).expect("adopted claim"),
    );
    let result = crate::validate_file_peer_program_manifest_with_renodx_dlss(
        RENODX_DLSS_FIX_INSTALL,
        ROOT,
        None,
        &projection,
        &foreign_replace_manifest("preflight-foreign-replace"),
    );
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn specialized_preflight_rejects_a_late_companion_endpoint() {
    let projection = projection(source(), COMPANION);
    assert!(
        crate::validate_file_peer_program_manifest_with_renodx_dlss(
            RENODX_DLSS_FIX_INSTALL,
            ROOT,
            None,
            &projection,
            &later_companion_manifest("preflight-late-companion"),
        )
        .is_err()
    );
}

#[test]
fn feature_specific_recovery_classifies_exact_empty_dlss_as_no_physical_program() {
    let manifest = recovery_empty_manifest("recovery-empty");
    assert!(crate::validated_file_peer_recovery_program("recovery-empty", &manifest).is_err());
    assert_eq!(
        crate::validated_file_peer_recovery_program_for_feature(
            "recovery-empty",
            RENODX_DLSS_FIX_INSTALL,
            &manifest,
        )
        .expect("feature-specific recovery validation"),
        None
    );
}

#[test]
fn specialized_present_preimage_binds_guard_length_and_declared_bytes() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = GameId::new("game:renodx-dlss-present-guard").expect("game id");
    let id = "renodx-dlss-present-guard";
    begin(&storage, id, &game_id);
    let runtime = PeerStorageRuntime::new(storage);
    let before_seed = InstalledAddon::new(game_id.clone(), AddonKind::RenoDx, path(ADDON));
    runtime
        .repositories()
        .upsert_installed_addon(&before_seed)
        .expect("seed peer");
    let before = runtime
        .repositories()
        .get_installed_addon(&game_id)
        .expect("seeded peer")
        .expect("seeded peer exists");
    let projection = RenoDxDlssProjection::new(
        path(COMPANION),
        RenoDxDlssBeforeImage::present(
            "native-dlss",
            Sha256Hash::new(ABC_DIGEST).expect("digest"),
            3,
            b"abc".to_vec(),
        )
        .expect("valid preimage"),
        RenoDxDlssClaim::absent(),
        RenoDxDlssClaim::absent(),
    );
    let guards = guard_evidence(Some(&before), Some(&before), &projection);
    let bad_guards = guards
        .iter()
        .map(|guard| {
            let observed = guard.observed().map(|image| {
                renderpilot_domain::PeerFileImage::new(
                    image.identity(),
                    image.sha256().clone(),
                    image.length() + 1,
                )
                .expect("guard image")
            });
            PeerReadGuardEvidence::new(guard.path().clone(), observed)
        })
        .collect::<Vec<_>>();
    assert!(
        preparation(DlssPreparation {
            runtime: &runtime,
            id,
            game_id: &game_id,
            before_peer: Some(&before),
            after_peer: Some(&before),
            manifest: &empty_manifest(id),
            projection,
            initial_read_guards: &bad_guards,
        })
        .is_err(),
        "guard length must be bound to the typed preimage"
    );
    assert!(
        RenoDxDlssBeforeImage::present(
            "native-dlss",
            Sha256Hash::new(ABC_DIGEST).expect("digest"),
            3,
            b"xyz".to_vec(),
        )
        .is_err(),
        "the declared preimage bytes must match their digest"
    );
}

#[test]
fn specialized_projection_rejects_path_claim_and_preimage_mismatch() {
    for (suffix, projection, initial_read_guards) in [
        (
            "path",
            projection(source(), "C:/game/other.dll"),
            Vec::new(),
        ),
        (
            "claim",
            RenoDxDlssProjection::new(
                path(COMPANION),
                RenoDxDlssBeforeImage::absent(),
                RenoDxDlssClaim::absent(),
                RenoDxDlssClaim::absent(),
            ),
            Vec::new(),
        ),
        (
            "preimage",
            RenoDxDlssProjection::new(
                path(COMPANION),
                RenoDxDlssBeforeImage::present(
                    "native-dlss",
                    Sha256Hash::new(ABC_DIGEST).expect("digest"),
                    3,
                    b"abc".to_vec(),
                )
                .expect("preimage"),
                RenoDxDlssClaim::absent(),
                RenoDxDlssClaim::new(true, Some(source())).expect("claim"),
            ),
            Vec::new(),
        ),
    ] {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game_id = GameId::new(format!("game:renodx-dlss-{suffix}")).expect("game id");
        let id = format!("renodx-dlss-{suffix}");
        begin(&storage, &id, &game_id);
        let runtime = PeerStorageRuntime::new(storage);
        let (before_seed, _, _) = peers(&game_id);
        runtime
            .repositories()
            .upsert_installed_addon(&before_seed)
            .expect("seed peer");
        let before = runtime
            .repositories()
            .get_installed_addon(&game_id)
            .expect("seeded peer")
            .expect("seeded peer exists");
        let after = before
            .clone()
            .with_created_file(path(COMPANION))
            .with_tracked_sources(vec![source()]);
        let before_for_case = if suffix == "preimage" {
            after.clone()
        } else {
            before
        };
        let after_for_case = after;
        assert!(
            preparation(DlssPreparation {
                runtime: &runtime,
                id: &id,
                game_id: &game_id,
                before_peer: Some(&before_for_case),
                after_peer: Some(&after_for_case),
                manifest: &empty_manifest(&id),
                projection,
                initial_read_guards: &initial_read_guards,
            })
            .is_err(),
            "{suffix} mismatch must be rejected"
        );
    }
}

fn topology(game_id: &GameId) -> GameProxyTopology {
    let root = path("C:/game/dxgi.dll");
    GameProxyTopology {
        id: "topology:renodx-dlss-drift".to_owned(),
        game_id: game_id.clone(),
        root_slot: root.clone(),
        outer: ProxyLink {
            implementation: ProxyImplementation::OptiScaler,
            path: root,
            receipt: FileReceipt::owned("outer", Sha256Hash::new(DIGEST).expect("digest"))
                .expect("receipt"),
        },
        downstream: None,
        downstream_origin: None,
        root_prestate: ProxyRootPrestate::Absent,
    }
}

#[test]
fn specialized_commit_rejects_peer_and_topology_drift() {
    for drift in ["peer", "topology"] {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game_id = GameId::new(format!("game:renodx-dlss-{drift}-drift")).expect("game id");
        let id = format!("renodx-dlss-{drift}-drift");
        begin(&storage, &id, &game_id);
        let runtime = PeerStorageRuntime::new(storage);
        let (before_seed, _, source) = peers(&game_id);
        runtime
            .repositories()
            .upsert_installed_addon(&before_seed.with_created_file(path(COMPANION)))
            .expect("seed peer");
        let before = runtime
            .repositories()
            .get_installed_addon(&game_id)
            .expect("seeded peer")
            .expect("seeded peer exists");
        let after = before.clone().with_tracked_sources(vec![source.clone()]);
        let projection = claim_refresh_projection(source, COMPANION);
        let guards = guard_evidence(Some(&before), Some(&after), &projection);
        let permit = preparation(DlssPreparation {
            runtime: &runtime,
            id: &id,
            game_id: &game_id,
            before_peer: Some(&before),
            after_peer: Some(&after),
            manifest: &empty_manifest(&id),
            projection,
            initial_read_guards: &guards,
        })
        .expect("specialized preparation");
        if drift == "peer" {
            runtime
                .repositories()
                .upsert_installed_addon(&before.with_addon_version("drift"))
                .expect("drift peer");
        } else {
            runtime
                .repositories()
                .upsert_game(&GameInstallation::new(
                    GameIdentity::new(game_id.clone(), "RenoDX DLSS drift", Launcher::Steam)
                        .expect("game identity"),
                    Platform::Windows,
                    GameRuntime::NativeWindows,
                    path(ROOT),
                ))
                .expect("seed game");
            runtime
                .repositories()
                .with_transaction(|transaction| {
                    transaction
                        .execute(
                            "INSERT INTO game_proxy_topologies
                             (id, game_id, topology_json, created_at, updated_at)
                             VALUES (?1, ?2, ?3, 1, 1)",
                            rusqlite::params![
                                topology(&game_id).id,
                                game_id.as_str(),
                                serde_json::to_string(&topology(&game_id)).expect("topology json"),
                            ],
                        )
                        .map_err(crate::error::storage_error)?;
                    Ok(())
                })
                .expect("drift topology");
        }
        assert!(
            runtime
                .seal_and_commit_ordinary_peer(permit, Vec::new(), guards)
                .is_err()
        );
        assert_eq!(
            runtime
                .repositories()
                .get_pending_file_mutation(&id)
                .expect("row")
                .expect("row exists")
                .state,
            PendingFileMutationState::Prepared
        );
    }
}
