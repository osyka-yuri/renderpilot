//! Catalog relationship and root-recommendation policy.

use super::*;

pub(super) struct RootAdvisor<'a> {
    selected_root: &'a renderpilot_domain::InstallRoot,
    games: &'a [renderpilot_domain::GameInstallation],
    launcher_roots: &'a [renderpilot_domain::InstallRoot],
}

impl<'a> RootAdvisor<'a> {
    pub(super) fn new(
        selected_root: &'a renderpilot_domain::InstallRoot,
        games: &'a [renderpilot_domain::GameInstallation],
        launcher_roots: &'a [renderpilot_domain::InstallRoot],
    ) -> Self {
        Self {
            selected_root,
            games,
            launcher_roots,
        }
    }

    pub(super) fn relationship(&self) -> InstallRelationship {
        classify_relationship(self.selected_root, self.games, self.launcher_roots)
    }

    pub(super) fn recommendation(
        &self,
        relationship: &InstallRelationship,
    ) -> Option<(renderpilot_domain::InstallRoot, RootRecommendationSource)> {
        if relationship.kind == InstallRelationshipKind::InsideExisting {
            if let Some(id) = relationship.game_ids.first()
                && let Some(game) = self.games.iter().find(|game| game.id().as_str() == id)
            {
                let source = if game.root_authority() == RootAuthority::LauncherManifest {
                    RootRecommendationSource::LauncherManifest
                } else {
                    RootRecommendationSource::ExistingCatalog
                };
                return Some((game.install_root().clone(), source));
            }

            let containing = relationship
                .proven_install_roots
                .iter()
                .filter(|root| root.contains_root(self.selected_root))
                .max_by_key(|root| root.path().as_str().len())
                .cloned();
            return containing.map(|root| (root, RootRecommendationSource::LauncherManifest));
        }

        let mut alternatives = relationship
            .proven_install_roots
            .iter()
            .filter(|root| *root != self.selected_root)
            .cloned()
            .collect::<Vec<_>>();
        alternatives.sort();
        alternatives.dedup();
        if alternatives.len() == 1 {
            return alternatives
                .pop()
                .map(|root| (root, RootRecommendationSource::LauncherManifest));
        }

        None
    }
}

pub(super) fn classify_relationship(
    selected_root: &renderpilot_domain::InstallRoot,
    games: &[renderpilot_domain::GameInstallation],
    launcher_roots: &[renderpilot_domain::InstallRoot],
) -> InstallRelationship {
    let mut exact = Vec::new();
    let mut containing = Vec::new();
    let mut contained = Vec::new();

    for game in games {
        let install = game.install_root();
        if install == selected_root {
            exact.push(game);
        } else if install.contains_root(selected_root) {
            containing.push(game);
        } else if selected_root.contains_root(install) {
            contained.push(game);
        }
    }

    let mut launcher_exact = Vec::new();
    let mut launcher_containing = Vec::new();
    let mut launcher_contained = Vec::new();
    for root in launcher_roots {
        if root == selected_root {
            launcher_exact.push(root.clone());
        } else if root.contains_root(selected_root) {
            launcher_containing.push(root.clone());
        } else if selected_root.contains_root(root) {
            launcher_contained.push(root.clone());
        }
    }
    let mut proven_install_roots = launcher_exact
        .iter()
        .chain(&launcher_containing)
        .chain(&launcher_contained)
        .cloned()
        .collect::<Vec<_>>();
    proven_install_roots.sort();
    proven_install_roots.dedup();
    launcher_contained.sort();
    launcher_contained.dedup();

    let catalog_proven_contained = contained.to_vec();
    let mut proven_root_keys = launcher_contained
        .iter()
        .map(|root| root.key().clone())
        .chain(
            catalog_proven_contained
                .iter()
                .map(|game| game.install_key().clone()),
        )
        .collect::<Vec<_>>();
    proven_root_keys.sort();
    proven_root_keys.dedup();

    let (kind, involved) = if !exact.is_empty() {
        (InstallRelationshipKind::ExactExisting, exact)
    } else if !containing.is_empty() {
        containing.sort_by_key(|game| std::cmp::Reverse(game.install_path().as_str().len()));
        (InstallRelationshipKind::InsideExisting, containing)
    } else if !launcher_containing.is_empty() {
        (InstallRelationshipKind::InsideExisting, Vec::new())
    } else if proven_root_keys.len() > 1 {
        (
            InstallRelationshipKind::ContainsMultiple,
            catalog_proven_contained,
        )
    } else if proven_root_keys.len() == 1 {
        (
            InstallRelationshipKind::ContainsProvenInstall,
            catalog_proven_contained,
        )
    } else {
        (InstallRelationshipKind::New, Vec::new())
    };

    InstallRelationship {
        kind,
        game_ids: involved
            .into_iter()
            .map(|game| game.id().as_str().to_owned())
            .collect(),
        proven_install_roots,
    }
}

