use std::fs;

use renderpilot_domain::{
    Architecture, ComponentFile, ComponentId, ComponentKind, GameId, GraphicsComponent,
    GraphicsTechnology, ManagedAddonFile, ManagedFileBaseline, PathRef, PeCompatibilityProfile,
    PeExportSet, Swappability,
};

use super::*;

#[test]
fn adopts_a_valid_unrecorded_classic_sidecar() {
    let root = tempfile::tempdir().expect("root");
    let live = root.path().join("nvngx_dlss.dll");
    fs::write(&live, b"overlay").expect("live");
    fs::write(root.path().join("nvngx_dlss.dll.bak"), b"original").expect("bak");

    let resolved = BaselineResolver::new(root.path(), &[], GraphicsTechnology::DlssSuperResolution)
        .resolve(&live, None)
        .expect("valid sidecar");

    assert!(matches!(
        resolved,
        ResolvedBaseline::ExistingSidecarBaseline(_)
    ));
}

#[test]
fn rejects_an_empty_unrecorded_sidecar() {
    let root = tempfile::tempdir().expect("root");
    let live = root.path().join("nvngx_dlss.dll");
    fs::write(&live, b"overlay").expect("live");
    fs::write(root.path().join("nvngx_dlss.dll.bak"), b"").expect("bak");

    assert!(matches!(
        BaselineResolver::new(root.path(), &[], GraphicsTechnology::DlssSuperResolution,)
            .resolve(&live, None),
        Err(BaselineConflict::Empty(_))
    ));
}

#[test]
fn recorded_baseline_availability_follows_the_physical_sidecar() {
    let root = tempfile::tempdir().expect("root");
    let live = root.path().join("nvngx_dlss.dll");
    let backup = root.path().join("nvngx_dlss.dll.bak");
    fs::write(&live, b"original").expect("live");
    let original_hash = renderpilot_detection::sha256_file(&live).expect("hash");
    let recorded = vec![
        ComponentFile::new(PathRef::new(live.to_string_lossy().into_owned()).expect("path"))
            .with_sha256(original_hash.clone()),
    ];
    fs::write(&live, b"overlay").expect("overlay");
    let overlay = vec![
        ComponentFile::new(PathRef::new(live.to_string_lossy().into_owned()).expect("path"))
            .with_sha256(renderpilot_detection::sha256_file(&live).expect("overlay hash")),
    ];

    assert!(!super::backup::baseline_sources_appear_available(
        &recorded, &overlay
    ));

    fs::write(&backup, b"").expect("empty backup");
    assert!(!super::backup::baseline_sources_appear_available(
        &recorded, &overlay
    ));

    fs::write(&backup, b"original").expect("backup");
    assert!(super::backup::baseline_sources_appear_available(
        &recorded, &overlay
    ));

    fs::remove_file(&backup).expect("remove backup");
    assert!(!super::backup::baseline_sources_appear_available(
        &recorded, &overlay
    ));
    fs::create_dir(&backup).expect("directory at backup path");
    assert!(!super::backup::baseline_sources_appear_available(
        &recorded, &overlay
    ));
    fs::remove_dir(&backup).expect("remove backup directory");

    let missing_hash = vec![ComponentFile::new(
        PathRef::new(live.to_string_lossy().into_owned()).expect("path"),
    )];
    assert!(!super::backup::baseline_sources_appear_available(
        &missing_hash,
        &overlay
    ));

    fs::write(&live, b"original").expect("unchanged live baseline");
    let unchanged = vec![
        ComponentFile::new(PathRef::new(live.to_string_lossy().into_owned()).expect("path"))
            .with_sha256(original_hash),
    ];
    assert!(
        super::backup::baseline_sources_appear_available(&recorded, &unchanged),
        "an untouched live member does not require a sidecar"
    );
    assert!(
        super::backup::baseline_sources_appear_available(&[], &[]),
        "an empty baseline rolls back by removing overlay-created files"
    );
}

#[test]
fn owned_absent_binding_wins_over_the_current_live_overlay() {
    let root = tempfile::tempdir().expect("root");
    let live = root.path().join("nvngx_dlss.dll");
    fs::write(&live, b"luma-overlay").expect("live");
    let binding = ManagedAddonFile::owned(
        PathRef::new(live.to_string_lossy().into_owned()).expect("path"),
        ManagedFileBaseline::Absent,
        renderpilot_detection::sha256_file(&live).expect("hash"),
    );

    let resolved = BaselineResolver::new(
        root.path(),
        &[binding],
        GraphicsTechnology::DlssSuperResolution,
    )
    .resolve(&live, None)
    .expect("owned binding");

    assert_eq!(resolved, ResolvedBaseline::AddonOwnedAbsent);
}

