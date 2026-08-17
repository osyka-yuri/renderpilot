use super::*;

/// The FSR 3 <-> 4 toggle: a dx12-lineage game (loads `amd_fidelityfx_dx12.dll`) we
/// upgraded to FSR 4 can pick a **unified FSR 3.1** version again from the selector.
/// The re-swap reverts to the FSR 3.1 baseline first -- deleting the upscaler and
/// frame-gen -- then installs the chosen 3.1.x, so the folder lands on a clean FSR 3.1
/// (no FSR 4 leftovers) while the original baseline is preserved for rollback.
#[test]
fn dx12_lineage_downgrade_to_unified_fsr3_cleans_up_split_members() {
    let fixture = CatalogFixture::new("fsr-downgrade");
    let game_folder = TempGameFolder::new("fsr-downgrade-game");
    let fsr4_folder = TempGameFolder::new("fsr-downgrade-fsr4");
    let fsr3_folder = TempGameFolder::new("fsr-downgrade-fsr3");
    for folder in [&game_folder, &fsr4_folder, &fsr3_folder] {
        fs::create_dir_all(folder.path()).expect("folder");
    }

    let original_name = FSR_ENTRY_POINT_FILE;
    let original_path = game_folder.path().join(original_name);
    fs::write(&original_path, b"fsr3-original").expect("original written");

    // FSR 4 package: loader (as the dx12 entry point) + upscaler + frame generation.
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
    let (fsr4_artifact, fsr4_id) = write_fsr_bundle_artifact(fsr4_folder.path(), &bundle);

    // A newer unified FSR 3.1.4 backend (single `amd_fidelityfx_dx12.dll`).
    let fsr314_source = fsr3_folder.path().join(original_name);
    fs::write(&fsr314_source, b"fsr3.1.4").expect("fsr3.1.4 written");
    let fsr314 = sample_artifact(
        "artifact:fsr-3.1.4",
        LibraryTechnology::AmdFsr,
        &path_string(&fsr314_source),
        Some("3.1.4"),
        &sha256_hex(b"fsr3.1.4"),
        None,
    );
    let fsr314_id = fsr314.id().as_str().to_owned();

    let game = store_manual_game(&fixture, &game_folder, "FSR Game");
    store_single_file_fsr_component(&fixture, &game, &original_path, "3.1.0", b"fsr3-original");
    fixture.store_artifact(&fsr4_artifact);
    fixture.store_artifact(&fsr314);

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

    // 3.1 -> 4, then 4 -> unified 3.1.4 (the downgrade via the selector).
    apply(&fsr4_id);
    apply(&fsr314_id);

    assert_eq!(
        dir_file_names(game_folder.path()),
        vec![
            "amd_fidelityfx_dx12.dll".to_string(),
            "amd_fidelityfx_dx12.dll.bak".to_string(),
        ],
        "downgrade leaves a clean FSR 3.1: the upscaler and frame-gen are gone"
    );
    assert_eq!(
        fs::read(&original_path).expect("entry point"),
        b"fsr3.1.4",
        "the entry point is now the chosen FSR 3.1.4 backend"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll.bak")).expect("backup"),
        b"fsr3-original",
        "the backup still holds the original FSR 3.1, so rollback returns there"
    );
    let components = fixture
        .storage()
        .list_components_for_game(game.id())
        .expect("components load");
    assert_eq!(
        components[0].files().len(),
        1,
        "the active set is a single unified FSR 3.1 file again"
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
        fs::read(&original_path).expect("restored"),
        b"fsr3-original",
        "rollback restores the original FSR 3.1"
    );
    assert_eq!(
        dir_file_names(game_folder.path()),
        vec![original_name.to_string()],
        "directory is clean after rollback"
    );
}

