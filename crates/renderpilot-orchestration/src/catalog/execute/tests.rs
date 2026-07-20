use std::fs;
use std::path::{Path, PathBuf};

use renderpilot_application::{
    AppErrorKind, ArtifactRepository, ComponentRepository, GameRepository, InstalledAddonRepository,
};
use renderpilot_domain::{
    AddonKind, ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
    GameIdentity, GameInstallation, GameRuntime, GraphicsComponent, GraphicsTechnology,
    InstalledAddon, Launcher, LibraryArtifact, ManagedAddonFile, ManagedFileBaseline, PathRef,
    Platform, Sha256Hash, Swappability, Version,
};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::Context;
use crate::catalog::execute::{apply_swap, rollback_component};

use super::ensure_artifact_sources_usable;
use super::fs_ops::{perform_apply_fs, revert_to_baseline_fs};
use super::planning::{fsr_members_to_remove, planned_target_files};
use super::prepare::load_apply_swap;
use super::source_integrity::rebind_planned_files_from_disk;
use super::types::PlannedFile;

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
    let baseline = vec![comp_file(&target).with_sha256(sha_of(&target))];
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
            target.clone(),
            bak_of(&target),
            dir.path().join("second.dll"),
        ],
    )
    .expect("durable transaction");
    let result = perform_apply_fs(&placeholder_component(), &baseline, &plans, &[]);

    assert!(result.is_err(), "missing source must fail the apply");
    mutation
        .rollback(context.storage())
        .expect("durable rollback");
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
        GraphicsTechnology::AmdFsr,
        "amd_fidelityfx_dx12.dll",
        vec![
            comp_file_str("C:/lib/amd_fidelityfx_dx12.dll")
                .with_sha256(Sha256Hash::new(HEX64).expect("sha")),
        ],
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
        vec![
            comp_file_str("C:/lib/amd_fidelityfx_dx12.dll")
                .with_sha256(Sha256Hash::new(HEX64).expect("sha")),
        ],
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

