use std::path::Path;

use renderpilot_domain::{
    AddonKind, FileReceipt, GameId, GameProxyTopology, PathRef, ProxyImplementation, ProxyLink,
    ProxyRootPrestate, RenoDxReshadeIniFeature, Sha256Hash, TrackedSource, TrackedSourceRole,
};

use crate::addons::peer_lifecycle::PeerRoots;
use crate::addons::renodx::peer::active_dlss::{
    ActiveDlssComposition, ActiveDlssEffect, ActiveDlssEndpointInput, ActiveDlssInput,
    compose_active_dlss,
};
use crate::addons::renodx::peer::{RenoDxConfigSourceSeal, RenoDxRootSeal};
use crate::peer_mutation_executor::{PeerPathSnapshot, observe_peer_path_snapshot};

fn game_id() -> GameId {
    GameId::new("manual:active-dlss-composer").expect("game id")
}

fn path(root: &Path, name: &str) -> PathRef {
    PathRef::new(root.join(name).to_string_lossy().into_owned()).expect("path")
}

fn root_seal(root: &Path) -> RenoDxRootSeal {
    let root = crate::paths::canonicalize_existing(root).expect("canonical root");
    let root_ref = PathRef::new(root.to_string_lossy().into_owned()).expect("root ref");
    RenoDxRootSeal {
        canonical_game_root: root.clone(),
        canonical_game_root_ref: root_ref,
        config_source: RenoDxConfigSourceSeal::Absent {
            exact_ini_path: root.join("ReShade.ini"),
        },
        effective_addon_root: root.clone(),
        payload_root: None,
        payload_root_ref: None,
        exact_ini_path: root.join("ReShade.ini"),
        exact_proxy_host: None,
        canonical_registered_exe: None,
        roots: PeerRoots::new(root, None).expect("roots"),
    }
}

fn external_root_seal(game_root: &Path, payload_root: &Path) -> RenoDxRootSeal {
    let game_root = crate::paths::canonicalize_existing(game_root).expect("canonical game root");
    let payload_root =
        crate::paths::canonicalize_existing(payload_root).expect("canonical payload root");
    let game_root_ref =
        PathRef::new(game_root.to_string_lossy().into_owned()).expect("game root ref");
    let payload_root_ref =
        PathRef::new(payload_root.to_string_lossy().into_owned()).expect("payload root ref");
    RenoDxRootSeal {
        canonical_game_root: game_root.clone(),
        canonical_game_root_ref: game_root_ref,
        config_source: RenoDxConfigSourceSeal::Absent {
            exact_ini_path: game_root.join("ReShade.ini"),
        },
        effective_addon_root: payload_root.clone(),
        payload_root: Some(payload_root.clone()),
        payload_root_ref: Some(payload_root_ref),
        exact_ini_path: game_root.join("ReShade.ini"),
        exact_proxy_host: None,
        canonical_registered_exe: None,
        roots: PeerRoots::new(game_root, Some(payload_root)).expect("roots"),
    }
}

fn topology(root: &Path) -> GameProxyTopology {
    let root_slot = path(root, "dxgi.dll");
    GameProxyTopology {
        id: "optiscaler:active-dlss-test".to_owned(),
        game_id: game_id(),
        root_slot: root_slot.clone(),
        outer: ProxyLink {
            implementation: ProxyImplementation::OptiScaler,
            path: root_slot,
            receipt: FileReceipt::owned("optiscaler", hash('a')).expect("receipt"),
        },
        downstream: None,
        downstream_origin: None,
        root_prestate: ProxyRootPrestate::Absent,
    }
}

fn hash(value: char) -> Sha256Hash {
    Sha256Hash::new(value.to_string().repeat(Sha256Hash::HEX_LENGTH)).expect("hash")
}

fn source(value: &str) -> TrackedSource {
    TrackedSource::new(
        TrackedSourceRole::DlssFix,
        "https://example.test/renodx-dlssfix.addon64",
        None,
        value,
    )
}

