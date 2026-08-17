use super::*;

#[test]
fn relationship_blocks_child_of_existing_install() {
    let game = game(
        "game:existing",
        "C:/Games/Example",
        RootAuthority::UserConfirmed,
    );
    let relationship = classify_relationship(&install_root("C:/Games/Example/bin"), &[game], &[]);
    assert_eq!(relationship.kind, InstallRelationshipKind::InsideExisting);
}

#[test]
fn geometric_relationship_does_not_infer_narrowing_from_stored_paths() {
    let broad = game("game:existing", "C:/Games", RootAuthority::UserConfirmed)
        .with_executable_candidate(PathRef::new("The Last of Us Part I/tlou-i.exe").expect("exe"))
        .with_executable_candidate(PathRef::new("Black Flag/AC4BFSP.exe").expect("exe"));
    let games = [broad];
    let selected = install_root("C:/Games/The Last of Us Part I");
    let advisor = RootAdvisor::new(&selected, &games, &[]);
    let relationship = advisor.relationship();

    assert_eq!(relationship.kind, InstallRelationshipKind::InsideExisting);
    assert_eq!(relationship.game_ids, vec!["game:existing"]);
    assert_eq!(
        advisor.recommendation(&relationship),
        Some((
            install_root("C:/Games"),
            RootRecommendationSource::ExistingCatalog
        ))
    );
}

#[test]
fn relationship_does_not_narrow_into_normal_game_structure() {
    let game = game(
        "game:existing",
        "C:/Games/Example",
        RootAuthority::UserConfirmed,
    )
    .with_executable_candidate(PathRef::new("Binaries/Win64/Example.exe").expect("exe"))
    .with_executable_candidate(PathRef::new("Multiplayer/ExampleMP.exe").expect("exe"));

    let relationship =
        classify_relationship(&install_root("C:/Games/Example/Binaries"), &[game], &[]);

    assert_eq!(relationship.kind, InstallRelationshipKind::InsideExisting);
}

#[test]
fn explicit_root_correction_rejects_internal_children_and_different_launcher_scopes() {
    let broad = game("game:existing", "C:/Games", RootAuthority::UserConfirmed);
    let internal = install_root("C:/Games/Binaries");
    let internal_relationship = classify_relationship(&internal, std::slice::from_ref(&broad), &[]);
    assert!(
        root_correction_target(
            &internal,
            &internal_relationship,
            std::slice::from_ref(&broad)
        )
        .is_none()
    );

    let selected = install_root("C:/Games/The Last of Us Part I");
    let launcher_root = install_root("C:/Games/The Last of Us Part I/LauncherGame");
    let launcher_relationship =
        classify_relationship(&selected, std::slice::from_ref(&broad), &[launcher_root]);
    assert!(
        root_correction_target(&selected, &launcher_relationship, &[broad]).is_none(),
        "a manual card must not absorb a different launcher-proven installation scope"
    );
}

#[cfg(windows)]
#[test]
fn add_game_corrects_an_oversized_manual_library_card_to_one_child() {
    let temp = tempfile::tempdir().expect("temp");
    let games_root = temp.path().join("Games");
    let selected_root = games_root.join("The Last of Us Part I");
    let other_root = games_root.join("Black Flag");
    std::fs::create_dir_all(&selected_root).expect("selected root");
    std::fs::create_dir_all(&other_root).expect("other root");
    let selected_executable = selected_root.join("tlou-i.exe");
    std::fs::copy(
        std::env::current_exe().expect("current exe"),
        &selected_executable,
    )
    .expect("copy PE executable");

    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let broad = game(
        "game:oversized-root",
        &canonical_path_text(&games_root).expect("root path"),
        RootAuthority::UserConfirmed,
    )
    .with_executable_candidate(
        PathRef::new("The Last of Us Part I/tlou-i.exe").expect("selected candidate"),
    )
    .with_executable_candidate(PathRef::new("Black Flag/AC4BFSP.exe").expect("other candidate"));
    context.storage().upsert_game(&broad).expect("seed");

    let inspection = inspect_game_install(&context, &selected_root).expect("inspect");
    assert_eq!(
        inspection.relationship.kind,
        InstallRelationshipKind::NarrowsExisting
    );
    assert_eq!(inspection.recommendation, None);

    let result = add_game(
        &context,
        AddGameRequest {
            selected_root: selected_root.clone(),
            root_choice: AddGameRootChoice::Selected,
            allow_root_correction: true,
            chosen_executable: None,
            inspection_fingerprint: inspection.inspection_fingerprint,
        },
    )
    .expect("correct root");

    assert_eq!(result.game_id, broad.id().as_str());
    assert_eq!(result.disposition, AddGameDisposition::RootCorrected);
    let corrected = context
        .storage()
        .find_game(broad.id())
        .expect("read")
        .expect("corrected game");
    assert_eq!(
        install_paths::install_path_match_key(corrected.install_path().as_str()),
        install_paths::install_path_match_key(
            &canonical_path_text(&selected_root).expect("selected path"),
        )
    );
    assert_eq!(context.storage().list_games().expect("games").len(), 1);
    assert!(
        other_root.is_dir(),
        "root correction must never modify sibling game folders"
    );
}

