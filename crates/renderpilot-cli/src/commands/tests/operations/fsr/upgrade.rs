use super::*;

#[test]
fn apply_then_rollback_fsr_upgrade_replaces_entrypoint_and_adds_members() {
    let fixture = CatalogFixture::new("fsr-upgrade");
    let game_folder = TempGameFolder::new("fsr-upgrade-game");
    let artifact_folder = TempGameFolder::new("fsr-upgrade-artifact");
    fs::create_dir_all(game_folder.path()).expect("game folder");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder");

    // The FSR 3.1 game loads a single `amd_fidelityfx_dx12.dll` entry point.
    let original_name = FSR_ENTRY_POINT_FILE;
    let original_path = game_folder.path().join(original_name);
    fs::write(&original_path, b"fsr3-original").expect("original written");

    // FSR 4 package: the loader takes over `amd_fidelityfx_dx12.dll` via `install_as`;
    // the upscaler (the representative member) and frame generation are added under
    // their own names.
    let bundle: [(&str, &[u8], Option<&str>); 3] = [
        ("amd_fidelityfx_upscaler_dx12.dll", b"fsr4-upscaler", None),
        (
            "amd_fidelityfx_loader_dx12.dll",
            b"fsr4-loader",
            Some(original_name),
        ),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"fsr4-framegen",
            None,
        ),
    ];
    let (artifact, artifact_id) = write_fsr_bundle_artifact(artifact_folder.path(), &bundle);

    let game = store_manual_game(&fixture, &game_folder, "FSR Game");
    store_single_file_fsr_component(&fixture, &game, &original_path, "3.1.0", b"fsr3-original");
    fixture.store_artifact(&artifact);

    // -- apply (1 -> 3) --
    fixture
        .run(args(&[
            "apply",
            "--game",
            game.id().as_str(),
            "--component",
            "component:fsr",
            "--artifact",
            &artifact_id,
        ]))
        .map(|output| serde_json::from_str::<serde_json::Value>(&output).expect("valid apply json"))
        .map(|json| {
            assert_eq!(json["component_id"], "component:fsr");
            assert_eq!(json["applied_path"], path_string(&original_path));
            assert_eq!(
                json["updated_file_count"], 3,
                "one replacement plus two additions affect three live files"
            );
            assert!(
                json.get("affected_file_count").is_none(),
                "the generic pre-refactor counter must not remain in the public contract"
            );
        })
        .expect("apply should succeed");

    // The loader took over the entry-point name; the original is backed up once.
    let original_bak = game_folder.path().join(format!("{original_name}.bak"));
    assert_eq!(
        fs::read(&original_path).expect("entry point present"),
        b"fsr4-loader",
        "the loader is installed as the entry point"
    );
    assert_eq!(
        fs::read(&original_bak).expect("entry point backed up"),
        b"fsr3-original",
        "the original FSR 3.1 entry point is preserved as .bak"
    );

    // The other members are added under their own names, with no `.bak`.
    let added: [(&str, &[u8]); 2] = [
        ("amd_fidelityfx_upscaler_dx12.dll", b"fsr4-upscaler"),
        ("amd_fidelityfx_framegeneration_dx12.dll", b"fsr4-framegen"),
    ];
    for (name, bytes) in added {
        let placed = game_folder.path().join(name);
        assert_eq!(fs::read(&placed).expect("member copied"), bytes);
        assert!(
            !game_folder.path().join(format!("{name}.bak")).exists(),
            "no .bak should be created for added member {name}"
        );
    }
    let components = fixture
        .storage()
        .list_components_for_game(game.id())
        .expect("components load");
    assert_eq!(components.len(), 1);
    assert_eq!(
        components[0].files().len(),
        3,
        "active set becomes the three-file FSR 4 package"
    );

    // -- rollback (3 -> 1) --
    fixture
        .run(args(&[
            "rollback",
            "--game",
            game.id().as_str(),
            "--component",
            "component:fsr",
        ]))
        .map(|output| {
            serde_json::from_str::<serde_json::Value>(&output).expect("valid rollback json")
        })
        .map(|json| {
            assert_eq!(
                json["restored_file_count"], 1,
                "the single baseline file is restored while overlay-only files are removed"
            );
            assert!(
                json.get("affected_file_count").is_none(),
                "the generic pre-refactor counter must not remain in the public contract"
            );
        })
        .expect("rollback should succeed");

    assert_eq!(
        fs::read(&original_path).expect("original restored"),
        b"fsr3-original",
        "rollback restores the original FSR 3.1 entry point"
    );
    assert!(!original_bak.exists(), ".bak consumed on restore");
    assert_eq!(
        dir_file_names(game_folder.path()),
        vec![original_name.to_string()],
        "directory is clean: only the original remains, no FSR 4 orphans"
    );
    let components = fixture
        .storage()
        .list_components_for_game(game.id())
        .expect("components load");
    assert_eq!(
        components[0].files().len(),
        1,
        "catalog rolled back to the single original file"
    );

    let second = fixture
        .run(args(&[
            "rollback",
            "--game",
            game.id().as_str(),
            "--component",
            "component:fsr",
        ]))
        .expect_err("second rollback should fail because the baseline is cleared");
    assert!(second.to_string().contains("no swap to roll back"));
}

