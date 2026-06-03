use super::scan_manual_folder;
use super::{
    json_string_array_set, path_string, query_all_game_cards, sample_game,
    sample_game_with_launcher, stored_install_paths, write_component_file, write_file,
    DesktopFixture, Launcher, DLSS_DLL,
};

use crate::catalog::auto_scan::{open_auto_scan_batch, scan_auto_in_batch};
use crate::catalog::{prune_auto_scan_orphans, with_catalog_storage};

fn store_manual_game(fixture: &DesktopFixture, title: &str, path: &str) {
    fixture.store_game(&sample_game(&format!("manual:{path}"), title, path));
}

#[test]
fn scan_auto_replaces_pre_existing_split_subfolder_entries_with_one_game() {
    let fixture = DesktopFixture::new("scan-auto-cleans-split-entries");

    let game_dir = tempfile::tempdir().expect("game dir");
    let amd_dir = game_dir.path().join("amd-ffx");
    let streamline_dir = game_dir.path().join("streamline");

    write_component_file(&amd_dir, DLSS_DLL, b"amd");
    write_component_file(&streamline_dir, DLSS_DLL, b"streamline");

    let amd_install_path = path_string(&amd_dir);
    let streamline_install_path = path_string(&streamline_dir);

    store_manual_game(&fixture, "amd-ffx", &amd_install_path);
    store_manual_game(&fixture, "streamline", &streamline_install_path);

    let batch = open_auto_scan_batch().expect("auto scan batch should open");
    let results = scan_auto_in_batch(&batch, game_dir.path()).expect("auto scan should succeed");

    assert_eq!(results.len(), 1);

    let games = fixture
        .storage
        .list_games()
        .expect("list games should succeed");
    let install_paths = games
        .iter()
        .map(|game| game.install_path().as_str().to_owned())
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(games.len(), 1, "stale subfolder rows should be pruned");
    assert!(install_paths.contains(path_string(game_dir.path()).as_str()));
    assert!(!install_paths.contains(amd_install_path.as_str()));
    assert!(!install_paths.contains(streamline_install_path.as_str()));
}

#[test]
fn scan_auto_treats_dlls_in_multiple_subfolders_as_a_single_game() {
    let _fixture = DesktopFixture::new("scan-auto-multiple-subfolders-one-game");

    let game_dir = tempfile::tempdir().expect("game dir");
    let amd_dir = game_dir.path().join("amd-ffx");
    let streamline_dir = game_dir.path().join("streamline");
    let nested_dir = game_dir
        .path()
        .join("Plugins")
        .join("NVIDIA")
        .join("Streamline");

    write_component_file(&amd_dir, DLSS_DLL, b"amd-bytes");
    write_component_file(&streamline_dir, DLSS_DLL, b"streamline-bytes");
    write_component_file(&nested_dir, DLSS_DLL, b"nested-bytes");

    let batch = open_auto_scan_batch().expect("auto scan batch should open");
    let results = scan_auto_in_batch(&batch, game_dir.path()).expect("auto scan should succeed");

    assert_eq!(
        results.len(),
        1,
        "auto scan must keep one game even when DLLs live in multiple sub-folders",
    );

    let game_cards = query_all_game_cards().expect("game cards should succeed");
    let cards = game_cards.items();

    assert_eq!(cards.len(), 1, "catalog must contain exactly one card");
    assert_eq!(cards[0]["install_path"], path_string(game_dir.path()));
    assert_eq!(
        cards[0]["component_count"], 3,
        "all three DLL sub-folders should bucket into the one game",
    );
}

