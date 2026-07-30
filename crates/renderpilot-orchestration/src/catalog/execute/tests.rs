use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use renderpilot_application::{
    ArtifactRepository, ComponentRepository, D3d12ExecutableAction, D3d12ExecutableProfile,
    GameRepository, InstalledAddonRepository,
};
use renderpilot_domain::{
    AddonKind, Architecture, ArtifactId, ArtifactMetadata, ArtifactTrustLevel, ComponentFile,
    ComponentId, ComponentKind, ComponentRollbackBaseline, D3d12ExecutableBaseline,
    D3d12ExecutableIdentity, GameId, GameIdentity, GameInstallation, GameRuntime, InstalledAddon,
    Launcher, LibraryArtifact, LibraryComponent, LibraryTechnology, ManagedAddonFile,
    ManagedFileBaseline, PathRef, PeCompatibilityProfile, PeExportSet, Platform,
    RuntimeCompatibility, RuntimeTarget, Sha256Hash, Swappability, UpstreamPackage,
    UpstreamPackageProvider, Version,
};
use renderpilot_platform_windows::DeveloperModeStatus;
use renderpilot_storage_sqlite::SqliteStorage;

use crate::Context;
use crate::catalog::execute::{apply_swap, rollback_component};

use super::fs_ops::{perform_apply_fs, revert_to_baseline_fs};
use super::planning::{fsr_members_to_remove, planned_target_files};
use super::types::{PlannedFile, PreparedApplySwap, PreparedD3d12Execution};

const HEX64: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn comp_file(path: &Path) -> ComponentFile {
    ComponentFile::new(PathRef::new(path.to_string_lossy().as_ref()).expect("valid path"))
}

fn comp_file_str(path: &str) -> ComponentFile {
    ComponentFile::new(PathRef::new(path).expect("valid path"))
}