/// Native FSR 4 components are single-file overlays: swapping the upscaler must
/// leave the loader and frame-generation siblings untouched, and rollback must
/// restore only that one DLL.
#[test]
fn apply_then_rollback_native_fsr_upscaler_only_touches_that_dll() {
    let fixture = CatalogFixture::new("native-fsr-upscaler");
    let game_folder = TempGameFolder::new("native-fsr-upscaler-game");
    let artifact_folder = TempGameFolder::new("native-fsr-upscaler-artifact");
    fs::create_dir_all(game_folder.path()).expect("game folder");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder");

    let loader_path = game_folder.path().join("amd_fidelityfx_loader_dx12.dll");
    let upscaler_path = game_folder.path().join("amd_fidelityfx_upscaler_dx12.dll");
    let framegen_path = game_folder
        .path()
        .join("amd_fidelityfx_framegeneration_dx12.dll");
    fs::write(&loader_path, b"native-loader").expect("loader written");
    fs::write(&upscaler_path, b"native-upscaler-a").expect("upscaler written");
    fs::write(&framegen_path, b"native-framegen").expect("framegen written");

    let replacement_path = artifact_folder
        .path()
        .join("amd_fidelityfx_upscaler_dx12.dll");
    fs::write(&replacement_path, b"native-upscaler-b").expect("replacement written");

    let install_path = path_string(game_folder.path());
    let game_id = format!("manual:{install_path}");
    let game = sample_game(&game_id, "Native FSR Game", &install_path);
    fixture.store_game(&game);
    fixture.store_complete_components(
        game.id(),
        &[
            sample_component(
                "component:fsr-loader",
                game.id().as_str(),
                LibraryTechnology::AmdFsrLoader,
                Swappability::Swappable,
                &path_string(&loader_path),
                Some("2.1.0"),
                &sha256_hex(b"native-loader"),
            ),
            sample_component(
                "component:fsr-upscaler",
                game.id().as_str(),
                LibraryTechnology::AmdFsrUpscaler,
                Swappability::Swappable,
                &path_string(&upscaler_path),
                Some("4.0.3"),
                &sha256_hex(b"native-upscaler-a"),
            ),
            sample_component(
                "component:fsr-framegen",
                game.id().as_str(),
                LibraryTechnology::AmdFsrFrameGeneration,
                Swappability::Swappable,
                &path_string(&framegen_path),
                Some("4.0.0"),
                &sha256_hex(b"native-framegen"),
            ),
        ],
    );
    fixture.store_artifact(&sample_artifact(
        "artifact:fsr-upscaler-4.1",
        LibraryTechnology::AmdFsrUpscaler,
        &path_string(&replacement_path),
        Some("4.1.0"),
        &sha256_hex(b"native-upscaler-b"),
        None,
    ));

    fixture
        .run(args(&[
            "apply",
            "--game",
            game.id().as_str(),
            "--component",
            "component:fsr-upscaler",
            "--artifact",
            "artifact:fsr-upscaler-4.1",
        ]))
        .expect("apply should succeed");

    assert_eq!(
        fs::read(&upscaler_path).expect("upscaler present"),
        b"native-upscaler-b",
        "the upscaler should be replaced"
    );
    assert_eq!(
        fs::read(&loader_path).expect("loader present"),
        b"native-loader",
        "the loader must remain untouched"
    );
    assert_eq!(
        fs::read(&framegen_path).expect("framegen present"),
        b"native-framegen",
        "frame generation must remain untouched"
    );

    let upscaler_bak = game_folder
        .path()
        .join("amd_fidelityfx_upscaler_dx12.dll.bak");
    assert_eq!(
        fs::read(&upscaler_bak).expect("upscaler backup present"),
        b"native-upscaler-a",
        "the original upscaler should be backed up for rollback"
    );
    assert!(
        !game_folder
            .path()
            .join("amd_fidelityfx_loader_dx12.dll.bak")
            .exists(),
        "untouched siblings must not receive backup sidecars"
    );

    let components = fixture
        .storage()
        .list_components_for_game(game.id())
        .expect("components load");
    assert_eq!(components.len(), 3);
    let upscaler_component = components
        .iter()
        .find(|component| component.id().as_str() == "component:fsr-upscaler")
        .expect("upscaler component present");
    assert_eq!(
        upscaler_component.files()[0]
            .sha256()
            .map(|sha| sha.as_str()),
        Some(sha256_hex(b"native-upscaler-b").as_str()),
        "the catalog should track the replaced upscaler only"
    );

    fixture
        .run(args(&[
            "rollback",
            "--game",
            game.id().as_str(),
            "--component",
            "component:fsr-upscaler",
        ]))
        .expect("rollback should succeed");

    assert_eq!(
        fs::read(&upscaler_path).expect("upscaler restored"),
        b"native-upscaler-a",
        "rollback restores the original upscaler"
    );
    assert!(
        !upscaler_bak.exists(),
        "the upscaler backup is consumed on restore"
    );
    assert_eq!(
        fs::read(&loader_path).expect("loader present after rollback"),
        b"native-loader",
        "rollback still leaves the loader untouched"
    );
    assert_eq!(
        fs::read(&framegen_path).expect("framegen present after rollback"),
        b"native-framegen",
        "rollback still leaves frame generation untouched"
    );
}

