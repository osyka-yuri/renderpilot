//! Confirmation, stale-state validation, and catalog mutation.

use super::*;

/// Adds or refreshes exactly one confirmed installation.
pub fn add_game(
    context: &crate::Context,
    request: AddGameRequest,
) -> Result<AddGameResult, ServiceError> {
    let _scan_guard = context.catalog_scan_guard();
    let selected = inspect_game_install_unlocked(context, &request.selected_root)?;
    if request.inspection_fingerprint != selected.inspection_fingerprint {
        return Err(ServiceError::StaleInstallInspection {
            selected_root: selected.selected_root.path().as_str().to_owned(),
            current_fingerprint: selected.inspection_fingerprint,
        });
    }

    let selected_option = selected
        .decision
        .option_for(request.root_choice)
        .ok_or_else(|| ServiceError::StaleInstallInspection {
            selected_root: selected.selected_root.path().as_str().to_owned(),
            current_fingerprint: selected.inspection_fingerprint.clone(),
        })?;
    if selected_option.catalog_action == AddGameCatalogAction::CorrectExistingRoot
        && !request.allow_root_correction
    {
        return Err(ServiceError::invalid_input(
            "root correction requires explicit caller permission",
        ));
    }
    let requested_text = match request.root_choice {
        AddGameRootChoice::Selected => selected.selected_root.clone(),
        AddGameRootChoice::Recommended => selected
            .recommendation
            .as_ref()
            .map(|recommendation| recommendation.root.clone())
            .ok_or_else(|| ServiceError::StaleInstallInspection {
                selected_root: selected.selected_root.path().as_str().to_owned(),
                current_fingerprint: selected.inspection_fingerprint.clone(),
            })?,
    };
    let expected_effective_fingerprint = match request.root_choice {
        AddGameRootChoice::Selected => compute_effective_root_fingerprint(
            &selected.selected_root,
            selected.catalog_generation,
            &selected.boundary,
            &selected.relationship,
            selected.root_correction.as_ref(),
            &selected.executables,
        )?,
        AddGameRootChoice::Recommended => selected
            .recommendation
            .as_ref()
            .map(|recommendation| recommendation.effective_fingerprint.clone())
            .ok_or_else(|| ServiceError::StaleInstallInspection {
                selected_root: selected.selected_root.path().as_str().to_owned(),
                current_fingerprint: selected.inspection_fingerprint.clone(),
            })?,
    };
    let requested = PathBuf::from(requested_text.path().as_str());

    // Re-inspect under the catalog guard. UI inspection is advisory and never
    // authorizes a stale mutation.
    let effective = if requested_text == selected.selected_root {
        selected
    } else {
        inspect_game_install_unlocked(context, &requested)?
    };
    let current_effective_fingerprint = compute_effective_root_fingerprint(
        &effective.selected_root,
        effective.catalog_generation,
        &effective.boundary,
        &effective.relationship,
        effective.root_correction.as_ref(),
        &effective.executables,
    )?;
    if current_effective_fingerprint != expected_effective_fingerprint {
        return Err(ServiceError::StaleInstallInspection {
            selected_root: effective.selected_root.path().as_str().to_owned(),
            current_fingerprint: effective.inspection_fingerprint,
        });
    }
    let current_action = effective
        .decision
        .option_for(AddGameRootChoice::Selected)
        .map(|option| option.catalog_action);
    if current_action != Some(selected_option.catalog_action) {
        return Err(ServiceError::StaleInstallInspection {
            selected_root: effective.selected_root.path().as_str().to_owned(),
            current_fingerprint: effective.inspection_fingerprint,
        });
    }
    let catalog_games = context.storage().list_games()?;
    let explicit_root_correction =
        selected_option.catalog_action == AddGameCatalogAction::CorrectExistingRoot;
    let persisted_executable = matches!(
        effective.relationship.kind,
        InstallRelationshipKind::ExactExisting
            | InstallRelationshipKind::ExpandsExisting
            | InstallRelationshipKind::NarrowsExisting
            | InstallRelationshipKind::InsideExisting
    )
    .then(|| {
        effective
            .relationship
            .game_ids
            .first()
            .and_then(|id| catalog_games.iter().find(|game| game.id().as_str() == id))
            .and_then(confirmed_executable_path)
    })
    .flatten()
    .filter(|path| inspection_contains_valid_executable(&effective, path));
    let explicit_executable = request.chosen_executable.or(persisted_executable);
    if explicit_root_correction {
        ensure_explicit_root_correction(&effective, &catalog_games)?;
        ensure_executable_ready(&effective, explicit_executable.as_deref())?;
    } else {
        ensure_addable(&effective, explicit_executable.as_deref())?;
    }
    ensure_root_correction_not_blocked(&effective)?;

    let existing_id = effective.relationship.game_ids.first().cloned();
    let game_id = match effective.relationship.kind {
        InstallRelationshipKind::ExactExisting
        | InstallRelationshipKind::ExpandsExisting
        | InstallRelationshipKind::NarrowsExisting => parse_existing_game_id(existing_id)?,
        InstallRelationshipKind::New => GameId::generate(),
        InstallRelationshipKind::InsideExisting if explicit_root_correction => {
            parse_existing_game_id(existing_id)?
        }
        InstallRelationshipKind::InsideExisting => {
            return Err(overlap_error(
                "selected folder is inside an existing game",
                &effective.relationship.game_ids,
            ));
        }
        InstallRelationshipKind::ContainsProvenInstall => {
            return Err(overlap_error(
                "selected folder contains a proven game installation",
                &effective.relationship.game_ids,
            ));
        }
        InstallRelationshipKind::ContainsMultiple => {
            return Err(overlap_error(
                "selected folder contains multiple existing games",
                &effective.relationship.game_ids,
            ));
        }
    };

    let root_authority = if effective
        .boundary
        .evidence
        .contains(&InstallBoundaryEvidence::LauncherManifest)
    {
        RootAuthority::LauncherManifest
    } else {
        RootAuthority::UserConfirmed
    };
    let consolidation_candidates =
        legacy_descendant_candidates(&requested_text, &catalog_games, &game_id);
    let scan = crate::catalog::scan::scan_explicit_install(
        context,
        requested,
        game_id,
        root_authority,
        explicit_executable,
        if explicit_root_correction {
            crate::catalog::scan::ExplicitRootChange::Narrowed
        } else {
            match effective.relationship.kind {
                InstallRelationshipKind::NarrowsExisting => {
                    crate::catalog::scan::ExplicitRootChange::Narrowed
                }
                InstallRelationshipKind::ExpandsExisting => {
                    crate::catalog::scan::ExplicitRootChange::Expanded
                }
                _ => crate::catalog::scan::ExplicitRootChange::Unchanged,
            }
        },
        &consolidation_candidates,
    )?;

    let disposition = if explicit_root_correction
        || matches!(
            effective.relationship.kind,
            InstallRelationshipKind::ExpandsExisting | InstallRelationshipKind::NarrowsExisting
        ) {
        AddGameDisposition::RootCorrected
    } else {
        match scan.change {
            CatalogScanChange::Added => AddGameDisposition::Added,
            CatalogScanChange::Unchanged => AddGameDisposition::Unchanged,
            CatalogScanChange::Updated => AddGameDisposition::Updated,
        }
    };

    let root_was_corrected = explicit_root_correction
        || matches!(
            effective.relationship.kind,
            InstallRelationshipKind::ExpandsExisting | InstallRelationshipKind::NarrowsExisting
        );
    let mut warnings = effective.warnings;
    if root_was_corrected {
        // These diagnostics explain why confirmation was requested. Once the
        // correction succeeds they are stale and must not become completion
        // warnings or global toasts.
        warnings.retain(|warning| {
            !matches!(
                warning,
                AddGameWarning::InsideExistingInstall | AddGameWarning::NarrowsExistingInstall
            )
        });
    }
    if !scan.consolidation.removed_game_ids.is_empty() {
        warnings.push(AddGameWarning::LegacyCardsConsolidated {
            count: scan.consolidation.removed_game_ids.len(),
        });
    }
    if !scan.consolidation.retained_candidate_game_ids.is_empty() {
        warnings.push(AddGameWarning::LegacyCardsRetained {
            count: scan.consolidation.retained_candidate_game_ids.len(),
        });
    }
    if let Some(path) = &scan.consolidation.recovery_bundle_path {
        warnings.push(AddGameWarning::RecoveryBundleCreated { path: path.clone() });
    }
    if let Some(path) = &scan.root_correction_recovery_bundle_path {
        warnings.push(AddGameWarning::RootCorrectionHistoryArchived { path: path.clone() });
    }

    let recovery_bundle_path = scan
        .root_correction_recovery_bundle_path
        .clone()
        .or_else(|| scan.consolidation.recovery_bundle_path.clone());

    Ok(AddGameResult {
        game_id: scan.game.id().as_str().to_owned(),
        effective_root: scan.game.install_path().as_str().to_owned(),
        disposition,
        root_authority: scan.game.root_authority(),
        detected_library_count: scan.libraries.len(),
        consolidated_game_ids: scan
            .consolidation
            .removed_game_ids
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect(),
        recovery_bundle_path,
        warnings,
    })
}