/// A folder upgraded to FSR 4 **outside** RenderPilot (loader installed as the dx12
/// entry point, no FSR 3.1 baseline) must still downgrade cleanly: the first swap
/// backs up and removes the split members, so a later rollback restores that external
/// FSR 4 state exactly.
#[test]
fn externally_upgraded_fsr4_downgrade_removes_split_members_on_first_swap() {
    let fixture = CatalogFixture::new("fsr-external");
    let game_folder = TempGameFolder::new("fsr-external-game");
    let fsr3_folder = TempGameFolder::new("fsr-external-fsr3");
    for folder in [&game_folder, &fsr3_folder] {
        fs::create_dir_all(folder.path()).expect("folder");
    }

    // Externally upgraded FSR 4: the loader sits under the dx12 entry-point name.
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

    let fsr314_source = fsr3_folder.path().join(FSR_ENTRY_POINT_FILE);
    fs::write(&fsr314_source, b"fsr3.1.4").expect("fsr3.1.4 written");
    let fsr314 = sample_artifact(
        "artifact:fsr-3.1.4",
        LibraryTechnology::AmdFsr,
        &path_string(&fsr314_source),
        Some("3.1.4"),
        &sha256_hex(b"fsr3.1.4"),
        None,
    );
    let fsr314_id = fsr314.id().as_str().to_owned();

    let game = store_manual_game(&fixture, &game_folder, "External FSR4 Game");
    store_written_fsr_bundle_component(&fixture, &game, &written);
    fixture.store_artifact(&fsr314);

    fixture
        .run(args(&[
            "apply",
            "--game",
            game.id().as_str(),
            "--component",
            "component:fsr",
            "--artifact",
            &fsr314_id,
        ]))
        .expect("apply should succeed");

    // The split members are removed (backed up), and the entry point is FSR 3.1.4.
    assert!(
        !game_folder
            .path()
            .join("amd_fidelityfx_upscaler_dx12.dll")
            .exists(),
        "the upscaler is removed on the downgrade"
    );
    assert!(
        !game_folder
            .path()
            .join("amd_fidelityfx_framegeneration_dx12.dll")
            .exists(),
        "the frame-gen is removed on the downgrade"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll")).expect("entry point"),
        b"fsr3.1.4",
    );
    let backups: [(&str, &[u8]); 3] = [
        ("amd_fidelityfx_dx12.dll.bak", b"X-loader"),
        ("amd_fidelityfx_upscaler_dx12.dll.bak", b"X-upscaler"),
        ("amd_fidelityfx_framegeneration_dx12.dll.bak", b"X-framegen"),
    ];
    for (name, bytes) in backups {
        assert_eq!(
            fs::read(game_folder.path().join(name)).expect("backup present"),
            bytes,
            "the external FSR 4 member is backed up so rollback can restore it"
        );
    }
    let components = fixture
        .storage()
        .list_components_for_game(game.id())
        .expect("components load");
    assert_eq!(
        components[0].files().len(),
        1,
        "active set is the single FSR 3.1"
    );

    // Rollback restores the external FSR 4 state exactly.
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
        dir_file_names(game_folder.path()),
        vec![
            "amd_fidelityfx_dx12.dll".to_string(),
            "amd_fidelityfx_framegeneration_dx12.dll".to_string(),
            "amd_fidelityfx_upscaler_dx12.dll".to_string(),
        ],
        "rollback restores the external FSR 4 set, no orphans"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll")).expect("loader"),
        b"X-loader",
    );
}

/// Mixed lineage: a real unified FSR 3.1 entry point next to a loader+denoiser
/// Ray Regeneration stack the developers ship themselves. A unified FSR 3.1.x
/// update must replace ONLY the entry point -- the RR stack is an independent,
/// working feature, not an upscaling leftover. A re-swap and a rollback must
/// leave it untouched too.
#[test]
fn mixed_lineage_unified_update_replaces_only_the_entry_point() {
    let fixture = CatalogFixture::new("fsr-mixed-unified");
    let game_folder = TempGameFolder::new("fsr-mixed-unified-game");
    let lib_a = TempGameFolder::new("fsr-mixed-unified-a");
    let lib_b = TempGameFolder::new("fsr-mixed-unified-b");
    for folder in [&game_folder, &lib_a, &lib_b] {
        fs::create_dir_all(folder.path()).expect("folder");
    }

    // Entry point = real unified FSR 3.1; loader+denoiser = the RR stack.
    let members: [(&str, &[u8], &str); 3] = [
        ("amd_fidelityfx_dx12.dll", b"fsr3-original", "1.0.1.41314"),
        ("amd_fidelityfx_loader_dx12.dll", b"rr-loader", "2.1.0.604"),
        (
            "amd_fidelityfx_denoiser_dx12.dll",
            b"rr-denoiser",
            "1.0.0.604",
        ),
    ];
    let written = write_versioned_component_members(game_folder.path(), &members);

    let unified_artifact = |folder: &TempGameFolder, bytes: &[u8], version: &str, id: &str| {
        let source = folder.path().join(FSR_ENTRY_POINT_FILE);
        fs::write(&source, bytes).expect("unified artifact written");
        sample_artifact(
            id,
            LibraryTechnology::AmdFsr,
            &path_string(&source),
            Some(version),
            &sha256_hex(bytes),
            None,
        )
    };
    let fsr313 = unified_artifact(&lib_a, b"fsr3.1.3", "3.1.3", "artifact:fsr-3.1.3");
    let fsr314 = unified_artifact(&lib_b, b"fsr3.1.4", "3.1.4", "artifact:fsr-3.1.4");
    let fsr313_id = fsr313.id().as_str().to_owned();
    let fsr314_id = fsr314.id().as_str().to_owned();

    let game = store_manual_game(&fixture, &game_folder, "Mixed Lineage Game");
    store_written_fsr_bundle_component(&fixture, &game, &written);
    fixture.store_artifact(&fsr313);
    fixture.store_artifact(&fsr314);

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

    // -- unified update: only the entry point changes --
    apply(&fsr314_id);

    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll")).expect("entry point"),
        b"fsr3.1.4",
        "the unified update replaces the FSR 3.1 the game loads"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll.bak")).expect("backup"),
        b"fsr3-original",
    );
    assert_eq!(
        dir_file_names(game_folder.path()),
        vec![
            "amd_fidelityfx_denoiser_dx12.dll".to_string(),
            "amd_fidelityfx_dx12.dll".to_string(),
            "amd_fidelityfx_dx12.dll.bak".to_string(),
            "amd_fidelityfx_loader_dx12.dll".to_string(),
        ],
        "the RR stack is untouched: no removals, no extra backups"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_loader_dx12.dll")).expect("rr loader"),
        b"rr-loader",
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_denoiser_dx12.dll")).expect("rr denoiser"),
        b"rr-denoiser",
    );

    // -- re-swap to another unified version: the RR stack still survives --
    apply(&fsr313_id);

    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll")).expect("entry point"),
        b"fsr3.1.3",
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll.bak")).expect("backup"),
        b"fsr3-original",
        "the baseline backup still holds the original, not the intermediate 3.1.4"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_loader_dx12.dll")).expect("rr loader"),
        b"rr-loader",
        "a re-swap must not resurrect, remove, or overwrite the RR loader"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_denoiser_dx12.dll")).expect("rr denoiser"),
        b"rr-denoiser",
    );

    // -- rollback: the original entry point returns, the RR stack persists --
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
        dir_file_names(game_folder.path()),
        vec![
            "amd_fidelityfx_denoiser_dx12.dll".to_string(),
            "amd_fidelityfx_dx12.dll".to_string(),
            "amd_fidelityfx_loader_dx12.dll".to_string(),
        ],
        "rollback restores the entry point and leaves the RR stack in place"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll")).expect("restored"),
        b"fsr3-original",
    );
}