fn bak_of(path: &Path) -> PathBuf {
    crate::fs::backup_path(path).expect("test paths include a file name")
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
fn placeholder_component() -> LibraryComponent {
    LibraryComponent::new(
        ComponentId::new("component:test").expect("component id"),
        GameId::new("manual:C:/Games/Test").expect("game id"),
        ComponentKind::NativeLibrary,
        LibraryTechnology::AmdFsr,
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
    let baseline = vec![comp_file(&target).with_sha256(sha_of(&target))];
    let changes = perform_apply_fs(&placeholder_component(), &baseline, &plans, &[])
        .expect("apply should succeed");

    assert_eq!(fs::read(&target).expect("target readable"), b"new-version");
    assert_eq!(
        fs::read(bak_of(&target)).expect("bak readable"),
        b"original"
    );
    assert_eq!(changes.copied, vec![target.clone()]);
    assert_eq!(
        changes.created_sidecars,
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
    let changes =
        perform_apply_fs(&placeholder_component(), &[], &plans, &[]).expect("apply should succeed");

    assert_eq!(fs::read(&target).expect("target readable"), b"fresh");
    assert!(
        !bak_of(&target).exists(),
        "no backup for a newly added file"
    );
    assert!(changes.created_sidecars.is_empty());
}

#[test]
fn removed_member_is_backed_up_then_deleted() {
    let dir = tempfile::tempdir().expect("temp dir");
    let member = dir.path().join("amd_fidelityfx_framegeneration_dx12.dll");
    write(&member, b"fsr4-member");

    let member_file = comp_file(&member).with_sha256(sha_of(&member));
    let removed = vec![member_file.clone()];
    let component = placeholder_component().with_file(member_file.clone());
    let changes =
        perform_apply_fs(&component, &[member_file], &[], &removed).expect("apply should succeed");

    assert!(!member.exists(), "removed member should be gone");
    assert_eq!(
        fs::read(bak_of(&member)).expect("bak readable"),
        b"fsr4-member",
        "removed member must be preserved as a .bak for rollback"
    );
    assert_eq!(
        changes.created_sidecars,
        vec![(member.clone(), bak_of(&member))]
    );
}

#[test]
fn dxc_pair_failure_midway_rolls_back_both_members() {
    let dir = tempfile::tempdir().expect("temp dir");
    let compiler = dir.path().join("dxcompiler.dll");
    let validator = dir.path().join("dxil.dll");
    let good_source = dir.path().join("new-dxcompiler.dll");
    let missing_source = dir.path().join("missing-dxil.dll");
    write(&compiler, b"original-compiler");
    write(&validator, b"original-validator");
    write(&good_source, b"new-compiler");

    let plans = vec![
        planned_copy(&good_source, &compiler),
        planned_copy(&missing_source, &validator),
    ];
    let baseline = vec![
        comp_file(&compiler).with_sha256(sha_of(&compiler)),
        comp_file(&validator).with_sha256(sha_of(&validator)),
    ];
    let component = LibraryComponent::new(
        ComponentId::new("component:dxc-atomicity").expect("component id"),
        GameId::new("manual:dxc-atomicity").expect("game id"),
        ComponentKind::NativeLibrary,
        LibraryTechnology::MicrosoftDxc,
        Swappability::BundleOnly,
    )
    .with_file(baseline[0].clone())
    .with_file(baseline[1].clone());
    let context = crate::Context::from_storage(SqliteStorage::in_memory().expect("storage"));
    let game_id =
        GameId::new(format!("manual:apply-failure-{}", ulid::Ulid::generate())).expect("game id");
    let guard = crate::game_mutation_lock::blocking_lock(&game_id);
    let mutation = crate::file_mutation::DurableFileTransaction::prepare(
        &context,
        &guard,
        &crate::file_mutation::MutationScope::single(dir.path()).expect("scope"),
        "test_apply_failure",
        None,
        [
            compiler.clone(),
            bak_of(&compiler),
            validator.clone(),
            bak_of(&validator),
        ],
    )
    .expect("durable transaction");
    let result = perform_apply_fs(&component, &baseline, &plans, &[]);

    assert!(result.is_err(), "missing source must fail the apply");
    mutation
        .rollback(context.storage())
        .expect("durable rollback");
    assert_eq!(
        fs::read(&compiler).expect("compiler readable"),
        b"original-compiler",
        "the compiler must be restored to its original bytes"
    );
    assert_eq!(
        fs::read(&validator).expect("validator readable"),
        b"original-validator",
        "the validator must be restored to its original bytes"
    );
    assert!(
        !bak_of(&compiler).exists() && !bak_of(&validator).exists(),
        "both recovery sidecars must be consumed by the restore"
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
    let baseline = vec![comp_file(&replaced).with_sha256(sha_of(&bak_of(&replaced)))];
    revert_to_baseline_fs(&current, &baseline).expect("revert should succeed");

    assert_eq!(fs::read(&replaced).expect("readable"), b"original");
    assert!(!bak_of(&replaced).exists(), "backup consumed by restore");
    assert!(!added.exists(), "overlay-added file removed on revert");
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
        LibraryTechnology::AmdFsr,
        "amd_fidelityfx_dx12.dll",
        vec![
            comp_file_str("C:/lib/amd_fidelityfx_dx12.dll")
                .with_sha256(Sha256Hash::new(HEX64).expect("sha")),
        ],
        ArtifactTrustLevel::CatalogDownloaded,
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
        LibraryTechnology::AmdFsr,
        "amd_fidelityfx_dx12.dll",
        vec![
            comp_file_str("C:/lib/amd_fidelityfx_dx12.dll")
                .with_sha256(Sha256Hash::new(HEX64).expect("sha")),
        ],
        ArtifactTrustLevel::CatalogDownloaded,
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

/// Planning helpers for Streamline path building; these assert that the shared
/// application transition policy is wired into `planned_target_files`.
fn streamline_component(installed_names: &[&str]) -> LibraryComponent {
    let mut component = LibraryComponent::new(
        ComponentId::new("component:streamline").expect("id"),
        GameId::new("manual:C:/Games/Test").expect("game id"),
        ComponentKind::NativeLibrary,
        LibraryTechnology::NvidiaStreamline,
        Swappability::BundleOnly,
    );
    for name in installed_names {
        component = component.with_file(comp_file_str(&format!("C:/game/{name}")));
    }
    component
}

fn streamline_package(member_names: &[&str]) -> LibraryArtifact {
    let files: Vec<_> = member_names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let sha = char::from(b'a' + index as u8).to_string().repeat(64);
            ComponentFile::new(PathRef::new(format!("C:/lib/{name}")).expect("path"))
                .with_sha256(Sha256Hash::new(sha).expect("sha"))
                .with_version(Version::parse("2.9.0.0").expect("version"))
        })
        .collect();
    LibraryArtifact::new(
        ArtifactId::new("artifact:sl-pkg").expect("id"),
        LibraryTechnology::NvidiaStreamline,
        member_names[0],
        files,
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("package")
}

#[test]
fn applied_path_uses_the_selected_renamed_member_after_projection() {
    let skipped_path = Path::new("C:/lib/skipped.dll");
    let selected_path = Path::new("C:/lib/selected.dll");
    let target_path = Path::new("C:/game/primary.dll");
    let artifact = LibraryArtifact::new(
        ArtifactId::new("artifact:projected-renamed").expect("id"),
        LibraryTechnology::NvidiaStreamline,
        "skipped.dll",
        vec![
            comp_file(skipped_path).with_sha256(Sha256Hash::new(HEX64).expect("sha")),
            comp_file(selected_path)
                .with_sha256(Sha256Hash::new("b".repeat(64)).expect("sha"))
                .with_install_as("primary.dll"),
        ],
        ArtifactTrustLevel::LocalObserved,
    )
    .expect("artifact");
    let component = streamline_component(&["primary.dll"]);
    let prepared = PreparedApplySwap {
        game_id: component.game_id().clone(),
        component_id: component.id().clone(),
        component,
        artifact,
        baseline: Vec::new(),
        rollback_baseline: None,
        planned: vec![planned_copy(selected_path, target_path)],
        removed: Vec::new(),
        first_swap: true,
        d3d12: None,
    };

    assert_eq!(prepared.applied_path(), target_path.to_string_lossy());
}

#[test]
fn silent_d3d12_repatch_snapshots_only_the_mutated_executable() {
    let (prepared, executable, backup) = prepared_d3d12_path_swap(619, 618, true);

    let action = &prepared.d3d12.as_ref().expect("D3D12 action").action;
    assert!(action.changes_executable());
    assert!(!action.requires_confirmation());

    let paths = super::apply_mutation_paths(&prepared);
    assert!(paths.contains(&executable));
    assert!(
        !paths.contains(&backup),
        "an existing immutable backup is read-only during apply"
    );
}

#[test]
fn first_d3d12_patch_tracks_the_backup_path_created_by_the_transaction() {
    let (prepared, executable, backup) = prepared_d3d12_path_swap(606, 619, false);

    let action = &prepared.d3d12.as_ref().expect("D3D12 action").action;
    assert!(action.changes_executable());
    assert!(action.requires_confirmation());

    let paths = super::apply_mutation_paths(&prepared);
    assert!(paths.contains(&executable));
    assert!(
        paths.contains(&backup),
        "recovery must remove a sidecar created by the first patch"
    );
}

fn prepared_d3d12_path_swap(
    current_sdk_version: u32,
    target_sdk_version: u32,
    backup_exists: bool,
) -> (PreparedApplySwap, PathBuf, PathBuf) {
    let executable = PathBuf::from("C:/game/game.exe");
    let backup = PathBuf::from("C:/game/game.exe.bak");
    let original_hash = Sha256Hash::new("a".repeat(64)).expect("original hash");
    let current_hash = if current_sdk_version == 606 {
        original_hash.clone()
    } else {
        Sha256Hash::new("b".repeat(64)).expect("current hash")
    };
    let action = D3d12ExecutableAction::for_swap(
        &D3d12ExecutableProfile::new(
            path_as_ref(&executable),
            path_as_ref(&backup),
            606,
            current_sdk_version,
            backup_exists,
            false,
        ),
        target_sdk_version,
    )
    .expect("D3D12 action");

    let component = LibraryComponent::new(
        ComponentId::new("component:d3d12-paths").expect("component id"),
        GameId::new("manual:C:/game").expect("game id"),
        ComponentKind::NativeLibrary,
        LibraryTechnology::D3D12Agility,
        Swappability::Swappable,
    );
    let artifact = LibraryArtifact::new(
        ArtifactId::new("artifact:d3d12-paths").expect("artifact id"),
        LibraryTechnology::D3D12Agility,
        "D3D12Core.dll",
        vec![
            comp_file_str("C:/library/D3D12Core.dll")
                .with_sha256(Sha256Hash::new("c".repeat(64)).expect("artifact hash")),
        ],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("artifact");
    let prepared = PreparedApplySwap {
        game_id: component.game_id().clone(),
        component_id: component.id().clone(),
        component,
        artifact,
        baseline: Vec::new(),
        rollback_baseline: None,
        planned: Vec::new(),
        removed: Vec::new(),
        first_swap: !backup_exists,
        d3d12: Some(PreparedD3d12Execution {
            state: crate::catalog::runtime_compatibility::D3d12ExecutableState {
                executable_path: executable.clone(),
                backup_path: backup.clone(),
                original_sha256: original_hash,
                current_sha256: current_hash,
                original_sdk_version: 606,
                current_sdk_version,
                backup_exists,
                repair_required: false,
            },
            action,
            confirmation_token: "test-token".to_owned(),
        }),
    };

    (prepared, executable, backup)
}

#[test]
fn planned_streamline_targets_intersect_package_with_installed_set() {
    let component = streamline_component(&["sl.common.dll", "sl.interposer.dll"]);
    let artifact = streamline_package(&["sl.common.dll", "sl.dlss.dll", "sl.interposer.dll"]);

    let planned = planned_target_files(&artifact, Path::new("C:/game"), &component)
        .expect("plan should succeed");
    let names: Vec<&str> = planned
        .iter()
        .filter_map(|p| p.file.path().file_name())
        .collect();
    assert_eq!(
        names,
        vec!["sl.common.dll", "sl.interposer.dll"],
        "optional SDK members must not expand the game's plugin set"
    );
}

#[test]
fn planned_streamline_empty_overlap_is_an_error() {
    let component = streamline_component(&["sl.common.dll"]);
    let artifact = streamline_package(&["sl.dlss.dll", "sl.reflex.dll"]);

    match planned_target_files(&artifact, Path::new("C:/game"), &component) {
        Err(err) => assert!(
            err.message().contains("no installable files"),
            "unexpected error: {}",
            err.message()
        ),
        Ok(_) => panic!("expected empty intersection to fail"),
    }
}

#[test]
fn planned_non_streamline_multi_file_package_keeps_all_members() {
    let component = LibraryComponent::new(
        ComponentId::new("component:fsr").expect("id"),
        GameId::new("manual:C:/Games/Test").expect("game id"),
        ComponentKind::NativeLibrary,
        LibraryTechnology::AmdFsr,
        Swappability::BundleOnly,
    )
    .with_file(comp_file_str("C:/game/amd_fidelityfx_dx12.dll"));

    let artifact = LibraryArtifact::new(
        ArtifactId::new("artifact:fsr-pkg").expect("id"),
        LibraryTechnology::AmdFsr,
        "amd_fidelityfx_upscaler_dx12.dll",
        vec![
            comp_file_str("C:/lib/amd_fidelityfx_upscaler_dx12.dll")
                .with_sha256(Sha256Hash::new("a".repeat(64)).expect("sha")),
            comp_file_str("C:/lib/amd_fidelityfx_loader_dx12.dll")
                .with_sha256(Sha256Hash::new("b".repeat(64)).expect("sha")),
        ],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("artifact");

    let planned = planned_target_files(&artifact, Path::new("C:/game"), &component)
        .expect("non-streamline multi-file plans all members");
    assert_eq!(planned.len(), 2);
}

// ---------------------------------------------------------------------------
// Integration: full apply_swap + prepare_apply_swap (ship-quality locks)
// ---------------------------------------------------------------------------

fn path_as_ref(path: &Path) -> PathRef {
    PathRef::new(path.to_string_lossy().as_ref()).expect("path ref")
}

fn sha_of(path: &Path) -> Sha256Hash {
    renderpilot_detection::sha256_file(path).expect("hash file")
}

fn sample_game_at(install: &Path) -> GameInstallation {
    let install_str = install.to_string_lossy();
    let id = format!("manual:{install_str}");
    let identity = GameIdentity::new(
        GameId::new(id).expect("game id"),
        "Test Game",
        Launcher::Manual,
    )
    .expect("identity");
    GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        path_as_ref(install),
    )
}

fn d3d12_artifact_at(source: &Path, sdk_line: u32) -> LibraryArtifact {
    let source_hash = sha_of(source);
    let package_version = format!("1.{sdk_line}.1");
    LibraryArtifact::new(
        ArtifactId::for_bundle([&source_hash]),
        LibraryTechnology::D3D12Agility,
        "D3D12Core.dll",
        vec![
            ComponentFile::new(path_as_ref(source))
                .with_sha256(source_hash)
                .with_version(Version::parse(&package_version).expect("version")),
        ],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("D3D12 artifact")
    .with_source("test-library")
    .expect("source")
    .with_metadata(
        ArtifactMetadata::default()
            .with_upstream_package(
                UpstreamPackage::new(
                    UpstreamPackageProvider::NuGet,
                    "Microsoft.Direct3D.D3D12",
                    &package_version,
                )
                .expect("package"),
            )
            .with_runtime_target(
                RuntimeTarget::new(Architecture::X64)
                    .with_compatibility(RuntimeCompatibility::D3d12Sdk { version: sdk_line }),
            ),
    )
}

#[cfg(windows)]
fn copy_current_executable_to(destinations: &[&Path]) {
    let process_image = std::env::current_exe().expect("current test executable");
    for destination in destinations {
        fs::copy(&process_image, destination).expect("copy PE fixture");
    }
}

#[cfg(windows)]
fn dxc_package_at(
    compiler_path: &Path,
    validator_path: &Path,
    architecture: Architecture,
) -> LibraryArtifact {
    const VERSION: &str = "1.8.2505.28";

    let compiler_hash = sha_of(compiler_path);
    let validator_hash = sha_of(validator_path);
    LibraryArtifact::new(
        ArtifactId::for_bundle([&compiler_hash, &validator_hash]),
        LibraryTechnology::MicrosoftDxc,
        "dxcompiler.dll",
        vec![
            ComponentFile::new(path_as_ref(compiler_path))
                .with_sha256(compiler_hash)
                .with_version(Version::parse(VERSION).expect("compiler version")),
            ComponentFile::new(path_as_ref(validator_path))
                .with_sha256(validator_hash)
                .with_version(Version::parse(VERSION).expect("validator version")),
        ],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("DXC artifact")
    .with_source("test-library")
    .expect("source")
    .with_metadata(
        ArtifactMetadata::default()
            .with_release(Version::parse(VERSION).expect("release version"), None)
            .expect("release")
            .with_upstream_package(
                UpstreamPackage::new(
                    UpstreamPackageProvider::NuGet,
                    "Microsoft.Direct3D.DXC",
                    VERSION,
                )
                .expect("package"),
            )
            .with_runtime_target(RuntimeTarget::new(architecture)),
    )
}

#[cfg(windows)]
#[test]
fn standalone_dxc_apply_and_rollback_preserve_the_games_file_set() {
    use std::io::Write as _;

    let root = tempfile::tempdir().expect("root");
    let game_dir = root.path().join("game");
    let library_dir = root.path().join("library");
    fs::create_dir_all(&game_dir).expect("game dir");
    fs::create_dir_all(&library_dir).expect("library dir");

    let game_executable = game_dir.join("game.exe");
    let live_compiler = game_dir.join("dxcompiler.dll");
    let source_compiler = library_dir.join("dxcompiler.dll");
    let source_validator = library_dir.join("dxil.dll");
    copy_current_executable_to(&[
        &game_executable,
        &live_compiler,
        &source_compiler,
        &source_validator,
    ]);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&live_compiler)
        .expect("open installed compiler")
        .write_all(b"standalone-installed-version")
        .expect("differentiate installed compiler");
    let original_bytes = fs::read(&live_compiler).expect("installed compiler bytes");

    let architecture = renderpilot_detection::inspect_pe(&source_compiler)
        .and_then(|inspection| inspection.architecture)
        .expect("test executable architecture");
    let game = sample_game_at(&game_dir);
    let component_id = ComponentId::new("component:dxc-standalone").expect("component");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::MicrosoftDxc,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_as_ref(&live_compiler))
            .with_sha256(sha_of(&live_compiler))
            .with_version(Version::parse("1.5.0.0").expect("version")),
    );

    let artifact = dxc_package_at(&source_compiler, &source_validator, architecture);

    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");
    storage
        .replace_components_for_game(game.id(), &[component])
        .expect("component");
    storage.upsert_artifact(&artifact).expect("artifact");
    let context = Context::from_storage(storage);

    let apply = apply_swap(&context, game.id(), &component_id, artifact.id())
        .expect("standalone DXC apply");
    assert_eq!(apply.updated_file_count, 1);
    assert_eq!(
        fs::read(&live_compiler).expect("updated compiler"),
        fs::read(&source_compiler).expect("source compiler")
    );
    assert!(
        !game_dir.join("dxil.dll").exists(),
        "a package-only validator must not be added to a standalone integration"
    );
    let stored = context
        .storage()
        .list_components_for_game(game.id())
        .expect("stored component");
    assert_eq!(stored[0].files().len(), 1);

    let rollback =
        rollback_component(&context, game.id(), &component_id).expect("standalone DXC rollback");
    assert_eq!(rollback.restored_file_count, 1);
    assert_eq!(
        fs::read(&live_compiler).expect("restored compiler"),
        original_bytes
    );
    assert!(!game_dir.join("dxil.dll").exists());
}

#[test]
fn d3d12_missing_executable_facts_are_blocked_at_plan_and_apply_boundaries() {
    let root = tempfile::tempdir().expect("root");
    let game_dir = root.path().join("game");
    let library_dir = root.path().join("library");
    fs::create_dir_all(&game_dir).expect("game dir");
    fs::create_dir_all(&library_dir).expect("library dir");

    let live = game_dir.join("D3D12Core.dll");
    let source = library_dir.join("D3D12Core.dll");
    write(&live, b"original-d3d12-core");
    write(&source, b"replacement-d3d12-core");

    let game = sample_game_at(&game_dir);
    let component_id = ComponentId::new("component:d3d12-policy").expect("component id");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::D3D12Agility,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_as_ref(&live))
            .with_sha256(sha_of(&live))
            .with_version(Version::parse("1.618.3").expect("version")),
    );

    let source_hash = sha_of(&source);
    let artifact = LibraryArtifact::new(
        ArtifactId::for_bundle([&source_hash]),
        LibraryTechnology::D3D12Agility,
        "D3D12Core.dll",
        vec![
            ComponentFile::new(path_as_ref(&source))
                .with_sha256(source_hash)
                .with_version(Version::parse("1.618.5").expect("version")),
        ],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("artifact")
    .with_source("test-library")
    .expect("source")
    .with_metadata(
        ArtifactMetadata::default()
            .with_upstream_package(
                UpstreamPackage::new(
                    UpstreamPackageProvider::NuGet,
                    "Microsoft.Direct3D.D3D12",
                    "1.618.5",
                )
                .expect("package"),
            )
            .with_runtime_target(
                RuntimeTarget::new(Architecture::X64)
                    .with_compatibility(RuntimeCompatibility::D3d12Sdk { version: 618 }),
            ),
    );

    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");
    storage
        .replace_components_for_game(game.id(), &[component])
        .expect("component");
    storage.upsert_artifact(&artifact).expect("artifact");
    let context = Context::from_storage(storage);

    let plan_error =
        crate::catalog::build_swap_plan(&context, game.id(), &component_id, artifact.id())
            .err()
            .expect("plan must fail without an unambiguous executable");
    assert!(matches!(plan_error, crate::ServiceError::InvalidInput(_)));

    let apply_error = apply_swap(&context, game.id(), &component_id, artifact.id())
        .expect_err("direct apply must enforce the same policy");
    assert!(matches!(apply_error, crate::ServiceError::InvalidInput(_)));
    assert_eq!(
        fs::read(&live).expect("live core"),
        b"original-d3d12-core",
        "policy rejection must happen before the first file mutation"
    );
    assert!(
        !bak_of(&live).exists(),
        "no recovery sidecar may be created"
    );
}

#[test]
fn patched_executable_backup_without_dll_backup_is_never_captured_as_a_mixed_baseline() {
    let root = tempfile::tempdir().expect("root");
    let game_dir = root.path().join("game");
    let library_dir = root.path().join("library");
    fs::create_dir_all(&game_dir).expect("game dir");
    fs::create_dir_all(&library_dir).expect("library dir");

    let executable = game_dir.join("game.exe");
    let executable_backup = bak_of(&executable);
    let live_dll = game_dir.join("D3D12Core.dll");
    let source_dll = library_dir.join("D3D12Core.dll");
    let original_executable =
        crate::catalog::runtime_compatibility::synthetic_d3d12_executable(606);
    let patched_executable = crate::catalog::runtime_compatibility::synthetic_d3d12_executable(619);
    write(&executable, &patched_executable);
    write(&executable_backup, &original_executable);
    write(&live_dll, b"manually-installed-sdk-619-runtime");
    write(
        &source_dll,
        &crate::catalog::runtime_compatibility::synthetic_d3d12_executable(620),
    );

    let game = sample_game_at(&game_dir).with_executable_candidate(path_as_ref(&executable));
    let component_id = ComponentId::new("component:d3d12-orphan-exe").expect("component id");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::D3D12Agility,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_as_ref(&live_dll))
            .with_sha256(sha_of(&live_dll))
            .with_version(Version::parse("1.619.1").expect("version")),
    );
    let artifact = d3d12_artifact_at(&source_dll, 620);
    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");
    storage
        .replace_components_for_game(game.id(), &[component])
        .expect("component");
    storage.upsert_artifact(&artifact).expect("artifact");
    let context = Context::from_storage(storage);

    let plan = crate::catalog::build_swap_plan(&context, game.id(), &component_id, artifact.id())
        .expect("repair plan");
    assert!(
        plan.plan
            .blockers()
            .iter()
            .any(|blocker| blocker.as_str() == "d3d12_executable_repair_required")
    );

    let error = apply_swap(&context, game.id(), &component_id, artifact.id())
        .expect_err("mixed baseline must be blocked");
    assert!(matches!(error, crate::ServiceError::InvalidInput(_)));
    assert_eq!(fs::read(&executable).expect("live EXE"), patched_executable);
    assert_eq!(
        fs::read(&executable_backup).expect("EXE backup"),
        original_executable
    );
    assert_eq!(
        fs::read(&live_dll).expect("live DLL"),
        b"manually-installed-sdk-619-runtime"
    );
    assert!(
        !bak_of(&live_dll).exists(),
        "a rejected operation must not manufacture a DLL backup"
    );
    assert!(
        context
            .storage()
            .get_component_backup(&component_id)
            .expect("baseline lookup")
            .is_none(),
        "a rejected operation must not persist a mixed rollback aggregate"
    );
}

#[test]
fn confirmed_d3d12_apply_reports_token_mismatch_when_the_live_dll_changes() {
    let root = tempfile::tempdir().expect("root");
    let game_dir = root.path().join("game");
    let library_dir = root.path().join("library");
    fs::create_dir_all(&game_dir).expect("game dir");
    fs::create_dir_all(&library_dir).expect("library dir");

    let executable = game_dir.join("game.exe");
    let live_dll = game_dir.join("D3D12Core.dll");
    let source_dll = library_dir.join("D3D12Core.dll");
    let original_executable =
        crate::catalog::runtime_compatibility::synthetic_d3d12_executable(606);
    write(&executable, &original_executable);
    write(&live_dll, b"scanned-sdk-606-runtime");
    write(
        &source_dll,
        &crate::catalog::runtime_compatibility::synthetic_d3d12_executable(619),
    );

    let game = sample_game_at(&game_dir).with_executable_candidate(path_as_ref(&executable));
    let component_id = ComponentId::new("component:d3d12-stale-apply").expect("component");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::D3D12Agility,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_as_ref(&live_dll))
            .with_sha256(sha_of(&live_dll))
            .with_version(Version::parse("1.606.1").expect("version")),
    );
    let artifact = d3d12_artifact_at(&source_dll, 619);
    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");
    storage
        .replace_components_for_game(game.id(), std::slice::from_ref(&component))
        .expect("component");
    storage.upsert_artifact(&artifact).expect("artifact");
    let context = Context::from_storage(storage);
    let target =
        crate::catalog::runtime_compatibility::target_profile(&context, &game, Some(&component))
            .expect("target profile");
    let action = renderpilot_application::replacement_executable_action(&artifact, &target.profile)
        .expect("policy")
        .expect("action");
    let token = renderpilot_application::d3d12_confirmation_token(
        &component,
        &artifact,
        &target.profile,
        &action,
    )
    .expect("token");

    write(&live_dll, b"externally-changed-runtime");
    let error = crate::catalog::apply_swap_confirmed(
        &context,
        game.id(),
        &component_id,
        artifact.id(),
        Some(&token),
    )
    .expect_err("stale confirmation must fail");

    assert!(matches!(
        error,
        crate::ServiceError::ConfirmationTokenMismatch
    ));
    assert_eq!(
        fs::read(&executable).expect("unchanged EXE"),
        original_executable
    );
    assert!(!bak_of(&executable).exists());
    assert!(!bak_of(&live_dll).exists());
}

