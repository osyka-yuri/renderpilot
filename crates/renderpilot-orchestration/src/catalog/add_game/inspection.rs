//! Filesystem and catalog inspection for explicit installation roots.

use super::*;

fn executable_basenames(executables: &[ExecutableInspection]) -> HashSet<String> {
    executables
        .iter()
        .filter_map(|candidate| Path::new(&candidate.path).file_name())
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .collect()
}

fn discover_launcher_paths() -> (Vec<PathBuf>, Vec<PathBuf>) {
    #[cfg(windows)]
    {
        let sources = renderpilot_platform_windows::game_libraries::discover_game_sources();
        let install_paths = sources
            .installs
            .into_iter()
            .map(|install| install.install_path)
            .collect();
        (install_paths, sources.library_roots)
    }
    #[cfg(not(windows))]
    {
        (Vec::new(), Vec::new())
    }
}

/// Inspects a folder without mutating catalog state.
pub fn inspect_game_install(
    context: &crate::Context,
    path: &Path,
) -> Result<AddGameInspection, ServiceError> {
    inspect_game_install_unlocked(context, path)
}

pub(super) fn inspect_game_install_unlocked(
    context: &crate::Context,
    path: &Path,
) -> Result<AddGameInspection, ServiceError> {
    loop {
        let catalog_generation = context.storage().catalog_generation();
        let inspection = inspect_game_install_once(context, path, catalog_generation)?;
        if context.storage().catalog_generation() == catalog_generation {
            return Ok(inspection);
        }
    }
}

