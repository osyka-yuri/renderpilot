use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use renderpilot_application::{GameRepository, InstalledAddonRepository};
use renderpilot_domain::{
    AddonKind, GameIdentity, GameInstallation, GameRuntime, InstalledAddon, Launcher,
    ManagedAddonFile, ManagedFileBaseline, PathRef, Platform, RootAuthority, Sha256Hash,
};
use renderpilot_storage_sqlite::{AuthorityCas, SqliteStorage};
use sha2::{Digest, Sha256};

use super::{LocalCleanupCategory, RetiredRootObservation, observe_registered_root, walker};

static NEXT_GAME: AtomicU64 = AtomicU64::new(0);

struct TempTree(tempfile::TempDir);

impl TempTree {
    fn new() -> Self {
        Self(tempfile::tempdir().expect("temporary test tree"))
    }

    fn install_root(&self) -> PathBuf {
        let root = self.0.path().join("Game");
        fs::create_dir(&root).expect("create installation root");
        root
    }
}

fn path_ref(path: &Path) -> PathRef {
    PathRef::new(path.to_string_lossy().replace('\\', "/")).expect("path ref")
}

fn game(root: &Path) -> GameInstallation {
    GameInstallation::new(
        GameIdentity::new(
            renderpilot_domain::GameId::new(format!(
                "manual:cleanup-{}",
                NEXT_GAME.fetch_add(1, Ordering::Relaxed)
            ))
            .expect("game id"),
            "Cleanup Test Game",
            Launcher::Steam,
        )
        .expect("game identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        path_ref(root),
    )
    .with_root_authority(RootAuthority::UserConfirmed)
}

fn store_owned_managed_file(
    storage: &SqliteStorage,
    game: &GameInstallation,
    addon_path: &Path,
    file_path: &Path,
    digest: Sha256Hash,
) {
    storage.upsert_game(game).expect("store game");
    let addon = InstalledAddon::new(game.id().clone(), AddonKind::RenoDx, path_ref(addon_path))
        .try_with_managed_files(vec![ManagedAddonFile::owned(
            path_ref(file_path),
            ManagedFileBaseline::Absent,
            digest,
        )])
        .expect("managed add-on file");
    storage
        .upsert_installed_addon(&addon)
        .expect("store add-on");
}

fn mark_absent(storage: &SqliteStorage, game: &GameInstallation) {
    let readiness = storage
        .catalog_readiness(game.id())
        .expect("readiness before absence");
    storage
        .mark_installation_absent(game, AuthorityCas::new(readiness.authority_epoch()))
        .expect("mark absent");
}

fn sha256(bytes: &[u8]) -> Sha256Hash {
    Sha256Hash::new(hex::encode(Sha256::digest(bytes))).expect("sha256")
}

#[test]
fn exact_managed_residue_is_read_only_and_absent_directory_only_root_collects() {
    let temp = TempTree::new();
    let root = temp.install_root();
    let unrecorded_directory = root.join("bin");
    fs::create_dir(&unrecorded_directory).expect("create unrecorded directory");
    let file_path = unrecorded_directory.join("owned.dll");
    let bytes = b"unchanged RenderPilot-owned bytes";
    fs::write(&file_path, bytes).expect("write owned file");

    let storage = SqliteStorage::in_memory().expect("storage");
    let game = game(&root);
    store_owned_managed_file(
        &storage,
        &game,
        &root.join("renderpilot.addon64"),
        &file_path,
        sha256(bytes),
    );
    mark_absent(&storage, &game);

    let owners = storage
        .read_local_cleanup_owners(game.id())
        .expect("owners");
    let RetiredRootObservation::ResidueOnly(residue) = observe_registered_root(&owners) else {
        panic!("exact managed file should be a residue-only root");
    };
    assert_eq!(residue.remnants().len(), 1);
    assert_eq!(residue.remnants()[0].path(), &path_ref(&file_path));
    assert!(residue.remnants()[0].removable());
    assert_eq!(fs::read(&file_path).expect("read after observation"), bytes);

    fs::remove_file(&file_path).expect("remove remnant for all-gone observation");
    assert!(unrecorded_directory.is_dir());
    let owners = storage
        .read_local_cleanup_owners(game.id())
        .expect("owners after removal");
    let RetiredRootObservation::ResidueOnly(residue) = observe_registered_root(&owners) else {
        panic!("empty already-absent root should be collectable");
    };
    assert!(residue.remnants().is_empty());
    let readiness = storage
        .catalog_readiness(game.id())
        .expect("readiness before all-gone collection");
    storage
        .collect_absent_installation(&game, AuthorityCas::new(readiness.authority_epoch()))
        .expect("established all-gone collection");
    assert!(
        storage
            .get_installed_addon(game.id())
            .expect("addon after all-gone collection")
            .is_none()
    );
    assert!(
        storage
            .is_installation_absent(game.id())
            .expect("still absent")
    );
    assert!(
        unrecorded_directory.is_dir(),
        "all-gone collection preserves unrecorded empty directories"
    );
}

#[test]
fn changed_owner_bytes_do_not_establish_residue_or_mutate_the_file() {
    let temp = TempTree::new();
    let root = temp.install_root();
    let file_path = root.join("owned.dll");
    fs::write(&file_path, b"initial bytes").expect("write original");

    let storage = SqliteStorage::in_memory().expect("storage");
    let game = game(&root);
    store_owned_managed_file(
        &storage,
        &game,
        &root.join("renderpilot.addon64"),
        &file_path,
        sha256(b"initial bytes"),
    );
    fs::write(&file_path, b"user changed bytes").expect("change bytes");

    let owners = storage
        .read_local_cleanup_owners(game.id())
        .expect("owners");
    assert!(matches!(
        observe_registered_root(&owners),
        RetiredRootObservation::PresentNonResidue { .. }
    ));
    assert_eq!(
        fs::read(&file_path).expect("file remains"),
        b"user changed bytes"
    );
}

#[test]
fn first_unknown_file_stops_before_content_hashing() {
    let temp = TempTree::new();
    let root = temp.install_root();
    fs::write(
        root.join("large-game-archive.bin"),
        vec![0x5a; 2 * 1024 * 1024],
    )
    .expect("write unknown game archive");

    let storage = SqliteStorage::in_memory().expect("storage");
    let game = game(&root);
    storage.upsert_game(&game).expect("store game");
    let owners = storage
        .read_local_cleanup_owners(game.id())
        .expect("owners");
    walker::reset_claimed_file_observations();

    assert!(matches!(
        observe_registered_root(&owners),
        RetiredRootObservation::PresentNonResidue { .. }
    ));
    assert!(walker::claimed_file_observations().is_empty());
}

#[test]
fn active_empty_root_is_not_absence_evidence() {
    let temp = TempTree::new();
    let root = temp.install_root();
    let storage = SqliteStorage::in_memory().expect("storage");
    let game = game(&root);
    storage.upsert_game(&game).expect("store game");
    let owners = storage
        .read_local_cleanup_owners(game.id())
        .expect("owners");

    assert!(matches!(
        observe_registered_root(&owners),
        RetiredRootObservation::PresentNonResidue { .. }
    ));
    assert!(owners.availability().is_active());
}

#[test]
fn active_directory_only_root_is_not_absence_evidence() {
    let temp = TempTree::new();
    let root = temp.install_root();
    let unrecorded_directory = root.join("bin");
    fs::create_dir(&unrecorded_directory).expect("create unrecorded directory");
    let storage = SqliteStorage::in_memory().expect("storage");
    let game = game(&root);
    storage.upsert_game(&game).expect("store game");
    let owners = storage
        .read_local_cleanup_owners(game.id())
        .expect("owners");

    assert!(matches!(
        observe_registered_root(&owners),
        RetiredRootObservation::PresentNonResidue { .. }
    ));
    assert!(owners.availability().is_active());
    assert!(unrecorded_directory.is_dir());
}

#[test]
fn normal_receipt_renodx_and_completed_component_receipts_observe_eight_exact_files_read_only() {
    let fixture = super::test_support::normal_renodx_fixture();
    let before = super::test_support::file_snapshot(&fixture.files);
    let owners = fixture
        .context
        .storage()
        .read_local_cleanup_owners(fixture.game.id())
        .expect("persisted producer receipts");

    let RetiredRootObservation::ResidueOnly(root) = observe_registered_root(&owners) else {
        panic!("the complete ordinary receipt should prove its eight-file root");
    };
    assert_eq!(root.remnants().len(), 8);
    assert_eq!(
        root.remnants()
            .iter()
            .filter(|remnant| remnant.category() == LocalCleanupCategory::AddonFile)
            .count(),
        3
    );
    assert_eq!(
        root.remnants()
            .iter()
            .filter(|remnant| remnant.category() == LocalCleanupCategory::ComponentFile)
            .count(),
        5
    );
    assert!(root.remnants().iter().all(|remnant| remnant.removable()));
    assert_eq!(super::test_support::file_snapshot(&fixture.files), before);
}