/// The mixed lineage upgraded to an FSR 4 package: the package's loader must
/// install AS the entry point (what the game loads for upscaling) -- never onto
/// the RR stack's `amd_fidelityfx_loader_dx12.dll`, which pairs with the game's
/// own denoiser. Rollback removes the added members and leaves the RR stack as
/// it was.
#[test]
fn mixed_lineage_fsr4_package_targets_entry_point_not_the_rr_loader() {
    let fixture = CatalogFixture::new("fsr-mixed-package");
    let game_folder = TempGameFolder::new("fsr-mixed-package-game");
    let artifact_folder = TempGameFolder::new("fsr-mixed-package-artifact");
    for folder in [&game_folder, &artifact_folder] {
        fs::create_dir_all(folder.path()).expect("folder");
    }

    let members: [(&str, &[u8], &str); 3] = [
        ("amd_fidelityfx_dx12.dll", b"fsr3-original", "1.0.1.41314"),
        ("amd_fidelityfx_loader_dx12.dll", b"rr-loader", "2.1.0.604"),
        (
            "amd_fidelityfx_denoiser_dx12.dll",
            b"rr-denoiser",
            "1.0.0.604",
        ),
    ];
    let written = write_versioned_component_members(game_folder.path(), &members);

    // FSR 4 package: loader (as the dx12 entry point) + upscaler + framegen.
    let bundle: [(&str, &[u8], Option<&str>); 3] = [
        ("amd_fidelityfx_upscaler_dx12.dll", b"fsr4-upscaler", None),
        (
            "amd_fidelityfx_loader_dx12.dll",
            b"fsr4-loader",
            Some(FSR_ENTRY_POINT_FILE),
        ),
        (
            "amd_fidelityfx_framegeneration_dx12.dll",
            b"fsr4-framegen",
            None,
        ),
    ];
    let (artifact, artifact_id) = write_fsr_bundle_artifact(artifact_folder.path(), &bundle);

    let game = store_manual_game(&fixture, &game_folder, "Mixed Lineage Game");
    store_written_fsr_bundle_component(&fixture, &game, &written);
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

    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll")).expect("entry point"),
        b"fsr4-loader",
        "the package loader takes over the entry point the game loads"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_loader_dx12.dll")).expect("rr loader"),
        b"rr-loader",
        "the RR stack's loader must not be overwritten by the package loader"
    );
    assert!(
        !game_folder
            .path()
            .join("amd_fidelityfx_loader_dx12.dll.bak")
            .exists(),
        "the RR loader is never touched, so it gets no backup"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_denoiser_dx12.dll")).expect("rr denoiser"),
        b"rr-denoiser",
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_upscaler_dx12.dll")).expect("upscaler"),
        b"fsr4-upscaler",
        "the upscaler is added under its own name"
    );
    assert_eq!(
        fs::read(
            game_folder
                .path()
                .join("amd_fidelityfx_framegeneration_dx12.dll")
        )
        .expect("framegen"),
        b"fsr4-framegen",
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
        dir_file_names(game_folder.path()),
        vec![
            "amd_fidelityfx_denoiser_dx12.dll".to_string(),
            "amd_fidelityfx_dx12.dll".to_string(),
            "amd_fidelityfx_loader_dx12.dll".to_string(),
        ],
        "rollback removes the added members; the RR stack and entry point are back to the original state"
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_dx12.dll")).expect("restored"),
        b"fsr3-original",
    );
    assert_eq!(
        fs::read(game_folder.path().join("amd_fidelityfx_loader_dx12.dll")).expect("rr loader"),
        b"rr-loader",
    );
}