#[test]
fn scan_auto_recovers_from_partial_non_empty_hash_cache() {
    let _fixture = DesktopFixture::new("scan-auto-recovers-partial-hash-cache");

    let game_dir = tempfile::tempdir().expect("game dir");
    let intel_dll = game_dir.path().join("libxess.dll");
    let amd_dll = game_dir.path().join("amd_fidelityfx_framegeneration.dll");
    let nvidia_dll = game_dir.path().join(DLSS_DLL);

    write_file(&intel_dll, b"intel-bytes");
    scan_manual_folder(game_dir.path().to_path_buf()).expect("warmup scan should succeed");

    write_file(&amd_dll, b"amd-bytes");
    write_file(&nvidia_dll, b"nvidia-bytes");

    let batch = open_auto_scan_batch().expect("auto scan batch should open");
    let results = scan_auto_in_batch(&batch, game_dir.path()).expect("auto scan should succeed");

    assert_eq!(results.len(), 1, "auto scan should keep one game result");

    let game_cards = query_all_game_cards().expect("game cards should succeed");
    let cards = game_cards.items();

    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0]["install_path"], path_string(game_dir.path()));
    assert_eq!(
        cards[0]["component_count"], 3,
        "auto scan should not keep stale partial fast-cache result",
    );

    let tags = json_string_array_set(&cards[0], "library_tags");
    assert!(tags.contains("intel_xess"));
    // The AMD FidelityFX frame-generation DLL is canonicalized to the AMD FSR
    // family, so the surfaced tag is `amd_fsr`, not `amd_fsr_frame_generation`.
    assert!(tags.contains("amd_fsr"));
    assert!(tags.contains("dlss_super_resolution"));
}

#[test]
fn prune_auto_scan_orphans_removes_library_root_and_unmatched_direct_children() {
    let fixture = DesktopFixture::new("prune-auto-scan-orphans-broad");

    let library_root_path = "C:/Program Files (x86)/Steam/steamapps/common";
    let real_game_path = "C:/Program Files (x86)/Steam/steamapps/common/RealGame";
    let runtime_path = "C:/Program Files (x86)/Steam/steamapps/common/Steam Controller Configs";
    let nested_path = "C:/Program Files (x86)/Steam/steamapps/common/RealGame/Plugins/MyMod";
    let unrelated_path = "D:/Games/Unrelated";

    store_manual_game(&fixture, "common", library_root_path);
    store_manual_game(&fixture, "RealGame", real_game_path);
    store_manual_game(&fixture, "Steam Controller Configs", runtime_path);
    store_manual_game(&fixture, "MyMod", nested_path);
    store_manual_game(&fixture, "Unrelated", unrelated_path);

    let removed = with_catalog_storage(|storage| {
        prune_auto_scan_orphans(
            storage,
            &[library_root_path.to_owned()],
            &[real_game_path.to_owned()],
        )
    })
    .expect("prune should succeed");

    assert_eq!(
        removed, 2,
        "library root and runtime sub-folder rows should be removed",
    );

    let remaining_paths = stored_install_paths(&fixture.storage);

    assert!(!remaining_paths.contains(library_root_path));
    assert!(!remaining_paths.contains(runtime_path));
    assert!(remaining_paths.contains(real_game_path));
    assert!(
        remaining_paths.contains(nested_path),
        "deeper-than-direct-child rows must be preserved",
    );
    assert!(remaining_paths.contains(unrelated_path));
}

#[test]
fn prune_auto_scan_orphans_is_case_insensitive_on_windows_paths() {
    let fixture = DesktopFixture::new("prune-auto-scan-orphans-case");

    let library_root_path = "C:/Program Files/EA Games";
    let stored_path = "C:/Program Files/EA Games";

    store_manual_game(&fixture, "EA Games", stored_path);

    let removed = with_catalog_storage(|storage| {
        prune_auto_scan_orphans(storage, &["c:/program files/ea games".to_owned()], &[])
    })
    .expect("prune should succeed");

    assert_eq!(removed, 1);
    assert!(
        fixture.storage.list_games().expect("list games").is_empty(),
        "case-insensitive match against {library_root_path} should remove the row",
    );
}

#[test]
fn prune_auto_scan_orphans_retains_case_and_trailing_slash_variants() {
    let fixture = DesktopFixture::new("prune-auto-scan-orphans-retained-variants");

    let library_root_path = "D:/SteamLibrary/steamapps/common";
    let stored_game_path = "d:/steamlibrary/steamapps/common/Game";
    let retained_path = "D:/SteamLibrary/steamapps/common/Game/";

    store_manual_game(&fixture, "Game", stored_game_path);

    let removed = with_catalog_storage(|storage| {
        prune_auto_scan_orphans(
            storage,
            &[library_root_path.to_owned()],
            &[retained_path.to_owned()],
        )
    })
    .expect("prune should succeed");

    assert_eq!(
        removed, 0,
        "retained path key must match stored install path",
    );

    let remaining_paths = stored_install_paths(&fixture.storage);
    assert!(
        remaining_paths.contains(stored_game_path),
        "game row must remain when retained list uses different case or trailing slash",
    );
}