/// Re-swapping a component (A -> B -> C) must keep the *original* A baseline so a
/// later rollback restores A, not the intermediate release B. Both FSR 4 releases
/// install their loader as the same `amd_fidelityfx_dx12.dll` entry point, so the
/// re-swap reverts to A before overlaying C -- the backup always holds A, and B's
/// dropped member leaves no orphan.
#[test]
fn reswap_preserves_original_baseline_then_rollback_restores_it() {
    let fixture = CatalogFixture::new("bundle-reswap");
    let game_folder = TempGameFolder::new("bundle-reswap-game");
    let lib_b = TempGameFolder::new("bundle-reswap-b");
    let lib_c = TempGameFolder::new("bundle-reswap-c");
    fs::create_dir_all(game_folder.path()).expect("game folder");
    fs::create_dir_all(lib_b.path()).expect("lib b");
    fs::create_dir_all(lib_c.path()).expect("lib c");

    // The FSR 3.1 game loads a single `amd_fidelityfx_dx12.dll` entry point = A.
    let original_name = FSR_ENTRY_POINT_FILE;
    let original_path = game_folder.path().join(original_name);
    fs::write(&original_path, b"original-A").expect("original written");

    // Release B = loader(as dx12) + upscaler; release C = loader(as dx12) + framegen.
    // Each loader takes over the entry point; C drops B's upscaler member.
    let bundle_b: [(&str, &[u8], Option<&str>); 2] = [
        (
            "amd_fidelityfx_loader_dx12.dll",
            b"B-loader",
            Some(original_name),
        ),
        ("amd_fidelityfx_upscaler_dx12.dll", b"B-upscaler", None),
    ];
    let bundle_c: [(&str, &[u8], Option<&str>); 2] = [
        (
            "amd_fidelityfx_loader_dx12.dll",
            b"C-loader",
            Some(original_name),
        ),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"C-framegen",
            None,
        ),
    ];
    let (artifact_b, id_b) = write_fsr_bundle_artifact(lib_b.path(), &bundle_b);
    let (artifact_c, id_c) = write_fsr_bundle_artifact(lib_c.path(), &bundle_c);

    let game = store_manual_game(&fixture, &game_folder, "Reswap Game");
    store_single_file_fsr_component(&fixture, &game, &original_path, "3.1.0", b"original-A");
    fixture.store_artifact(&artifact_b);
    fixture.store_artifact(&artifact_c);

    let apply = |artifact_id: &str| {
        fixture
            .run(args(&[
                "apply",
                "--game",
                game.id().as_str(),
                "--component",
                "component:fsr",
                "--artifact",
                artifact_id,
            ]))
            .expect("apply should succeed");
    };

    apply(&id_b);
    apply(&id_c);

    // After A -> B -> C the directory holds release C plus the original A backup.
    let original_bak = game_folder.path().join(format!("{original_name}.bak"));
    assert_eq!(
        dir_file_names(game_folder.path()),
        vec![
            "amd_fidelityfx_dx12.dll".to_string(),
            "amd_fidelityfx_dx12.dll.bak".to_string(),
            "amd_fidelityfx_framegeneration_dx12.dll".to_string(),
        ],
        "B's upscaler member is gone; the entry point is backed up once"
    );
    assert_eq!(
        fs::read(&original_path).expect("entry point present"),
        b"C-loader",
        "the current entry point is release C's loader"
    );
    assert_eq!(
        fs::read(&original_bak).expect("backup present"),
        b"original-A",
        "the backup still holds the original A, not the intermediate release B"
    );

    fixture
        .run(args(&[
            "rollback",
            "--game",
            game.id().as_str(),
            "--component",
            "component:fsr",
        ]))
        .expect("rollback should succeed");

    assert_eq!(
        fs::read(&original_path).expect("original A restored"),
        b"original-A",
        "rollback restores the original A, not intermediate release B"
    );
    assert_eq!(
        dir_file_names(game_folder.path()),
        vec![original_name.to_string()],
        "directory clean after rollback across re-swaps"
    );
}