pub(super) fn inspect_game_install_once(
    context: &crate::Context,
    path: &Path,
    catalog_generation: u64,
) -> Result<AddGameInspection, ServiceError> {
    let root = normalized_root(path)?;
    let selected_root = install_root_from_path(&root)?;
    validate_root_invariants(selected_root.path().as_str())?;

    let games = context.storage().list_games()?;
    let (launcher_install_paths, launcher_library_paths) = discover_launcher_paths();
    let launcher_roots = launcher_install_paths
        .iter()
        .filter_map(|path| install_root_from_path(path).ok())
        .collect::<Vec<_>>();
    let boundary_assessment = install_boundary::InstallBoundaryAnalyzer::inspect(
        install_boundary::InstallBoundaryRequest {
            selected_root: &root,
            launcher_install_roots: &launcher_install_paths,
            launcher_library_roots: &launcher_library_paths,
            cancellation: None,
        },
    );
    let advisor = RootAdvisor::new(&selected_root, &games, &launcher_roots);
    let relationship = refine_relationship_with_boundary(
        &selected_root,
        advisor.relationship(),
        &games,
        &boundary_assessment.selected,
        &launcher_install_paths,
    );
    let boundary = boundary_inspection(&boundary_assessment.selected)?;

    let executables = executable_inspections(&boundary_assessment.selected);
    let mut warnings = Vec::new();

    let has_ranked_pe = executables
        .iter()
        .any(|candidate| candidate.valid_windows_pe && candidate.rejection_kind.is_none());
    let has_any_pe = executables
        .iter()
        .any(|candidate| candidate.valid_windows_pe);
    let requires_explicit_executable = !has_ranked_pe && has_any_pe;

    let selected_root_launcher_proven = boundary
        .evidence
        .contains(&InstallBoundaryEvidence::LauncherManifest);
    let selected_executable_basenames = executable_basenames(&executables);
    let root_correction = if selected_root_launcher_proven || has_any_pe {
        root_correction_target(&selected_root, &relationship, &games)
            .map(|game| {
                root_correction::assess(
                    context,
                    game.id(),
                    selected_root.path().as_str(),
                    &selected_executable_basenames,
                    None,
                )
            })
            .transpose()?
    } else {
        None
    };
    let mut recommendation = if let Some((root, source)) = advisor.recommendation(&relationship) {
        Some(RootRecommendationInspection {
            root,
            source,
            confidence: if source == RootRecommendationSource::LauncherManifest {
                RootRecommendationConfidence::Authoritative
            } else {
                RootRecommendationConfidence::Suggested
            },
            completeness: TraversalCompleteness::Complete,
            evidence: if source == RootRecommendationSource::LauncherManifest {
                vec![InstallBoundaryEvidence::LauncherManifest]
            } else {
                Vec::new()
            },
            effective_fingerprint: String::new(),
        })
    } else {
        boundary_assessment
            .recommendation
            .as_ref()
            .map(root_recommendation_inspection)
            .transpose()?
    };
    if let Some(recommendation) = recommendation.as_mut() {
        recommendation.effective_fingerprint = inspect_recommended_root_fingerprint(
            context,
            recommendation,
            &games,
            &launcher_install_paths,
            &launcher_library_paths,
            &launcher_roots,
            catalog_generation,
        )?;
    }
    let recommendation_action = recommendation.as_ref().and_then(|recommendation| {
        match RootAdvisor::new(&recommendation.root, &games, &launcher_roots)
            .relationship()
            .kind
        {
            InstallRelationshipKind::New => Some(AddGameCatalogAction::Add),
            InstallRelationshipKind::ExactExisting => Some(AddGameCatalogAction::Rescan),
            InstallRelationshipKind::InsideExisting
            | InstallRelationshipKind::ExpandsExisting
            | InstallRelationshipKind::NarrowsExisting
            | InstallRelationshipKind::ContainsProvenInstall
            | InstallRelationshipKind::ContainsMultiple => None,
        }
    });

    if boundary.completeness == TraversalCompleteness::Incomplete
        && !boundary_assessment.selected.diagnostics.is_empty()
    {
        warnings.push(AddGameWarning::FilesystemProbeError);
    }
    if relationship.kind == InstallRelationshipKind::InsideExisting {
        warnings.push(AddGameWarning::InsideExistingInstall);
    }
    if relationship.kind == InstallRelationshipKind::NarrowsExisting {
        warnings.push(AddGameWarning::NarrowsExistingInstall);
    }
    if relationship.kind == InstallRelationshipKind::ContainsMultiple {
        warnings.push(AddGameWarning::MultipleProvenInstalls);
    } else if relationship.kind == InstallRelationshipKind::ContainsProvenInstall {
        warnings.push(AddGameWarning::ContainsProvenInstall);
    }
    if requires_explicit_executable {
        warnings.push(AddGameWarning::ExplicitExecutableRequired);
    } else if relationship.kind != InstallRelationshipKind::ExactExisting
        && !executables
            .iter()
            .any(|executable| executable.valid_windows_pe)
    {
        warnings.push(AddGameWarning::NoReadableExecutable);
    }

    let decision = derive_add_game_decision(DecisionFacts {
        selected_root: &selected_root,
        boundary: &boundary,
        recommendation: recommendation.as_ref(),
        recommendation_action,
        relationship: &relationship,
        executables: &executables,
        requires_explicit_executable,
        root_correction: root_correction.as_ref(),
        warnings: &warnings,
    });
    if matches!(&decision, AddGameDecision::Unavailable { .. }) {
        warnings.retain(|warning| matches!(warning, AddGameWarning::FilesystemProbeError));
    }
    let mut inspection = AddGameInspection {
        selected_root,
        inspection_fingerprint: String::new(),
        catalog_generation,
        boundary,
        recommendation,
        relationship,
        executables,
        requires_explicit_executable,
        root_correction,
        decision,
        warnings,
    };
    inspection.inspection_fingerprint = compute_inspection_fingerprint(&inspection)?;
    Ok(inspection)
}