#[test]
fn d3d12_preview_apply_rechecks_developer_mode_after_a_successful_plan() {
    let root = tempfile::tempdir().expect("root");
    let game_dir = root.path().join("game");
    let library_dir = root.path().join("library");
    fs::create_dir_all(&game_dir).expect("game dir");
    fs::create_dir_all(&library_dir).expect("library dir");

    let executable = game_dir.join("game.exe");
    let live_dll = game_dir.join("D3D12Core.dll");
    let source_dll = library_dir.join("D3D12Core.dll");
    let original_executable =
        crate::catalog::runtime_compatibility::synthetic_d3d12_executable(606);
    write(&executable, &original_executable);
    write(&live_dll, b"scanned-sdk-606-runtime");
    write(
        &source_dll,
        &crate::catalog::test_support::synthetic_versioned_d3d12_runtime(619),
    );
    let original_runtime = fs::read(&live_dll).expect("runtime fixture");

    let game = sample_game_at(&game_dir).with_executable_candidate(path_as_ref(&executable));
    let component_id = ComponentId::new("component:d3d12-developer-mode").expect("component");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::D3D12Agility,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_as_ref(&live_dll))
            .with_sha256(sha_of(&live_dll))
            .with_version(Version::parse("1.606.1").expect("version")),
    );
    let artifact = crate::catalog::test_support::d3d12_preview_artifact_at(&source_dll, 619);
    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");
    storage
        .replace_components_for_game(game.id(), std::slice::from_ref(&component))
        .expect("component");
    storage.upsert_artifact(&artifact).expect("artifact");

    let developer_mode_enabled = Arc::new(AtomicBool::new(true));
    let status = Arc::clone(&developer_mode_enabled);
    let context = Context::from_storage(storage).with_developer_mode_status_provider(move || {
        if status.load(Ordering::SeqCst) {
            DeveloperModeStatus::Enabled
        } else {
            DeveloperModeStatus::Disabled
        }
    });

    let plan = crate::catalog::build_swap_plan(&context, game.id(), &component_id, artifact.id())
        .expect("enabled Developer Mode should allow planning");
    assert!(plan.plan.blockers().is_empty());
    let token = plan.plan.confirmation_token().to_owned();
    assert!(!token.is_empty());

    developer_mode_enabled.store(false, Ordering::SeqCst);
    let error = crate::catalog::apply_swap_confirmed(
        &context,
        game.id(),
        &component_id,
        artifact.id(),
        Some(&token),
    )
    .expect_err("apply must recheck Developer Mode");

    match error {
        crate::ServiceError::InvalidInput(message) => {
            assert!(message.contains("developer_mode_required"));
        }
        other => panic!("unexpected error: {other}"),
    }
    assert_eq!(
        fs::read(&executable).expect("unchanged executable"),
        original_executable
    );
    assert_eq!(
        fs::read(&live_dll).expect("unchanged runtime"),
        original_runtime
    );
}

