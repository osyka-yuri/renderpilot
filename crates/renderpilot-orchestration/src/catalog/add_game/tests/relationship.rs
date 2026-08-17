use super::*;

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