fn peer(
    root: &Path,
    owned: bool,
    tracked: Option<TrackedSource>,
) -> renderpilot_domain::InstalledAddon {
    let addon = path(root, "renodx-game.addon64");
    let mut record = renderpilot_domain::InstalledAddon::new(game_id(), AddonKind::RenoDx, addon);
    if owned {
        record = record.with_created_file(path(root, "renodx-dlssfix.addon64"));
    }
    if let Some(tracked) = tracked {
        record = record.with_tracked_source(tracked);
    }
    record
}

fn snapshot(root: &Path, name: &str) -> PeerPathSnapshot {
    let root_ref = PathRef::new(
        crate::paths::canonicalize_existing(root)
            .expect("canonical root")
            .to_string_lossy()
            .into_owned(),
    )
    .expect("root ref");
    let target = path(root, name);
    observe_peer_path_snapshot(&target, &root_ref).expect("snapshot")
}

struct ActiveDlssFixtureInput<'a> {
    before: &'a renderpilot_domain::InstalledAddon,
    after: &'a renderpilot_domain::InstalledAddon,
    topology: &'a GameProxyTopology,
    root: &'a RenoDxRootSeal,
    companion_snapshot: &'a PeerPathSnapshot,
    companion_effect: ActiveDlssEffect,
    ini: Option<ActiveDlssEndpointInput<'a>>,
    feature: Option<RenoDxReshadeIniFeature>,
}