#[cfg(windows)]
#[test]
fn add_game_expands_an_internal_bin_root_but_not_to_the_library_parent() {
    let temp = tempfile::tempdir().expect("temp");
    let games_root = temp.path().join("Games");
    let game_root = games_root.join("The Last of Us Part I");
    let bin_root = game_root.join("Bin");
    std::fs::create_dir_all(&bin_root).expect("bin root");
    std::fs::copy(
        std::env::current_exe().expect("current exe"),
        bin_root.join("tlou-i.exe"),
    )
    .expect("copy PE executable");
    let data_root = game_root.join("Data");
    std::fs::create_dir_all(&data_root).expect("data root");
    std::fs::write(data_root.join("game.pak"), b"distribution payload")
        .expect("distribution payload");

    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let existing = game(
        "game:too-narrow",
        &canonical_path_text(&bin_root).expect("bin path"),
        RootAuthority::UserConfirmed,
    )
    .with_executable_candidate(PathRef::new("tlou-i.exe").expect("candidate"));
    context.storage().upsert_game(&existing).expect("seed");

    let inspection = inspect_game_install(&context, &game_root).expect("inspect game root");
    assert_eq!(
        inspection.relationship.kind,
        InstallRelationshipKind::ExpandsExisting
    );
    assert!(
        inspection
            .root_correction
            .as_ref()
            .is_some_and(RootCorrectionAssessment::is_ready)
    );

    let result = add_game(
        &context,
        AddGameRequest {
            selected_root: game_root.clone(),
            root_choice: AddGameRootChoice::Selected,
            allow_root_correction: true,
            chosen_executable: None,
            inspection_fingerprint: inspection.inspection_fingerprint,
        },
    )
    .expect("expand to exact game root");
    assert_eq!(result.game_id, existing.id().as_str());
    assert_eq!(result.disposition, AddGameDisposition::RootCorrected);
    let corrected = context
        .storage()
        .find_game(existing.id())
        .expect("read")
        .expect("game");
    assert_eq!(
        corrected.install_path().as_str(),
        canonical_path_text(&game_root).expect("game path")
    );
    assert!(
        corrected
            .executable_candidates()
            .iter()
            .any(|candidate| candidate.as_str().eq_ignore_ascii_case("Bin/tlou-i.exe")),
        "executable candidates must be reindexed relative to the corrected root"
    );

    let library_inspection =
        inspect_game_install(&context, &games_root).expect("library inspection");
    assert!(
        matches!(
            library_inspection.decision,
            AddGameDecision::Unavailable {
                ref reasons
            } if reasons.contains(&AddGameUnavailableReason::ContainsProvenInstall)
        ),
        "library parent must remain blocked after root correction: {library_inspection:?}"
    );
}

