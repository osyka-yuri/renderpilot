use super::*;

#[test]
fn filesystem_and_system_roots_have_a_typed_user_facing_rejection() {
    for root in ["/", "C:/", "//server/share", "C:/Windows"] {
        assert!(matches!(
            validate_root_invariants(root),
            Err(ServiceError::InvalidInstallRoot { .. })
        ));
    }
}

#[cfg(windows)]
#[test]
fn repeated_inspection_of_a_rejected_executable_is_stable() {
    let temp = tempfile::tempdir().expect("temp");
    let root = temp.path().join("Game");
    std::fs::create_dir_all(&root).expect("root");
    let mut bytes = vec![0_u8; 0x84];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[0x3c..0x40].copy_from_slice(&0x80_u32.to_le_bytes());
    bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
    std::fs::write(root.join("CustomLauncher.exe"), bytes).expect("launcher executable");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let first = inspect_game_install(&context, &root).expect("first inspection");
    let sibling = temp.path().join("Unrelated Game");
    std::fs::create_dir_all(&sibling).expect("sibling");
    std::fs::copy(
        std::env::current_exe().expect("current exe"),
        sibling.join("UnrelatedGame.exe"),
    )
    .expect("sibling executable");
    let second = inspect_game_install(&context, &root).expect("second inspection");

    assert_eq!(first, second);
}

#[cfg(windows)]
#[test]
fn add_game_rejects_a_stale_inspection_before_catalog_mutation() {
    let temp = tempfile::tempdir().expect("temp");
    let root = temp.path().join("Game");
    std::fs::create_dir_all(&root).expect("root");
    std::fs::copy(
        std::env::current_exe().expect("current exe"),
        root.join("Game.exe"),
    )
    .expect("game exe");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let inspection = inspect_game_install(&context, &root).expect("inspection");

    std::fs::copy(
        std::env::current_exe().expect("current exe"),
        root.join("GameMultiplayer.exe"),
    )
    .expect("second exe");

    let error = add_game(
        &context,
        AddGameRequest {
            selected_root: root,
            root_choice: AddGameRootChoice::Selected,
            allow_root_correction: false,
            chosen_executable: None,
            inspection_fingerprint: inspection.inspection_fingerprint,
        },
    )
    .expect_err("stale inspection");

    assert!(matches!(error, ServiceError::StaleInstallInspection { .. }));
    assert!(context.storage().list_games().expect("games").is_empty());
}

#[cfg(windows)]
#[test]
fn user_can_confirm_an_unreal_project_subtree_instead_of_its_recommendation() {
    let temp = tempfile::tempdir().expect("temp");
    let distribution_root = temp.path().join("Jedi Survivor");
    let project_root = distribution_root.join("SwGame");
    std::fs::create_dir_all(distribution_root.join("Engine").join("Binaries")).expect("engine");
    std::fs::create_dir_all(project_root.join("Content")).expect("content");
    let binary_root = project_root.join("Binaries").join("Win64");
    std::fs::create_dir_all(&binary_root).expect("binaries");
    std::fs::copy(
        std::env::current_exe().expect("current exe"),
        binary_root.join("JediSurvivor.exe"),
    )
    .expect("game executable");

    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let inspection = inspect_game_install(&context, &project_root).expect("inspection");

    assert_eq!(
        inspection.boundary.kind,
        InstallBoundaryKind::EngineProjectSubtree
    );
    assert_eq!(
        inspection
            .recommendation
            .as_ref()
            .map(|recommendation| recommendation.root.key()),
        install_paths::install_path_match_key(
            &canonical_path_text(&distribution_root).expect("distribution path"),
        )
        .as_ref()
    );

    let result = add_game(
        &context,
        AddGameRequest {
            selected_root: project_root.clone(),
            root_choice: AddGameRootChoice::Selected,
            allow_root_correction: false,
            chosen_executable: None,
            inspection_fingerprint: inspection.inspection_fingerprint,
        },
    )
    .expect("confirm selected subtree");

    let game_id = GameId::new(result.game_id).expect("game id");
    let game = context
        .storage()
        .find_game(&game_id)
        .expect("game query")
        .expect("game");
    assert_eq!(
        install_paths::install_path_match_key(game.install_path().as_str()),
        install_paths::install_path_match_key(
            &canonical_path_text(&project_root).expect("project path"),
        )
    );
}

#[cfg(windows)]
#[test]
fn recommended_root_confirmation_rejects_changed_executable_identity() {
    let temp = tempfile::tempdir().expect("temp");
    let distribution_root = temp.path().join("Jedi Survivor");
    let project_root = distribution_root.join("SwGame");
    std::fs::create_dir_all(distribution_root.join("Engine").join("Binaries")).expect("engine");
    std::fs::create_dir_all(project_root.join("Content")).expect("content");
    let binary_root = project_root.join("Binaries").join("Win64");
    std::fs::create_dir_all(&binary_root).expect("binaries");
    std::fs::copy(
        std::env::current_exe().expect("current exe"),
        binary_root.join("JediSurvivor.exe"),
    )
    .expect("game executable");

    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let inspection = inspect_game_install(&context, &project_root).expect("inspection");
    assert!(
        inspection
            .decision
            .option_for(AddGameRootChoice::Recommended)
            .is_some(),
        "fixture must expose the recommended root"
    );

    std::fs::copy(
        std::env::current_exe().expect("current exe"),
        distribution_root.join("JediSurvivorLauncher.exe"),
    )
    .expect("new recommended-root executable");

    let error = add_game(
        &context,
        AddGameRequest {
            selected_root: project_root,
            root_choice: AddGameRootChoice::Recommended,
            allow_root_correction: false,
            chosen_executable: None,
            inspection_fingerprint: inspection.inspection_fingerprint,
        },
    )
    .expect_err("changed effective-root identity must require reinspection");

    assert!(matches!(error, ServiceError::StaleInstallInspection { .. }));
    assert!(context.storage().list_games().expect("games").is_empty());
}

#[cfg(windows)]
#[test]
fn inspection_explains_why_a_folder_without_a_readable_executable_is_not_addable() {
    let temp = tempfile::tempdir().expect("temp");
    let root = temp.path().join("D3D12");
    std::fs::create_dir_all(&root).expect("root");
    std::fs::write(root.join("D3D12Core.dll"), b"not an executable").expect("component");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let inspection = inspect_game_install(&context, &root).expect("inspection");

    assert!(matches!(
        inspection.decision,
        AddGameDecision::Unavailable { ref reasons }
            if reasons == &[AddGameUnavailableReason::NoReadableExecutable]
    ));
    assert!(
        inspection.warnings.is_empty(),
        "an unavailable reason must not also be returned as a warning"
    );
    assert!(ensure_addable(&inspection, None).is_err());
}
