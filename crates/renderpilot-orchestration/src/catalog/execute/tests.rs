use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use renderpilot_application::{
    AppErrorKind, ArtifactRepository, ComponentRepository, D3d12ExecutableAction,
    D3d12ExecutableProfile, ExternalAliasRequirements, GameRepository, InstalledAddonRepository,
    ResolvedPathDisposition, resolve_transition,
};
use renderpilot_domain::{
    AddonKind, Architecture, ArtifactId, ArtifactMetadata, ArtifactTrustLevel, ComponentFile,
    ComponentId, ComponentKind, ComponentRollbackBaseline, GameId, GameIdentity, GameInstallation,
    GameRuntime, InstalledAddon, Launcher, LibraryArtifact, LibraryComponent, LibraryTechnology,
    ManagedAddonFile, ManagedFileBaseline, PathRef, PeCompatibilityProfile, PeExportSet, Platform,
    RuntimeTarget, Sha256Hash, Swappability, UpstreamPackage, UpstreamPackageProvider, Version,
};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::Context;
use crate::catalog::execute::rollback_component;

use super::planning::{fsr_members_to_remove, planned_target_files};
use super::types::{PlannedFile, PreparedApplySwap, PreparedD3d12Execution};
use xiph_pe::{synthetic_xiph_pe, synthetic_xiph_pe_with_delay};

mod d3d12_execution;
mod fs_transitions;
mod unreal_xiph_lifecycle;
mod xiph_lifecycle;
mod xiph_pe;

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

fn apply_swap_with_current_safety(
    context: &Context,
    game_id: &GameId,
    component_id: &ComponentId,
    artifact_id: &ArtifactId,
) -> Result<super::types::SwapResult, crate::ServiceError> {
    let assessment = crate::FileSafetyAuthority::new().issue_game_assessment(context, game_id)?;
    apply_swap_confirmed(
        context,
        game_id,
        component_id,
        artifact_id,
        None,
        Some(&assessment.context_token),
    )
}

fn apply_swap_confirmed(
    context: &Context,
    game_id: &GameId,
    component_id: &ComponentId,
    artifact_id: &ArtifactId,
    executable_confirmation: Option<&str>,
    context_token: Option<&str>,
) -> Result<super::types::SwapResult, crate::ServiceError> {
    let authority = crate::FileSafetyAuthority::new();
    let current_assessment = context_token
        .is_none()
        .then(|| authority.issue_game_assessment(context, game_id))
        .transpose()?;
    let context_token = context_token.or_else(|| {
        current_assessment
            .as_ref()
            .map(|assessment| assessment.context_token.as_str())
    });
    let safety = authority.game_permit(game_id.clone(), context_token)?;
    crate::catalog::apply_swap(crate::catalog::ApplySwapRequest {
        context,
        game_id,
        component_id,
        artifact_id,
        executable_confirmation,
        safety: &safety,
    })
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
    assert_eq!(
        removed,
        vec![baseline[1].clone(), baseline[2].clone()],
        "a unified FSR 3.1 downgrade drops exactly the unmatched upscaling members"
    );
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
    assert!(
        removed
            .iter()
            .all(|file| file.path().file_name() != Some("amd_fidelityfx_dx12.dll")),
        "planned entry point must not be removed"
    );
}