#[test]
fn confirmed_first_d3d12_apply_reports_token_mismatch_when_the_executable_changes_or_disappears() {
    let root = tempfile::tempdir().expect("root");
    let game_dir = root.path().join("game");
    let library_dir = root.path().join("library");
    fs::create_dir_all(&game_dir).expect("game dir");
    fs::create_dir_all(&library_dir).expect("library dir");

    let executable = game_dir.join("game.exe");
    let live_dll = game_dir.join("D3D12Core.dll");
    let source_dll = library_dir.join("D3D12Core.dll");
    write(
        &executable,
        &crate::catalog::runtime_compatibility::synthetic_d3d12_executable(606),
    );
    write(&live_dll, b"scanned-sdk-606-runtime");
    write(
        &source_dll,
        &crate::addons::test_support::build_nvidia_dlss_pe([1, 619, 1, 0]),
    );

    let game = sample_game_at(&game_dir).with_executable_candidate(path_as_ref(&executable));
    let component_id = ComponentId::new("component:d3d12-missing-after-plan").expect("component");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::D3D12Agility,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_as_ref(&live_dll))
            .with_sha256(sha_of(&live_dll))
            .with_version(Version::parse("1.606.1").expect("version")),
    );
    let artifact = d3d12_artifact_at(&source_dll, 619);
    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");
    storage
        .replace_components_for_game(game.id(), std::slice::from_ref(&component))
        .expect("component");
    storage.upsert_artifact(&artifact).expect("artifact");
    let context = Context::from_storage(storage);
    let plan = crate::catalog::build_swap_plan(&context, game.id(), &component_id, artifact.id())
        .expect("plan");
    let token = plan.plan.confirmation_token().to_owned();

    write(
        &executable,
        &crate::catalog::runtime_compatibility::synthetic_d3d12_executable(620),
    );
    let error = crate::catalog::apply_swap_confirmed(
        &context,
        game.id(),
        &component_id,
        artifact.id(),
        Some(&token),
    )
    .expect_err("an incompatible executable change must reject the confirmation");
    assert!(matches!(
        error,
        crate::ServiceError::ConfirmationTokenMismatch
    ));
    assert_eq!(
        fs::read(&live_dll).expect("unchanged DLL"),
        b"scanned-sdk-606-runtime"
    );
    assert!(!bak_of(&live_dll).exists());
    assert!(!bak_of(&executable).exists());

    fs::remove_file(&executable).expect("remove executable after planning");
    let error = crate::catalog::apply_swap_confirmed(
        &context,
        game.id(),
        &component_id,
        artifact.id(),
        Some(&token),
    )
    .expect_err("stale executable state must reject the confirmation");

    assert!(matches!(
        error,
        crate::ServiceError::ConfirmationTokenMismatch
    ));
    assert_eq!(
        fs::read(&live_dll).expect("unchanged DLL"),
        b"scanned-sdk-606-runtime"
    );
    assert!(!bak_of(&live_dll).exists());
}

#[test]
fn every_d3d12_apply_stage_rolls_back_dll_exe_sidecars_and_database_together() {
    use super::{D3d12ApplyFailurePoint, set_d3d12_apply_failure_point};

    let failure_points = [
        D3d12ApplyFailurePoint::AfterExecutableBackup,
        D3d12ApplyFailurePoint::AfterDllMutation,
        D3d12ApplyFailurePoint::AfterExecutableMutation,
        D3d12ApplyFailurePoint::BeforeDatabaseCommit,
    ];

    for (index, failure_point) in failure_points.into_iter().enumerate() {
        let root = tempfile::tempdir().expect("root");
        let game_dir = root.path().join("game");
        let library_dir = root.path().join("library");
        fs::create_dir_all(&game_dir).expect("game dir");
        fs::create_dir_all(&library_dir).expect("library dir");

        let executable = game_dir.join("game.exe");
        let live_dll = game_dir.join("D3D12Core.dll");
        let source_dll = library_dir.join("D3D12Core.dll");
        let original_executable =
            crate::catalog::runtime_compatibility::synthetic_d3d12_executable(606);
        let original_dll = b"original-sdk-606-runtime";
        write(&executable, &original_executable);
        write(&live_dll, original_dll);
        write(
            &source_dll,
            &crate::addons::test_support::build_nvidia_dlss_pe([1, 619, 1, 0]),
        );

        let game = sample_game_at(&game_dir).with_executable_candidate(path_as_ref(&executable));
        let component_id =
            ComponentId::new(format!("component:d3d12-failure-{index}")).expect("component");
        let original_component = LibraryComponent::new(
            component_id.clone(),
            game.id().clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::D3D12Agility,
            Swappability::Swappable,
        )
        .with_file(
            ComponentFile::new(path_as_ref(&live_dll))
                .with_sha256(sha_of(&live_dll))
                .with_version(Version::parse("1.606.1").expect("version")),
        );
        let artifact = d3d12_artifact_at(&source_dll, 619);
        let storage = SqliteStorage::in_memory().expect("storage");
        storage.upsert_game(&game).expect("game");
        storage
            .replace_components_for_game(game.id(), std::slice::from_ref(&original_component))
            .expect("component");
        storage.upsert_artifact(&artifact).expect("artifact");
        let context = Context::from_storage(storage);
        let plan =
            crate::catalog::build_swap_plan(&context, game.id(), &component_id, artifact.id())
                .expect("plan");
        let token = plan.plan.confirmation_token().to_owned();

        let _failure = set_d3d12_apply_failure_point(failure_point);
        crate::catalog::apply_swap_confirmed(
            &context,
            game.id(),
            &component_id,
            artifact.id(),
            Some(&token),
        )
        .expect_err("injected failure must abort the entire durable mutation");

        assert_eq!(
            fs::read(&live_dll).expect("live DLL"),
            original_dll,
            "DLL bytes were not restored after {failure_point:?}"
        );
        assert_eq!(
            fs::read(&executable).expect("live EXE"),
            original_executable,
            "EXE bytes were not restored after {failure_point:?}"
        );
        assert!(
            !bak_of(&live_dll).exists(),
            "DLL backup created by a failed first swap survived {failure_point:?}"
        );
        assert!(
            !bak_of(&executable).exists(),
            "EXE backup created by a failed first swap survived {failure_point:?}"
        );
        assert!(
            context
                .storage()
                .get_component_backup(&component_id)
                .expect("baseline lookup")
                .is_none(),
            "rollback aggregate was persisted after {failure_point:?}"
        );
        assert_eq!(
            context
                .storage()
                .list_components_for_game(game.id())
                .expect("stored components"),
            vec![original_component],
            "component catalog changed after {failure_point:?}"
        );
    }
}