pub(super) fn boundary_inspection(
    assessment: &install_boundary::CandidateBoundaryAssessment,
) -> Result<InstallBoundaryInspection, ServiceError> {
    Ok(InstallBoundaryInspection {
        kind: match assessment.kind {
            install_boundary::InstallBoundaryKind::SingleInstall => {
                InstallBoundaryKind::SingleInstall
            }
            install_boundary::InstallBoundaryKind::EngineProjectSubtree => {
                InstallBoundaryKind::EngineProjectSubtree
            }
            install_boundary::InstallBoundaryKind::BinarySubtree => {
                InstallBoundaryKind::BinarySubtree
            }
            install_boundary::InstallBoundaryKind::SingleInstallContainer => {
                InstallBoundaryKind::SingleInstallContainer
            }
            install_boundary::InstallBoundaryKind::MultipleInstallContainer => {
                InstallBoundaryKind::MultipleInstallContainer
            }
            install_boundary::InstallBoundaryKind::Ambiguous => InstallBoundaryKind::Ambiguous,
            install_boundary::InstallBoundaryKind::Incomplete => InstallBoundaryKind::Incomplete,
        },
        completeness: map_boundary_completeness(assessment.completeness),
        candidate_roots: assessment
            .candidate_roots
            .iter()
            .map(|root| install_root_from_path(root))
            .collect::<Result<Vec<_>, _>>()?,
        evidence: assessment
            .evidence
            .iter()
            .copied()
            .map(map_boundary_evidence)
            .collect(),
    })
}

pub(super) fn root_recommendation_inspection(
    recommendation: &install_boundary::RootRecommendation,
) -> Result<RootRecommendationInspection, ServiceError> {
    Ok(RootRecommendationInspection {
        root: install_root_from_path(&recommendation.root)?,
        source: match recommendation.source {
            install_boundary::RootRecommendationSource::LauncherManifest => {
                RootRecommendationSource::LauncherManifest
            }
            install_boundary::RootRecommendationSource::EngineDistributionRoot => {
                RootRecommendationSource::EngineDistributionRoot
            }
            install_boundary::RootRecommendationSource::RootExecutable => {
                RootRecommendationSource::RootExecutable
            }
            install_boundary::RootRecommendationSource::ComponentContext => {
                RootRecommendationSource::ComponentContext
            }
        },
        confidence: if recommendation.authoritative() {
            RootRecommendationConfidence::Authoritative
        } else {
            RootRecommendationConfidence::Suggested
        },
        completeness: map_boundary_completeness(recommendation.completeness),
        evidence: recommendation
            .evidence
            .iter()
            .copied()
            .map(map_boundary_evidence)
            .collect(),
        effective_fingerprint: String::new(),
    })
}

pub(super) fn inspect_recommended_root_fingerprint(
    context: &crate::Context,
    recommendation: &RootRecommendationInspection,
    games: &[renderpilot_domain::GameInstallation],
    launcher_install_paths: &[PathBuf],
    launcher_library_paths: &[PathBuf],
    launcher_roots: &[renderpilot_domain::InstallRoot],
    catalog_generation: u64,
) -> Result<String, ServiceError> {
    let root = PathBuf::from(recommendation.root.path().as_str());
    let assessment = install_boundary::InstallBoundaryAnalyzer::inspect(
        install_boundary::InstallBoundaryRequest {
            selected_root: &root,
            launcher_install_roots: launcher_install_paths,
            launcher_library_roots: launcher_library_paths,
            cancellation: None,
        },
    )
    .selected;
    let relationship = refine_relationship_with_boundary(
        &recommendation.root,
        RootAdvisor::new(&recommendation.root, games, launcher_roots).relationship(),
        games,
        &assessment,
        launcher_install_paths,
    );
    let boundary = boundary_inspection(&assessment)?;
    let executables = executable_inspections(&assessment);
    let has_any_pe = executables
        .iter()
        .any(|candidate| candidate.valid_windows_pe);
    let root_correction = if assessment.launcher_proven || has_any_pe {
        root_correction_target(&recommendation.root, &relationship, games)
            .map(|game| {
                root_correction::assess(
                    context,
                    game.id(),
                    recommendation.root.path().as_str(),
                    &executable_basenames(&executables),
                    None,
                )
            })
            .transpose()?
    } else {
        None
    };
    compute_effective_root_fingerprint(
        &recommendation.root,
        catalog_generation,
        &boundary,
        &relationship,
        root_correction.as_ref(),
        &executables,
    )
}