fn input(input: ActiveDlssFixtureInput<'_>) -> ActiveDlssInput<'_> {
    let ActiveDlssFixtureInput {
        before,
        after,
        topology,
        root,
        companion_snapshot,
        companion_effect,
        ini,
        feature,
    } = input;
    ActiveDlssInput {
        before_peer: before,
        after_peer: after,
        topology,
        root,
        companion: ActiveDlssEndpointInput::new(
            path(&root.effective_addon_root, "renodx-dlssfix.addon64"),
            companion_snapshot,
            companion_effect,
        ),
        ini,
        ini_feature: feature,
    }
}

macro_rules! input {
    ($before:expr, $after:expr, $topology:expr, $root:expr, $companion_snapshot:expr, $companion_effect:expr, $ini:expr, $feature:expr $(,)?) => {
        input(ActiveDlssFixtureInput {
            before: $before,
            after: $after,
            topology: $topology,
            root: $root,
            companion_snapshot: $companion_snapshot,
            companion_effect: $companion_effect,
            ini: $ini,
            feature: $feature,
        })
    };
}

#[test]
fn sealed_external_addon_root_accepts_its_exact_companion() {
    let game = tempfile::tempdir().expect("game");
    let payload = tempfile::tempdir().expect("payload");
    std::fs::write(payload.path().join("renodx-dlssfix.addon64"), b"existing").expect("companion");
    let root = external_root_seal(game.path(), payload.path());
    let topology = topology(game.path());
    let before = peer(payload.path(), true, Some(source("old")));
    let after = peer(payload.path(), true, Some(source("new")));
    let companion = snapshot(payload.path(), "renodx-dlssfix.addon64");

    let composition = compose_active_dlss(&input!(
        &before,
        &after,
        &topology,
        &root,
        &companion,
        ActiveDlssEffect::Unchanged,
        None,
        None,
    ))
    .expect("sealed external companion");
    assert!(matches!(
        composition,
        ActiveDlssComposition::ClaimOnly { .. }
    ));
}

#[test]
fn external_addon_root_does_not_expand_topology_or_ini_authority() {
    let game = tempfile::tempdir().expect("game");
    let payload = tempfile::tempdir().expect("payload");
    std::fs::write(payload.path().join("renodx-dlssfix.addon64"), b"existing").expect("companion");
    let root = external_root_seal(game.path(), payload.path());
    let before = peer(payload.path(), true, Some(source("old")));
    let after = peer(payload.path(), true, Some(source("new")));
    let companion = snapshot(payload.path(), "renodx-dlssfix.addon64");

    let payload_topology = topology(payload.path());
    assert!(
        compose_active_dlss(&input!(
            &before,
            &after,
            &payload_topology,
            &root,
            &companion,
            ActiveDlssEffect::Unchanged,
            None,
            None,
        ))
        .is_err()
    );

    let game_topology = topology(game.path());
    let external_ini = PeerPathSnapshot::Absent;
    assert!(
        compose_active_dlss(&input!(
            &before,
            &after,
            &game_topology,
            &root,
            &companion,
            ActiveDlssEffect::Unchanged,
            Some(ActiveDlssEndpointInput::new(
                path(payload.path(), "ReShade.ini"),
                &external_ini,
                ActiveDlssEffect::Write(b"[RENODX-DLSSFIX]".to_vec()),
            )),
            Some(RenoDxReshadeIniFeature::DlssFixInstall),
        ))
        .is_err()
    );
}

#[test]
fn paths_outside_both_sealed_roots_are_rejected() {
    let game = tempfile::tempdir().expect("game");
    let payload = tempfile::tempdir().expect("payload");
    let outside = tempfile::tempdir().expect("outside");
    let root = external_root_seal(game.path(), payload.path());
    let topology = topology(game.path());
    let before = peer(outside.path(), false, None);
    let after = peer(outside.path(), true, Some(source("new")));

    assert!(
        compose_active_dlss(&ActiveDlssInput {
            before_peer: &before,
            after_peer: &after,
            topology: &topology,
            root: &root,
            companion: ActiveDlssEndpointInput::new(
                path(outside.path(), "renodx-dlssfix.addon64"),
                &PeerPathSnapshot::Absent,
                ActiveDlssEffect::Write(b"outside".to_vec()),
            ),
            ini: None,
            ini_feature: None,
        })
        .is_err()
    );
}

#[test]
fn source_refresh_without_bytes_is_claim_only() {
    let directory = tempfile::tempdir().expect("directory");
    std::fs::write(directory.path().join("renodx-dlssfix.addon64"), b"existing")
        .expect("companion");
    let root = root_seal(directory.path());
    let topology = topology(directory.path());
    let before = peer(directory.path(), true, Some(source("old")));
    let after = peer(directory.path(), true, Some(source("new")));
    let companion = snapshot(directory.path(), "renodx-dlssfix.addon64");

    let composition = compose_active_dlss(&input!(
        &before,
        &after,
        &topology,
        &root,
        &companion,
        ActiveDlssEffect::Unchanged,
        None,
        None,
    ))
    .expect("claim-only composition");
    assert!(matches!(
        composition,
        ActiveDlssComposition::ClaimOnly { .. }
    ));
}

#[test]
fn byte_identical_adoption_is_claim_only_not_generic_noop() {
    let directory = tempfile::tempdir().expect("directory");
    let bytes = b"foreign but suitable";
    std::fs::write(directory.path().join("renodx-dlssfix.addon64"), bytes).expect("companion");
    let root = root_seal(directory.path());
    let topology = topology(directory.path());
    let before = peer(directory.path(), false, None);
    let after = peer(directory.path(), true, Some(source("adopted")));
    let companion = snapshot(directory.path(), "renodx-dlssfix.addon64");

    let composition = compose_active_dlss(&input!(
        &before,
        &after,
        &topology,
        &root,
        &companion,
        ActiveDlssEffect::Write(bytes.to_vec()),
        None,
        None,
    ))
    .expect("adoption composition");
    assert!(matches!(
        composition,
        ActiveDlssComposition::ClaimOnly { .. }
    ));
}

#[test]
fn explicit_install_can_replace_foreign_regular_companion() {
    let directory = tempfile::tempdir().expect("directory");
    std::fs::write(
        directory.path().join("renodx-dlssfix.addon64"),
        b"foreign bytes",
    )
    .expect("foreign companion");
    let root = root_seal(directory.path());
    let topology = topology(directory.path());
    let before = peer(directory.path(), false, None);
    let after = peer(directory.path(), true, Some(source("installed")));
    let present = snapshot(directory.path(), "renodx-dlssfix.addon64");

    let composition = compose_active_dlss(&input!(
        &before,
        &after,
        &topology,
        &root,
        &present,
        ActiveDlssEffect::Write(b"renodx bytes".to_vec()),
        None,
        None,
    ))
    .expect("install may replace a foreign regular companion");
    assert!(matches!(
        composition,
        ActiveDlssComposition::Physical { .. }
    ));
}

#[test]
fn source_only_repair_cannot_replace_present_unowned_companion() {
    let directory = tempfile::tempdir().expect("directory");
    std::fs::write(
        directory.path().join("renodx-dlssfix.addon64"),
        b"existing bytes",
    )
    .expect("companion");
    let root = root_seal(directory.path());
    let topology = topology(directory.path());
    let before = peer(directory.path(), false, Some(source("tracked")));
    let after = peer(directory.path(), true, Some(source("repaired")));
    let present = snapshot(directory.path(), "renodx-dlssfix.addon64");

    assert!(
        compose_active_dlss(&input!(
            &before,
            &after,
            &topology,
            &root,
            &present,
            ActiveDlssEffect::Write(b"replacement bytes".to_vec()),
            None,
            None,
        ))
        .is_err()
    );
}

#[test]
fn unchanged_effect_cannot_adopt_or_create_a_companion_without_bytes() {
    let directory = tempfile::tempdir().expect("directory");
    let root = root_seal(directory.path());
    let topology = topology(directory.path());
    let before = peer(directory.path(), false, None);
    let after = peer(directory.path(), true, Some(source("adopted")));

    assert!(
        compose_active_dlss(&input!(
            &before,
            &after,
            &topology,
            &root,
            &PeerPathSnapshot::Absent,
            ActiveDlssEffect::Unchanged,
            None,
            None,
        ))
        .is_err()
    );
}

#[test]
fn removal_clears_the_dlss_source_claim() {
    let directory = tempfile::tempdir().expect("directory");
    std::fs::write(
        directory.path().join("renodx-dlssfix.addon64"),
        b"owned bytes",
    )
    .expect("companion");
    let root = root_seal(directory.path());
    let topology = topology(directory.path());
    let before = peer(directory.path(), true, Some(source("owned")));
    let after = peer(directory.path(), false, Some(source("stale")));
    let present = snapshot(directory.path(), "renodx-dlssfix.addon64");

    assert!(
        compose_active_dlss(&input!(
            &before,
            &after,
            &topology,
            &root,
            &present,
            ActiveDlssEffect::Remove,
            None,
            None,
        ))
        .is_err()
    );
}

#[test]
fn create_replace_and_remove_lower_exact_companion_operations() {
    let directory = tempfile::tempdir().expect("directory");
    let root = root_seal(directory.path());
    let topology = topology(directory.path());

    let before = peer(directory.path(), false, None);
    let after = peer(directory.path(), true, Some(source("created")));
    let absent = PeerPathSnapshot::Absent;
    let created = compose_active_dlss(&input!(
        &before,
        &after,
        &topology,
        &root,
        &absent,
        ActiveDlssEffect::Write(b"created".to_vec()),
        None,
        None,
    ))
    .expect("create");
    let ActiveDlssComposition::Physical {
        program, payloads, ..
    } = created
    else {
        panic!("create must be physical")
    };
    assert_eq!(program.endpoints().len(), 1);
    assert_eq!(
        program.endpoints()[0].after(),
        &crate::peer_mutation_executor::EndpointPostcondition::File(
            renderpilot_detection::sha256_bytes(b"created").expect("digest")
        )
    );
    assert_eq!(payloads, vec![Some(b"created".to_vec())]);

    let old = b"old";
    std::fs::write(directory.path().join("renodx-dlssfix.addon64"), old).expect("companion");
    let before = peer(directory.path(), true, Some(source("old")));
    let after = peer(directory.path(), true, Some(source("new")));
    let present = snapshot(directory.path(), "renodx-dlssfix.addon64");
    let replaced = compose_active_dlss(&input!(
        &before,
        &after,
        &topology,
        &root,
        &present,
        ActiveDlssEffect::Write(b"new".to_vec()),
        None,
        None,
    ))
    .expect("replace");
    let ActiveDlssComposition::Physical { program, .. } = replaced else {
        panic!("replace must be physical")
    };
    assert_eq!(program.endpoints().len(), 1);
    assert!(matches!(
        program.endpoints()[0].before(),
        crate::peer_mutation_executor::EndpointExpectation::File(_)
    ));

    let after = peer(directory.path(), false, None);
    let removed = compose_active_dlss(&input!(
        &before,
        &after,
        &topology,
        &root,
        &present,
        ActiveDlssEffect::Remove,
        None,
        None,
    ))
    .expect("remove");
    let ActiveDlssComposition::Physical {
        program, payloads, ..
    } = removed
    else {
        panic!("remove must be physical")
    };
    assert_eq!(program.endpoints().len(), 1);
    assert_eq!(payloads, vec![None]);
    assert!(matches!(
        program.endpoints()[0].after(),
        crate::peer_mutation_executor::EndpointPostcondition::Absent
    ));
}

#[test]
fn companion_precedes_typed_ini_and_intents_remain_aligned() {
    let directory = tempfile::tempdir().expect("directory");
    let root = root_seal(directory.path());
    let topology = topology(directory.path());
    let before = peer(directory.path(), false, None);
    let after = peer(directory.path(), true, Some(source("new")));
    let companion = PeerPathSnapshot::Absent;
    let ini = PeerPathSnapshot::Absent;
    let ini_path = path(directory.path(), "ReShade.ini");
    let composition = compose_active_dlss(&input!(
        &before,
        &after,
        &topology,
        &root,
        &companion,
        ActiveDlssEffect::Write(b"companion".to_vec()),
        Some(ActiveDlssEndpointInput::new(
            ini_path,
            &ini,
            ActiveDlssEffect::Write(b"[RENODX-DLSSFIX]".to_vec()),
        )),
        Some(RenoDxReshadeIniFeature::DlssFixInstall),
    ))
    .expect("compound composition");
    let ActiveDlssComposition::Physical {
        program,
        payloads,
        game_intents,
        reshade_ini_authority,
        ..
    } = composition
    else {
        panic!("compound transition must be physical")
    };
    assert_eq!(program.endpoints().len(), 2);
    assert_eq!(payloads.len(), 2);
    assert_eq!(game_intents.len(), 2);
    assert_eq!(
        program.endpoints()[0].role(),
        renderpilot_domain::PeerEndpointRole::Disjoint
    );
    assert_eq!(
        program.endpoints()[1].role(),
        renderpilot_domain::PeerEndpointRole::RenoDxReshadeIni
    );
    assert!(reshade_ini_authority.is_some());
}

#[test]
fn wrong_path_and_unrelated_record_drift_fail_closed() {
    let directory = tempfile::tempdir().expect("directory");
    let root = root_seal(directory.path());
    let topology = topology(directory.path());
    let before = peer(directory.path(), false, None);
    let after = peer(directory.path(), true, Some(source("new")));
    let absent = PeerPathSnapshot::Absent;
    let wrong = ActiveDlssInput {
        before_peer: &before,
        after_peer: &after,
        topology: &topology,
        root: &root,
        companion: ActiveDlssEndpointInput::new(
            path(directory.path(), "other.addon64"),
            &absent,
            ActiveDlssEffect::Write(b"bytes".to_vec()),
        ),
        ini: None,
        ini_feature: None,
    };
    assert!(compose_active_dlss(&wrong).is_err());

    let drifted = after.with_addon_version("unrelated");
    assert!(
        compose_active_dlss(&input!(
            &before,
            &drifted,
            &topology,
            &root,
            &absent,
            ActiveDlssEffect::Write(b"bytes".to_vec()),
            None,
            None,
        ))
        .is_err()
    );
}

#[test]
fn malformed_present_snapshot_is_rejected_before_lowering() {
    let directory = tempfile::tempdir().expect("directory");
    let root = root_seal(directory.path());
    let path = directory.path().join("renodx-dlssfix.addon64");
    std::fs::create_dir(&path).expect("directory endpoint");
    assert!(
        observe_peer_path_snapshot(
            &PathRef::new(path.to_string_lossy().into_owned()).expect("path"),
            root.canonical_game_root_ref(),
        )
        .is_err()
    );
}
