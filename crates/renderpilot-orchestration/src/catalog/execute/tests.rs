use std::fs;
use std::path::{Path, PathBuf};

use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
    GraphicsComponent, GraphicsTechnology, LibraryArtifact, PathRef, Sha256Hash, Swappability,
};

use super::fs_ops::{perform_apply_fs, revert_to_baseline_fs};
use super::planning::fsr_members_to_remove;
use super::types::{AppliedFsChanges, PlannedFile};

const HEX64: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn comp_file(path: &Path) -> ComponentFile {
    ComponentFile::new(PathRef::new(path.to_string_lossy().as_ref()).expect("valid path"))
}

fn comp_file_str(path: &str) -> ComponentFile {
    ComponentFile::new(PathRef::new(path).expect("valid path"))
}

fn bak_of(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.bak", path.display()))
}

fn write(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).expect("write fixture file");
}

fn planned_copy(source: &Path, target: &Path) -> PlannedFile {
    PlannedFile {
        source: source.to_path_buf(),
        file: comp_file(target),
    }
}

/// Minimal FSR component placeholder; `component` is only read on the
/// re-swap (`first_swap == false`) revert path, so these tests pass it
/// `first_swap = true` and never depend on its files.
fn placeholder_component() -> GraphicsComponent {
    GraphicsComponent::new(
        ComponentId::new("component:test").expect("component id"),
        GameId::new("manual:C:/Games/Test").expect("game id"),
        ComponentKind::NativeLibrary,
        GraphicsTechnology::AmdFsr,
        Swappability::Swappable,
    )
}

#[test]
fn overlay_backs_up_existing_target_and_installs_durably() {
    let dir = tempfile::tempdir().expect("temp dir");
    let target = dir.path().join("nvngx_dlss.dll");
    let source = dir.path().join("source.dll");
    write(&target, b"original");
    write(&source, b"new-version");

    let plans = vec![planned_copy(&source, &target)];
    let changes = perform_apply_fs(&placeholder_component(), &[], &plans, &[], true)
        .expect("apply should succeed");

    assert_eq!(fs::read(&target).expect("target readable"), b"new-version");
    assert_eq!(
        fs::read(bak_of(&target)).expect("bak readable"),
        b"original"
    );
    assert_eq!(changes.copied, vec![target.clone()]);
    assert_eq!(
        changes.renamed_to_bak,
        vec![(target.clone(), bak_of(&target))]
    );
}

#[test]
fn overlay_adds_new_file_without_creating_backup() {
    let dir = tempfile::tempdir().expect("temp dir");
    let target = dir.path().join("amd_fidelityfx_upscaler_dx12.dll");
    let source = dir.path().join("source.dll");
    write(&source, b"fresh");

    let plans = vec![planned_copy(&source, &target)];
    let changes = perform_apply_fs(&placeholder_component(), &[], &plans, &[], true)
        .expect("apply should succeed");

    assert_eq!(fs::read(&target).expect("target readable"), b"fresh");
    assert!(
        !bak_of(&target).exists(),
        "no backup for a newly added file"
    );
    assert!(changes.renamed_to_bak.is_empty());
}

#[test]
fn removed_member_is_backed_up_then_deleted() {
    let dir = tempfile::tempdir().expect("temp dir");
    let member = dir.path().join("amd_fidelityfx_framegeneration_dx12.dll");
    write(&member, b"fsr4-member");

    let removed = vec![comp_file(&member)];
    let changes = perform_apply_fs(&placeholder_component(), &[], &[], &removed, true)
        .expect("apply should succeed");

    assert!(!member.exists(), "removed member should be gone");
    assert_eq!(
        fs::read(bak_of(&member)).expect("bak readable"),
        b"fsr4-member",
        "removed member must be preserved as a .bak for rollback"
    );
    assert_eq!(
        changes.renamed_to_bak,
        vec![(member.clone(), bak_of(&member))]
    );
}

#[test]
fn apply_failure_midway_rolls_back_every_change() {
    let dir = tempfile::tempdir().expect("temp dir");
    let target = dir.path().join("nvngx_dlss.dll");
    let good_source = dir.path().join("good.dll");
    let missing_source = dir.path().join("does-not-exist.dll");
    write(&target, b"original");
    write(&good_source, b"new-version");

    let plans = vec![
        planned_copy(&good_source, &target),
        planned_copy(&missing_source, &dir.path().join("second.dll")),
    ];
    let result = perform_apply_fs(&placeholder_component(), &[], &plans, &[], true);

    assert!(result.is_err(), "missing source must fail the apply");
    assert_eq!(
        fs::read(&target).expect("target readable"),
        b"original",
        "the first file must be restored to its original bytes"
    );
    assert!(
        !bak_of(&target).exists(),
        "backup must be consumed by the restore"
    );
    assert!(
        !dir.path().join("second.dll").exists(),
        "the failed file must not be left behind"
    );
}

#[test]
fn revert_to_baseline_restores_backup_and_deletes_added_files() {
    let dir = tempfile::tempdir().expect("temp dir");
    let replaced = dir.path().join("nvngx_dlss.dll");
    let added = dir.path().join("nvngx_dlssg.dll");
    write(&replaced, b"overlay");
    write(&bak_of(&replaced), b"original");
    write(&added, b"added-by-swap");

    let current = vec![comp_file(&replaced), comp_file(&added)];
    let baseline = vec![comp_file(&replaced)];
    revert_to_baseline_fs(&current, &baseline).expect("revert should succeed");

    assert_eq!(fs::read(&replaced).expect("readable"), b"original");
    assert!(!bak_of(&replaced).exists(), "backup consumed by restore");
    assert!(!added.exists(), "overlay-added file removed on revert");
}

