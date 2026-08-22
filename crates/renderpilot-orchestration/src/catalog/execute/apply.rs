//! Apply-side orchestration for catalog component overlays.

use super::*;

/// Complete request for one safety-authorized additive component overlay.
#[derive(Clone, Copy)]
pub struct ApplySwapRequest<'a> {
    /// Application services and storage.
    pub context: &'a crate::Context,
    /// Game that owns the target component.
    pub game_id: &'a GameId,
    /// Component whose overlay is being replaced.
    pub component_id: &'a ComponentId,
    /// Catalog artifact to install.
    pub artifact_id: &'a ArtifactId,
    /// Optional confirmation for the independent executable-file risk gate.
    pub executable_confirmation: Option<&'a str>,
    /// Fresh permit authorizing this game-file mutation.
    pub safety: &'a crate::GameSafetyPermit,
}

/// Installs an artifact package over a component as an **additive overlay**.
pub fn apply_swap(request: ApplySwapRequest<'_>) -> Result<SwapResult, ServiceError> {
    let ApplySwapRequest {
        context,
        game_id,
        component_id,
        artifact_id,
        executable_confirmation: confirmation_token,
        safety,
    } = request;
    let guard = crate::mutation_boundary::enter_game_mutation_boundary(context, game_id)?;
    let storage = context.storage();
    let preflight = match crate::catalog::swap::load_swap_preflight(
        context,
        game_id,
        component_id,
        artifact_id,
        crate::catalog::swap::SwapPreflightMode::Apply {
            confirmation_supplied: confirmation_token.is_some(),
        },
    )? {
        crate::catalog::swap::SwapPreflight::Ready(preflight) => *preflight,
        crate::catalog::swap::SwapPreflight::UnusableSource { artifact_id, issue } => {
            invalidate_stale_artifact(storage, &artifact_id, &issue.to_string());
            return Err(AppError::stale_replacement_source().into());
        }
    };
    if confirmation_token.is_some()
        && preflight
            .operation_plan
            .d3d12_executable_action()
            .is_some_and(|action| {
                action.kind() == renderpilot_application::D3d12ExecutableActionKind::RepairRequired
            })
    {
        return Err(AppError::confirmation_token_mismatch().into());
    }
    let game_root = std::path::PathBuf::from(preflight.game.install_path().as_str());
    let mut prepared = prepare_apply_swap(game_id, component_id, preflight)?;
    validate_executable_confirmation(&prepared, confirmation_token)?;
    let mut executable_guard = prepared
        .d3d12
        .as_ref()
        .filter(|d3d12| d3d12.action.changes_executable())
        .map(|d3d12| mutation_guard::D3d12ExecutableMutationGuard::acquire(&d3d12.state))
        .transpose()?;
    let scope = crate::file_mutation::MutationScope::single(&game_root)?;
    crate::FileSafetyAuthority::new().authorize_game_commit(
        context,
        crate::addons::mutation_features::CATALOG_SWAP,
        &guard,
        safety,
        || {
            crate::file_mutation::run_durable_mutation(
                crate::file_mutation::DurableMutation {
                    context,
                    guard: &guard,
                    scope: &scope,
                    feature: crate::addons::mutation_features::CATALOG_SWAP,
                    subject_id: Some(component_id.as_str()),
                    paths: apply_mutation_paths(&prepared),
                },
                |mutation_id| -> AppResult<SwapResult> {
                    let result = (|| -> AppResult<SwapResult> {
                        #[cfg(test)]
                        run_before_copy_hook();

                        if let (Some(guard), Some(d3d12)) =
                            (executable_guard.as_mut(), prepared.d3d12.as_ref())
                        {
                            guard.ensure_backup(&d3d12.state, &d3d12.action)?;
                        }
                        #[cfg(test)]
                        inject_d3d12_apply_failure(D3d12ApplyFailurePoint::AfterExecutableBackup)?;

                        perform_apply_fs(
                            &prepared.component,
                            &prepared.baseline,
                            &prepared.planned,
                            &prepared.removed,
                        )?;
                        #[cfg(test)]
                        inject_d3d12_apply_failure(D3d12ApplyFailurePoint::AfterDllMutation)?;

                        let active_executable_sha256 =
                            match (executable_guard.as_mut(), prepared.d3d12.as_ref()) {
                                (Some(guard), Some(d3d12)) => {
                                    Some(guard.apply_action(&d3d12.state, &d3d12.action)?)
                                }
                                _ => None,
                            };
                        #[cfg(test)]
                        inject_d3d12_apply_failure(
                            D3d12ApplyFailurePoint::AfterExecutableMutation,
                        )?;

                        if let Err(error) = rebind_planned_files_for_technology(
                            &mut prepared.planned,
                            prepared.component.technology(),
                        ) {
                            if matches!(error.kind(), AppErrorKind::StaleReplacementSource) {
                                invalidate_stale_artifact(
                                    storage,
                                    prepared.artifact.id(),
                                    "installed target content or PE metadata mismatch",
                                );
                            }
                            return Err(error);
                        }

                        let (next_components, to_version) = rebuild_component_set_after_overlay(
                            storage,
                            &prepared.game_id,
                            &prepared.component,
                            &prepared.component_id,
                            &prepared.baseline,
                            &prepared.planned,
                            &prepared.removed,
                        )?;

                        let executable_baseline = build_executable_baseline(
                            &prepared,
                            active_executable_sha256.as_ref(),
                        )?;
                        let expected_active_files = next_components
                            .iter()
                            .find(|component| component.id() == &prepared.component_id)
                            .map(|component| component.files().to_vec())
                            .unwrap_or_default();
                        let rollback_baseline = match executable_baseline.clone() {
                            Some(executable) => {
                                ComponentRollbackBaseline::new(prepared.baseline.clone())
                                    .with_expected_active_files(expected_active_files.clone())
                                    .with_d3d12_executable(executable)
                            }
                            None => ComponentRollbackBaseline::new(prepared.baseline.clone())
                                .with_expected_active_files(expected_active_files.clone()),
                        };
                        let expected_active = expected_active_executable_identity(
                            &prepared,
                            active_executable_sha256.as_ref(),
                        );
                        let mut baseline_mutations = if prepared.first_swap {
                            vec![ComponentBaselineMutation::Capture {
                                component_id: &prepared.component_id,
                                baseline: &rollback_baseline,
                            }]
                        } else {
                            vec![ComponentBaselineMutation::UpdateExpectedActiveFiles {
                                component_id: &prepared.component_id,
                                files: &expected_active_files,
                            }]
                        };
                        if !prepared.first_swap {
                            if let Some(baseline) = executable_baseline.as_ref().filter(|_| {
                                prepared
                                    .rollback_baseline
                                    .as_ref()
                                    .is_some_and(|baseline| baseline.d3d12_executable().is_none())
                            }) {
                                baseline_mutations.push(
                                    ComponentBaselineMutation::CaptureD3d12Executable {
                                        component_id: &prepared.component_id,
                                        baseline,
                                    },
                                );
                            } else if let Some(expected_active) = expected_active.as_ref() {
                                baseline_mutations.push(
                                    ComponentBaselineMutation::UpdateD3d12ExecutableState {
                                        component_id: &prepared.component_id,
                                        expected_active,
                                    },
                                );
                            }
                        }
                        #[cfg(test)]
                        inject_d3d12_apply_failure(D3d12ApplyFailurePoint::BeforeDatabaseCommit)?;
                        storage.commit_game_mutation(GameMutationCommit {
                            game_id: &prepared.game_id,
                            component_set: Some(&next_components),
                            baseline_mutations: &baseline_mutations,
                            addon: InstalledAddonMutation::Keep,
                            mutation_id: Some(mutation_id),
                        })?;

                        let d3d12_executable_action = prepared.d3d12.as_ref().and_then(|d3d12| {
                            types::D3d12ExecutableActionResult::from_action(&d3d12.action)
                        });
                        let mut journal_items = prepared
                            .planned
                            .iter()
                            .map(|plan| {
                                JournalEntryItem::component_file(
                                    plan.file.path(),
                                    Some(prepared.artifact.id().clone()),
                                )
                            })
                            .collect::<Vec<_>>();
                        if let Some(action) = prepared
                            .d3d12
                            .as_ref()
                            .map(|d3d12| &d3d12.action)
                            .filter(|action| action.changes_executable())
                        {
                            journal_items.push(JournalEntryItem::d3d12_executable(action));
                        }
                        record_operation_journal_entry(
                            storage,
                            JournalEntryParams {
                                game_id: &prepared.game_id,
                                component_id: &prepared.component_id,
                                kind: OperationKind::ReplaceComponent,
                                component: &prepared.component,
                                to_version: to_version.as_deref(),
                                items: journal_items,
                                d3d12_executable_action: d3d12_executable_action.clone(),
                            },
                        );

                        Ok(SwapResult {
                            game_id: prepared.game_id.as_str().to_owned(),
                            component_id: prepared.component_id.as_str().to_owned(),
                            applied_path: prepared.applied_path(),
                            replacement_path: prepared.replacement_path(),
                            updated_file_count: prepared.planned.len(),
                            d3d12_executable_action,
                        })
                    })();
                    // The transaction may need to restore the snapshotted EXE on error.
                    // Release our deny-write handle before control reaches that rollback.
                    drop(executable_guard.take());
                    result
                },
                |_| {},
                || {},
            )
        },
    )
}