#[test]
fn every_d3d12_rollback_stage_restores_the_active_state_and_keeps_retry_sidecars() {
    use super::{D3d12RollbackFailurePoint, set_d3d12_rollback_failure_point};

    let failure_points = [
        D3d12RollbackFailurePoint::AfterDllRestore,
        D3d12RollbackFailurePoint::AfterExecutableRestore,
        D3d12RollbackFailurePoint::AfterDllSidecarRelease,
        D3d12RollbackFailurePoint::AfterExecutableSidecarRelease,
        D3d12RollbackFailurePoint::BeforeDatabaseCommit,
    ];

    for (index, failure_point) in failure_points.into_iter().enumerate() {
        let root = tempfile::tempdir().expect("root");
        let game_dir = root.path().join("game");
        fs::create_dir_all(&game_dir).expect("game dir");
        let executable = game_dir.join("game.exe");
        let live_dll = game_dir.join("D3D12Core.dll");
        let executable_backup = bak_of(&executable);
        let dll_backup = bak_of(&live_dll);
        let original_executable =
            crate::catalog::runtime_compatibility::synthetic_d3d12_executable(606);
        let active_executable =
            crate::catalog::runtime_compatibility::synthetic_d3d12_executable(619);
        let original_dll = b"original-sdk-606-runtime";
        let active_dll = b"active-sdk-619-runtime";
        write(&executable, &active_executable);
        write(&executable_backup, &original_executable);
        write(&live_dll, active_dll);
        write(&dll_backup, original_dll);

        let game = sample_game_at(&game_dir).with_executable_candidate(path_as_ref(&executable));
        let component_id = ComponentId::new(format!("component:d3d12-rollback-failure-{index}"))
            .expect("component");
        let active_component = LibraryComponent::new(
            component_id.clone(),
            game.id().clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::D3D12Agility,
            Swappability::Swappable,
        )
        .with_file(
            ComponentFile::new(path_as_ref(&live_dll))
                .with_sha256(sha_of(&live_dll))
                .with_version(Version::parse("1.619.1").expect("version")),
        );
        let rollback_baseline = ComponentRollbackBaseline::new(vec![
            ComponentFile::new(path_as_ref(&live_dll))
                .with_sha256(sha_of(&dll_backup))
                .with_version(Version::parse("1.606.1").expect("version")),
        ])
        .with_d3d12_executable(D3d12ExecutableBaseline::new(
            path_as_ref(&executable),
            D3d12ExecutableIdentity::new(
                606,
                renderpilot_detection::sha256_bytes(&original_executable)
                    .expect("original EXE hash"),
            ),
            D3d12ExecutableIdentity::new(
                619,
                renderpilot_detection::sha256_bytes(&active_executable).expect("active EXE hash"),
            ),
        ));
        let storage = SqliteStorage::in_memory().expect("storage");
        storage.upsert_game(&game).expect("game");
        storage
            .replace_components_for_game(game.id(), std::slice::from_ref(&active_component))
            .expect("component");
        storage
            .recover_component_rollback_baseline(game.id(), &component_id, &rollback_baseline)
            .expect("rollback aggregate");
        let context = Context::from_storage(storage);
        let plan = crate::catalog::build_rollback_plan(&context, game.id(), &component_id)
            .expect("rollback plan");
        let action = plan.d3d12_executable_action().expect("EXE restore");
        assert!(!action.requires_confirmation());

        let _failure = set_d3d12_rollback_failure_point(failure_point);
        crate::catalog::rollback_component(&context, game.id(), &component_id)
            .expect_err("injected failure must abort the entire durable rollback");

        assert_eq!(
            fs::read(&live_dll).expect("live DLL"),
            active_dll,
            "active DLL bytes were not restored after {failure_point:?}"
        );
        assert_eq!(
            fs::read(&executable).expect("live EXE"),
            active_executable,
            "active EXE bytes were not restored after {failure_point:?}"
        );
        assert_eq!(
            fs::read(&dll_backup).expect("DLL backup"),
            original_dll,
            "retryable DLL sidecar was lost after {failure_point:?}"
        );
        assert_eq!(
            fs::read(&executable_backup).expect("EXE backup"),
            original_executable,
            "retryable EXE sidecar was lost after {failure_point:?}"
        );
        assert_eq!(
            context
                .storage()
                .get_component_backup(&component_id)
                .expect("baseline lookup"),
            Some(rollback_baseline),
            "rollback aggregate changed after {failure_point:?}"
        );
        assert_eq!(
            context
                .storage()
                .list_components_for_game(game.id())
                .expect("stored components"),
            vec![active_component],
            "component catalog changed after {failure_point:?}"
        );
    }
}

#[test]
fn d3d12_rollback_revalidates_live_dll_without_user_confirmation() {
    let root = tempfile::tempdir().expect("root");
    let game_dir = root.path().join("game");
    fs::create_dir_all(&game_dir).expect("game dir");
    let executable = game_dir.join("game.exe");
    let live_dll = game_dir.join("D3D12Core.dll");
    let original_executable =
        crate::catalog::runtime_compatibility::synthetic_d3d12_executable(606);
    let active_executable = crate::catalog::runtime_compatibility::synthetic_d3d12_executable(619);
    write(&executable, &active_executable);
    write(&bak_of(&executable), &original_executable);
    write(&live_dll, b"active-sdk-619-runtime");
    write(&bak_of(&live_dll), b"original-sdk-606-runtime");

    let game = sample_game_at(&game_dir).with_executable_candidate(path_as_ref(&executable));
    let component_id = ComponentId::new("component:d3d12-stale-rollback").expect("component");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::D3D12Agility,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_as_ref(&live_dll))
            .with_sha256(sha_of(&live_dll))
            .with_version(Version::parse("1.619.1").expect("version")),
    );
    let rollback_baseline = ComponentRollbackBaseline::new(vec![
        ComponentFile::new(path_as_ref(&live_dll))
            .with_sha256(sha_of(&bak_of(&live_dll)))
            .with_version(Version::parse("1.606.1").expect("version")),
    ])
    .with_d3d12_executable(D3d12ExecutableBaseline::new(
        path_as_ref(&executable),
        D3d12ExecutableIdentity::new(
            606,
            renderpilot_detection::sha256_bytes(&original_executable).expect("original EXE hash"),
        ),
        D3d12ExecutableIdentity::new(
            619,
            renderpilot_detection::sha256_bytes(&active_executable).expect("active EXE hash"),
        ),
    ));
    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");
    storage
        .replace_components_for_game(game.id(), std::slice::from_ref(&component))
        .expect("component");
    storage
        .recover_component_rollback_baseline(game.id(), &component_id, &rollback_baseline)
        .expect("rollback aggregate");
    let context = Context::from_storage(storage);
    let plan = crate::catalog::build_rollback_plan(&context, game.id(), &component_id)
        .expect("rollback plan");
    let affected = plan
        .affected_files()
        .iter()
        .map(PathRef::as_str)
        .collect::<Vec<_>>();
    assert!(
        affected
            .iter()
            .any(|path| crate::paths::same_path(Path::new(path), &bak_of(&live_dll))),
        "DLL sidecar is missing from {affected:?}"
    );
    assert!(
        affected
            .iter()
            .any(|path| crate::paths::same_path(Path::new(path), &bak_of(&executable))),
        "EXE sidecar is missing from {affected:?}"
    );
    let action = plan.d3d12_executable_action().expect("EXE restore");
    assert!(!action.requires_confirmation());

    write(&live_dll, b"externally-changed-after-plan");
    crate::catalog::rollback_component(&context, game.id(), &component_id)
        .expect_err("changed live DLL must fail fresh rollback validation");
    assert_eq!(
        fs::read(&executable).expect("unchanged active EXE"),
        active_executable
    );
    assert_eq!(
        fs::read(bak_of(&executable)).expect("unchanged EXE backup"),
        original_executable
    );
}