#[test]
fn fsr_downgrade_spares_the_games_own_loader_and_optional_effects() {
    // Mixed lineage — the loader+denoiser+radiancecache Ray Regeneration stack is
    // independent of the upscaling backend and must survive a unified
    // FSR 3.1 update untouched.
    let baseline = vec![
        comp_file_str("C:/game/amd_fidelityfx_dx12.dll"),
        comp_file_str("C:/game/amd_fidelityfx_loader_dx12.dll"),
        comp_file_str("C:/game/amd_fidelityfx_denoiser_dx12.dll"),
        comp_file_str("C:/game/amd_fidelityfx_radiancecache_dx12.dll"),
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
    assert!(
        removed.is_empty(),
        "the loader and optional effects (denoiser, radiance cache) are not part of the upscaling lineage"
    );
}

#[test]
fn fsr_downgrade_mixed_baseline_removes_upscaling_and_preserves_loader_and_effects() {
    let baseline = vec![
        comp_file_str("C:/game/amd_fidelityfx_dx12.dll"),
        comp_file_str("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
        comp_file_str("C:/game/amd_fidelityfx_framegeneration_dx12.dll"),
        comp_file_str("C:/game/amd_fidelityfx_loader_dx12.dll"),
        comp_file_str("C:/game/amd_fidelityfx_denoiser_dx12.dll"),
        comp_file_str("C:/game/amd_fidelityfx_radiancecache_dx12.dll"),
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
        removed,
        vec![baseline[1].clone(), baseline[2].clone()],
        "mixed baseline must remove only upscaling members, sparing entry point, loader and optional effects"
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
    let component = streamline_component(&["primary.dll"]).rebuild_with_files(vec![
        comp_file_str("C:/game/primary.dll")
            .with_sha256(Sha256Hash::new("c".repeat(64)).expect("current hash")),
    ]);
    let transition = resolve_transition(
        &component,
        &artifact,
        component.files(),
        &ExternalAliasRequirements::NotRequired,
    )
    .expect("transition");
    assert_eq!(transition.paths().len(), 1);
    match &transition.paths()[0] {
        ResolvedPathDisposition::Write(write) => {
            assert_eq!(write.target().as_str(), "C:/game/primary.dll");
            assert_eq!(write.source().path().as_str(), "C:/lib/selected.dll");
            let expected_sha = Sha256Hash::new("b".repeat(64)).expect("sha");
            assert_eq!(write.source().sha256(), Some(&expected_sha));
        }
        other => panic!("expected resolved write, got {other:?}"),
    }

    let prepared = PreparedApplySwap {
        game_id: component.game_id().clone(),
        component_id: component.id().clone(),
        component,
        artifact,
        baseline: Vec::new(),
        rollback_baseline: None,
        writes: vec![planned_copy(selected_path, target_path)],
        transition,
        xiph_import_proof: None,
        first_swap: true,
        d3d12: None,
    };

    assert_eq!(prepared.writes.len(), 1);
    assert_eq!(prepared.writes[0].source, selected_path);
    assert_eq!(
        prepared.writes[0].file.path().as_str(),
        "C:/game/primary.dll"
    );
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
    )
    .with_file(
        comp_file_str("C:/game/D3D12Core.dll")
            .with_sha256(Sha256Hash::new("a".repeat(64)).expect("current hash")),
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
    let transition = resolve_transition(
        &component,
        &artifact,
        component.files(),
        &ExternalAliasRequirements::NotRequired,
    )
    .expect("transition");
    let prepared = PreparedApplySwap {
        game_id: component.game_id().clone(),
        component_id: component.id().clone(),
        component,
        artifact,
        baseline: Vec::new(),
        rollback_baseline: None,
        writes: Vec::new(),
        transition,
        xiph_import_proof: None,
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
    assert_eq!(
        planned.len(),
        2,
        "planned targets must contain exactly the 2 intersecting files"
    );
    assert_eq!(planned[0].source, PathBuf::from("C:/lib/sl.common.dll"));
    assert_eq!(planned[0].file.path().as_str(), "C:/game/sl.common.dll");
    let sha_a = Sha256Hash::new("a".repeat(64)).expect("sha");
    let sha_c = Sha256Hash::new("c".repeat(64)).expect("sha");
    assert_eq!(planned[0].file.sha256(), Some(&sha_a));
    assert_eq!(
        planned[0].file.version().map(Version::as_str),
        Some("2.9.0.0")
    );

    assert_eq!(planned[1].source, PathBuf::from("C:/lib/sl.interposer.dll"));
    assert_eq!(planned[1].file.path().as_str(), "C:/game/sl.interposer.dll");
    assert_eq!(planned[1].file.sha256(), Some(&sha_c));
    assert_eq!(
        planned[1].file.version().map(Version::as_str),
        Some("2.9.0.0")
    );

    let names: Vec<&str> = planned
        .iter()
        .filter_map(|p| p.file.path().file_name())
        .collect();
    assert_eq!(
        names,
        vec!["sl.common.dll", "sl.interposer.dll"],
        "optional SDK members must not expand the game's plugin set"
    );
    assert!(
        planned
            .iter()
            .all(|p| p.file.path().file_name() != Some("sl.dlss.dll")),
        "sl.dlss.dll must not be included when not installed"
    );
}

#[test]
fn planned_streamline_empty_overlap_is_an_error() {
    let component = streamline_component(&["sl.common.dll"]);
    let artifact = streamline_package(&["sl.dlss.dll", "sl.reflex.dll"]);

    let err = planned_target_files(&artifact, Path::new("C:/game"), &component)
        .expect_err("expected empty intersection to fail");
    assert_eq!(err.kind(), &AppErrorKind::InvalidInput);
    assert!(
        err.message().contains("no installable files"),
        "unexpected error: {}",
        err.message()
    );
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
    assert_eq!(
        planned[0].source,
        PathBuf::from("C:/lib/amd_fidelityfx_upscaler_dx12.dll")
    );
    assert_eq!(
        planned[0].file.path().as_str(),
        "C:/game/amd_fidelityfx_upscaler_dx12.dll"
    );
    let sha_a = Sha256Hash::new("a".repeat(64)).expect("sha");
    let sha_b = Sha256Hash::new("b".repeat(64)).expect("sha");
    assert_eq!(planned[0].file.sha256(), Some(&sha_a));
    assert_eq!(
        planned[1].source,
        PathBuf::from("C:/lib/amd_fidelityfx_loader_dx12.dll")
    );
    assert_eq!(
        planned[1].file.path().as_str(),
        "C:/game/amd_fidelityfx_loader_dx12.dll"
    );
    assert_eq!(planned[1].file.sha256(), Some(&sha_b));
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

fn observed_xiph_file(path: &Path) -> ComponentFile {
    let profile = renderpilot_detection::inspect_pe(path)
        .expect("read synthetic PE")
        .compatibility_profile()
        .expect("complete synthetic Xiph compatibility profile");
    ComponentFile::new(path_as_ref(path))
        .with_sha256(sha_of(path))
        .with_pe_compatibility(profile)
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

#[test]
fn apply_swap_rolls_back_when_late_xiph_reservation_is_recreated_before_commit() {
    let root = tempfile::tempdir().expect("root");
    let game_dir = root.path().join("game");
    let library_dir = root.path().join("library");
    fs::create_dir_all(&game_dir).expect("game directory");
    fs::create_dir_all(&library_dir).expect("library directory");

    let engine = game_dir.join("engine.exe");
    let old_wrapper = game_dir.join("vorbisfile_vs2010_x64_rwdi.dll");
    let old_vorbis = game_dir.join("vorbis_vs2010_x64_rwdi.dll");
    let old_ogg = game_dir.join("ogg_vs2010_x64_rwdi.dll");
    let old_wrapper_bytes = synthetic_xiph_pe(
        "ov_open",
        &["vorbis_vs2010_x64_rwdi.dll", "ogg_vs2010_x64_rwdi.dll"],
    );
    let old_vorbis_bytes = synthetic_xiph_pe("vorbis_info_init", &["ogg_vs2010_x64_rwdi.dll"]);
    let old_ogg_bytes = synthetic_xiph_pe("ogg_sync_init", &[]);
    write(
        &engine,
        &synthetic_xiph_pe("engine_marker", &["vorbisfile_vs2010_x64_rwdi.dll"]),
    );
    write(&old_wrapper, &old_wrapper_bytes);
    write(&old_vorbis, &old_vorbis_bytes);
    write(&old_ogg, &old_ogg_bytes);

    let new_wrapper = library_dir.join("vorbisfile.dll");
    let new_vorbis = library_dir.join("vorbis.dll");
    let new_ogg = library_dir.join("ogg.dll");
    write(
        &new_wrapper,
        &synthetic_xiph_pe("ov_open", &["vorbis.dll", "ogg.dll"]),
    );
    write(
        &new_vorbis,
        &synthetic_xiph_pe("vorbis_info_init", &["ogg.dll"]),
    );
    write(&new_ogg, &synthetic_xiph_pe("ogg_sync_init", &[]));

    let game = sample_game_at(&game_dir);
    let component_id = ComponentId::new("component:dide-late-apply-reservation").expect("id");
    let component = [
        observed_xiph_file(&old_wrapper),
        observed_xiph_file(&old_vorbis),
        observed_xiph_file(&old_ogg),
    ]
    .into_iter()
    .fold(
        LibraryComponent::new(
            component_id.clone(),
            game.id().clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::XiphVorbis,
            Swappability::BundleOnly,
        ),
        LibraryComponent::with_file,
    );
    let artifact = LibraryArtifact::new(
        ArtifactId::new("artifact:dide-late-apply-reservation").expect("artifact id"),
        LibraryTechnology::XiphVorbis,
        "vorbisfile.dll",
        vec![
            observed_xiph_file(&new_wrapper),
            observed_xiph_file(&new_vorbis),
            observed_xiph_file(&new_ogg),
        ],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("Xiph artifact")
    .with_metadata(
        ArtifactMetadata::default().with_runtime_target(RuntimeTarget::new(Architecture::X64)),
    );

    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");
    storage
        .replace_components_for_game(game.id(), std::slice::from_ref(&component))
        .expect("component");
    storage.upsert_artifact(&artifact).expect("artifact");
    let context = Context::from_storage(storage);

    // This hook is deliberately after filesystem transition + rebind/rebuild,
    // and immediately before the commit-boundary reservation recheck.  Moving
    // that recheck earlier makes this test commit instead of roll back.
    let recreated_vorbis = old_vorbis.clone();
    let hook_ran = Arc::new(AtomicBool::new(false));
    let hook_ran_in_apply = Arc::clone(&hook_ran);
    let _hook = super::test_hooks::set_before_transition_commit_hook(move || {
        write(&recreated_vorbis, b"raced-recreated-vendor-vorbis");
        hook_ran_in_apply.store(true, Ordering::SeqCst);
    });
    let error = apply_swap_with_current_safety(&context, game.id(), &component_id, artifact.id())
        .expect_err("late recreation of archived vendor Vorbis must abort apply");
    assert!(
        hook_ran.load(Ordering::SeqCst),
        "the test must reach the post-rebind, pre-commit reservation boundary"
    );
    assert!(
        matches!(error, crate::ServiceError::InvalidInput(_)),
        "late reservation rejection must be surfaced as a failed apply: {error:?}"
    );

    assert_eq!(
        context
            .storage()
            .list_components_for_game(game.id())
            .expect("stored components"),
        vec![component],
        "the failed durable mutation must leave the active component projection unchanged"
    );
    assert!(
        context
            .storage()
            .get_component_backup(&component_id)
            .expect("backup query")
            .is_none(),
        "a failed first apply must not persist a rollback baseline"
    );

    for (path, original) in [
        (&old_wrapper, old_wrapper_bytes.as_slice()),
        (&old_vorbis, old_vorbis_bytes.as_slice()),
        (&old_ogg, old_ogg_bytes.as_slice()),
    ] {
        assert_eq!(
            fs::read(path).expect("restored vendor original"),
            original,
            "durable rollback must restore {}",
            path.display()
        );
        assert!(
            !bak_of(path).exists(),
            "durable rollback must remove the temporary sidecar for {}",
            path.display()
        );
    }
    for canonical in [&game_dir.join("vorbis.dll"), &game_dir.join("ogg.dll")] {
        assert!(
            !canonical.exists() && !bak_of(canonical).exists(),
            "durable rollback must remove newly created canonical target {}",
            canonical.display()
        );
    }
}

#[test]
fn apply_swap_rolls_back_when_late_xiph_external_import_changes_before_commit() {
    let root = tempfile::tempdir().expect("root");
    let game_dir = root.path().join("game");
    let library_dir = root.path().join("library");
    fs::create_dir_all(&game_dir).expect("game directory");
    fs::create_dir_all(&library_dir).expect("library directory");

    let engine = game_dir.join("engine.exe");
    let old_wrapper = game_dir.join("vorbisfile_vs2010_x64_rwdi.dll");
    let old_vorbis = game_dir.join("vorbis_vs2010_x64_rwdi.dll");
    let old_ogg = game_dir.join("ogg_vs2010_x64_rwdi.dll");
    let old_wrapper_bytes = synthetic_xiph_pe(
        "ov_open",
        &["vorbis_vs2010_x64_rwdi.dll", "ogg_vs2010_x64_rwdi.dll"],
    );
    let old_vorbis_bytes = synthetic_xiph_pe("vorbis_info_init", &["ogg_vs2010_x64_rwdi.dll"]);
    let old_ogg_bytes = synthetic_xiph_pe("ogg_sync_init", &[]);
    write(
        &engine,
        &synthetic_xiph_pe("engine_marker", &["vorbisfile_vs2010_x64_rwdi.dll"]),
    );
    write(&old_wrapper, &old_wrapper_bytes);
    write(&old_vorbis, &old_vorbis_bytes);
    write(&old_ogg, &old_ogg_bytes);

    let new_wrapper = library_dir.join("vorbisfile.dll");
    let new_vorbis = library_dir.join("vorbis.dll");
    let new_ogg = library_dir.join("ogg.dll");
    write(
        &new_wrapper,
        &synthetic_xiph_pe("ov_open", &["vorbis.dll", "ogg.dll"]),
    );
    write(
        &new_vorbis,
        &synthetic_xiph_pe("vorbis_info_init", &["ogg.dll"]),
    );
    write(&new_ogg, &synthetic_xiph_pe("ogg_sync_init", &[]));

    let game = sample_game_at(&game_dir);
    let component_id = ComponentId::new("component:dide-late-import-proof").expect("id");
    let component = [
        observed_xiph_file(&old_wrapper),
        observed_xiph_file(&old_vorbis),
        observed_xiph_file(&old_ogg),
    ]
    .into_iter()
    .fold(
        LibraryComponent::new(
            component_id.clone(),
            game.id().clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::XiphVorbis,
            Swappability::BundleOnly,
        ),
        LibraryComponent::with_file,
    );
    let artifact = LibraryArtifact::new(
        ArtifactId::new("artifact:dide-late-import-proof").expect("artifact id"),
        LibraryTechnology::XiphVorbis,
        "vorbisfile.dll",
        vec![
            observed_xiph_file(&new_wrapper),
            observed_xiph_file(&new_vorbis),
            observed_xiph_file(&new_ogg),
        ],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("Xiph artifact")
    .with_metadata(
        ArtifactMetadata::default().with_runtime_target(RuntimeTarget::new(Architecture::X64)),
    );

    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");
    storage
        .replace_components_for_game(game.id(), std::slice::from_ref(&component))
        .expect("component");
    storage.upsert_artifact(&artifact).expect("artifact");
    let context = Context::from_storage(storage);

    // The seam runs after rebind/rebuild and immediately before the durable
    // commit.  Unlike the reservation race above, no transition path is
    // recreated: only an external importer changes.  Without the final proof
    // recheck this would commit a missing vendor dependency.
    let raced_engine = engine;
    let raced_engine_bytes = synthetic_xiph_pe("engine_raced", &["vorbis_vs2010_x64_rwdi.dll"]);
    let hook_ran = Arc::new(AtomicBool::new(false));
    let hook_ran_in_apply = Arc::clone(&hook_ran);
    let _hook = super::test_hooks::set_before_transition_commit_hook(move || {
        write(&raced_engine, &raced_engine_bytes);
        hook_ran_in_apply.store(true, Ordering::SeqCst);
    });
    let error = apply_swap_with_current_safety(&context, game.id(), &component_id, artifact.id())
        .expect_err("late external vendor import must abort apply");
    assert!(
        hook_ran.load(Ordering::SeqCst),
        "the test must reach the post-rebind, pre-commit proof boundary"
    );
    assert!(
        error
            .to_string()
            .contains("Xiph external-import proof changed during swap"),
        "the final external-import proof must reject the race: {error:?}"
    );

    assert_eq!(
        context
            .storage()
            .list_components_for_game(game.id())
            .expect("stored components"),
        vec![component],
        "the failed durable mutation must leave the active component projection unchanged"
    );
    assert!(
        context
            .storage()
            .get_component_backup(&component_id)
            .expect("backup query")
            .is_none(),
        "a failed first apply must not persist a rollback baseline"
    );

    for (path, original) in [
        (&old_wrapper, old_wrapper_bytes.as_slice()),
        (&old_vorbis, old_vorbis_bytes.as_slice()),
        (&old_ogg, old_ogg_bytes.as_slice()),
    ] {
        assert_eq!(
            fs::read(path).expect("restored vendor original"),
            original,
            "durable rollback must restore {}",
            path.display()
        );
        assert!(
            !bak_of(path).exists(),
            "durable rollback must remove the temporary sidecar for {}",
            path.display()
        );
    }
    for canonical in [&game_dir.join("vorbis.dll"), &game_dir.join("ogg.dll")] {
        assert!(
            !canonical.exists() && !bak_of(canonical).exists(),
            "durable rollback must remove newly created canonical target {}",
            canonical.display()
        );
    }
}

#[cfg(windows)]
fn synthetic_x64_pe() -> Vec<u8> {
    synthetic_xiph_pe("RenderPilotTestFixture", &[])
}

#[cfg(windows)]
fn write_x64_pe_fixture_to(destinations: &[&Path]) {
    let fixture = synthetic_x64_pe();
    for destination in destinations {
        fs::write(destination, &fixture).expect("write x64 PE fixture");
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
    write_x64_pe_fixture_to(&[
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

    let apply = apply_swap_with_current_safety(&context, game.id(), &component_id, artifact.id())
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
        apply_swap_with_current_safety(&context, game.id(), &component_id, artifact.id()).is_err(),
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

    apply_swap_with_current_safety(&fx.context, fx.game.id(), &fx.component_id, b.id())
        .expect("A to B");
    let sidecar = bak_of(&fx.live);
    assert_eq!(fs::read(&sidecar).unwrap(), b"original-a");
    apply_swap_with_current_safety(&fx.context, fx.game.id(), &fx.component_id, c.id())
        .expect("B to C");

    assert_eq!(fs::read(&fx.live).expect("live C"), b"replacement-c");
    assert_eq!(sha_of(&fx.live), sha_of(&source_c));
    assert_eq!(fs::read(&sidecar).unwrap(), b"original-a");
    assert_eq!(sha_of(&sidecar), original_hash);

    let baseline = fx
        .context
        .storage()
        .get_component_backup(&fx.component_id)
        .unwrap()
        .expect("baseline");
    assert_eq!(baseline.files()[0].sha256(), Some(&original_hash));

    let stored_components = fx
        .context
        .storage()
        .list_components_for_game(fx.game.id())
        .expect("stored components");
    assert_eq!(stored_components.len(), 1);
    assert_eq!(
        stored_components[0].files()[0].sha256(),
        Some(&sha_of(&source_c)),
        "active stored component file must reflect replacement-c"
    );

    assert!(
        fx.context
            .storage()
            .pending_file_mutations_for_game(fx.game.id())
            .expect("pending mutation query")
            .is_empty(),
        "pending mutation journal must be clean after second swap"
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

    apply_swap_with_current_safety(&fx.context, fx.game.id(), &fx.component_id, b.id())
        .expect("A to B");
    let sidecar = bak_of(&fx.live);
    fs::remove_file(&sidecar).expect("manual backup removal");

    assert!(
        fx.context
            .storage()
            .pending_file_mutations_for_game(fx.game.id())
            .expect("pending mutation query")
            .is_empty(),
        "pending journal must be clean after A to B swap"
    );

    assert!(
        crate::catalog::backup_component_ids(&fx.context, fx.game.id())
            .expect("backup availability")
            .is_empty(),
        "a stale database row must not advertise rollback"
    );
    let rollback_err = rollback_component(&fx.context, fx.game.id(), &fx.component_id)
        .expect_err("rollback without physical baseline bytes must be rejected");
    assert!(
        matches!(
            rollback_err,
            crate::ServiceError::InvalidInput(ref message)
                if message.contains("rollback baseline for component") && message.contains("is incomplete")
        ),
        "expected incomplete baseline error on rollback, got {rollback_err:?}"
    );
    assert_eq!(
        fs::read(&fx.live).expect("B remains live"),
        b"replacement-b"
    );

    let reswap_err =
        apply_swap_with_current_safety(&fx.context, fx.game.id(), &fx.component_id, c.id())
            .expect_err("a missing immutable baseline must block the reswap");
    assert!(
        matches!(
            reswap_err,
            crate::ServiceError::InvalidInput(ref message)
                if message.contains("rollback baseline for component") && message.contains("is incomplete")
        ),
        "expected incomplete baseline error on reswap, got {reswap_err:?}"
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
    assert!(
        fx.context
            .storage()
            .pending_file_mutations_for_game(fx.game.id())
            .expect("pending mutation query")
            .is_empty(),
        "pending journal must remain clean after rejected rollback and reswap"
    );
}

#[test]
fn catalog_apply_rejects_external_or_missing_live_state_before_mutation() {
    let fx = fresh_dlss_fixture("fresh-live", b"scanned-original");
    let initial_components = fx
        .context
        .storage()
        .list_components_for_game(fx.game.id())
        .expect("components");
    let source = write_library_dlss(&fx.library_dir, "lib", b"replacement");
    let artifact = dlss_artifact(&source, "3.8.0.0");
    fx.context
        .storage()
        .upsert_artifact(&artifact)
        .expect("artifact");

    write(&fx.live, b"external-replacement");
    let error_modified =
        apply_swap_with_current_safety(&fx.context, fx.game.id(), &fx.component_id, artifact.id())
            .expect_err("modified live bytes must fail preflight");
    assert!(
        matches!(
            error_modified,
            crate::ServiceError::InvalidInput(ref message) if message.contains("changed on disk")
        ),
        "expected changed-on-disk error, got {error_modified:?}"
    );
    assert_eq!(fs::read(&fx.live).expect("live"), b"external-replacement");
    assert!(!bak_of(&fx.live).exists());
    assert_eq!(
        fx.context
            .storage()
            .list_components_for_game(fx.game.id())
            .expect("components"),
        initial_components,
        "component projection must remain unchanged on preflight rejection"
    );
    assert!(
        fx.context
            .storage()
            .get_component_backup(&fx.component_id)
            .expect("backup query")
            .is_none(),
        "preflight rejection must not create a rollback baseline row"
    );
    assert!(
        fx.context
            .storage()
            .pending_file_mutations_for_game(fx.game.id())
            .expect("pending mutation query")
            .is_empty(),
        "preflight rejection must not prepare a durable mutation"
    );

    fs::remove_file(&fx.live).expect("remove");
    let error_missing =
        apply_swap_with_current_safety(&fx.context, fx.game.id(), &fx.component_id, artifact.id())
            .expect_err("missing live file must fail preflight");
    assert!(
        matches!(
            error_missing,
            crate::ServiceError::InvalidInput(ref message) if message.contains("changed on disk")
        ),
        "expected changed-on-disk error for missing live file, got {error_missing:?}"
    );
    assert!(!fx.live.exists());
    assert!(!bak_of(&fx.live).exists());
    assert_eq!(
        fx.context
            .storage()
            .list_components_for_game(fx.game.id())
            .expect("components"),
        initial_components,
        "component projection must remain unchanged on missing live file rejection"
    );
    assert!(
        fx.context
            .storage()
            .get_component_backup(&fx.component_id)
            .expect("backup query")
            .is_none(),
        "missing live file rejection must not create a rollback baseline row"
    );
    assert!(
        fx.context
            .storage()
            .pending_file_mutations_for_game(fx.game.id())
            .expect("pending mutation query")
            .is_empty(),
        "missing live file rejection must not prepare a durable mutation"
    );
}

#[test]
fn catalog_swap_missing_or_blank_safety_context_rejects_permit_acquisition() {
    let game_id = GameId::new("manual:missing-safety-context").expect("game id");
    let authority = crate::FileSafetyAuthority::new();

    for context_token in [None, Some(""), Some(" \t ")] {
        let error = authority
            .game_permit(game_id.clone(), context_token)
            .expect_err("missing or blank context must not issue a game permit");

        assert_eq!(
            error,
            crate::ServiceError::SafetyContextMissing {
                scope: crate::file_safety::SafetyScope::Game(game_id.clone()),
            }
        );
    }
}

#[test]
fn catalog_apply_rejects_stale_safety_context_before_target_or_durable_write() {
    let fx = fresh_dlss_fixture("stale-safety-context", b"original");
    let source = write_library_dlss(&fx.library_dir, "lib", b"overlay");
    let artifact = dlss_artifact(&source, "3.8.0.0");
    fx.context
        .storage()
        .upsert_artifact(&artifact)
        .expect("artifact");
    let assessment = crate::FileSafetyAuthority::new()
        .issue_game_assessment(&fx.context, fx.game.id())
        .expect("fresh game safety assessment");

    // The marker changes the detector observation without changing the catalog
    // target or source, so the apply reaches the safety boundary before any
    // filesystem or durable mutation work.
    write(
        &fx.live.parent().expect("game root").join("EasyAntiCheat"),
        b"detected marker",
    );

    let error = apply_swap_confirmed(
        &fx.context,
        fx.game.id(),
        &fx.component_id,
        artifact.id(),
        None,
        Some(&assessment.context_token),
    )
    .expect_err("stale game safety context must reject the apply");
    assert_eq!(
        error,
        crate::ServiceError::SafetyContextStale {
            scope: crate::file_safety::SafetyScope::Game(fx.game.id().clone()),
        }
    );
    assert_eq!(
        fs::read(&fx.live).expect("target remains readable"),
        b"original",
        "stale safety rejection must precede the first target write"
    );
    assert!(!bak_of(&fx.live).exists(), "no backup may be created");
    assert!(
        fx.context
            .storage()
            .get_component_backup(&fx.component_id)
            .expect("baseline query")
            .is_none(),
        "stale safety rejection must not create a durable baseline row"
    );
    assert!(
        fx.context
            .storage()
            .pending_file_mutations_for_game(fx.game.id())
            .expect("pending mutation query")
            .is_empty(),
        "stale safety rejection must not prepare a durable mutation"
    );
}

#[test]
fn catalog_apply_rejects_a_different_game_safety_context_before_target_or_durable_write() {
    let fx = fresh_dlss_fixture("scope-mismatched-safety-context", b"original");
    let source = write_library_dlss(&fx.library_dir, "lib", b"overlay");
    let artifact = dlss_artifact(&source, "3.8.0.0");
    fx.context
        .storage()
        .upsert_artifact(&artifact)
        .expect("artifact");

    let other_root = tempfile::tempdir().expect("other game root");
    let other_game = sample_game_at(other_root.path());
    fx.context
        .storage()
        .upsert_game(&other_game)
        .expect("other game");
    let assessment = crate::FileSafetyAuthority::new()
        .issue_game_assessment(&fx.context, other_game.id())
        .expect("other game safety assessment");

    let error = apply_swap_confirmed(
        &fx.context,
        fx.game.id(),
        &fx.component_id,
        artifact.id(),
        None,
        Some(&assessment.context_token),
    )
    .expect_err("a permit for another game must reject the apply");
    assert_eq!(
        error,
        crate::ServiceError::SafetyContextScopeMismatch {
            expected: crate::file_safety::SafetyScope::Game(fx.game.id().clone()),
            actual: crate::file_safety::SafetyScope::Unknown,
        }
    );
    assert_eq!(
        fs::read(&fx.live).expect("target remains readable"),
        b"original",
        "scope rejection must precede the first target write"
    );
    assert!(!bak_of(&fx.live).exists(), "no backup may be created");
    assert!(
        fx.context
            .storage()
            .get_component_backup(&fx.component_id)
            .expect("baseline query")
            .is_none(),
        "scope rejection must not create a durable baseline row"
    );
    assert!(
        fx.context
            .storage()
            .pending_file_mutations_for_game(fx.game.id())
            .expect("pending mutation query")
            .is_empty(),
        "scope rejection must not prepare a durable mutation"
    );
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
    apply_swap_with_current_safety(&fx.context, fx.game.id(), &fx.component_id, artifact.id())
        .expect("swap");

    let baseline_before = fx
        .context
        .storage()
        .get_component_backup(&fx.component_id)
        .expect("baseline query");
    assert!(
        baseline_before.is_some(),
        "swap must record a component rollback baseline"
    );
    let components_before = fx
        .context
        .storage()
        .list_components_for_game(fx.game.id())
        .expect("stored components query");
    let pending_before = fx
        .context
        .storage()
        .pending_file_mutations_for_game(fx.game.id())
        .expect("pending mutation query");

    write(&bak_of(&fx.live), b"tampered");
    let error = rollback_component(&fx.context, fx.game.id(), &fx.component_id)
        .expect_err("tampered sidecar must fail rollback");
    assert!(
        matches!(
            error,
            crate::ServiceError::InvalidInput(ref message) if message.contains("cannot validate rollback baseline")
        ),
        "expected cannot-validate-baseline error, got {error:?}"
    );
    assert_eq!(fs::read(&fx.live).expect("live"), b"overlay");
    assert_eq!(fs::read(bak_of(&fx.live)).expect("sidecar"), b"tampered");
    assert_eq!(
        fx.context
            .storage()
            .get_component_backup(&fx.component_id)
            .expect("baseline query"),
        baseline_before,
        "rollback rejection must preserve the previously recorded baseline"
    );
    assert_eq!(
        fx.context
            .storage()
            .list_components_for_game(fx.game.id())
            .expect("stored components query"),
        components_before,
        "rollback rejection must preserve the current stored component projection"
    );
    assert_eq!(
        fx.context
            .storage()
            .pending_file_mutations_for_game(fx.game.id())
            .expect("pending mutation query"),
        pending_before,
        "rollback rejection must preserve the pending file mutation journal state"
    );
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

    apply_swap_with_current_safety(&context, game.id(), &component_id, artifact.id())
        .expect("swap");

    assert_eq!(fs::read(&live).unwrap(), b"library-overlay");
    assert_eq!(sha_of(&live), sha_of(&source));
    assert_eq!(fs::read(&sidecar).unwrap(), b"exact-original");
    assert_eq!(sha_of(&sidecar), original_hash);

    let baseline = context
        .storage()
        .get_component_backup(&component_id)
        .unwrap()
        .expect("baseline");
    assert_eq!(baseline.files().len(), 1);
    assert_eq!(baseline.files()[0].sha256(), Some(&original_hash));

    let stored_components = context
        .storage()
        .list_components_for_game(game.id())
        .expect("stored components");
    assert_eq!(stored_components.len(), 1);
    assert_eq!(
        stored_components[0].files()[0].sha256(),
        Some(&sha_of(&source))
    );

    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(game.id())
            .expect("pending mutation query")
            .is_empty(),
        "pending mutation journal must be clean after swap over luma-owned DLSS"
    );

    rollback_component(&context, game.id(), &component_id).expect("catalog rollback");
    assert_eq!(fs::read(&live).unwrap(), b"exact-original");
    assert!(!sidecar.exists());
    assert!(
        context
            .storage()
            .get_component_backup(&component_id)
            .expect("baseline query")
            .is_none(),
        "baseline record must be deleted upon catalog rollback"
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
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(game.id())
            .expect("pending mutation query")
            .is_empty(),
        "pending mutation journal must be clean after rollback"
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

    apply_swap_with_current_safety(&context, game.id(), &component_id, artifact.id())
        .expect("swap");

    assert_eq!(fs::read(&live).unwrap(), b"library-overlay");
    assert_eq!(sha_of(&live), sha_of(&source));
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

    let stored_components = context
        .storage()
        .list_components_for_game(game.id())
        .expect("stored components");
    assert_eq!(stored_components.len(), 1);
    assert_eq!(
        stored_components[0].files()[0].sha256(),
        Some(&sha_of(&source))
    );

    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(game.id())
            .expect("pending mutation query")
            .is_empty(),
        "pending mutation journal must be clean after swap over absent luma file"
    );

    rollback_component(&context, game.id(), &component_id).expect("catalog rollback");
    assert!(!live.exists());
    assert!(!bak_of(&live).exists());
    assert!(
        context
            .storage()
            .get_component_backup(&component_id)
            .expect("baseline query")
            .is_none(),
        "baseline record must be deleted upon catalog rollback"
    );
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
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(game.id())
            .expect("pending mutation query")
            .is_empty(),
        "pending mutation journal must be clean after absent rollback"
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
    let result = apply_swap_with_current_safety(
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

    // Physical backup sidecars must exist for the 3 installed plugins and retain original contents.
    for name in ["sl.common.dll", "sl.interposer.dll", "sl.dlss.dll"] {
        let sidecar = bak_of(&game_dir.join(name));
        assert!(
            sidecar.exists(),
            "sidecar backup for {name} must exist on disk"
        );
        assert_eq!(
            fs::read(&sidecar).expect("read backup sidecar"),
            format!("old-{name}").as_bytes(),
            "sidecar for {name} must preserve original game bytes"
        );
    }
    // Extra package member must not have a sidecar backup.
    assert!(
        !bak_of(&game_dir.join("sl.pcl.dll")).exists(),
        "extra uninstalled package member must not produce a backup sidecar"
    );

    // Stored baseline in SQLite must contain exactly the 3 original plugins.
    let baseline = context
        .storage()
        .get_component_backup(&ComponentId::new("component:streamline").expect("id"))
        .expect("baseline query")
        .expect("baseline must be recorded for package swap");
    assert_eq!(
        baseline.files().len(),
        3,
        "baseline must record exactly the 3 pre-existing plugins"
    );
    for file in baseline.files() {
        let name = file.path().file_name().expect("baseline basename");
        let expected_hash = renderpilot_detection::sha256_bytes(format!("old-{name}").as_bytes())
            .expect("hash bytes");
        assert_eq!(
            file.sha256(),
            Some(&expected_hash),
            "baseline hash for {name} must match original game file"
        );
    }

    // Pending mutation journal must be clean after committed apply.
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(game.id())
            .expect("pending mutation query")
            .is_empty(),
        "pending file mutation journal must be empty after successful package apply"
    );
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
        .replace_components_for_game(game.id(), std::slice::from_ref(&component))
        .expect("store components");
    storage.upsert_artifact(&artifact).expect("store artifact");
    let artifact_id = artifact.id().clone();
    let component_id = ComponentId::new("component:dlss").expect("id");
    let context = Context::from_storage(storage);

    let preview_error =
        crate::catalog::build_swap_plan(&context, game.id(), &component_id, &artifact_id)
            .err()
            .expect("preview must reject stale source");
    assert_eq!(preview_error, crate::ServiceError::StaleReplacementSource);

    let remaining = context.storage().list_artifacts().expect("list artifacts");
    assert!(
        remaining
            .iter()
            .any(|artifact| artifact.id() == &artifact_id),
        "read-only preview must preserve the stale catalog row"
    );
    assert_eq!(
        fs::read(&target).expect("game file"),
        b"dlss-game-original",
        "read-only preview must not modify the installed DLL"
    );
    assert_eq!(
        context
            .storage()
            .list_components_for_game(game.id())
            .expect("components"),
        vec![component.clone()],
        "read-only preview must not modify the stored component projection"
    );
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(game.id())
            .expect("pending")
            .is_empty(),
        "read-only preview must not create any pending file mutations"
    );

    let apply_error =
        apply_swap_with_current_safety(&context, game.id(), &component_id, &artifact_id)
            .expect_err("apply must reject stale source");
    assert_eq!(apply_error, crate::ServiceError::StaleReplacementSource);

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
    assert!(
        !bak_of(&target).exists(),
        "rejected apply must not create a target backup sidecar"
    );
    assert!(
        context
            .storage()
            .get_component_backup(&component_id)
            .expect("backup query")
            .is_none(),
        "rejected apply must not record an incorrect rollback baseline"
    );
    assert_eq!(
        context
            .storage()
            .list_components_for_game(game.id())
            .expect("components query"),
        vec![component],
        "rejected apply must preserve the stored component projection"
    );
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(game.id())
            .expect("pending mutation query")
            .is_empty(),
        "rejected apply must leave no pending file mutations behind"
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

    let pending_before = fx
        .context
        .storage()
        .pending_file_mutations_for_game(fx.game.id())
        .expect("pending mutations before preview");
    assert!(
        !pending_before.is_empty(),
        "a pending file mutation journal row must exist before preview"
    );

    crate::catalog::build_swap_plan(&fx.context, fx.game.id(), &fx.component_id, artifact.id())
        .expect("preview");

    let pending_after = fx
        .context
        .storage()
        .pending_file_mutations_for_game(fx.game.id())
        .expect("pending mutations after preview");
    assert_eq!(
        pending_before, pending_after,
        "preview must not advance, cancel, finalize, or modify existing pending journal rows"
    );

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
    assert!(
        fx.context
            .storage()
            .pending_file_mutations_for_game(fx.game.id())
            .expect("pending mutations after cleanup")
            .is_empty(),
        "test cleanup rollback must clear the pending file mutation journal"
    );
}

#[test]
fn source_change_between_preflight_and_copy_rolls_back_and_invalidates_atomically() {
    let fx = fresh_dlss_fixture("post-copy-source-race", b"installed-original");
    let components_before = fx
        .context
        .storage()
        .list_components_for_game(fx.game.id())
        .expect("components before apply");
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
    let service_error =
        apply_swap_with_current_safety(&fx.context, fx.game.id(), &fx.component_id, &artifact_id)
            .expect_err("post-preflight source change must fail");
    assert_eq!(
        service_error,
        crate::ServiceError::StaleReplacementSource,
        "stable stale-source error expected"
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
    assert_eq!(
        stored, components_before,
        "component storage must remain bound to the restored pre-apply component state"
    );
    assert!(
        fx.context
            .storage()
            .get_component_backup(&fx.component_id)
            .expect("baseline lookup")
            .is_none(),
        "aborted apply must leave no rollback baseline in storage"
    );
    assert!(
        fx.context
            .storage()
            .pending_file_mutations_for_game(fx.game.id())
            .expect("pending mutations lookup")
            .is_empty(),
        "durable rollback must clean up the pending file mutation journal"
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
        removed,
        vec![baseline[1].clone()],
        "reswap must determine upscaling removals from the immutable baseline ComponentFile"
    );
    assert_eq!(
        removed[0].path().file_name(),
        Some("amd_fidelityfx_upscaler_dx12.dll")
    );
}

#[test]
fn public_catalog_apply_waits_at_the_game_mutation_boundary() {
    let game_root = tempfile::tempdir().expect("game root");
    let storage = SqliteStorage::in_memory().expect("storage");
    let game = sample_game_at(game_root.path());
    storage.upsert_game(&game).expect("game");
    let context = std::sync::Arc::new(crate::Context::from_storage(storage));
    let game_id = game.id().clone();
    let component_id = ComponentId::new("component:lock-test").expect("component");
    let artifact_id = ArtifactId::new("artifact:lock-test").expect("artifact");
    let assessment = crate::FileSafetyAuthority::new()
        .issue_game_assessment(&context, &game_id)
        .expect("assessment");
    let safety = crate::FileSafetyAuthority::new()
        .game_permit(game_id.clone(), Some(&assessment.context_token))
        .expect("permit");
    let held = crate::game_mutation_lock::blocking_lock(&game_id);
    let (attempt_tx, attempt_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    crate::game_mutation_lock::set_lock_attempt_hook(&game_id, attempt_tx);

    let worker_context = std::sync::Arc::clone(&context);
    let worker_game = game_id;
    let worker = std::thread::spawn(move || {
        let result = super::apply_swap(super::ApplySwapRequest {
            context: &worker_context,
            game_id: &worker_game,
            component_id: &component_id,
            artifact_id: &artifact_id,
            executable_confirmation: None,
            safety: &safety,
        });
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

    let planned = vec![planned_copy(
        Path::new("C:/lib/amd_fidelityfx_upscaler_dx12.dll"),
        Path::new("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
    )];

    assert!(
        fsr_members_to_remove(&baseline, &artifact, &planned).is_empty(),
        "split artifact must not remove upscaling members even with planned copies"
    );
    assert!(
        fsr_members_to_remove(&baseline, &artifact, &[]).is_empty(),
        "split artifact must not remove upscaling members without planned copies"
    );
}