pub(super) fn refine_relationship_with_boundary(
    selected_root: &renderpilot_domain::InstallRoot,
    mut relationship: InstallRelationship,
    games: &[renderpilot_domain::GameInstallation],
    selected_boundary: &install_boundary::CandidateBoundaryAssessment,
    launcher_install_roots: &[PathBuf],
) -> InstallRelationship {
    let involved = relationship
        .game_ids
        .iter()
        .filter_map(|id| games.iter().find(|game| game.id().as_str() == id))
        .collect::<Vec<_>>();
    let launcher_inside_selection = launcher_install_roots.iter().any(|root| {
        super::inspection::install_root_from_path(root)
            .is_ok_and(|root| selected_root.contains_root(&root) && root != *selected_root)
    });

    match relationship.kind {
        InstallRelationshipKind::InsideExisting if involved.len() == 1 => {
            let game = involved[0];
            if is_user_confirmed_manual(game)
                && !launcher_inside_selection
                && selected_boundary.kind == install_boundary::InstallBoundaryKind::SingleInstall
            {
                let existing = install_boundary::InstallBoundaryAnalyzer::inspect_candidate(
                    Path::new(game.install_path().as_str()),
                    launcher_install_roots,
                );
                if matches!(
                    existing.kind,
                    install_boundary::InstallBoundaryKind::SingleInstallContainer
                        | install_boundary::InstallBoundaryKind::MultipleInstallContainer
                ) && assessment_candidate_matches(&existing, selected_root)
                {
                    relationship.kind = InstallRelationshipKind::NarrowsExisting;
                }
            }
        }
        InstallRelationshipKind::ContainsProvenInstall if involved.len() == 1 => {
            let game = involved[0];
            let selected_is_single_install = selected_boundary.kind
                == install_boundary::InstallBoundaryKind::SingleInstall
                || (selected_boundary.kind == install_boundary::InstallBoundaryKind::Ambiguous
                    && selected_boundary.evidence.contains(
                        &install_boundary::InstallBoundaryEvidenceKind::ComponentContext,
                    )
                    && assessment_contains_confirmed_executable(selected_boundary, game));
            if is_user_confirmed_manual(game)
                && !launcher_inside_selection
                && selected_is_single_install
            {
                let existing = install_boundary::InstallBoundaryAnalyzer::inspect_candidate(
                    Path::new(game.install_path().as_str()),
                    launcher_install_roots,
                );
                let engine_proves_parent = existing.engine_layout.iter().any(|evidence| {
                    matches!(
                        evidence.role,
                        install_boundary::EngineBoundaryRole::ProjectSubtree
                            | install_boundary::EngineBoundaryRole::BinarySubtree
                    ) && evidence.distribution_root.as_ref().is_some_and(|root| {
                        super::inspection::install_root_from_path(root)
                            .is_ok_and(|root| root == *selected_root)
                    })
                });
                let structure_proves_parent = selected_boundary
                    .evidence
                    .contains(&install_boundary::InstallBoundaryEvidenceKind::ComponentContext)
                    && (assessment_candidate_matches(selected_boundary, game.install_root())
                        || assessment_contains_confirmed_executable(selected_boundary, game));
                if engine_proves_parent || structure_proves_parent {
                    relationship.kind = InstallRelationshipKind::ExpandsExisting;
                }
            }
        }
        InstallRelationshipKind::ContainsMultiple
            if !launcher_inside_selection
                && selected_boundary.kind
                    == install_boundary::InstallBoundaryKind::SingleInstall
                && !involved.is_empty()
                && involved.iter().all(|game| {
                    game.identity().launcher() == Launcher::Manual
                        && game.root_authority() == RootAuthority::Legacy
                        && {
                            let child =
                                install_boundary::InstallBoundaryAnalyzer::inspect_candidate(
                                    Path::new(game.install_path().as_str()),
                                    launcher_install_roots,
                                );
                            !child.launcher_proven && !child.has_accepted_executable
                        }
                }) =>
        {
            relationship.kind = InstallRelationshipKind::New;
            relationship.game_ids.clear();
        }
        _ => {}
    }

    relationship
}

