use std::fs;

use renderpilot_domain::{GameId, ManagedFileBaseline, ManagedFileMode};
use tempfile::tempdir;

use super::binding::owned_binding;
use super::plan::{plan_install, plan_update};
use super::*;
use crate::Context;
use crate::addons::luma::fetch::types::LumaPayloadFile;
use crate::coordinated_files::CoordinatedFilePlan;

fn payload(version: [u16; 4]) -> Vec<LumaPayloadFile> {
    vec![LumaPayloadFile {
        relative_path: NVNGX_DLSS_FILE_NAME.to_owned(),
        bytes: crate::addons::luma::test_support::build_nvidia_dlss_pe(version),
    }]
}

fn context() -> (tempfile::TempDir, Context, GameId) {
    let db = tempdir().expect("db");
    let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:dlss-plan").expect("game id");
    (db, context, game_id)
}

#[test]
fn planning_an_overlay_does_not_create_a_sidecar() {
    let (_db, context, game_id) = context();
    let root = tempdir().expect("root");
    let live = root.path().join(NVNGX_DLSS_FILE_NAME);
    fs::write(
        &live,
        crate::addons::luma::test_support::build_nvidia_dlss_pe([2, 5, 0, 0]),
    )
    .expect("live");

    let planned =
        plan_install(&context, &game_id, root.path(), &payload([3, 7, 0, 0])).expect("plan");

    assert!(matches!(
        planned.action,
        CoordinatedFilePlan::CreateBaselineAndOverlay { .. }
    ));
    assert!(!root.path().join("nvngx_dlss.dll.bak").exists());
    planned.execute().expect("execute");
    assert!(root.path().join("nvngx_dlss.dll.bak").is_file());
}

#[test]
fn newer_or_equal_live_is_reused_without_writes() {
    let (_db, context, game_id) = context();
    let root = tempdir().expect("root");
    let live = root.path().join(NVNGX_DLSS_FILE_NAME);
    let bytes = crate::addons::luma::test_support::build_nvidia_dlss_pe([3, 7, 0, 0]);
    fs::write(&live, &bytes).expect("live");

    let planned =
        plan_install(&context, &game_id, root.path(), &payload([3, 5, 0, 0])).expect("plan");
    assert_eq!(
        planned.binding.as_ref().expect("binding").mode(),
        ManagedFileMode::Reused
    );
    planned.execute().expect("execute");
    assert_eq!(fs::read(live).expect("live"), bytes);
}

#[test]
fn incompatible_dlss_generation_is_rejected() {
    let (_db, context, game_id) = context();
    let root = tempdir().expect("root");
    fs::write(
        root.path().join(NVNGX_DLSS_FILE_NAME),
        crate::addons::luma::test_support::build_nvidia_dlss_pe([1, 5, 0, 0]),
    )
    .expect("live");

    let error = plan_install(&context, &game_id, root.path(), &payload([2, 5, 0, 0]))
        .expect_err("incompatible");
    assert!(error.to_string().contains("incompatible"));
}

#[test]
fn absent_path_becomes_owned_without_inventing_a_sidecar() {
    let (_db, context, game_id) = context();
    let root = tempdir().expect("root");

    let planned =
        plan_install(&context, &game_id, root.path(), &payload([3, 7, 0, 0])).expect("plan");
    assert_eq!(
        planned.binding.as_ref().expect("binding").baseline(),
        &ManagedFileBaseline::Absent
    );
    planned.execute().expect("execute");
    assert!(root.path().join(NVNGX_DLSS_FILE_NAME).is_file());
    assert!(!root.path().join("nvngx_dlss.dll.bak").exists());
}

#[test]
fn owned_update_never_downgrades_and_keeps_ownership() {
    let (_db, context, game_id) = context();
    let root = tempdir().expect("root");
    let live = root.path().join(NVNGX_DLSS_FILE_NAME);
    let bytes = crate::addons::luma::test_support::build_nvidia_dlss_pe([4, 0, 0, 0]);
    fs::write(&live, &bytes).expect("live");
    let hash = renderpilot_detection::sha256_file(&live).expect("hash");
    let owned = owned_binding(&live, ManagedFileBaseline::Absent, hash).expect("binding");

    let planned = plan_update(
        &context,
        &game_id,
        root.path(),
        &payload([3, 7, 0, 0]),
        Some(&owned),
        false,
    )
    .expect("plan");
    assert_eq!(
        planned.binding.as_ref().expect("binding").mode(),
        ManagedFileMode::Owned
    );
    planned.execute().expect("execute");
    assert_eq!(fs::read(live).expect("live"), bytes);
}

#[test]
fn manual_replacement_of_an_owned_dll_requires_repair() {
    let (_db, context, game_id) = context();
    let root = tempdir().expect("root");
    let live = root.path().join(NVNGX_DLSS_FILE_NAME);
    let sidecar = root.path().join("nvngx_dlss.dll.bak");
    let recorded = crate::addons::luma::test_support::build_nvidia_dlss_pe([3, 5, 0, 0]);
    let external = crate::addons::luma::test_support::build_nvidia_dlss_pe([3, 6, 0, 0]);
    fs::write(&live, &external).expect("live");
    fs::write(&sidecar, b"original").expect("sidecar");
    let baseline_hash = renderpilot_detection::sha256_file(&sidecar).expect("hash");
    let owned = owned_binding(
        &live,
        ManagedFileBaseline::Present {
            sha256: baseline_hash,
        },
        renderpilot_detection::sha256_bytes(&recorded).expect("recorded hash"),
    )
    .expect("binding");

    let error = plan_update(
        &context,
        &game_id,
        root.path(),
        &payload([3, 7, 0, 0]),
        Some(&owned),
        false,
    )
    .expect_err("external replacement");
    assert!(error.to_string().contains("repair"));
}

#[test]
fn disappearing_owned_payload_restores_and_releases_the_baseline() {
    let (_db, context, game_id) = context();
    let root = tempdir().expect("root");
    let live = root.path().join(NVNGX_DLSS_FILE_NAME);
    let sidecar = root.path().join("nvngx_dlss.dll.bak");
    fs::write(
        &live,
        crate::addons::luma::test_support::build_nvidia_dlss_pe([3, 7, 0, 0]),
    )
    .expect("live");
    fs::write(&sidecar, b"original").expect("sidecar");
    let live_hash = renderpilot_detection::sha256_file(&live).expect("hash");
    let baseline_hash = renderpilot_detection::sha256_file(&sidecar).expect("hash");
    let owned = owned_binding(
        &live,
        ManagedFileBaseline::Present {
            sha256: baseline_hash,
        },
        live_hash,
    )
    .expect("binding");

    let planned =
        plan_update(&context, &game_id, root.path(), &[], Some(&owned), false).expect("plan");
    assert!(planned.binding.is_none());
    planned.execute().expect("execute");
    assert_eq!(fs::read(live).expect("live"), b"original");
    assert!(!sidecar.exists());
}