/// A game **already on FSR 4** (the three split DLLs, with the loader installed as
/// `amd_fidelityfx_dx12.dll`) upgraded to a newer FSR 4 release: every member is a
/// Replace (each backed up once), and rollback restores the *previous FSR 4
/// release* -- never a synthetic FSR 3. There is no FSR 3 to fall back to here, so
/// the baseline is the FSR 4 set that was present when RenderPilot first swapped.
#[test]
fn already_fsr4_upgrade_replaces_all_members_then_rollback_restores_prior_release() {
    let fixture = CatalogFixture::new("fsr4-to-fsr4");
    let game_folder = TempGameFolder::new("fsr4-to-fsr4-game");
    let artifact_folder = TempGameFolder::new("fsr4-to-fsr4-artifact");
    fs::create_dir_all(game_folder.path()).expect("game folder");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder");

    // The game is already on FSR 4 release X: the loader sits under the entry-point
    // name `amd_fidelityfx_dx12.dll`, alongside the upscaler and frame generation.
    let members: [(&str, &[u8], &str); 3] = [
        ("amd_fidelityfx_dx12.dll", b"X-loader", "2.0.0"),
        ("amd_fidelityfx_upscaler_dx12.dll", b"X-upscaler", "4.0.2"),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"X-framegen",
            "3.1.5",
        ),
    ];
    let written = write_versioned_component_members(game_folder.path(), &members);

    let game = store_manual_game(&fixture, &game_folder, "FSR4 Game");
    store_written_fsr_bundle_component(&fixture, &game, &written);

    // FSR 4 release Y package: loader (as the dx12 entry point) + upscaler + framegen.
    let bundle: [(&str, &[u8], Option<&str>); 3] = [
        ("amd_fidelityfx_upscaler_dx12.dll", b"Y-upscaler", None),
        (
            "amd_fidelityfx_loader_dx12.dll",
            b"Y-loader",
            Some("amd_fidelityfx_dx12.dll"),
        ),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"Y-framegen",
            None,
        ),
    ];
    let (artifact, artifact_id) = write_fsr_bundle_artifact(artifact_folder.path(), &bundle);
    fixture.store_artifact(&artifact);

    // -- apply X -> Y: every member is replaced, each backed up once --
    fixture
        .run(args(&[
            "apply",
            "--game",
            game.id().as_str(),
            "--component",
            "component:fsr",
            "--artifact",
            &artifact_id,
        ]))
        .expect("apply should succeed");

    let expectations: [(&str, &[u8], &[u8]); 3] = [
        ("amd_fidelityfx_dx12.dll", b"Y-loader", b"X-loader"),
        (
            "amd_fidelityfx_upscaler_dx12.dll",
            b"Y-upscaler",
            b"X-upscaler",
        ),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"Y-framegen",
            b"X-framegen",
        ),
    ];
    for (name, current, backup) in expectations {
        assert_eq!(
            fs::read(game_folder.path().join(name)).expect("member present"),
            current
        );
        assert_eq!(
            fs::read(game_folder.path().join(format!("{name}.bak"))).expect("member backup"),
            backup,
            "each replaced FSR 4 member is backed up once"
        );
    }
    let components = fixture
        .storage()
        .list_components_for_game(game.id())
        .expect("components load");
    assert_eq!(
        components[0].files().len(),
        3,
        "the active set is still the three-file FSR 4 release"
    );

    // -- rollback Y -> X: restores the prior FSR 4 release, not FSR 3 --
    fixture
        .run(args(&[
            "rollback",
            "--game",
            game.id().as_str(),
            "--component",
            "component:fsr",
        ]))
        .expect("rollback should succeed");

    let originals: [(&str, &[u8]); 3] = [
        ("amd_fidelityfx_dx12.dll", b"X-loader"),
        ("amd_fidelityfx_upscaler_dx12.dll", b"X-upscaler"),
        ("amd_fidelityfx_framegeneration_dx12.dll", b"X-framegen"),
    ];
    for (name, original) in originals {
        assert_eq!(
            fs::read(game_folder.path().join(name)).expect("restored member"),
            original,
            "rollback restores the prior FSR 4 release, not FSR 3"
        );
    }
    assert_eq!(
        dir_file_names(game_folder.path()),
        vec![
            "amd_fidelityfx_dx12.dll".to_string(),
            "amd_fidelityfx_framegeneration_dx12.dll".to_string(),
            "amd_fidelityfx_upscaler_dx12.dll".to_string(),
        ],
        "exactly the prior FSR 4 release remains, with no .bak leftovers"
    );
}

