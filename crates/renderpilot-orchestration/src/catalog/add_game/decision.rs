//! Backend-owned add-game decision policy and semantic fingerprint.

use super::*;

#[derive(Clone, Copy)]
pub(super) struct DecisionFacts<'a> {
    pub selected_root: &'a renderpilot_domain::InstallRoot,
    pub boundary: &'a InstallBoundaryInspection,
    pub recommendation: Option<&'a RootRecommendationInspection>,
    pub recommendation_action: Option<AddGameCatalogAction>,
    pub relationship: &'a InstallRelationship,
    pub executables: &'a [ExecutableInspection],
    pub requires_explicit_executable: bool,
    pub root_correction: Option<&'a RootCorrectionAssessment>,
    pub warnings: &'a [AddGameWarning],
}

pub(super) fn derive_add_game_decision(facts: DecisionFacts<'_>) -> AddGameDecision {
    let DecisionFacts {
        selected_root,
        boundary,
        recommendation,
        recommendation_action,
        relationship,
        executables,
        requires_explicit_executable,
        root_correction,
        warnings,
    } = facts;
    let has_valid_executable = executables
        .iter()
        .any(|candidate| candidate.valid_windows_pe);
    let selected_root_proven = boundary
        .evidence
        .contains(&InstallBoundaryEvidence::LauncherManifest);
    let correction_available = matches!(
        relationship.kind,
        InstallRelationshipKind::InsideExisting
            | InstallRelationshipKind::ExpandsExisting
            | InstallRelationshipKind::NarrowsExisting
    ) && root_correction
        .is_some_and(|assessment| assessment.status != RootCorrectionStatus::Blocked);

    let mut options = Vec::new();
    match relationship.kind {
        InstallRelationshipKind::New
            if boundary.kind != InstallBoundaryKind::MultipleInstallContainer
                && (has_valid_executable || selected_root_proven) =>
        {
            options.push(AddGameOption {
                root_choice: AddGameRootChoice::Selected,
                catalog_action: AddGameCatalogAction::Add,
            });
        }
        InstallRelationshipKind::ExactExisting => options.push(AddGameOption {
            root_choice: AddGameRootChoice::Selected,
            catalog_action: AddGameCatalogAction::Rescan,
        }),
        InstallRelationshipKind::InsideExisting
        | InstallRelationshipKind::ExpandsExisting
        | InstallRelationshipKind::NarrowsExisting
            if correction_available =>
        {
            options.push(AddGameOption {
                root_choice: AddGameRootChoice::Selected,
                catalog_action: AddGameCatalogAction::CorrectExistingRoot,
            });
        }
        _ => {}
    }
    if relationship.kind == InstallRelationshipKind::New
        && let (Some(catalog_action), Some(recommendation)) =
            (recommendation_action, recommendation)
        && recommendation.root != *selected_root
    {
        options.push(AddGameOption {
            root_choice: AddGameRootChoice::Recommended,
            catalog_action,
        });
    }
    options.sort();
    options.dedup();

    if options.is_empty() {
        return unavailable_decision(
            boundary,
            relationship,
            root_correction,
            has_valid_executable,
            selected_root_proven,
        );
    }

    let recommended_option = options
        .iter()
        .copied()
        .find(|option| option.root_choice == AddGameRootChoice::Recommended);
    let selected_option = options
        .iter()
        .copied()
        .find(|option| option.root_choice == AddGameRootChoice::Selected);
    let authoritative_recommendation = recommendation.is_some_and(|recommendation| {
        recommendation.confidence == RootRecommendationConfidence::Authoritative
    });
    let default_option = if authoritative_recommendation {
        recommended_option.or(selected_option)
    } else {
        selected_option.or(recommended_option)
    };
    let Some(default_option) = default_option else {
        return unavailable_decision(
            boundary,
            relationship,
            root_correction,
            has_valid_executable,
            selected_root_proven,
        );
    };

    let simple_boundary = boundary.kind == InstallBoundaryKind::SingleInstall;
    let simple_relationship = matches!(
        relationship.kind,
        InstallRelationshipKind::New | InstallRelationshipKind::ExactExisting
    );
    let suggested_alternative = recommended_option.is_some() && !authoritative_recommendation;
    let requires_review = requires_explicit_executable
        || !warnings.is_empty()
        || !simple_boundary
        || !simple_relationship
        || suggested_alternative
        || default_option.catalog_action == AddGameCatalogAction::CorrectExistingRoot;

    if requires_review {
        match AddGameReview::new(default_option, options) {
            Some(review) => AddGameDecision::Review(review),
            None => unavailable_decision(
                boundary,
                relationship,
                root_correction,
                has_valid_executable,
                selected_root_proven,
            ),
        }
    } else {
        AddGameDecision::Automatic {
            option: default_option,
        }
    }
}