/// Planning helpers for Streamline path building (policy lives in
/// `streamline_install`; these assert `planned_target_files` wiring).
fn streamline_component(installed_names: &[&str]) -> GraphicsComponent {
    let mut component = GraphicsComponent::new(
        ComponentId::new("component:streamline").expect("id"),
        GameId::new("manual:C:/Games/Test").expect("game id"),
        ComponentKind::NativeLibrary,
        GraphicsTechnology::NvidiaStreamline,
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
        GraphicsTechnology::NvidiaStreamline,
        member_names[0],
        files,
        ArtifactTrustLevel::ManifestDownloaded,
    )
    .expect("package")
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
    let component = GraphicsComponent::new(
        ComponentId::new("component:fsr").expect("id"),
        GameId::new("manual:C:/Games/Test").expect("game id"),
        ComponentKind::NativeLibrary,
        GraphicsTechnology::AmdFsr,
        Swappability::BundleOnly,
    )
    .with_file(comp_file_str("C:/game/amd_fidelityfx_dx12.dll"));

    let artifact = LibraryArtifact::new(
        ArtifactId::new("artifact:fsr-pkg").expect("id"),
        GraphicsTechnology::AmdFsr,
        "amd_fidelityfx_upscaler_dx12.dll",
        vec![
            comp_file_str("C:/lib/amd_fidelityfx_upscaler_dx12.dll")
                .with_sha256(Sha256Hash::new("a".repeat(64)).expect("sha")),
            comp_file_str("C:/lib/amd_fidelityfx_loader_dx12.dll")
                .with_sha256(Sha256Hash::new("b".repeat(64)).expect("sha")),
        ],
        ArtifactTrustLevel::ManifestDownloaded,
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

fn dlss_artifact(path: &Path, version: &str) -> LibraryArtifact {
    let sha = sha_of(path);
    LibraryArtifact::new(
        ArtifactId::for_bundle([&sha]),
        GraphicsTechnology::DlssSuperResolution,
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
    let component = GraphicsComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        GraphicsTechnology::DlssSuperResolution,
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
            .expect("baseline")[0]
            .sha256(),
        Some(&original_hash)
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
    let component = GraphicsComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        GraphicsTechnology::DlssSuperResolution,
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
    assert_eq!(baseline[0].sha256(), Some(&original_hash));

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
    let component = GraphicsComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        GraphicsTechnology::DlssSuperResolution,
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
    assert_eq!(
        context
            .storage()
            .get_component_backup(&component_id)
            .unwrap()
            .expect("empty baseline"),
        Vec::<ComponentFile>::new()
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
        GraphicsTechnology::NvidiaStreamline,
        "sl.common.dll",
        package_files,
        ArtifactTrustLevel::ManifestDownloaded,
    )
    .expect("package artifact")
    .with_source("manifest-download")
    .expect("source");

    let game = sample_game_at(&game_dir);
    let mut component = GraphicsComponent::new(
        ComponentId::new("component:streamline").expect("id"),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        GraphicsTechnology::NvidiaStreamline,
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
    apply_swap(
        &context,
        game.id(),
        &ComponentId::new("component:streamline").expect("id"),
        artifact.id(),
    )
    .expect("package apply should succeed");

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
fn preflight_rejects_stale_source_invalidates_artifact_and_leaves_game_untouched() {
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
        GraphicsTechnology::DlssSuperResolution,
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
    let component = GraphicsComponent::new(
        ComponentId::new("component:dlss").expect("id"),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        GraphicsTechnology::DlssSuperResolution,
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

    let loaded = load_apply_swap(
        &storage,
        game.id(),
        &ComponentId::new("component:dlss").expect("id"),
        &artifact_id,
    )
    .expect("load apply swap");
    match ensure_artifact_sources_usable(&storage, &loaded.artifact) {
        Ok(()) => panic!("stale source must fail preflight"),
        Err(error) => assert_eq!(
            *error.kind(),
            AppErrorKind::StaleReplacementSource,
            "expected StaleReplacementSource, got {:?}",
            error.kind()
        ),
    }

    let remaining = storage.list_artifacts().expect("list artifacts");
    assert!(
        remaining.iter().all(|a| a.id() != &artifact_id),
        "stale artifact row must be invalidated"
    );
    assert_eq!(
        fs::read(&target).expect("game file"),
        b"dlss-game-original",
        "game DLL must not be modified when prepare fails"
    );
}

#[test]
fn post_copy_hash_mismatch_uses_apply_recovery_boundary() {
    // TOCTOU after copy cannot be injected into apply_swap without a seam, so
    // this test drives the same recovery function apply_swap calls when rebind
    // returns StaleReplacementSource. Reimplementing undo/delete here would
    // pass even if apply_swap stopped recovering.
    let root = tempfile::tempdir().expect("tempdir");
    let game_dir = root.path().join("game");
    let lib_dir = root.path().join("lib");
    fs::create_dir_all(&game_dir).expect("game dir");
    fs::create_dir_all(&lib_dir).expect("lib dir");

    let source = lib_dir.join("nvngx_dlss.dll");
    let target = game_dir.join("nvngx_dlss.dll");
    write(&source, b"dlss-replacement-bytes");
    write(&target, b"dlss-game-original");
    let expected_sha = sha_of(&source);

    let artifact = LibraryArtifact::new(
        ArtifactId::for_bundle([&expected_sha]),
        GraphicsTechnology::DlssSuperResolution,
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

    let game = sample_game_at(&game_dir);
    let component = GraphicsComponent::new(
        ComponentId::new("component:dlss").expect("id"),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_as_ref(&target))
            .with_sha256(sha_of(&target))
            .with_version(Version::parse("3.1.0.0").expect("version")),
    );

    let storage = SqliteStorage::in_memory().expect("sqlite");
    storage.upsert_game(&game).expect("store game");
    // Keep `component` for direct plan/apply calls below — unlike the full
    // apply_swap path which reloads from storage by id.
    storage
        .replace_components_for_game(game.id(), std::slice::from_ref(&component))
        .expect("store components");
    storage.upsert_artifact(&artifact).expect("store artifact");
    let artifact_id = artifact.id().clone();

    let mut planned = planned_target_files(&artifact, &game_dir, &component).expect("plan");
    let _receipt = perform_apply_fs(&component, component.files(), &planned, &[])
        .expect("copy should succeed");
    assert_eq!(
        fs::read(&target).expect("copied target"),
        b"dlss-replacement-bytes",
        "precondition: overlay must install the replacement before the mutation"
    );

    write(&target, b"mutated-after-copy");
    let rebind_error =
        rebind_planned_files_from_disk(&mut planned).expect_err("mutated target must fail rebind");
    assert_eq!(*rebind_error.kind(), AppErrorKind::StaleReplacementSource);

    super::invalidate_stale_artifact(&storage, &artifact_id, "installed target hash mismatch");
    let service_error: crate::ServiceError = rebind_error.into();
    assert!(
        matches!(service_error, crate::ServiceError::StaleReplacementSource),
        "recovery must preserve the stable stale-source error, got {service_error:?}"
    );

    let remaining = storage.list_artifacts().expect("list artifacts");
    assert!(
        remaining.iter().all(|a| a.id() != &artifact_id),
        "stale artifact row must be invalidated after post-copy mismatch"
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
        GraphicsTechnology::AmdFsr,
        "amd_fidelityfx_dx12.dll",
        vec![
            comp_file_str("C:/lib/amd_fidelityfx_dx12.dll")
                .with_sha256(Sha256Hash::new(HEX64).expect("sha")),
        ],
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
        GraphicsTechnology::AmdFsr,
        "amd_fidelityfx_upscaler_dx12.dll",
        vec![
            comp_file_str("C:/lib/amd_fidelityfx_upscaler_dx12.dll")
                .with_sha256(Sha256Hash::new(HEX64).expect("sha")),
        ],
        ArtifactTrustLevel::ManifestDownloaded,
    )
    .expect("artifact");

    assert!(fsr_members_to_remove(&baseline, &artifact, &[]).is_empty());
}