pub(super) fn inspection_contains_valid_executable(
    inspection: &AddGameInspection,
    executable: &Path,
) -> bool {
    let executable_key = normalized_path_key(&executable.to_string_lossy());
    inspection.executables.iter().any(|candidate| {
        candidate.valid_windows_pe && normalized_path_key(&candidate.path) == executable_key
    })
}

pub(super) fn ensure_addable(
    inspection: &AddGameInspection,
    chosen_executable: Option<&Path>,
) -> Result<(), ServiceError> {
    match inspection.boundary.kind {
        InstallBoundaryKind::MultipleInstallContainer => {
            return Err(ServiceError::MultipleInstallsDetected(
                "the selected folder contains multiple independent game installations".to_owned(),
            ));
        }
        InstallBoundaryKind::SingleInstall
        | InstallBoundaryKind::EngineProjectSubtree
        | InstallBoundaryKind::BinarySubtree
        | InstallBoundaryKind::SingleInstallContainer
        | InstallBoundaryKind::Ambiguous
        | InstallBoundaryKind::Incomplete => {}
    }
    if matches!(
        inspection.relationship.kind,
        InstallRelationshipKind::InsideExisting
            | InstallRelationshipKind::ContainsProvenInstall
            | InstallRelationshipKind::ContainsMultiple
    ) {
        return Ok(());
    }

    ensure_executable_ready(inspection, chosen_executable)
}