#[test]
fn current_snapshot_rejects_external_replacement_and_missing_members() {
    let root = tempfile::tempdir().expect("root");
    let live = root.path().join("nvngx_dlss.dll");
    fs::write(&live, b"scanned").expect("live");
    let scanned_hash = renderpilot_detection::sha256_file(&live).expect("hash");
    let component = GraphicsComponent::new(
        ComponentId::new("component:freshness").expect("component"),
        GameId::new("manual:freshness").expect("game"),
        ComponentKind::NativeLibrary,
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(PathRef::new(live.to_string_lossy().into_owned()).expect("path"))
            .with_sha256(scanned_hash),
    );

    fs::write(&live, b"external").expect("replace");
    assert!(matches!(
        current_component_snapshot(&component, &[]),
        Err(BaselineConflict::ActiveHashMismatch { .. })
    ));
    fs::remove_file(&live).expect("remove");
    assert!(matches!(
        current_component_snapshot(&component, &[]),
        Err(BaselineConflict::MissingActiveFile(_))
    ));
}

#[test]
fn openvr_snapshot_and_recorded_baseline_discard_stale_pe_metadata() {
    let root = tempfile::tempdir().expect("root");
    let live = root.path().join("openvr_api.dll");
    fs::write(&live, b"not-a-pe").expect("live");
    let hash = renderpilot_detection::sha256_file(&live).expect("hash");
    let stale =
        ComponentFile::new(PathRef::new(live.to_string_lossy().into_owned()).expect("path"))
            .with_sha256(hash)
            .with_pe_compatibility(PeCompatibilityProfile::new(
                Architecture::X64,
                PeExportSet::from_canonical_names(vec!["VR_InitInternal".into()]).expect("exports"),
            ));
    let component = GraphicsComponent::new(
        ComponentId::new("component:openvr-freshness").expect("component"),
        GameId::new("manual:openvr-freshness").expect("game"),
        ComponentKind::NativeLibrary,
        GraphicsTechnology::OpenVr,
        Swappability::Swappable,
    )
    .with_file(stale);

    let current = current_component_snapshot(&component, &[])
        .expect("snapshot")
        .into_component();
    assert_eq!(current.files()[0].pe_compatibility(), None);

    let baseline = resolve_component_baseline(
        root.path(),
        GraphicsTechnology::OpenVr,
        component.files(),
        Some(component.files()),
        &[],
    )
    .expect("baseline");
    assert_eq!(baseline[0].pe_compatibility(), None);
}

#[test]
fn executor_rechecks_sidecar_immediately_before_overlay() {
    let root = tempfile::tempdir().expect("root");
    let live = root.path().join("nvngx_dlss.dll");
    let sidecar = root.path().join("nvngx_dlss.dll.bak");
    fs::write(&live, b"active").expect("live");
    fs::write(&sidecar, b"original").expect("sidecar");
    let live_hash = renderpilot_detection::sha256_file(&live).expect("hash");
    let baseline_hash = renderpilot_detection::sha256_file(&sidecar).expect("hash");
    let next_source = root.path().join("next.dll");
    fs::write(&next_source, b"next").expect("next_source");

    let plan = CoordinatedFilePlan::OverlayPreservingBaseline {
        path: live.clone(),
        baseline: ManagedFileBaseline::Present {
            sha256: baseline_hash,
        },
        expected_live: ExpectedLive::Hashes(vec![live_hash]),
        source: next_source,
    };

    fs::write(&sidecar, b"tampered").expect("tamper");
    assert!(execute_file_plan(&plan).is_err());
    assert_eq!(fs::read(live).expect("live"), b"active");
}

#[test]
fn overlay_from_path_copies_source() {
    let root = tempfile::tempdir().expect("root");
    let live = root.path().join("nvngx_dlss.dll");
    let source = root.path().join("source.dll");
    fs::write(&source, b"next-bytes").expect("source");

    execute_file_plan(&CoordinatedFilePlan::OverlayFromPath {
        path: live.clone(),
        source,
    })
    .expect("overlay");

    assert_eq!(fs::read(live).expect("live"), b"next-bytes");
}

#[test]
fn ensure_baseline_sidecar_creates_from_live() {
    let root = tempfile::tempdir().expect("root");
    let live = root.path().join("nvngx_dlss.dll");
    let sidecar = root.path().join("nvngx_dlss.dll.bak");
    fs::write(&live, b"original").expect("live");
    let expected_live = renderpilot_detection::sha256_file(&live).expect("hash");

    execute_file_plan(&CoordinatedFilePlan::EnsureBaselineSidecar {
        path: live.clone(),
        expected_live: expected_live.clone(),
    })
    .expect("ensure sidecar");

    assert!(sidecar.exists());
    assert_eq!(
        renderpilot_detection::sha256_file(&sidecar).expect("sidecar hash"),
        expected_live
    );
    assert_eq!(fs::read(live).expect("live"), b"original");
}