#[cfg(windows)]
#[test]
fn broad_manual_parent_offers_a_boundary_proven_root_correction() {
    let temp = tempfile::tempdir().expect("temp");
    let games_root = temp.path().join("Games");
    let selected_root = games_root.join("The Last of Us Part I");
    std::fs::create_dir_all(&selected_root).expect("selected root");
    std::fs::copy(
        std::env::current_exe().expect("current exe"),
        selected_root.join("tlou-i.exe"),
    )
    .expect("copy PE executable");

    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let broad = game(
        "game:oversized-root",
        &canonical_path_text(&games_root).expect("root path"),
        RootAuthority::UserConfirmed,
    );
    context.storage().upsert_game(&broad).expect("seed");

    let inspection = inspect_game_install(&context, &selected_root).expect("inspect");

    assert_eq!(
        inspection.relationship.kind,
        InstallRelationshipKind::NarrowsExisting
    );
    assert!(
        inspection
            .root_correction
            .as_ref()
            .is_some_and(RootCorrectionAssessment::is_ready),
        "an explicit correction must be offered for one direct PE-bearing child of a manual root"
    );
    assert!(
        inspection
            .warnings
            .iter()
            .any(|warning| matches!(warning, AddGameWarning::NarrowsExistingInstall))
    );
    assert!(
        inspection
            .warnings
            .iter()
            .all(|warning| !matches!(warning, AddGameWarning::InsideExistingInstall))
    );

    let result = add_game(
        &context,
        AddGameRequest {
            selected_root: selected_root.clone(),
            root_choice: AddGameRootChoice::Selected,
            allow_root_correction: true,
            chosen_executable: None,
            inspection_fingerprint: inspection.inspection_fingerprint,
        },
    )
    .expect("explicitly correct broad root");
    assert_eq!(result.game_id, broad.id().as_str());
    assert_eq!(result.disposition, AddGameDisposition::RootCorrected);
    assert!(
        result
            .warnings
            .iter()
            .all(|warning| !matches!(warning, AddGameWarning::InsideExistingInstall)),
        "a successful correction must not publish its pre-confirmation overlap diagnostic"
    );
    assert_eq!(
        context
            .storage()
            .find_game(broad.id())
            .expect("read corrected game")
            .expect("corrected game")
            .install_path()
            .as_str(),
        canonical_path_text(&selected_root).expect("selected path")
    );

    let inspection = inspect_game_install(&context, &games_root)
        .expect("a proven game parent must produce a decision");
    assert!(matches!(
        inspection.decision,
        AddGameDecision::Unavailable {
            ref reasons
        } if reasons.contains(&AddGameUnavailableReason::ContainsProvenInstall)
    ));
    assert_eq!(
        context
            .storage()
            .find_game(broad.id())
            .expect("read unchanged game")
            .expect("existing game")
            .install_path()
            .as_str(),
        canonical_path_text(&selected_root).expect("selected path")
    );
}

#[cfg(windows)]
#[test]
fn root_correction_revalidates_state_after_inspection_before_persisting() {
    let temp = tempfile::tempdir().expect("temp");
    let games_root = temp.path().join("Games");
    let selected_root = games_root.join("Selected");
    std::fs::create_dir_all(&selected_root).expect("selected root");
    std::fs::copy(
        std::env::current_exe().expect("current exe"),
        selected_root.join("Game.exe"),
    )
    .expect("copy PE executable");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let broad = game(
        "game:stale-assessment",
        &canonical_path_text(&games_root).expect("games root"),
        RootAuthority::UserConfirmed,
    );
    context.storage().upsert_game(&broad).expect("game");
    let inspection = inspect_game_install(&context, &selected_root).expect("inspection");
    assert!(
        inspection
            .root_correction
            .as_ref()
            .is_some_and(RootCorrectionAssessment::is_ready)
    );

    context
        .storage()
        .begin_file_mutation_preparation(&BeginFileMutationPreparation {
            id: "mutation:after-inspection".to_owned(),
            game_id: broad.id().clone(),
            feature: "test".to_owned(),
            subject_id: None,
            initial_manifest_json: "{}".to_owned(),
        })
        .expect("pending mutation");

    let error = add_game(
        &context,
        AddGameRequest {
            selected_root,
            root_choice: AddGameRootChoice::Selected,
            allow_root_correction: true,
            chosen_executable: None,
            inspection_fingerprint: inspection.inspection_fingerprint,
        },
    )
    .expect_err("stale ready assessment must be rejected");
    assert!(
        matches!(error, ServiceError::StaleInstallInspection { .. }),
        "unexpected stale-state error: {error:?}"
    );
    assert_eq!(
        context
            .storage()
            .find_game(broad.id())
            .expect("read game")
            .expect("game")
            .install_path()
            .as_str(),
        canonical_path_text(&games_root).expect("games root")
    );
}