#[test]
fn prune_auto_scan_orphans_removes_steam_launcher_direct_child_not_in_retained() {
    let fixture = DesktopFixture::new("prune-auto-scan-orphans-steam-launcher");

    let library_root_path = "C:/Program Files (x86)/Steam/steamapps/common";
    let real_steam_game_path = "C:/Program Files (x86)/Steam/steamapps/common/Portal";
    let steam_redist_path =
        "C:/Program Files (x86)/Steam/steamapps/common/Steamworks Common Redistributables";
    let steam_shared_path = "C:/Program Files (x86)/Steam/steamapps/common/Steamworks Shared";

    fixture.store_game(&sample_game_with_launcher(
        &format!("manual:{real_steam_game_path}"),
        "Portal",
        real_steam_game_path,
        Launcher::Steam,
        Some("400"),
    ));
    fixture.store_game(&sample_game_with_launcher(
        &format!("manual:{steam_redist_path}"),
        "Steamworks Common Redistributables",
        steam_redist_path,
        Launcher::Steam,
        Some("228980"),
    ));
    fixture.store_game(&sample_game_with_launcher(
        &format!("manual:{steam_shared_path}"),
        "Steamworks Shared",
        steam_shared_path,
        Launcher::Steam,
        Some("999999"),
    ));

    let removed = with_catalog_storage(|storage| {
        prune_auto_scan_orphans(
            storage,
            &[library_root_path.to_owned()],
            &[real_steam_game_path.to_owned()],
        )
    })
    .expect("prune should succeed");

    assert_eq!(
        removed, 2,
        "both Steam-launcher orphan rows should be pruned",
    );

    let remaining_paths = stored_install_paths(&fixture.storage);

    assert!(
        remaining_paths.contains(real_steam_game_path),
        "real Steam game must be retained because it is in retained_install_paths",
    );
    assert!(!remaining_paths.contains(steam_redist_path));
    assert!(!remaining_paths.contains(steam_shared_path));
}

#[test]
fn prune_auto_scan_orphans_keeps_legit_steam_card_when_in_retained() {
    let fixture = DesktopFixture::new("prune-auto-scan-orphans-steam-keep");

    let library_root_path = "C:/Program Files (x86)/Steam/steamapps/common";
    let portal_path = "C:/Program Files (x86)/Steam/steamapps/common/Portal";
    let half_life_path = "C:/Program Files (x86)/Steam/steamapps/common/Half-Life";

    fixture.store_game(&sample_game_with_launcher(
        &format!("manual:{portal_path}"),
        "Portal",
        portal_path,
        Launcher::Steam,
        Some("400"),
    ));
    fixture.store_game(&sample_game_with_launcher(
        &format!("manual:{half_life_path}"),
        "Half-Life",
        half_life_path,
        Launcher::Steam,
        Some("70"),
    ));

    let removed = with_catalog_storage(|storage| {
        prune_auto_scan_orphans(
            storage,
            &[library_root_path.to_owned()],
            &[portal_path.to_owned(), half_life_path.to_owned()],
        )
    })
    .expect("prune should succeed");

    assert_eq!(removed, 0, "rediscovered Steam games must not be pruned");
    assert_eq!(fixture.storage.list_games().expect("list games").len(), 2);
}

#[test]
fn prune_auto_scan_orphans_with_empty_library_roots_is_noop() {
    let fixture = DesktopFixture::new("prune-auto-scan-orphans-noop");

    let path = "C:/Games/Keep";
    store_manual_game(&fixture, "Keep", path);

    let removed = with_catalog_storage(|storage| prune_auto_scan_orphans(storage, &[], &[]))
        .expect("prune should succeed");

    assert_eq!(removed, 0);
    assert_eq!(fixture.storage.list_games().expect("list games").len(), 1);
}