#[test]
fn archive_live_to_sidecar_and_remove() {
    let root = tempfile::tempdir().expect("root");
    let live = root.path().join("nvngx_dlss.dll");
    let sidecar = root.path().join("nvngx_dlss.dll.bak");
    fs::write(&live, b"to-archive").expect("live");
    let expected_live = renderpilot_detection::sha256_file(&live).expect("hash");

    execute_file_plan(&CoordinatedFilePlan::ArchiveLiveToSidecarAndRemove {
        path: live.clone(),
        expected_live,
    })
    .expect("archive");

    assert!(!live.exists());
    assert_eq!(fs::read(sidecar).expect("sidecar"), b"to-archive");
}

#[test]
fn restore_preserving_sidecar() {
    let root = tempfile::tempdir().expect("root");
    let live = root.path().join("nvngx_dlss.dll");
    let sidecar = root.path().join("nvngx_dlss.dll.bak");
    fs::write(&live, b"overlay").expect("live");
    fs::write(&sidecar, b"baseline").expect("sidecar");
    let baseline_sha256 = renderpilot_detection::sha256_file(&sidecar).expect("hash");

    execute_file_plan(&CoordinatedFilePlan::RestorePreservingSidecar {
        path: live.clone(),
        baseline_sha256,
    })
    .expect("restore");

    assert_eq!(fs::read(&live).expect("live"), b"baseline");
    assert_eq!(fs::read(sidecar).expect("sidecar"), b"baseline");
}

#[test]
fn release_sidecar_removes_bak_and_tolerates_missing() {
    let root = tempfile::tempdir().expect("root");
    let live = root.path().join("nvngx_dlss.dll");
    let sidecar = root.path().join("nvngx_dlss.dll.bak");
    fs::write(&live, b"restored").expect("live");
    fs::write(&sidecar, b"baseline").expect("sidecar");

    execute_file_plan(&CoordinatedFilePlan::ReleaseSidecar { path: live.clone() })
        .expect("release");
    assert!(!sidecar.exists());

    execute_file_plan(&CoordinatedFilePlan::ReleaseSidecar { path: live })
        .expect("missing sidecar is ok");
}

#[test]
fn remove_live_deletes_file() {
    let root = tempfile::tempdir().expect("root");
    let live = root.path().join("added.dll");
    fs::write(&live, b"temp").expect("live");

    execute_file_plan(&CoordinatedFilePlan::RemoveLive { path: live.clone() }).expect("remove");

    assert!(!live.exists());
}

#[test]
fn restore_batch_enforces_kinds() {
    let root = tempfile::tempdir().expect("root");
    let live = root.path().join("nvngx_dlss.dll");
    fs::write(&live, b"x").expect("live");
    let hash = renderpilot_detection::sha256_file(&live).expect("hash");

    let err = execute_restore_batch(
        [CoordinatedFilePlan::OverlayFromPath {
            path: live.clone(),
            source: live.clone(),
        }],
        [],
    )
    .expect_err("non-restore plan");
    assert!(err.to_string().contains("non-restore"));

    let err = execute_restore_batch(
        [CoordinatedFilePlan::RestorePreservingSidecar {
            path: live.clone(),
            baseline_sha256: hash,
        }],
        [CoordinatedFilePlan::RemoveLive { path: live }],
    )
    .expect_err("non-release plan");
    assert!(err.to_string().contains("non-release"));
}

#[test]
fn restore_batch_runs_restore_before_release() {
    let root = tempfile::tempdir().expect("root");
    let live = root.path().join("nvngx_dlss.dll");
    let sidecar = root.path().join("nvngx_dlss.dll.bak");
    fs::write(&live, b"overlay").expect("live");
    fs::write(&sidecar, b"baseline").expect("sidecar");
    let baseline_sha256 = renderpilot_detection::sha256_file(&sidecar).expect("hash");

    execute_restore_batch(
        [CoordinatedFilePlan::RestorePreservingSidecar {
            path: live.clone(),
            baseline_sha256,
        }],
        [CoordinatedFilePlan::ReleaseSidecar { path: live.clone() }],
    )
    .expect("batch");

    assert_eq!(fs::read(live).expect("live"), b"baseline");
    assert!(!sidecar.exists());
}