pub(super) fn ensure_explicit_root_correction(
    inspection: &AddGameInspection,
    games: &[renderpilot_domain::GameInstallation],
) -> Result<(), ServiceError> {
    if root_correction_target(&inspection.selected_root, &inspection.relationship, games).is_none()
    {
        return Err(overlap_error(
            "the selected folder cannot safely replace the existing game root",
            &inspection.relationship.game_ids,
        ));
    }

    let Some(assessment) = &inspection.root_correction else {
        return Err(overlap_error(
            "the selected folder has insufficient executable evidence for root correction",
            &inspection.relationship.game_ids,
        ));
    };
    if inspection.relationship.game_ids.first() != Some(&assessment.game_id) {
        return Err(overlap_error(
            "the root-correction assessment no longer matches the existing game",
            &inspection.relationship.game_ids,
        ));
    }

    // The inspection assessment is advisory. The authoritative decision must
    // use the prospective full component set under the catalog/game locks in
    // `scan_explicit_install`; otherwise a component appearing or disappearing
    // between inspection and confirmation could authorize stale UI state.
    Ok(())
}

pub(super) fn ensure_root_correction_not_blocked(
    inspection: &AddGameInspection,
) -> Result<(), ServiceError> {
    let Some(assessment) = &inspection.root_correction else {
        return Ok(());
    };
    if assessment.status != RootCorrectionStatus::Blocked {
        return Ok(());
    }

    Err(ServiceError::RootCorrectionBlocked {
        game_id: assessment.game_id.clone(),
        blockers: assessment
            .blockers
            .iter()
            .map(|blocker| blocker.as_str().to_owned())
            .collect(),
    })
}

pub(super) fn ensure_executable_ready(
    inspection: &AddGameInspection,
    chosen_executable: Option<&Path>,
) -> Result<(), ServiceError> {
    let selected_root_proven = inspection
        .boundary
        .evidence
        .contains(&InstallBoundaryEvidence::LauncherManifest)
        || inspection.relationship.kind == InstallRelationshipKind::ExactExisting;
    let valid_ranked = inspection
        .executables
        .iter()
        .any(|exe| exe.valid_windows_pe && exe.rejection_kind.is_none());
    if selected_root_proven || valid_ranked {
        if let Some(chosen) = chosen_executable {
            validate_chosen_executable(inspection, chosen)?;
        }
        return Ok(());
    }

    let Some(chosen) = chosen_executable else {
        return Err(ServiceError::invalid_input(
            "a readable Windows PE game executable is required",
        ));
    };
    validate_chosen_executable(inspection, chosen)
}

pub(super) fn validate_chosen_executable(
    inspection: &AddGameInspection,
    chosen: &Path,
) -> Result<(), ServiceError> {
    let chosen_key = normalized_path_key(&chosen.to_string_lossy());
    let candidate = inspection
        .executables
        .iter()
        .find(|exe| normalized_path_key(&exe.path) == chosen_key);
    if candidate.is_some_and(|candidate| candidate.valid_windows_pe) {
        Ok(())
    } else {
        Err(ServiceError::invalid_input(
            "chosen executable is not a readable PE file inside the inspected root",
        ))
    }
}

pub(super) fn parse_existing_game_id(value: Option<String>) -> Result<GameId, ServiceError> {
    let value = value.ok_or_else(|| {
        ServiceError::invalid_input("catalog relationship did not include an existing game")
    })?;
    GameId::new(value).map_err(|error| ServiceError::invalid_input(error.to_string()))
}

pub(super) fn overlap_error(message: &str, game_ids: &[String]) -> ServiceError {
    if game_ids.is_empty() {
        ServiceError::invalid_input(message)
    } else {
        ServiceError::invalid_input(format!("{message}: {}", game_ids.join(", ")))
    }
}