fn validate_executable_confirmation(
    prepared: &types::PreparedApplySwap,
    provided: Option<&str>,
) -> AppResult<()> {
    let Some(d3d12) = prepared.d3d12.as_ref() else {
        return Ok(());
    };
    if d3d12.action.requires_confirmation() && Some(d3d12.confirmation_token.as_str()) != provided {
        return Err(AppError::confirmation_token_mismatch());
    }
    Ok(())
}

fn build_executable_baseline(
    prepared: &types::PreparedApplySwap,
    active_sha256: Option<&renderpilot_domain::Sha256Hash>,
) -> AppResult<Option<D3d12ExecutableBaseline>> {
    let (Some(d3d12), Some(active_sha256)) = (prepared.d3d12.as_ref(), active_sha256) else {
        return Ok(None);
    };
    let state = &d3d12.state;
    let action = &d3d12.action;
    if !action.changes_executable() {
        return Ok(None);
    }
    let executable_path = PathRef::new(state.executable_path.to_string_lossy().into_owned())
        .map_err(|error| AppError::invalid_input(error.to_string()))?;
    Ok(Some(D3d12ExecutableBaseline::new(
        executable_path,
        D3d12ExecutableIdentity::new(state.original_sdk_version, state.original_sha256.clone()),
        D3d12ExecutableIdentity::new(action.target_sdk_version(), active_sha256.clone()),
    )))
}

