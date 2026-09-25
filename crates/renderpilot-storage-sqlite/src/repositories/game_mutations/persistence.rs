use super::*;

pub(super) fn apply_optiscaler_transition(
    transaction: &rusqlite::Transaction<'_>,
    game_id: &GameId,
    mutation: OptiScalerAggregateMutation<'_>,
) -> AppResult<()> {
    match mutation {
        OptiScalerAggregateMutation::AdoptExactMetadata { state, topology } => {
            super::proxy_topologies::upsert_within_transaction(transaction, topology)?;
            super::optiscaler_states::upsert_within_transaction(transaction, state)?;
        }
        OptiScalerAggregateMutation::Filesystem {
            before_state,
            after_state,
            before_topology,
            after_topology,
            peer,
            ..
        }
        | OptiScalerAggregateMutation::FilesystemWithAuxiliary {
            before_state,
            after_state,
            before_topology,
            after_topology,
            peer,
            ..
        } => {
            if before_state.is_some() {
                super::optiscaler_states::delete_within_transaction(transaction, game_id)?;
            }
            if before_topology.is_some() {
                super::proxy_topologies::delete_within_transaction(transaction, game_id)?;
            }
            if let Some(topology) = after_topology {
                super::proxy_topologies::upsert_within_transaction(transaction, topology)?;
            }
            if let Some(state) = after_state {
                super::optiscaler_states::upsert_within_transaction(transaction, state)?;
            }
            match peer {
                OptiScalerPeerMutation::Keep => {}
                OptiScalerPeerMutation::Replace { after, .. } => {
                    installed_addons::upsert_within_transaction(transaction, after)?;
                }
            }
        }
    }
    Ok(())
}

pub(super) fn validate_optiscaler_receipt_metadata(
    state: &OptiScalerInstallState,
    topology: &GameProxyTopology,
) -> AppResult<()> {
    for receipt in &state.release_files {
        pending_file_mutations::validate_optiscaler_file_receipt_metadata(&receipt.path)?;
    }
    for binding in &state.runtime_bindings {
        pending_file_mutations::validate_optiscaler_file_receipt_metadata(&binding.path)?;
    }
    for receipt in &state.directory_receipts {
        pending_file_mutations::validate_optiscaler_directory_receipt_metadata(
            &receipt.path,
            &receipt.identity,
        )?;
    }
    pending_file_mutations::validate_optiscaler_file_receipt_metadata(&topology.outer.path)?;
    if let Some(downstream) = &topology.downstream {
        pending_file_mutations::validate_optiscaler_file_receipt_metadata(&downstream.path)?;
    }
    Ok(())
}

pub(super) fn ensure_state_game(game_id: &GameId, state: &OptiScalerInstallState) -> AppResult<()> {
    if &state.game_id != game_id {
        return Err(renderpilot_application::AppError::invalid_input(
            "OptiScaler state belongs to another game",
        ));
    }
    Ok(())
}

pub(super) fn ensure_topology_game(
    game_id: &GameId,
    topology: &GameProxyTopology,
) -> AppResult<()> {
    if &topology.game_id != game_id {
        return Err(renderpilot_application::AppError::invalid_input(
            "proxy topology belongs to another game",
        ));
    }
    Ok(())
}

pub(super) fn ensure_peer_game(game_id: &GameId, addon: &InstalledAddon) -> AppResult<()> {
    if addon.game_id() != game_id {
        return Err(renderpilot_application::AppError::invalid_input(
            "OptiScaler peer receipt belongs to another game",
        ));
    }
    Ok(())
}