#[test]
fn openvr_apply_reinspects_installed_dll_and_fails_before_mutation() {
    let root = tempfile::tempdir().expect("root");
    let game_dir = root.path().join("game");
    let library_dir = root.path().join("library");
    fs::create_dir_all(&game_dir).expect("game dir");
    fs::create_dir_all(&library_dir).expect("library dir");

    let live = game_dir.join("openvr_api.dll");
    let source = library_dir.join("openvr_api.dll");
    write(&live, b"malformed-installed-openvr");
    write(&source, b"candidate-openvr");

    let exports =
        PeExportSet::from_canonical_names(vec!["VR_InitInternal".into()]).expect("exports");
    let game = sample_game_at(&game_dir);
    let component_id = ComponentId::new("component:openvr-boundary").expect("component");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::OpenVr,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_as_ref(&live))
            .with_sha256(sha_of(&live))
            .with_pe_compatibility(PeCompatibilityProfile::new(
                Architecture::X64,
                exports.clone(),
            )),
    );

    let source_hash = sha_of(&source);
    let artifact = LibraryArtifact::new(
        ArtifactId::for_bundle([&source_hash]),
        LibraryTechnology::OpenVr,
        "openvr_api.dll",
        vec![
            ComponentFile::new(path_as_ref(&source))
                .with_sha256(source_hash)
                .with_install_as("openvr_api.dll")
                .with_pe_compatibility(PeCompatibilityProfile::new(Architecture::X64, exports)),
        ],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("artifact")
    .with_source("test-library")
    .expect("source")
    .with_metadata(
        ArtifactMetadata::default()
            .with_release_version(Version::parse("2.15.6").expect("version"))
            .with_upstream_package(
                UpstreamPackage::new(
                    UpstreamPackageProvider::GitHub,
                    "ValveSoftware/openvr",
                    "2.15.6",
                )
                .expect("package"),
            )
            .with_runtime_target(RuntimeTarget::new(Architecture::X64)),
    );

    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");
    storage
        .replace_components_for_game(game.id(), &[component])
        .expect("component");
    storage.upsert_artifact(&artifact).expect("artifact");
    let context = Context::from_storage(storage);

    assert!(
        crate::catalog::build_swap_plan(&context, game.id(), &component_id, artifact.id()).is_err(),
        "preview must reject stale scan metadata"
    );
    assert!(
        apply_swap(&context, game.id(), &component_id, artifact.id()).is_err(),
        "apply must repeat the same fail-closed check"
    );
    assert_eq!(
        fs::read(&live).expect("live"),
        b"malformed-installed-openvr"
    );
    assert!(
        !bak_of(&live).exists(),
        "rejection must precede disk writes"
    );
}

fn dlss_artifact(path: &Path, version: &str) -> LibraryArtifact {
    let sha = sha_of(path);
    LibraryArtifact::new(
        ArtifactId::for_bundle([&sha]),
        LibraryTechnology::DlssSuperResolution,
        "nvngx_dlss.dll",
        vec![
            ComponentFile::new(path_as_ref(path))
                .with_sha256(sha)
                .with_version(Version::parse(version).expect("version")),
        ],
        ArtifactTrustLevel::LocalObserved,
    )
    .expect("artifact")
    .with_source("test-library")
    .expect("source")
}

/// Shared DLSS catalog-swap fixture: temp game + library dirs, live DLL, component, context.
struct FreshDlssFixture {
    _root: tempfile::TempDir,
    library_dir: PathBuf,
    live: PathBuf,
    game: GameInstallation,
    component_id: ComponentId,
    context: Context,
}

fn fresh_dlss_fixture(component_suffix: &str, live_bytes: &[u8]) -> FreshDlssFixture {
    let root = tempfile::tempdir().expect("root");
    let game_dir = root.path().join("game");
    let library_dir = root.path().join("library");
    fs::create_dir_all(&game_dir).expect("game");
    fs::create_dir_all(&library_dir).expect("library");
    let live = game_dir.join("nvngx_dlss.dll");
    write(&live, live_bytes);
    let game = sample_game_at(&game_dir);
    let component_id =
        ComponentId::new(format!("component:{component_suffix}")).expect("component id");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
    )
    .with_file(ComponentFile::new(path_as_ref(&live)).with_sha256(sha_of(&live)));
    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");
    storage
        .replace_components_for_game(game.id(), &[component])
        .expect("component");
    let context = Context::from_storage(storage);
    FreshDlssFixture {
        _root: root,
        library_dir,
        live,
        game,
        component_id,
        context,
    }
}

fn write_library_dlss(library_dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = library_dir.join(name).join("nvngx_dlss.dll");
    fs::create_dir_all(path.parent().unwrap()).expect("library subdir");
    write(&path, bytes);
    path
}

#[test]
fn reswap_keeps_one_immutable_classic_baseline() {
    let fx = fresh_dlss_fixture("dlss-reswap", b"original-a");
    let original_hash = sha_of(&fx.live);
    let source_b = write_library_dlss(&fx.library_dir, "b", b"replacement-b");
    let source_c = write_library_dlss(&fx.library_dir, "c", b"replacement-c");
    let b = dlss_artifact(&source_b, "3.5.0.0");
    let c = dlss_artifact(&source_c, "3.7.0.0");
    fx.context.storage().upsert_artifact(&b).expect("b");
    fx.context.storage().upsert_artifact(&c).expect("c");

    apply_swap(&fx.context, fx.game.id(), &fx.component_id, b.id()).expect("A to B");
    let sidecar = bak_of(&fx.live);
    assert_eq!(fs::read(&sidecar).unwrap(), b"original-a");
    apply_swap(&fx.context, fx.game.id(), &fx.component_id, c.id()).expect("B to C");

    assert_eq!(fs::read(&sidecar).unwrap(), b"original-a");
    assert_eq!(sha_of(&sidecar), original_hash);
    assert_eq!(
        fx.context
            .storage()
            .get_component_backup(&fx.component_id)
            .unwrap()
            .expect("baseline")
            .files()[0]
            .sha256(),
        Some(&original_hash)
    );
}

#[test]
fn reswap_does_not_rebase_a_record_whose_classic_sidecar_was_removed() {
    let fx = fresh_dlss_fixture("dlss-missing-sidecar", b"original-a");
    let original_hash = sha_of(&fx.live);
    let source_b = write_library_dlss(&fx.library_dir, "b", b"replacement-b");
    let source_c = write_library_dlss(&fx.library_dir, "c", b"replacement-c");
    let b = dlss_artifact(&source_b, "3.5.0.0");
    let c = dlss_artifact(&source_c, "3.7.0.0");
    fx.context.storage().upsert_artifact(&b).expect("b");
    fx.context.storage().upsert_artifact(&c).expect("c");

    apply_swap(&fx.context, fx.game.id(), &fx.component_id, b.id()).expect("A to B");
    let sidecar = bak_of(&fx.live);
    fs::remove_file(&sidecar).expect("manual backup removal");

    assert!(
        crate::catalog::backup_component_ids(&fx.context, fx.game.id())
            .expect("backup availability")
            .is_empty(),
        "a stale database row must not advertise rollback"
    );
    assert!(
        rollback_component(&fx.context, fx.game.id(), &fx.component_id).is_err(),
        "rollback without physical baseline bytes must be rejected"
    );
    assert_eq!(
        fs::read(&fx.live).expect("B remains live"),
        b"replacement-b"
    );

    assert!(
        apply_swap(&fx.context, fx.game.id(), &fx.component_id, c.id()).is_err(),
        "a missing immutable baseline must block the reswap"
    );
    assert!(
        !sidecar.exists(),
        "the missing sidecar must not be recreated"
    );
    assert_eq!(
        fs::read(&fx.live).expect("B remains live"),
        b"replacement-b"
    );
    assert_eq!(
        fx.context
            .storage()
            .get_component_backup(&fx.component_id)
            .expect("backup query")
            .expect("original row")
            .files()[0]
            .sha256(),
        Some(&original_hash)
    );
    assert!(
        crate::catalog::backup_component_ids(&fx.context, fx.game.id())
            .expect("backup availability")
            .is_empty(),
        "rollback must remain unavailable without physical baseline bytes"
    );
}

#[test]
fn catalog_apply_rejects_external_or_missing_live_state_before_mutation() {
    let fx = fresh_dlss_fixture("fresh-live", b"scanned-original");
    let source = write_library_dlss(&fx.library_dir, "lib", b"replacement");
    let artifact = dlss_artifact(&source, "3.8.0.0");
    fx.context
        .storage()
        .upsert_artifact(&artifact)
        .expect("artifact");

    write(&fx.live, b"external-replacement");
    assert!(apply_swap(&fx.context, fx.game.id(), &fx.component_id, artifact.id()).is_err());
    assert_eq!(fs::read(&fx.live).expect("live"), b"external-replacement");
    assert!(!bak_of(&fx.live).exists());

    fs::remove_file(&fx.live).expect("remove");
    assert!(apply_swap(&fx.context, fx.game.id(), &fx.component_id, artifact.id()).is_err());
    assert!(!fx.live.exists());
    assert!(!bak_of(&fx.live).exists());
}