#[cfg(windows)]
#[test]
fn root_correction_archives_pruned_history_before_changing_the_catalog() {
    let temp = tempfile::tempdir().expect("temp");
    let games_root = temp.path().join("Games");
    let selected_root = games_root.join("Selected");
    let sibling_root = games_root.join("Sibling");
    std::fs::create_dir_all(&selected_root).expect("selected root");
    std::fs::create_dir_all(&sibling_root).expect("sibling root");
    std::fs::copy(
        std::env::current_exe().expect("current exe"),
        selected_root.join("Game.exe"),
    )
    .expect("copy PE executable");
    std::fs::copy(
        std::env::current_exe().expect("current exe"),
        sibling_root.join("Sibling.exe"),
    )
    .expect("copy sibling PE executable");

    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let broad = game(
        "game:history-archive",
        &canonical_path_text(&games_root).expect("games root"),
        RootAuthority::UserConfirmed,
    );
    context.storage().upsert_game(&broad).expect("game");
    let operation_id =
        seed_external_operation(&context, &broad, &sibling_root.join("nvngx_dlss.dll"));
    let inspection = inspect_game_install(&context, &selected_root).expect("inspection");

    let result = add_game(
        &context,
        AddGameRequest {
            selected_root,
            root_choice: AddGameRootChoice::Selected,
            allow_root_correction: true,
            chosen_executable: None,
            inspection_fingerprint: inspection.inspection_fingerprint,
        },
    )
    .expect("correct root");

    let bundle = result
        .recovery_bundle_path
        .as_ref()
        .expect("recovery bundle");
    assert!(std::path::Path::new(bundle).join("catalog.db").is_file());
    assert!(result.warnings.iter().any(|warning| matches!(
        warning,
        AddGameWarning::RootCorrectionHistoryArchived { .. }
    )));
    assert!(
        context
            .storage()
            .find_operation_entry(&operation_id)
            .expect("operation query")
            .is_none()
    );
}

#[cfg(windows)]
#[test]
fn recovery_bundle_failure_leaves_root_and_history_unchanged() {
    let temp = tempfile::tempdir().expect("temp");
    let games_root = temp.path().join("Games");
    let selected_root = games_root.join("Selected");
    let sibling_root = games_root.join("Sibling");
    std::fs::create_dir_all(&selected_root).expect("selected root");
    std::fs::create_dir_all(&sibling_root).expect("sibling root");
    std::fs::copy(
        std::env::current_exe().expect("current exe"),
        selected_root.join("Game.exe"),
    )
    .expect("copy PE executable");
    std::fs::copy(
        std::env::current_exe().expect("current exe"),
        sibling_root.join("Sibling.exe"),
    )
    .expect("copy sibling PE executable");

    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let broad = game(
        "game:history-bundle-failure",
        &canonical_path_text(&games_root).expect("games root"),
        RootAuthority::UserConfirmed,
    );
    context.storage().upsert_game(&broad).expect("game");
    let operation_id =
        seed_external_operation(&context, &broad, &sibling_root.join("nvngx_dlss.dll"));
    std::fs::write(temp.path().join("recovery"), b"blocks recovery directory")
        .expect("recovery blocker");
    let inspection = inspect_game_install(&context, &selected_root).expect("inspection");

    add_game(
        &context,
        AddGameRequest {
            selected_root,
            root_choice: AddGameRootChoice::Selected,
            allow_root_correction: true,
            chosen_executable: None,
            inspection_fingerprint: inspection.inspection_fingerprint,
        },
    )
    .expect_err("bundle failure must abort root correction");

    assert_eq!(
        context
            .storage()
            .find_game(broad.id())
            .expect("game query")
            .expect("game")
            .install_path()
            .as_str(),
        canonical_path_text(&games_root).expect("games root")
    );
    assert!(
        context
            .storage()
            .find_operation_entry(&operation_id)
            .expect("operation query")
            .is_some()
    );
}