#[test]
fn undo_removes_copies_and_restores_backups() {
    let dir = tempfile::tempdir().expect("temp dir");
    let target = dir.path().join("nvngx_dlss.dll");
    write(&target, b"overlay");
    write(&bak_of(&target), b"original");

    let changes = AppliedFsChanges {
        renamed_to_bak: vec![(target.clone(), bak_of(&target))],
        copied: vec![target.clone()],
    };
    changes.undo();

    assert_eq!(fs::read(&target).expect("readable"), b"original");
    assert!(!bak_of(&target).exists());
}

#[test]
fn fsr_downgrade_removes_unmatched_upscaling_members() {
    let baseline = vec![
        comp_file_str("C:/game/amd_fidelityfx_dx12.dll"),
        comp_file_str("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
        comp_file_str("C:/game/amd_fidelityfx_framegeneration_dx12.dll"),
    ];

    let artifact = LibraryArtifact::new(
        ArtifactId::new("artifact:fsr31").expect("artifact id"),
        GraphicsTechnology::AmdFsr,
        "amd_fidelityfx_dx12.dll",
        vec![comp_file_str("C:/lib/amd_fidelityfx_dx12.dll")
            .with_sha256(Sha256Hash::new(HEX64).expect("sha"))],
        ArtifactTrustLevel::ManifestDownloaded,
    )
    .expect("artifact");

    let planned = vec![planned_copy(
        Path::new("C:/lib/amd_fidelityfx_dx12.dll"),
        Path::new("C:/game/amd_fidelityfx_dx12.dll"),
    )];

    let removed = fsr_members_to_remove(&baseline, &artifact, &planned);
    let names: Vec<&str> = removed
        .iter()
        .filter_map(|file| file.path().file_name())
        .collect();
    assert_eq!(
        names,
        vec![
            "amd_fidelityfx_upscaler_dx12.dll",
            "amd_fidelityfx_framegeneration_dx12.dll",
        ],
        "a unified FSR 3.1 downgrade drops the upscaling members it does not install"
    );
}

#[test]
fn fsr_downgrade_spares_the_games_own_loader_and_optional_effects() {
    // Mixed lineage — the loader+denoiser Ray Regeneration stack is
    // independent of the upscaling backend and must survive a unified
    // FSR 3.1 update untouched.
    let baseline = vec![
        comp_file_str("C:/game/amd_fidelityfx_dx12.dll"),
        comp_file_str("C:/game/amd_fidelityfx_loader_dx12.dll"),
        comp_file_str("C:/game/amd_fidelityfx_denoiser_dx12.dll"),
    ];

    let artifact = LibraryArtifact::new(
        ArtifactId::new("artifact:fsr31").expect("artifact id"),
        GraphicsTechnology::AmdFsr,
        "amd_fidelityfx_dx12.dll",
        vec![comp_file_str("C:/lib/amd_fidelityfx_dx12.dll")
            .with_sha256(Sha256Hash::new(HEX64).expect("sha"))],
        ArtifactTrustLevel::ManifestDownloaded,
    )
    .expect("artifact");

    let planned = vec![planned_copy(
        Path::new("C:/lib/amd_fidelityfx_dx12.dll"),
        Path::new("C:/game/amd_fidelityfx_dx12.dll"),
    )];

    assert!(
        fsr_members_to_remove(&baseline, &artifact, &planned).is_empty(),
        "the loader+denoiser stack is not part of the upscaling lineage"
    );
}

#[test]
fn fsr_members_to_remove_reads_the_baseline_not_the_live_component() {
    // Re-swap scenario: the live component was already cleaned by an earlier
    // unified swap, but the revert-to-baseline that precedes the overlay
    // restores the baseline's upscaling members — they must be removed again.
    let baseline = vec![
        comp_file_str("C:/game/amd_fidelityfx_dx12.dll"),
        comp_file_str("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
    ];

    let artifact = LibraryArtifact::new(
        ArtifactId::new("artifact:fsr31").expect("artifact id"),
        GraphicsTechnology::AmdFsr,
        "amd_fidelityfx_dx12.dll",
        vec![comp_file_str("C:/lib/amd_fidelityfx_dx12.dll")
            .with_sha256(Sha256Hash::new(HEX64).expect("sha"))],
        ArtifactTrustLevel::ManifestDownloaded,
    )
    .expect("artifact");

    let planned = vec![planned_copy(
        Path::new("C:/lib/amd_fidelityfx_dx12.dll"),
        Path::new("C:/game/amd_fidelityfx_dx12.dll"),
    )];

    let removed = fsr_members_to_remove(&baseline, &artifact, &planned);
    assert_eq!(
        removed
            .iter()
            .filter_map(|file| file.path().file_name())
            .collect::<Vec<_>>(),
        vec!["amd_fidelityfx_upscaler_dx12.dll"],
    );
}

#[test]
fn fsr_members_to_remove_is_empty_for_a_split_artifact() {
    let baseline = vec![
        comp_file_str("C:/game/amd_fidelityfx_dx12.dll"),
        comp_file_str("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
    ];

    // The artifact's primary file *is* the upscaler (split marker) → not a
    // unified downgrade, so nothing is removed.
    let artifact = LibraryArtifact::new(
        ArtifactId::new("artifact:fsr4").expect("artifact id"),
        GraphicsTechnology::AmdFsr,
        "amd_fidelityfx_upscaler_dx12.dll",
        vec![comp_file_str("C:/lib/amd_fidelityfx_upscaler_dx12.dll")
            .with_sha256(Sha256Hash::new(HEX64).expect("sha"))],
        ArtifactTrustLevel::ManifestDownloaded,
    )
    .expect("artifact");

    assert!(fsr_members_to_remove(&baseline, &artifact, &[]).is_empty());
}