pub(super) fn executable_inspections(
    assessment: &install_boundary::CandidateBoundaryAssessment,
) -> Vec<ExecutableInspection> {
    assessment
        .executables
        .iter()
        .map(|candidate| ExecutableInspection {
            path: candidate.absolute_path.to_string_lossy().replace('\\', "/"),
            relative_path: candidate.relative_path.clone(),
            size_bytes: candidate.size_bytes,
            rank_score: candidate.rank_score,
            valid_windows_pe: candidate.valid_windows_pe,
            rejection_kind: candidate.rejection_kind.clone(),
            rejection_token: candidate.rejection_token.clone(),
        })
        .collect()
}

const fn map_boundary_completeness(
    completeness: install_boundary::BoundaryCompleteness,
) -> TraversalCompleteness {
    match completeness {
        install_boundary::BoundaryCompleteness::Complete => TraversalCompleteness::Complete,
        install_boundary::BoundaryCompleteness::Incomplete => TraversalCompleteness::Incomplete,
    }
}

const fn map_boundary_evidence(
    evidence: install_boundary::InstallBoundaryEvidenceKind,
) -> InstallBoundaryEvidence {
    match evidence {
        install_boundary::InstallBoundaryEvidenceKind::LauncherManifest => {
            InstallBoundaryEvidence::LauncherManifest
        }
        install_boundary::InstallBoundaryEvidenceKind::EngineDistributionRoot => {
            InstallBoundaryEvidence::EngineDistributionRoot
        }
        install_boundary::InstallBoundaryEvidenceKind::RootExecutable => {
            InstallBoundaryEvidence::RootExecutable
        }
        install_boundary::InstallBoundaryEvidenceKind::EngineStructure => {
            InstallBoundaryEvidence::EngineStructure
        }
        install_boundary::InstallBoundaryEvidenceKind::ComponentContext => {
            InstallBoundaryEvidence::ComponentContext
        }
        install_boundary::InstallBoundaryEvidenceKind::ExecutableBranch => {
            InstallBoundaryEvidence::ExecutableBranch
        }
    }
}

pub(super) fn normalized_root(path: &Path) -> Result<PathBuf, ServiceError> {
    if !path.exists() {
        return Err(ServiceError::invalid_input(format!(
            "game folder does not exist: {}",
            path.display()
        )));
    }
    if !path.is_dir() {
        return Err(ServiceError::invalid_input(format!(
            "game folder is not a directory: {}",
            path.display()
        )));
    }
    renderpilot_platform_windows::canonicalize_install_path(path).map_err(|error| {
        ServiceError::invalid_input(format!(
            "game folder could not be resolved to a stable filesystem identity: {} ({error})",
            path.display()
        ))
    })
}

pub(super) fn install_root_from_path(
    path: &Path,
) -> Result<renderpilot_domain::InstallRoot, ServiceError> {
    PathRef::new(path.to_string_lossy())
        .map(renderpilot_domain::InstallRoot::new)
        .map_err(|error| ServiceError::invalid_input(error.to_string()))
}

pub(super) fn validate_root_invariants(path: &str) -> Result<(), ServiceError> {
    let key = normalized_path_key(path);
    let is_drive_root =
        key.len() == 3 && key.as_bytes()[0].is_ascii_alphabetic() && &key.as_bytes()[1..] == b":/";
    let is_unc_share_root = key.starts_with("//")
        && key
            .trim_start_matches('/')
            .split('/')
            .filter(|part| !part.is_empty())
            .count()
            <= 2;
    if key == "/" || is_drive_root || is_unc_share_root {
        return Err(ServiceError::invalid_install_root(
            InvalidInstallRootReason::FilesystemRoot,
            "filesystem roots and UNC share roots cannot be added as a game".to_owned(),
        ));
    }

    let normalized = key.trim_end_matches('/');
    let system_root = ["windows", "program files", "program files (x86)"]
        .iter()
        .any(|name| normalized.ends_with(&format!("/{name}")));
    if system_root {
        return Err(ServiceError::invalid_install_root(
            InvalidInstallRootReason::SystemDirectory,
            "system directories cannot be added as a game".to_owned(),
        ));
    }
    Ok(())
}