pub(super) fn assessment_candidate_matches(
    assessment: &install_boundary::CandidateBoundaryAssessment,
    root: &renderpilot_domain::InstallRoot,
) -> bool {
    assessment
        .candidate_roots
        .iter()
        .filter_map(|candidate| super::inspection::install_root_from_path(candidate).ok())
        .any(|candidate| candidate == *root)
}

pub(super) fn assessment_contains_confirmed_executable(
    assessment: &install_boundary::CandidateBoundaryAssessment,
    game: &renderpilot_domain::GameInstallation,
) -> bool {
    let Some(confirmed) = confirmed_executable_path(game) else {
        return false;
    };
    let confirmed_key = normalized_path_key(&confirmed.to_string_lossy());
    assessment.executables.iter().any(|candidate| {
        candidate.valid_windows_pe
            && normalized_path_key(&candidate.absolute_path.to_string_lossy()) == confirmed_key
    })
}

pub(super) fn confirmed_executable_path(
    game: &renderpilot_domain::GameInstallation,
) -> Option<PathBuf> {
    game.confirmed_executable()
        .map(|relative| PathBuf::from(game.install_path().as_str()).join(relative.as_str()))
}

pub(super) fn is_user_confirmed_manual(game: &renderpilot_domain::GameInstallation) -> bool {
    game.identity().launcher() == Launcher::Manual
        && game.root_authority() == RootAuthority::UserConfirmed
}

pub(super) fn root_correction_target<'a>(
    selected_root: &renderpilot_domain::InstallRoot,
    relationship: &InstallRelationship,
    games: &'a [renderpilot_domain::GameInstallation],
) -> Option<&'a renderpilot_domain::GameInstallation> {
    if !matches!(
        relationship.kind,
        InstallRelationshipKind::NarrowsExisting | InstallRelationshipKind::ExpandsExisting
    ) || relationship.game_ids.len() != 1
    {
        return None;
    }

    let game = games
        .iter()
        .find(|game| game.id().as_str() == relationship.game_ids[0])?;
    if !is_user_confirmed_manual(game) {
        return None;
    }

    let has_conflicting_launcher_scope = relationship
        .proven_install_roots
        .iter()
        .any(|root| root != selected_root);
    (!has_conflicting_launcher_scope).then_some(game)
}

pub(super) fn legacy_descendant_candidates(
    install_root: &renderpilot_domain::InstallRoot,
    games: &[renderpilot_domain::GameInstallation],
    destination_id: &GameId,
) -> Vec<GameId> {
    games
        .iter()
        .filter(|game| game.id() != destination_id)
        .filter(|game| game.root_authority() == RootAuthority::Legacy)
        .filter(|game| game.identity().launcher() == Launcher::Manual)
        .filter(|game| {
            install_root.contains_root(game.install_root()) && game.install_root() != install_root
        })
        .map(|game| game.id().clone())
        .collect()
}