pub(super) fn persisted_install_root(
    transaction: &rusqlite::Transaction<'_>,
    game_id: &GameId,
) -> AppResult<PathRef> {
    let path: Option<String> = transaction
        .query_row(
            "SELECT install_path FROM games WHERE id=?1",
            [game_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(crate::error::storage_error)?;
    let Some(path) = path else {
        return Err(renderpilot_application::AppError::storage_failed(format!(
            "OptiScaler commit requires persisted game install {}",
            game_id.as_str()
        )));
    };
    PathRef::new(path).map_err(crate::error::invalid_row)
}

pub(super) fn ensure_exact_before_state(
    transaction: &rusqlite::Transaction<'_>,
    game_id: &GameId,
    expected: Option<&OptiScalerInstallState>,
) -> AppResult<()> {
    let actual = super::optiscaler_states::get_within_transaction(transaction, game_id)?;
    if actual.as_ref() != expected {
        return Err(renderpilot_application::AppError::storage_failed(
            "OptiScaler state changed before aggregate commit",
        ));
    }
    Ok(())
}

pub(super) fn ensure_exact_before_topology(
    transaction: &rusqlite::Transaction<'_>,
    game_id: &GameId,
    expected: Option<&GameProxyTopology>,
) -> AppResult<()> {
    let actual = super::proxy_topologies::get_within_transaction(transaction, game_id)?;
    if actual.as_ref() != expected {
        return Err(renderpilot_application::AppError::storage_failed(
            "proxy topology changed before aggregate commit",
        ));
    }
    Ok(())
}

pub(super) fn ensure_exact_before_peer(
    transaction: &rusqlite::Transaction<'_>,
    game_id: &GameId,
    expected: &InstalledAddon,
) -> AppResult<()> {
    let actual = super::installed_addons::get_within_transaction(transaction, game_id)?;
    if actual.as_ref() != Some(expected) {
        return Err(renderpilot_application::AppError::storage_failed(
            "OptiScaler peer receipt changed before aggregate commit",
        ));
    }
    Ok(())
}

pub(super) fn ensure_state_paths_within_install(
    install_root: &PathRef,
    state: &OptiScalerInstallState,
) -> AppResult<()> {
    ensure_path_within_install(
        install_root,
        &state.target_dir,
        "OptiScaler target directory",
    )?;
    ensure_path_within_install(
        install_root,
        &state.target_exe_path,
        "OptiScaler target executable",
    )
}

pub(super) fn ensure_topology_paths_within_install(
    install_root: &PathRef,
    topology: &GameProxyTopology,
) -> AppResult<()> {
    for path in topology.participant_paths() {
        ensure_path_within_install(install_root, path, "proxy topology path")?;
    }
    Ok(())
}

pub(super) fn ensure_peer_paths_within_install(
    install_root: &PathRef,
    addon: &InstalledAddon,
) -> AppResult<()> {
    for path in addon
        .created_files()
        .iter()
        .chain(addon.backed_up_files())
        .chain(
            addon
                .managed_files()
                .iter()
                .map(renderpilot_domain::ManagedAddonFile::path),
        )
        .chain(addon.registered_exe_path())
    {
        ensure_path_within_install(install_root, path, "OptiScaler peer path")?;
    }
    Ok(())
}

pub(super) fn ensure_path_within_install(
    install_root: &PathRef,
    path: &PathRef,
    field: &str,
) -> AppResult<()> {
    let root_key = normalized_path_key(install_root.as_str());
    let path_key = normalized_path_key(path.as_str());
    if path_key == root_key
        || path_key
            .strip_prefix(&root_key)
            .is_some_and(|rest| rest.starts_with('/'))
    {
        Ok(())
    } else {
        Err(renderpilot_application::AppError::invalid_input(format!(
            "{field} '{}' escapes persisted game install '{}'",
            path.as_str(),
            install_root.as_str()
        )))
    }
}

pub(super) fn ensure_state_topology(
    state: Option<&OptiScalerInstallState>,
    topology: Option<&GameProxyTopology>,
    side: &str,
) -> AppResult<()> {
    if let Some(topology) = topology
        && topology.outer.implementation != ProxyImplementation::OptiScaler
    {
        return Err(renderpilot_application::AppError::invalid_input(format!(
            "OptiScaler {side} topology must have an OptiScaler outer",
        )));
    }
    if let Some(state) = state {
        let Some(topology_id) = state.proxy_topology_id.as_deref() else {
            return Err(renderpilot_application::AppError::invalid_input(format!(
                "OptiScaler {side} state must name its proxy topology",
            )));
        };
        let Some(topology) = topology else {
            return Err(renderpilot_application::AppError::invalid_input(format!(
                "OptiScaler {side} state topology is missing",
            )));
        };
        if topology.id != topology_id {
            return Err(renderpilot_application::AppError::invalid_input(format!(
                "OptiScaler {side} state topology id does not match aggregate",
            )));
        }
        let target_key = normalized_path_key(state.target_dir.as_str());
        let root_key = normalized_path_key(topology.root_slot.as_str());
        if root_key == target_key
            || !root_key
                .strip_prefix(&target_key)
                .is_some_and(|rest| rest.starts_with('/'))
        {
            return Err(renderpilot_application::AppError::invalid_input(format!(
                "OptiScaler {side} proxy root slot must be inside the selected target directory",
            )));
        }
    } else if topology.is_some() {
        return Err(renderpilot_application::AppError::invalid_input(format!(
            "OptiScaler {side} topology cannot exist without state",
        )));
    }
    Ok(())
}