fn unavailable_decision(
    boundary: &InstallBoundaryInspection,
    relationship: &InstallRelationship,
    root_correction: Option<&RootCorrectionAssessment>,
    has_valid_executable: bool,
    selected_root_proven: bool,
) -> AddGameDecision {
    let mut reasons = Vec::new();
    let root_reason = match relationship.kind {
        InstallRelationshipKind::ContainsMultiple => {
            Some(AddGameUnavailableReason::ContainsMultipleCatalogInstalls)
        }
        InstallRelationshipKind::InsideExisting
        | InstallRelationshipKind::ExpandsExisting
        | InstallRelationshipKind::NarrowsExisting => {
            Some(AddGameUnavailableReason::InsideExistingInstall)
        }
        InstallRelationshipKind::ContainsProvenInstall
            if boundary.kind != InstallBoundaryKind::MultipleInstallContainer =>
        {
            Some(AddGameUnavailableReason::ContainsProvenInstall)
        }
        _ if boundary.kind == InstallBoundaryKind::MultipleInstallContainer => {
            Some(AddGameUnavailableReason::MultipleInstalls)
        }
        _ => None,
    };
    if let Some(reason) = root_reason {
        reasons.push(reason);
    }
    if root_correction.is_some_and(|assessment| assessment.status == RootCorrectionStatus::Blocked)
    {
        reasons.push(AddGameUnavailableReason::RootCorrectionBlocked);
    }
    if !has_valid_executable && !selected_root_proven {
        reasons.push(AddGameUnavailableReason::NoReadableExecutable);
    }
    reasons.sort();
    reasons.dedup();
    AddGameDecision::Unavailable { reasons }
}

#[cfg(test)]
mod tests {
    use super::super::fingerprint::compute_inspection_fingerprint;
    use super::*;

    #[test]
    fn recommended_existing_root_is_derived_as_a_rescan() {
        let boundary = boundary(InstallBoundaryKind::SingleInstall);
        let recommendation = recommendation();
        let relationship = relationship(InstallRelationshipKind::New);
        let executables = vec![executable("C:/Games/Selected/Game.exe", 1)];
        let selected_root = install_root("C:/Games/Selected");

        let decision = derive_add_game_decision(DecisionFacts {
            selected_root: &selected_root,
            boundary: &boundary,
            recommendation: Some(&recommendation),
            recommendation_action: Some(AddGameCatalogAction::Rescan),
            relationship: &relationship,
            executables: &executables,
            requires_explicit_executable: false,
            root_correction: None,
            warnings: &[],
        });

        assert!(matches!(
            decision,
            AddGameDecision::Review(review)
                if review.options().contains(&AddGameOption {
                    root_choice: AddGameRootChoice::Recommended,
                    catalog_action: AddGameCatalogAction::Rescan,
                })
        ));
    }

    #[test]
    fn unavailable_decisions_always_explain_why_no_option_exists() {
        let boundary = boundary(InstallBoundaryKind::MultipleInstallContainer);
        let relationship = relationship(InstallRelationshipKind::ContainsMultiple);
        let selected_root = install_root("C:/Games");
        let decision = derive_add_game_decision(DecisionFacts {
            selected_root: &selected_root,
            boundary: &boundary,
            recommendation: None,
            recommendation_action: None,
            relationship: &relationship,
            executables: &[],
            requires_explicit_executable: false,
            root_correction: None,
            warnings: &[],
        });

        assert!(matches!(
            decision,
            AddGameDecision::Unavailable { reasons } if !reasons.is_empty()
        ));
    }

    #[test]
    fn unavailable_decision_reports_one_non_redundant_root_reason() {
        let cases = [
            (
                InstallBoundaryKind::MultipleInstallContainer,
                InstallRelationshipKind::ContainsProvenInstall,
                AddGameUnavailableReason::MultipleInstalls,
            ),
            (
                InstallBoundaryKind::MultipleInstallContainer,
                InstallRelationshipKind::ContainsMultiple,
                AddGameUnavailableReason::ContainsMultipleCatalogInstalls,
            ),
            (
                InstallBoundaryKind::SingleInstall,
                InstallRelationshipKind::ContainsProvenInstall,
                AddGameUnavailableReason::ContainsProvenInstall,
            ),
            (
                InstallBoundaryKind::MultipleInstallContainer,
                InstallRelationshipKind::InsideExisting,
                AddGameUnavailableReason::InsideExistingInstall,
            ),
        ];

        for (boundary_kind, relationship_kind, expected) in cases {
            let decision = unavailable_decision(
                &boundary(boundary_kind),
                &relationship(relationship_kind),
                None,
                true,
                false,
            );
            assert_eq!(
                decision,
                AddGameDecision::Unavailable {
                    reasons: vec![expected]
                }
            );
        }
    }