fn expected_active_executable_identity(
    prepared: &types::PreparedApplySwap,
    active_sha256: Option<&renderpilot_domain::Sha256Hash>,
) -> Option<D3d12ExecutableIdentity> {
    let (Some(recorded), Some(d3d12), Some(active_sha256)) = (
        prepared.rollback_baseline.as_ref(),
        prepared.d3d12.as_ref(),
        active_sha256,
    ) else {
        return None;
    };
    if !d3d12.action.changes_executable() || recorded.d3d12_executable().is_none() {
        return None;
    }
    Some(D3d12ExecutableIdentity::new(
        d3d12.action.target_sdk_version(),
        active_sha256.clone(),
    ))
}

/// Computes the canonical set of filesystem paths touched by an apply.
fn apply_mutation_paths_set(
    current: &[renderpilot_domain::ComponentFile],
    baseline: &[renderpilot_domain::ComponentFile],
    planned: &[types::PlannedFile],
    removed: &[renderpilot_domain::ComponentFile],
) -> Vec<std::path::PathBuf> {
    let mut live: Vec<std::path::PathBuf> = current
        .iter()
        .chain(baseline)
        .chain(removed)
        .map(|file| std::path::PathBuf::from(file.path().as_str()))
        .collect();
    live.extend(planned.iter().map(types::PlannedFile::target));
    crate::fs::expand_with_sidecars(live)
}

pub(super) fn apply_mutation_paths(prepared: &types::PreparedApplySwap) -> Vec<std::path::PathBuf> {
    let mut paths = apply_mutation_paths_set(
        prepared.component.files(),
        &prepared.baseline,
        &prepared.planned,
        &prepared.removed,
    );
    if let Some(d3d12) = prepared
        .d3d12
        .as_ref()
        .filter(|d3d12| d3d12.action.changes_executable())
    {
        let state = &d3d12.state;
        paths.push(state.executable_path.clone());
        // An existing immutable backup is opened read-only and never changed by
        // apply. Snapshot it only on the first patch, when the transaction may
        // create the sidecar and must be able to remove it during recovery.
        if !state.backup_exists {
            paths.push(state.backup_path.clone());
        }
    }
    paths
}

fn invalidate_stale_artifact(storage: &SqliteStorage, artifact_id: &ArtifactId, reason: &str) {
    if let Err(error) = storage.delete_artifact(artifact_id) {
        log::warn!(
            "failed to invalidate stale artifact {} ({reason}): {error}",
            artifact_id.as_str()
        );
    } else {
        log::info!(
            "invalidated stale artifact {} ({reason})",
            artifact_id.as_str()
        );
    }
}