#[test]
fn catalog_rollback_rejects_a_tampered_sidecar_without_changes() {
    let fx = fresh_dlss_fixture("tampered-baseline", b"original");
    let source = write_library_dlss(&fx.library_dir, "lib", b"overlay");
    let artifact = dlss_artifact(&source, "3.8.0.0");
    fx.context
        .storage()
        .upsert_artifact(&artifact)
        .expect("artifact");
    apply_swap(&fx.context, fx.game.id(), &fx.component_id, artifact.id()).expect("swap");

    write(&bak_of(&fx.live), b"tampered");
    assert!(rollback_component(&fx.context, fx.game.id(), &fx.component_id).is_err());
    assert_eq!(fs::read(&fx.live).expect("live"), b"overlay");
    assert_eq!(fs::read(bak_of(&fx.live)).expect("sidecar"), b"tampered");
}

#[test]
fn swap_over_luma_owned_dlss_adopts_its_original_sidecar_without_rewriting_it() {
    let root = tempfile::tempdir().expect("root");
    let game_dir = root.path().join("game");
    let library_dir = root.path().join("library");
    fs::create_dir_all(&game_dir).expect("game");
    fs::create_dir_all(&library_dir).expect("library");
    let live = game_dir.join("nvngx_dlss.dll");
    let sidecar = bak_of(&live);
    let addon = game_dir.join("Luma-Game.addon");
    let source = library_dir.join("nvngx_dlss.dll");
    write(&live, b"luma-overlay");
    write(&sidecar, b"exact-original");
    write(&addon, b"addon");
    write(&source, b"library-overlay");
    let original_hash = sha_of(&sidecar);
    let luma_hash = sha_of(&live);
    let game = sample_game_at(&game_dir);
    let component_id = ComponentId::new("component:dlss-luma").expect("id");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
    )
    .with_file(ComponentFile::new(path_as_ref(&live)).with_sha256(luma_hash.clone()));
    let artifact = dlss_artifact(&source, "3.8.0.0");
    let managed = ManagedAddonFile::owned(
        path_as_ref(&live),
        ManagedFileBaseline::Present {
            sha256: original_hash.clone(),
        },
        luma_hash,
    );
    let record = InstalledAddon::new(game.id().clone(), AddonKind::Luma, path_as_ref(&addon))
        .try_with_managed_files(vec![managed])
        .expect("valid binding");
    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");
    storage
        .replace_components_for_game(game.id(), &[component])
        .expect("component");
    storage.upsert_artifact(&artifact).expect("artifact");
    storage.upsert_installed_addon(&record).expect("record");
    let context = Context::from_storage(storage);

    apply_swap(&context, game.id(), &component_id, artifact.id()).expect("swap");

    assert_eq!(fs::read(&sidecar).unwrap(), b"exact-original");
    let baseline = context
        .storage()
        .get_component_backup(&component_id)
        .unwrap()
        .expect("baseline");
    assert_eq!(baseline.files()[0].sha256(), Some(&original_hash));

    rollback_component(&context, game.id(), &component_id).expect("catalog rollback");
    assert_eq!(fs::read(&live).unwrap(), b"exact-original");
    assert!(!sidecar.exists());
    assert!(
        context
            .storage()
            .get_installed_addon(game.id())
            .unwrap()
            .expect("Luma record")
            .managed_files()
            .is_empty()
    );
    crate::addons::luma::use_cases::commands::uninstall::uninstall(&context, game.id())
        .expect("Luma uninstall after catalog rollback");
    assert_eq!(fs::read(&live).unwrap(), b"exact-original");
}

#[test]
fn swap_over_luma_owned_absent_dlss_does_not_promote_luma_bytes_to_baseline() {
    let root = tempfile::tempdir().expect("root");
    let game_dir = root.path().join("game");
    let library_dir = root.path().join("library");
    fs::create_dir_all(&game_dir).expect("game");
    fs::create_dir_all(&library_dir).expect("library");
    let live = game_dir.join("nvngx_dlss.dll");
    let addon = game_dir.join("Luma-Game.addon");
    let source = library_dir.join("nvngx_dlss.dll");
    write(&live, b"luma-created-dlss");
    write(&addon, b"addon");
    write(&source, b"library-overlay");
    let luma_hash = sha_of(&live);
    let game = sample_game_at(&game_dir);
    let component_id = ComponentId::new("component:dlss-luma-absent").expect("id");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
    )
    .with_file(ComponentFile::new(path_as_ref(&live)).with_sha256(luma_hash.clone()));
    let artifact = dlss_artifact(&source, "3.8.0.0");
    let managed =
        ManagedAddonFile::owned(path_as_ref(&live), ManagedFileBaseline::Absent, luma_hash);
    let record = InstalledAddon::new(game.id().clone(), AddonKind::Luma, path_as_ref(&addon))
        .try_with_managed_files(vec![managed])
        .expect("valid binding");
    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");
    storage
        .replace_components_for_game(game.id(), &[component])
        .expect("component");
    storage.upsert_artifact(&artifact).expect("artifact");
    storage.upsert_installed_addon(&record).expect("record");
    let context = Context::from_storage(storage);

    apply_swap(&context, game.id(), &component_id, artifact.id()).expect("swap");

    assert_eq!(fs::read(&live).unwrap(), b"library-overlay");
    assert!(!bak_of(&live).exists());
    let baseline = context
        .storage()
        .get_component_backup(&component_id)
        .unwrap()
        .expect("empty original baseline");
    assert!(baseline.files().is_empty());
    assert_eq!(baseline.expected_active_files().len(), 1);
    assert_eq!(
        baseline.expected_active_files()[0].sha256(),
        Some(&sha_of(&live))
    );

    rollback_component(&context, game.id(), &component_id).expect("catalog rollback");
    assert!(!live.exists());
    assert!(
        context
            .storage()
            .list_components_for_game(game.id())
            .unwrap()
            .iter()
            .all(|component| component.id() != &component_id)
    );
    assert!(
        context
            .storage()
            .get_installed_addon(game.id())
            .unwrap()
            .expect("Luma record")
            .managed_files()
            .is_empty()
    );
    crate::addons::luma::use_cases::commands::uninstall::uninstall(&context, game.id())
        .expect("Luma uninstall after absent rollback");
    assert!(!live.exists());
}