/// A game **natively on FSR 4** loads the loader under its own name
/// `amd_fidelityfx_loader_dx12.dll` (it was never an FSR 3.1 game, so there is no
/// `amd_fidelityfx_dx12.dll`). An update must overwrite the loader *in place* -- not
/// strand it behind a fresh `amd_fidelityfx_dx12.dll`. Every member is replaced, no
/// orphan entry point appears, and rollback restores the prior release.
#[test]
fn native_split_fsr4_update_targets_the_loader_in_place_without_orphan_entrypoint() {
    let fixture = CatalogFixture::new("native-fsr4");
    let game_folder = TempGameFolder::new("native-fsr4-game");
    let artifact_folder = TempGameFolder::new("native-fsr4-artifact");
    fs::create_dir_all(game_folder.path()).expect("game folder");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder");

    // Native FSR 4 release X: the loader is under its OWN name; no `amd_fidelityfx_dx12.dll`.
    let members: [(&str, &[u8], &str); 3] = [
        ("amd_fidelityfx_loader_dx12.dll", b"X-loader", "2.0.0"),
        ("amd_fidelityfx_upscaler_dx12.dll", b"X-upscaler", "4.0.2"),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"X-framegen",
            "3.1.5",
        ),
    ];
    let written = write_versioned_component_members(game_folder.path(), &members);

    let game = store_manual_game(&fixture, &game_folder, "Native FSR4 Game");
    store_written_fsr_bundle_component(&fixture, &game, &written);

    // FSR 4 release Y package: the loader's `install_as` default is `amd_fidelityfx_dx12.dll`,
    // but it must adapt to the game's real entry point (`amd_fidelityfx_loader_dx12.dll`).
    let bundle: [(&str, &[u8], Option<&str>); 3] = [
        ("amd_fidelityfx_upscaler_dx12.dll", b"Y-upscaler", None),
        (
            "amd_fidelityfx_loader_dx12.dll",
            b"Y-loader",
            Some("amd_fidelityfx_dx12.dll"),
        ),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"Y-framegen",
            None,
        ),
    ];
    let (artifact, artifact_id) = write_fsr_bundle_artifact(artifact_folder.path(), &bundle);
    fixture.store_artifact(&artifact);

    fixture
        .run(args(&[
            "apply",
            "--game",
            game.id().as_str(),
            "--component",
            "component:fsr",
            "--artifact",
            &artifact_id,
        ]))
        .expect("apply should succeed");

    // The loader was updated IN PLACE under its own name; no orphan dx12 entry point.
    assert!(
        !game_folder.path().join("amd_fidelityfx_dx12.dll").exists(),
        "no stray amd_fidelityfx_dx12.dll is created for a natively split game"
    );
    let expectations: [(&str, &[u8], &[u8]); 3] = [
        ("amd_fidelityfx_loader_dx12.dll", b"Y-loader", b"X-loader"),
        (
            "amd_fidelityfx_upscaler_dx12.dll",
            b"Y-upscaler",
            b"X-upscaler",
        ),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"Y-framegen",
            b"X-framegen",
        ),
    ];
    for (name, current, backup) in expectations {
        assert_eq!(
            fs::read(game_folder.path().join(name)).expect("member present"),
            current,
            "the loader and members are updated in place"
        );
        assert_eq!(
            fs::read(game_folder.path().join(format!("{name}.bak"))).expect("member backup"),
            backup,
        );
    }

    // Rollback restores release X exactly, still no orphan dx12.
    fixture
        .run(args(&[
            "rollback",
            "--game",
            game.id().as_str(),
            "--component",
            "component:fsr",
        ]))
        .expect("rollback should succeed");

    assert!(!game_folder.path().join("amd_fidelityfx_dx12.dll").exists());
    assert_eq!(
        dir_file_names(game_folder.path()),
        vec![
            "amd_fidelityfx_framegeneration_dx12.dll".to_string(),
            "amd_fidelityfx_loader_dx12.dll".to_string(),
            "amd_fidelityfx_upscaler_dx12.dll".to_string(),
        ],
        "rollback restores the prior native FSR 4 release in place"
    );
}