    #[test]
    fn fingerprint_ignores_warning_presentation_and_collection_order() {
        let mut left = inspection();
        left.warnings = vec![AddGameWarning::FilesystemProbeError];
        let mut right = left.clone();
        right.executables.reverse();
        right.boundary.evidence.reverse();
        right.warnings = vec![AddGameWarning::NoReadableExecutable];

        assert_eq!(
            compute_inspection_fingerprint(&left).expect("left fingerprint"),
            compute_inspection_fingerprint(&right).expect("right fingerprint")
        );
    }

    #[test]
    fn fingerprint_binds_the_catalog_generation() {
        let left = inspection();
        let mut right = left.clone();
        right.catalog_generation += 1;

        assert_ne!(
            compute_inspection_fingerprint(&left).expect("left fingerprint"),
            compute_inspection_fingerprint(&right).expect("right fingerprint")
        );
    }

    #[test]
    fn fingerprint_binds_the_recommended_root_identity() {
        let left = inspection();
        let mut right = left.clone();
        right
            .recommendation
            .as_mut()
            .expect("recommendation")
            .effective_fingerprint = "changed-effective-root".to_owned();

        assert_ne!(
            compute_inspection_fingerprint(&left).expect("left fingerprint"),
            compute_inspection_fingerprint(&right).expect("right fingerprint")
        );
    }

    fn inspection() -> AddGameInspection {
        AddGameInspection {
            selected_root: install_root("C:/Games/Selected"),
            inspection_fingerprint: String::new(),
            catalog_generation: 17,
            boundary: InstallBoundaryInspection {
                kind: InstallBoundaryKind::SingleInstall,
                completeness: TraversalCompleteness::Complete,
                candidate_roots: vec![
                    install_root("C:/Games/Selected"),
                    install_root(r"c:\games\selected"),
                ],
                evidence: vec![
                    InstallBoundaryEvidence::RootExecutable,
                    InstallBoundaryEvidence::EngineStructure,
                ],
            },
            recommendation: Some(recommendation()),
            relationship: relationship(InstallRelationshipKind::New),
            executables: vec![
                executable("C:/Games/Selected/B.exe", 2),
                executable("C:/Games/Selected/A.exe", 1),
            ],
            requires_explicit_executable: false,
            root_correction: None,
            decision: AddGameDecision::Review(
                AddGameReview::new(
                    AddGameOption {
                        root_choice: AddGameRootChoice::Selected,
                        catalog_action: AddGameCatalogAction::Add,
                    },
                    vec![
                        AddGameOption {
                            root_choice: AddGameRootChoice::Recommended,
                            catalog_action: AddGameCatalogAction::Rescan,
                        },
                        AddGameOption {
                            root_choice: AddGameRootChoice::Selected,
                            catalog_action: AddGameCatalogAction::Add,
                        },
                    ],
                )
                .expect("valid review"),
            ),
            warnings: Vec::new(),
        }
    }

    fn boundary(kind: InstallBoundaryKind) -> InstallBoundaryInspection {
        InstallBoundaryInspection {
            kind,
            completeness: TraversalCompleteness::Complete,
            candidate_roots: Vec::new(),
            evidence: vec![InstallBoundaryEvidence::RootExecutable],
        }
    }

    fn recommendation() -> RootRecommendationInspection {
        RootRecommendationInspection {
            root: install_root("C:/Games/Existing"),
            source: RootRecommendationSource::ExistingCatalog,
            confidence: RootRecommendationConfidence::Suggested,
            completeness: TraversalCompleteness::Complete,
            evidence: Vec::new(),
            effective_fingerprint: "effective-root".to_owned(),
        }
    }

    fn relationship(kind: InstallRelationshipKind) -> InstallRelationship {
        InstallRelationship {
            kind,
            game_ids: Vec::new(),
            proven_install_roots: Vec::new(),
        }
    }

    fn executable(path: &str, size_bytes: u64) -> ExecutableInspection {
        ExecutableInspection {
            path: path.to_owned(),
            relative_path: path.rsplit('/').next().expect("basename").to_owned(),
            size_bytes,
            rank_score: 100,
            valid_windows_pe: true,
            rejection_kind: None,
            rejection_token: None,
        }
    }

    fn install_root(path: &str) -> renderpilot_domain::InstallRoot {
        renderpilot_domain::InstallRoot::new(PathRef::new(path).expect("valid install root"))
    }
}