#[test]
fn streamline_package_apply_updates_all_installed_plugins_not_extras() {
    let root = tempfile::tempdir().expect("tempdir");
    let game_dir = root.path().join("game");
    let lib_dir = root.path().join("lib");
    fs::create_dir_all(&game_dir).expect("game dir");
    fs::create_dir_all(&lib_dir).expect("lib dir");

    // Package on disk: three release members + one SDK extra the game never had.
    let pkg = [
        ("sl.common.dll", b"common-v2.9".as_slice()),
        ("sl.interposer.dll", b"interposer-v2.9".as_slice()),
        ("sl.dlss.dll", b"dlss-v2.9".as_slice()),
        ("sl.pcl.dll", b"pcl-v2.9-extra".as_slice()),
    ];
    for (name, bytes) in &pkg {
        write(&lib_dir.join(name), bytes);
    }

    // Game currently has three older plugins only.
    for name in ["sl.common.dll", "sl.interposer.dll", "sl.dlss.dll"] {
        write(&game_dir.join(name), format!("old-{name}").as_bytes());
    }

    let mut package_files = Vec::new();
    for (name, _) in &pkg {
        let path = lib_dir.join(name);
        package_files.push(
            ComponentFile::new(path_as_ref(&path))
                .with_sha256(sha_of(&path))
                .with_version(Version::parse("2.9.0.0").expect("version")),
        );
    }
    let shas: Vec<_> = package_files
        .iter()
        .map(|f| f.sha256().expect("sha").clone())
        .collect();
    let artifact = LibraryArtifact::new(
        ArtifactId::for_bundle(shas.iter()),
        LibraryTechnology::NvidiaStreamline,
        "sl.common.dll",
        package_files,
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("package artifact")
    .with_source("catalog-v1")
    .expect("source");

    let game = sample_game_at(&game_dir);
    let mut component = LibraryComponent::new(
        ComponentId::new("component:streamline").expect("id"),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::NvidiaStreamline,
        Swappability::BundleOnly,
    );
    for name in ["sl.common.dll", "sl.interposer.dll", "sl.dlss.dll"] {
        let path = game_dir.join(name);
        component = component.with_file(
            ComponentFile::new(path_as_ref(&path))
                .with_sha256(sha_of(&path))
                .with_version(Version::parse("2.4.0.0").expect("version")),
        );
    }

    let storage = SqliteStorage::in_memory().expect("sqlite");
    storage.upsert_game(&game).expect("store game");
    storage
        .replace_components_for_game(game.id(), &[component])
        .expect("store components");
    storage.upsert_artifact(&artifact).expect("store artifact");

    let context = Context::from_storage(storage);
    let result = apply_swap(
        &context,
        game.id(),
        &ComponentId::new("component:streamline").expect("id"),
        artifact.id(),
    )
    .expect("package apply should succeed");
    assert_eq!(
        result.updated_file_count, 3,
        "result must report physical files, not one component operation"
    );

    // All three installed plugins updated to package bytes.
    for (name, bytes) in &pkg[..3] {
        assert_eq!(
            fs::read(game_dir.join(name)).expect("read installed"),
            *bytes,
            "{name} must receive package content"
        );
    }
    // Extra package member must not be added to the game folder.
    assert!(
        !game_dir.join("sl.pcl.dll").exists(),
        "intersection policy must not expand the plugin set"
    );

    let stored = context
        .storage()
        .list_components_for_game(game.id())
        .expect("list components");
    assert_eq!(stored.len(), 1);
    assert_eq!(
        stored[0].files().len(),
        3,
        "component must still have exactly the installed plugin set"
    );
    for file in stored[0].files() {
        let name = file.path().file_name().expect("installed basename");
        let on_disk = sha_of(&game_dir.join(name));
        assert_eq!(
            file.sha256(),
            Some(&on_disk),
            "{name}: catalog must store the re-read installed hash, not a stale plan snapshot"
        );
        assert_eq!(
            file.version().map(Version::as_str),
            None,
            "each stored file must remain version-unknown when PE metadata is absent"
        );
    }
}

#[test]
fn preview_keeps_stale_artifact_but_apply_invalidates_it_at_mutation_boundary() {
    let root = tempfile::tempdir().expect("tempdir");
    let game_dir = root.path().join("game");
    let lib_dir = root.path().join("lib");
    fs::create_dir_all(&game_dir).expect("game dir");
    fs::create_dir_all(&lib_dir).expect("lib dir");

    let source = lib_dir.join("nvngx_dlss.dll");
    let target = game_dir.join("nvngx_dlss.dll");
    write(&source, b"dlss-310.7-original");
    write(&target, b"dlss-game-original");
    let expected_sha = sha_of(&source);

    // Snapshot says 310.7, then the source file is overwritten (manual restore).
    let artifact = LibraryArtifact::new(
        ArtifactId::for_bundle([&expected_sha]),
        LibraryTechnology::DlssSuperResolution,
        "nvngx_dlss.dll",
        vec![
            ComponentFile::new(path_as_ref(&source))
                .with_sha256(expected_sha)
                .with_version(Version::parse("310.7.0.0").expect("version")),
        ],
        ArtifactTrustLevel::LocalObserved,
    )
    .expect("artifact")
    .with_source("scan-folder")
    .expect("source");

    write(&source, b"dlss-3.1.30-after-manual-restore");

    let game = sample_game_at(&game_dir);
    let component = LibraryComponent::new(
        ComponentId::new("component:dlss").expect("id"),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_as_ref(&target))
            .with_sha256(sha_of(&target))
            .with_version(Version::parse("3.1.0.0").expect("version")),
    );

    let storage = SqliteStorage::in_memory().expect("sqlite");
    storage.upsert_game(&game).expect("store game");
    storage
        .replace_components_for_game(game.id(), &[component])
        .expect("store components");
    storage.upsert_artifact(&artifact).expect("store artifact");
    let artifact_id = artifact.id().clone();
    let component_id = ComponentId::new("component:dlss").expect("id");
    let context = Context::from_storage(storage);

    let preview_error =
        crate::catalog::build_swap_plan(&context, game.id(), &component_id, &artifact_id)
            .err()
            .expect("preview must reject stale source");
    assert!(matches!(
        preview_error,
        crate::ServiceError::StaleReplacementSource
    ));

    let remaining = context.storage().list_artifacts().expect("list artifacts");
    assert!(
        remaining
            .iter()
            .any(|artifact| artifact.id() == &artifact_id),
        "read-only preview must preserve the stale catalog row"
    );

    let apply_error = apply_swap(&context, game.id(), &component_id, &artifact_id)
        .expect_err("apply must reject stale source");
    assert!(matches!(
        apply_error,
        crate::ServiceError::StaleReplacementSource
    ));

    let remaining = context.storage().list_artifacts().expect("list artifacts");
    assert!(
        remaining
            .iter()
            .all(|artifact| artifact.id() != &artifact_id),
        "apply boundary must invalidate the stale catalog row"
    );
    assert_eq!(
        fs::read(&target).expect("game file"),
        b"dlss-game-original",
        "game DLL must not be modified when prepare fails"
    );
}

#[test]
fn preview_does_not_recover_an_unrelated_pending_file_mutation() {
    let fx = fresh_dlss_fixture("preview-read-only", b"installed-original");
    let source = write_library_dlss(&fx.library_dir, "candidate", b"replacement");
    let artifact = dlss_artifact(&source, "3.7.0.0");
    fx.context
        .storage()
        .upsert_artifact(&artifact)
        .expect("artifact");

    let sentinel = fx
        .live
        .parent()
        .expect("game directory")
        .join("preview-sentinel.txt");
    write(&sentinel, b"before-pending-mutation");
    let guard = crate::game_mutation_lock::blocking_lock(fx.game.id());
    let transaction = crate::file_mutation::DurableFileTransaction::prepare(
        &fx.context,
        &guard,
        &crate::file_mutation::MutationScope::single(fx.live.parent().expect("game directory"))
            .expect("scope"),
        "preview_read_only_test",
        None,
        [sentinel.clone()],
    )
    .expect("pending mutation");
    drop(guard);
    write(&sentinel, b"pending-mutated-bytes");

    crate::catalog::build_swap_plan(&fx.context, fx.game.id(), &fx.component_id, artifact.id())
        .expect("preview");

    assert_eq!(
        fs::read(&sentinel).expect("sentinel"),
        b"pending-mutated-bytes",
        "preview must not run pending-mutation recovery"
    );

    transaction
        .rollback(fx.context.storage())
        .expect("test cleanup rollback");
    assert_eq!(
        fs::read(&sentinel).expect("restored sentinel"),
        b"before-pending-mutation"
    );
}

#[test]
fn source_change_between_preflight_and_copy_rolls_back_and_invalidates_atomically() {
    let fx = fresh_dlss_fixture("post-copy-source-race", b"installed-original");
    let source = write_library_dlss(&fx.library_dir, "candidate", b"replacement-as-declared");
    let artifact = dlss_artifact(&source, "3.7.0.0");
    let artifact_id = artifact.id().clone();
    fx.context
        .storage()
        .upsert_artifact(&artifact)
        .expect("artifact");

    let raced_source = source;
    let _hook_guard = super::set_before_copy_hook(move || {
        write(&raced_source, b"source-changed-after-preflight");
    });
    let service_error = apply_swap(&fx.context, fx.game.id(), &fx.component_id, &artifact_id)
        .expect_err("post-preflight source change must fail");
    assert!(
        matches!(service_error, crate::ServiceError::StaleReplacementSource),
        "stable stale-source error expected, got {service_error:?}"
    );

    assert_eq!(
        fs::read(&fx.live).expect("live target"),
        b"installed-original",
        "durable rollback must restore the exact pre-apply target"
    );
    assert!(
        !bak_of(&fx.live).exists(),
        "rollback must remove the sidecar created by the failed apply"
    );

    let remaining = fx
        .context
        .storage()
        .list_artifacts()
        .expect("list artifacts");
    assert!(
        remaining.iter().all(|a| a.id() != &artifact_id),
        "stale artifact row must be invalidated after post-copy mismatch"
    );
    let stored = fx
        .context
        .storage()
        .list_components_for_game(fx.game.id())
        .expect("stored component");
    let restored_hash = sha_of(&fx.live);
    assert_eq!(stored.len(), 1);
    assert_eq!(
        stored[0].files()[0].sha256(),
        Some(&restored_hash),
        "component storage must remain bound to the restored bytes"
    );
}

#[test]
fn fsr_members_to_remove_reads_the_baseline_not_the_live_component() {
    // Re-swap scenario: the live component was already cleaned by an earlier
    // unified swap, while the immutable baseline still contains the original
    // upscaling members. The next desired set must continue to exclude them.
    let baseline = vec![
        comp_file_str("C:/game/amd_fidelityfx_dx12.dll"),
        comp_file_str("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
    ];

    let artifact = LibraryArtifact::new(
        ArtifactId::new("artifact:fsr31").expect("artifact id"),
        LibraryTechnology::AmdFsr,
        "amd_fidelityfx_dx12.dll",
        vec![
            comp_file_str("C:/lib/amd_fidelityfx_dx12.dll")
                .with_sha256(Sha256Hash::new(HEX64).expect("sha")),
        ],
        ArtifactTrustLevel::CatalogDownloaded,
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
fn public_catalog_apply_waits_at_the_game_mutation_boundary() {
    let context = std::sync::Arc::new(crate::Context::from_storage(
        SqliteStorage::in_memory().expect("storage"),
    ));
    let game_id = GameId::new(format!("manual:lock-{}", ulid::Ulid::generate())).expect("id");
    let component_id = ComponentId::new("component:lock-test").expect("component");
    let artifact_id = ArtifactId::new("artifact:lock-test").expect("artifact");
    let held = crate::game_mutation_lock::blocking_lock(&game_id);
    let (attempt_tx, attempt_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    crate::game_mutation_lock::set_lock_attempt_hook(&game_id, attempt_tx);

    let worker_context = std::sync::Arc::clone(&context);
    let worker_game = game_id;
    let worker = std::thread::spawn(move || {
        let result = super::apply_swap(&worker_context, &worker_game, &component_id, &artifact_id);
        done_tx.send(result).expect("report result");
    });

    attempt_rx.recv().expect("entrypoint reached lock");
    assert!(matches!(
        done_rx.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    drop(held);
    assert!(done_rx.recv().expect("completed").is_err());
    worker.join().expect("worker");
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
        LibraryTechnology::AmdFsr,
        "amd_fidelityfx_upscaler_dx12.dll",
        vec![
            comp_file_str("C:/lib/amd_fidelityfx_upscaler_dx12.dll")
                .with_sha256(Sha256Hash::new(HEX64).expect("sha")),
        ],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("artifact");

    assert!(fsr_members_to_remove(&baseline, &artifact, &[]).is_empty());
}
