#[cfg(windows)]
use renderpilot_application::{
    ComponentRepository, GameRepository, OperationItemRecord, OperationJournalEntry, OperationKind,
    OperationRecord, OperationRepository, OperationStatus, UnixTimestampMillis,
};
#[cfg(windows)]
use renderpilot_domain::{
    ComponentFile, ComponentId, ComponentKind, LibraryComponent, LibraryTechnology, OperationId,
    Swappability,
};
use renderpilot_domain::{GameIdentity, GameInstallation, GameRuntime, Platform};
#[cfg(windows)]
use renderpilot_storage_sqlite::{PendingFileMutationRow, PendingFileMutationState};

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
        .prepare_file_mutation(&PendingFileMutationRow {
            id: "mutation:after-inspection".to_owned(),
            game_id: broad.id().clone(),
            feature: "test".to_owned(),
            subject_id: None,
            state: PendingFileMutationState::Preparing,
            manifest_json: "{}".to_owned(),
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

#[test]
fn raw_relationship_never_uses_a_legacy_child_name_as_boundary_proof() {
    let game = game(
        "manual:C:/Games/Example/D3D12",
        "C:/Games/Example/D3D12",
        RootAuthority::Legacy,
    );
    let relationship = classify_relationship(&install_root("C:/Games/Example"), &[game], &[]);
    assert_eq!(
        relationship.kind,
        InstallRelationshipKind::ContainsProvenInstall
    );
}

#[test]
fn only_internal_legacy_descendants_can_be_consolidated_under_a_parent() {
    let false_children = vec![
        game(
            "manual:d3d12",
            "C:/Games/Example/D3D12",
            RootAuthority::Legacy,
        ),
        game(
            "manual:streamline",
            "C:/Games/Example/NVStreamline",
            RootAuthority::Legacy,
        ),
    ];
    assert_eq!(
        classify_relationship(&install_root("C:/Games/Example"), &false_children, &[],).kind,
        InstallRelationshipKind::ContainsMultiple
    );

    let independent_games = vec![
        game("manual:alpha", "C:/Games/Alpha", RootAuthority::Legacy),
        game("manual:beta", "C:/Games/Beta", RootAuthority::Legacy),
    ];
    assert_eq!(
        classify_relationship(&install_root("C:/Games"), &independent_games, &[]).kind,
        InstallRelationshipKind::ContainsMultiple
    );
}

#[test]
fn relationship_never_splits_parent_with_multiple_catalog_installs() {
    let games = vec![
        game("game:a", "C:/Games/A", RootAuthority::UserConfirmed),
        game("game:b", "C:/Games/B", RootAuthority::UserConfirmed),
    ];
    let relationship = classify_relationship(&install_root("C:/Games"), &games, &[]);
    assert_eq!(relationship.kind, InstallRelationshipKind::ContainsMultiple);
}

#[test]
fn launcher_provider_blocks_parent_of_multiple_proven_installs() {
    let launcher_roots = vec![
        install_root("C:/Games/Alpha"),
        install_root("C:/Games/Beta"),
    ];

    let relationship = classify_relationship(&install_root("C:/Games"), &[], &launcher_roots);

    assert_eq!(relationship.kind, InstallRelationshipKind::ContainsMultiple);
    assert_eq!(relationship.proven_install_roots, launcher_roots);
}

#[test]
fn launcher_provider_recommends_the_only_proven_child_without_claiming_multiple() {
    let launcher_roots = vec![install_root("C:/Games/Black Flag")];
    let selected = install_root("C:/Games");
    let advisor = RootAdvisor::new(&selected, &[], &launcher_roots);
    let relationship = advisor.relationship();

    assert_eq!(
        relationship.kind,
        InstallRelationshipKind::ContainsProvenInstall
    );
    assert_eq!(
        advisor.recommendation(&relationship),
        Some((
            install_root("C:/Games/Black Flag"),
            RootRecommendationSource::LauncherManifest
        ))
    );
}

#[test]
fn raw_relationship_does_not_collapse_confirmed_and_legacy_children() {
    let games = vec![
        game(
            "game:confirmed",
            "C:/Games/Black Flag/D3D12",
            RootAuthority::UserConfirmed,
        ),
        game(
            "manual:false-child",
            "C:/Games/Black Flag/NVStreamline",
            RootAuthority::Legacy,
        ),
    ];

    let relationship = classify_relationship(&install_root("C:/Games/Black Flag"), &games, &[]);

    assert_eq!(relationship.kind, InstallRelationshipKind::ContainsMultiple);
    assert_eq!(
        relationship.game_ids,
        vec!["game:confirmed", "manual:false-child"]
    );
}

#[test]
fn raw_relationship_does_not_infer_parent_roles_from_directory_names() {
    let game = game(
        "game:too-narrow",
        "D:/Games/The Last of Us Part I/Bin/Win64",
        RootAuthority::UserConfirmed,
    );

    let game_root = classify_relationship(
        &install_root("D:/Games/The Last of Us Part I"),
        std::slice::from_ref(&game),
        &[],
    );
    assert_eq!(
        game_root.kind,
        InstallRelationshipKind::ContainsProvenInstall
    );

    let library_root = classify_relationship(&install_root("D:/Games"), &[game], &[]);
    assert_eq!(
        library_root.kind,
        InstallRelationshipKind::ContainsProvenInstall,
    );
}

#[test]
fn launcher_provider_recommends_root_for_selected_child() {
    let launcher_roots = vec![install_root("C:/Games/Example")];
    let selected = install_root("C:/Games/Example/bin");
    let advisor = RootAdvisor::new(&selected, &[], &launcher_roots);
    let relationship = advisor.relationship();

    assert_eq!(relationship.kind, InstallRelationshipKind::InsideExisting);
    assert_eq!(
        advisor.recommendation(&relationship),
        Some((
            install_root("C:/Games/Example"),
            RootRecommendationSource::LauncherManifest
        ))
    );
}

#[test]
fn catalog_relationship_alone_does_not_prove_a_single_install() {
    assert_eq!(
        classify_relationship(&install_root("C:/Games"), &[], &[]).kind,
        InstallRelationshipKind::New
    );
}

#[test]
fn authoritative_recommendation_does_not_prove_selected_root() {
    let inspection = AddGameInspection {
        selected_root: install_root("C:/Games/Example/bin"),
        inspection_fingerprint: "test".to_owned(),
        catalog_generation: 0,
        boundary: test_boundary(InstallBoundaryKind::Ambiguous),
        recommendation: Some(RootRecommendationInspection {
            root: install_root("C:/Games/Example"),
            source: RootRecommendationSource::LauncherManifest,
            confidence: RootRecommendationConfidence::Authoritative,
            completeness: TraversalCompleteness::Complete,
            evidence: vec![InstallBoundaryEvidence::LauncherManifest],
            effective_fingerprint: "effective-root".to_owned(),
        }),
        relationship: InstallRelationship {
            kind: InstallRelationshipKind::New,
            game_ids: Vec::new(),
            proven_install_roots: vec![install_root("C:/Games/Example")],
        },
        executables: Vec::new(),
        requires_explicit_executable: false,
        root_correction: None,
        decision: AddGameDecision::Review(
            AddGameReview::new(
                AddGameOption {
                    root_choice: AddGameRootChoice::Recommended,
                    catalog_action: AddGameCatalogAction::Add,
                },
                vec![AddGameOption {
                    root_choice: AddGameRootChoice::Recommended,
                    catalog_action: AddGameCatalogAction::Add,
                }],
            )
            .expect("valid review"),
        ),
        warnings: Vec::new(),
    };

    let error = ensure_addable(&inspection, None)
        .expect_err("recommendation evidence must not authorize selected root");
    assert!(error.to_string().contains("Windows PE"));
}

#[test]
fn exact_confirmed_root_can_be_repaired_without_current_executable_evidence() {
    let inspection = AddGameInspection {
        selected_root: install_root("C:/Games/Example"),
        inspection_fingerprint: "test".to_owned(),
        catalog_generation: 0,
        boundary: test_boundary(InstallBoundaryKind::Ambiguous),
        recommendation: None,
        relationship: InstallRelationship {
            kind: InstallRelationshipKind::ExactExisting,
            game_ids: vec!["game:existing".to_owned()],
            proven_install_roots: Vec::new(),
        },
        executables: Vec::new(),
        requires_explicit_executable: false,
        root_correction: None,
        decision: AddGameDecision::Automatic {
            option: AddGameOption {
                root_choice: AddGameRootChoice::Selected,
                catalog_action: AddGameCatalogAction::Rescan,
            },
        },
        warnings: Vec::new(),
    };

    ensure_addable(&inspection, None).expect("confirmed exact root should be repairable");
}

#[test]
fn engine_subtree_with_readable_game_executable_can_be_confirmed_explicitly() {
    let inspection = AddGameInspection {
        selected_root: install_root("C:/Games/Jedi Survivor/SwGame"),
        inspection_fingerprint: "test".to_owned(),
        catalog_generation: 0,
        boundary: test_boundary(InstallBoundaryKind::EngineProjectSubtree),
        recommendation: None,
        relationship: InstallRelationship {
            kind: InstallRelationshipKind::New,
            game_ids: Vec::new(),
            proven_install_roots: Vec::new(),
        },
        executables: vec![ExecutableInspection {
            path: "C:/Games/Jedi Survivor/SwGame/Binaries/Win64/JediSurvivor.exe".to_owned(),
            relative_path: "Binaries/Win64/JediSurvivor.exe".to_owned(),
            size_bytes: 0,
            rank_score: 100,
            valid_windows_pe: true,
            rejection_kind: None,
            rejection_token: None,
        }],
        requires_explicit_executable: false,
        root_correction: None,
        decision: AddGameDecision::Automatic {
            option: AddGameOption {
                root_choice: AddGameRootChoice::Selected,
                catalog_action: AddGameCatalogAction::Add,
            },
        },
        warnings: Vec::new(),
    };

    ensure_addable(&inspection, None).expect("readable PE makes recommendation advisory");
}

#[test]
fn multiple_install_container_remains_a_hard_invariant() {
    let inspection = AddGameInspection {
        selected_root: install_root("C:/Games"),
        inspection_fingerprint: "test".to_owned(),
        catalog_generation: 0,
        boundary: test_boundary(InstallBoundaryKind::MultipleInstallContainer),
        recommendation: None,
        relationship: InstallRelationship {
            kind: InstallRelationshipKind::New,
            game_ids: Vec::new(),
            proven_install_roots: Vec::new(),
        },
        executables: vec![ExecutableInspection {
            path: "C:/Games/A/GameA.exe".to_owned(),
            relative_path: "A/GameA.exe".to_owned(),
            size_bytes: 0,
            rank_score: 100,
            valid_windows_pe: true,
            rejection_kind: None,
            rejection_token: None,
        }],
        requires_explicit_executable: false,
        root_correction: None,
        decision: AddGameDecision::Unavailable {
            reasons: vec![AddGameUnavailableReason::MultipleInstalls],
        },
        warnings: Vec::new(),
    };

    assert!(matches!(
        ensure_addable(&inspection, None),
        Err(ServiceError::MultipleInstallsDetected(_))
    ));
}

fn test_boundary(kind: InstallBoundaryKind) -> InstallBoundaryInspection {
    InstallBoundaryInspection {
        kind,
        completeness: TraversalCompleteness::Complete,
        candidate_roots: Vec::new(),
        evidence: Vec::new(),
    }
}

fn install_root(path: &str) -> renderpilot_domain::InstallRoot {
    renderpilot_domain::InstallRoot::new(PathRef::new(path).expect("valid install root"))
}

fn game(id: &str, path: &str, authority: RootAuthority) -> GameInstallation {
    GameInstallation::new(
        GameIdentity::new(GameId::new(id).expect("id"), "Game", Launcher::Manual)
            .expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(path).expect("path"),
    )
    .with_root_authority(authority)
}

#[cfg(windows)]
fn canonical_path_text(path: &Path) -> Result<String, ServiceError> {
    // Windows runners may expose `%TEMP%` through an 8.3 alias such as
    // `RUNNER~1`. Fixtures must cross the same filesystem-identity boundary as
    // production inspection before they enter domain or storage values.
    let canonical =
        renderpilot_platform_windows::canonicalize_install_path(path).map_err(|error| {
            ServiceError::invalid_input(format!(
                "test fixture path could not be canonicalized: {} ({error})",
                path.display()
            ))
        })?;
    PathRef::new(canonical.to_string_lossy())
        .map(|path| path.as_str().to_owned())
        .map_err(|error| ServiceError::invalid_input(error.to_string()))
}

#[cfg(windows)]
fn seed_external_operation(
    context: &crate::Context,
    game: &GameInstallation,
    component_path: &Path,
) -> OperationId {
    std::fs::write(component_path, b"external component").expect("component file");
    let component_path =
        canonical_path_text(component_path).expect("canonical external component path");
    let component_id =
        ComponentId::new(format!("component:{}:external", game.id())).expect("component id");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
    )
    .with_file(ComponentFile::new(
        PathRef::new(&component_path).expect("component path"),
    ));
    context
        .storage()
        .replace_components_for_game(game.id(), &[component])
        .expect("component");

    let operation_id =
        OperationId::new(format!("operation:{}:external", game.id())).expect("operation id");
    let operation = OperationRecord::new(
        operation_id.clone(),
        game.id().clone(),
        OperationKind::Scan,
        OperationStatus::Completed,
        UnixTimestampMillis::new(1).expect("timestamp"),
    );
    let item = OperationItemRecord::new(
        operation_id.clone(),
        component_id,
        PathRef::new(component_path).expect("source path"),
        OperationStatus::Completed,
    );
    context
        .storage()
        .save_operation_entry(
            &OperationJournalEntry::try_new(operation, vec![item]).expect("entry"),
        )
        .expect("operation");
    operation_id
}
