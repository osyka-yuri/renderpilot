pub(in crate::file_mutation) fn recover_pending_optiscaler(
    context: &Context,
    guard: &GameMutationGuard,
    proof: RecoveringOptiScalerJournalAggregate,
) -> Result<(), ServiceError> {
    if proof.game_id() != guard.game_id() {
        return Err(crate::failed(
            "pending OptiScaler mutation belongs to another game",
        ));
    }
    let state = proof.state();
    let journal = proof.journal().clone();
    journal
        .validate()
        .map_err(|error| crate::failed(error.to_string()))?;
    validate_private_workspace_paths(proof.operation_id(), &journal)?;
    preflight_threat_model(journal.threat_model())?;
    let id = proof.operation_id().to_owned();
    let game_id = proof.game_id().clone();
    let journal_json = proof.current_journal_json().to_owned();
    let managed_endpoint_roots = crate::addons::optiscaler::stored_status(context, &game_id)?
        .map_or_default(|state| crate::addons::optiscaler::managed_state_endpoint_roots(&state))
        .into_iter()
        .map(|(root, endpoint)| (crate::paths::normalized_key(&endpoint), root))
        .collect();
    let prepared = PreparedFileMutation {
        id,
        game_id,
        executor: context.peer_mutation_executor(),
        authority: JournalAuthority::Recovering(Box::new(proof)),
        transaction_root: context.file_mutation_root().to_path_buf(),
        journal,
        journal_json,
        next_operation_id: 0,
        managed_endpoint_roots,
    };
    match state {
        PendingFileMutationState::Preparing | PendingFileMutationState::Prepared => {
            rollback_prepared(prepared)?;
        }
        PendingFileMutationState::Committed => {
            let mut prepared = prepared;
            prepared.apply_post_commit_directory_cleanup()?;
            cleanup_committed(&mut prepared)?;
        }
    }
    Ok(())
}
