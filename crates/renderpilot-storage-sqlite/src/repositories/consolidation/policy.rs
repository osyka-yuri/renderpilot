//! Typed per-table conflict policy for runtime catalog consolidation.

use std::collections::{BTreeSet, HashSet};

use renderpilot_application::AppResult;
use renderpilot_domain::{
    AddonKind, EngineConfigJournal, GameId, GameProxyTopology, OptiScalerAdoptionState,
    OptiScalerInstallState, OptiScalerInstallStateParts, OptiScalerPrerequisiteBinding, PathRef,
    ProxyImplementation, Sha256Hash, from_persisted,
};
use rusqlite::{Connection, OptionalExtension, Row, named_params};

use crate::error::{storage_context, storage_error};
use crate::mapping;

use super::{ConsolidationConflictSummary, ConsolidationPlan, validation::validate_plan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::repositories::consolidation) enum OptiScalerAggregate {
    Absent,
    Complete {
        state: Box<OptiScalerInstallState>,
        topology: Box<GameProxyTopology>,
    },
    InvalidPartial {
        reason: String,
    },
}

#[derive(Debug)]
struct RawOptiScalerState {
    game_id: String,
    release_id: String,
    manifest_revision: String,
    archive_sha256: Option<String>,
    source: Option<String>,
    target_exe_path: String,
    target_dir: String,
    modules_json: String,
    release_files_json: String,
    runtime_bindings_json: String,
    directory_receipts_json: String,
    proxy_topology_id: String,
    config_schema: u32,
    config_base_release: String,
    adoption_state: String,
    prerequisite_binding: String,
    created_at: i64,
    updated_at: i64,
    configuration_baseline_json: String,
}

#[derive(Debug)]
struct RawProxyTopology {
    row_id: String,
    game_id: String,
    topology_json: String,
}

pub(in crate::repositories::consolidation) fn load_optiscaler_aggregate(
    connection: &Connection,
    game_id: &GameId,
) -> AppResult<OptiScalerAggregate> {
    let state = connection
        .query_row(
            "SELECT game_id, release_id, manifest_revision, archive_sha256, source,
                    target_exe_path, target_dir, modules_json, release_files_json,
                    runtime_bindings_json, directory_receipts_json, proxy_topology_id,
                    config_schema, config_base_release, adoption_state, prerequisite_binding,
                    created_at, updated_at,
                    configuration_baseline_json
               FROM optiscaler_install_states WHERE game_id = :game_id",
            named_params! { ":game_id": game_id.as_str() },
            raw_state_from_row,
        )
        .optional()
        .map_err(storage_error)?
        .map(parse_state);
    let topology = connection
        .query_row(
            "SELECT id, game_id, topology_json
               FROM game_proxy_topologies WHERE game_id = :game_id",
            named_params! { ":game_id": game_id.as_str() },
            |row| {
                Ok(RawProxyTopology {
                    row_id: row.get(0)?,
                    game_id: row.get(1)?,
                    topology_json: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(storage_error)?
        .map(parse_topology);

    match (state, topology) {
        (None, None) => Ok(OptiScalerAggregate::Absent),
        (Some(Ok(state)), Some(Ok(topology))) => {
            if state.game_id != topology.game_id
                || state.proxy_topology_id.as_deref() != Some(topology.id.as_str())
                || topology.outer.implementation != ProxyImplementation::OptiScaler
            {
                return Ok(OptiScalerAggregate::InvalidPartial {
                    reason: "OptiScaler state/topology identity or outer is inconsistent"
                        .to_owned(),
                });
            }
            Ok(OptiScalerAggregate::Complete {
                state: Box::new(state),
                topology: Box::new(topology),
            })
        }
        (Some(Err(reason)), _) | (_, Some(Err(reason))) => {
            Ok(OptiScalerAggregate::InvalidPartial { reason })
        }
        (Some(Ok(_)), None) => Ok(OptiScalerAggregate::InvalidPartial {
            reason: "OptiScaler install state exists without proxy topology".to_owned(),
        }),
        (None, Some(Ok(_))) => Ok(OptiScalerAggregate::InvalidPartial {
            reason: "OptiScaler proxy topology exists without install state".to_owned(),
        }),
    }
}

pub(in crate::repositories::consolidation) fn equivalent_after_rebase(
    left: &OptiScalerAggregate,
    right: &OptiScalerAggregate,
    destination: &GameId,
) -> bool {
    let (
        OptiScalerAggregate::Complete {
            state: left_state,
            topology: left_topology,
        },
        OptiScalerAggregate::Complete {
            state: right_state,
            topology: right_topology,
        },
    ) = (left, right)
    else {
        return false;
    };
    let left = canonical_rebase(left_state, left_topology, destination);
    let right = canonical_rebase(right_state, right_topology, destination);
    left == right
}

pub(in crate::repositories::consolidation) fn canonical_rebase(
    state: &OptiScalerInstallState,
    topology: &GameProxyTopology,
    destination: &GameId,
) -> (OptiScalerInstallState, GameProxyTopology) {
    let topology_id = format!("optiscaler:{}", destination.as_str());
    let mut state = state.clone();
    state.game_id = destination.clone();
    state.proxy_topology_id = Some(topology_id.clone());
    state.created_at = None;
    state.updated_at = None;
    let mut topology = topology.clone();
    topology.game_id = destination.clone();
    topology.id = topology_id;
    (state, topology)
}

fn raw_state_from_row(row: &Row<'_>) -> rusqlite::Result<RawOptiScalerState> {
    Ok(RawOptiScalerState {
        game_id: row.get(0)?,
        release_id: row.get(1)?,
        manifest_revision: row.get(2)?,
        archive_sha256: row.get(3)?,
        source: row.get(4)?,
        target_exe_path: row.get(5)?,
        target_dir: row.get(6)?,
        modules_json: row.get(7)?,
        release_files_json: row.get(8)?,
        runtime_bindings_json: row.get(9)?,
        directory_receipts_json: row.get(10)?,
        proxy_topology_id: row.get(11)?,
        config_schema: row.get(12)?,
        config_base_release: row.get(13)?,
        adoption_state: row.get(14)?,
        prerequisite_binding: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
        configuration_baseline_json: row.get(18)?,
    })
}

fn parse_state(raw: RawOptiScalerState) -> Result<OptiScalerInstallState, String> {
    let baseline_json = raw.configuration_baseline_json;
    let parts = OptiScalerInstallStateParts {
        game_id: GameId::new(raw.game_id).map_err(|error| error.to_string())?,
        release_id: raw.release_id,
        manifest_revision: raw.manifest_revision,
        archive_sha256: raw
            .archive_sha256
            .map(|value| Sha256Hash::new(value).map_err(|error| error.to_string()))
            .transpose()?,
        source: raw.source,
        target_exe_path: PathRef::new(raw.target_exe_path).map_err(|error| error.to_string())?,
        target_dir: PathRef::new(raw.target_dir).map_err(|error| error.to_string())?,
        modules: mapping::deserialize_json(&raw.modules_json).map_err(|error| error.to_string())?,
        release_files: mapping::deserialize_json(&raw.release_files_json)
            .map_err(|error| error.to_string())?,
        runtime_bindings: mapping::deserialize_json(&raw.runtime_bindings_json)
            .map_err(|error| error.to_string())?,
        directory_receipts: mapping::deserialize_json(&raw.directory_receipts_json)
            .map_err(|error| error.to_string())?,
        proxy_topology_id: Some(raw.proxy_topology_id),
        config_schema: raw.config_schema,
        config_base_release: raw.config_base_release,
        adoption_state: raw
            .adoption_state
            .parse::<OptiScalerAdoptionState>()
            .map_err(|()| "unknown OptiScaler adoption state".to_owned())?,
        prerequisite_binding: raw
            .prerequisite_binding
            .parse::<OptiScalerPrerequisiteBinding>()
            .map_err(|()| "unknown OptiScaler prerequisite binding".to_owned())?,
        created_at: Some(raw.created_at),
        updated_at: Some(raw.updated_at),
    };
    from_persisted(
        parts,
        super::super::optiscaler_states::codec::decode(&baseline_json)
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

fn parse_topology(raw: RawProxyTopology) -> Result<GameProxyTopology, String> {
    let RawProxyTopology {
        row_id,
        game_id,
        topology_json,
    } = raw;
    let topology: GameProxyTopology =
        mapping::deserialize_json(&topology_json).map_err(|error| error.to_string())?;
    if topology.id != row_id || topology.game_id.as_str() != game_id {
        return Err("proxy topology row identity disagrees with its payload".to_owned());
    }
    topology.validate().map_err(|error| error.to_string())?;
    Ok(topology)
}

pub(in crate::repositories) fn inspect_conflicts(
    connection: &Connection,
    plan: &ConsolidationPlan,
) -> AppResult<ConsolidationConflictSummary> {
    validate_plan(plan)?;
    let destination = plan.destination_game_id.as_str();
    let mut destination_wins = BTreeSet::<String>::new();
    let mut blocking = BTreeSet::<String>::new();

    let destination_pending = if !plan.sources.is_empty() {
        engine_config_journal_is_pending(connection, destination)?
    } else {
        false
    };

    for source in &plan.sources {
        let source_id = source.source_game_id.as_str();
        for table in [
            "nvapi_owned_profiles",
            "nvapi_game_claim_refs",
            "pending_drs_operations",
        ] {
            if row_exists(connection, table, destination)?
                || row_exists(connection, table, source_id)?
            {
                blocking.insert(table.to_owned());
            }
        }
        // A pending Engine.ini publication must be recovered/finalized by its
        // owning add-on before game identity consolidation.  Copying that raw
        // token to a rebased row would make the stage/path ownership ambiguous;
        // stable journals are safe to copy as part of the ordinary row move.
        if destination_pending || engine_config_journal_is_pending(connection, source_id)? {
            blocking.insert("installed_addons".to_owned());
        }
        if row_exists(connection, "installed_addons", destination)?
            && row_exists(connection, "installed_addons", source_id)?
            && !singleton_rows_equal(
                connection,
                "installed_addons",
                &[
                    "kind",
                    "addon_file",
                    "addon_version",
                    "created_files_json",
                    "backed_up_files_json",
                    "managed_files_json",
                    "tracked_sources_json",
                    "host_kind",
                    "reshade_channel",
                    "registered_exe_path",
                    "renodx_config_receipt_json",
                    "engine_config_journal_json",
                ],
                destination,
                source_id,
            )?
        {
            blocking.insert("installed_addons".to_owned());
        }
        if row_exists(connection, "game_covers", destination)?
            && row_exists(connection, "game_covers", source_id)?
            && !singleton_rows_equal(
                connection,
                "game_covers",
                &["file_name"],
                destination,
                source_id,
            )?
        {
            destination_wins.insert("game_covers".to_owned());
        }
        if row_exists(connection, "nvapi_executable_overrides", destination)?
            && row_exists(connection, "nvapi_executable_overrides", source_id)?
            && !singleton_rows_equal(
                connection,
                "nvapi_executable_overrides",
                &["selected_path", "selected_basename"],
                destination,
                source_id,
            )?
        {
            blocking.insert("nvapi_executable_overrides".to_owned());
        }
        if row_exists(connection, "pending_file_mutations", destination)?
            || row_exists(connection, "pending_file_mutations", source_id)?
        {
            blocking.insert("pending_file_mutations".to_owned());
        }
        if shared_mutation_exists(connection, destination)?
            || shared_mutation_exists(connection, source_id)?
        {
            blocking.insert("pending_shared_vulkan_mutations".to_owned());
        }

        inspect_optiscaler_pair(
            connection,
            &plan.destination_game_id,
            &source.source_game_id,
            &mut destination_wins,
            &mut blocking,
        )?;

        if keyed_rows_differ(
            connection,
            "profile_addon_capabilities",
            "addon_kind",
            &["source_revision"],
            destination,
            source_id,
        )? {
            destination_wins.insert("profile_addon_capabilities".to_owned());
        }

        let mapped_components = source
            .component_rekeys
            .iter()
            .map(|rekey| rekey.source_component_id.as_str())
            .collect::<HashSet<_>>();
        if operation_component_ids(connection, source_id)?
            .iter()
            .any(|component_id| !mapped_components.contains(component_id.as_str()))
        {
            blocking.insert("operations".to_owned());
        }

        for rekey in &source.component_rekeys {
            let source_exists: bool = connection
                .query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM component_backups
                        WHERE game_id = :source_game_id AND component_id = :source_component_id
                    )",
                    named_params! {
                        ":source_game_id": source_id,
                        ":source_component_id": rekey.source_component_id,
                    },
                    |row| row.get(0),
                )
                .map_err(storage_error)?;
            let destination_exists: bool = connection
                .query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM component_backups
                        WHERE game_id = :destination_game_id
                          AND component_id = :destination_component_id
                    )",
                    named_params! {
                        ":destination_game_id": destination,
                        ":destination_component_id": rekey.destination_component_id,
                    },
                    |row| row.get(0),
                )
                .map_err(storage_error)?;
            if source_exists && destination_exists {
                let equal: bool = connection
                    .query_row(
                        "SELECT EXISTS(
                            SELECT 1
                              FROM component_backups source
                              JOIN component_backups destination
                                ON destination.component_id = :destination_component_id
                             WHERE source.component_id = :source_component_id
                               AND source.game_id = :source_game_id
                               AND destination.game_id = :destination_game_id
                               AND source.files_json IS destination.files_json
                               AND source.auxiliary_json IS destination.auxiliary_json
                        )",
                        named_params! {
                            ":source_component_id": rekey.source_component_id,
                            ":source_game_id": source_id,
                            ":destination_component_id": rekey.destination_component_id,
                            ":destination_game_id": destination,
                        },
                        |row| row.get(0),
                    )
                    .map_err(storage_error)?;
                if !equal {
                    blocking.insert("component_backups".to_owned());
                }
            }
        }
    }

    inspect_source_to_source_conflicts(connection, plan, &mut blocking)?;

    Ok(ConsolidationConflictSummary {
        destination_wins_tables: destination_wins.into_iter().collect(),
        blocking_tables: blocking.into_iter().collect(),
    })
}

fn engine_config_journal_is_pending(connection: &Connection, game_id: &str) -> AppResult<bool> {
    let raw = connection
        .query_row(
            "SELECT kind, engine_config_journal_json
               FROM installed_addons WHERE game_id = :game_id",
            named_params! { ":game_id": game_id },
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .map_err(storage_error)?;
    let Some((kind, Some(raw))) = raw else {
        return Ok(false);
    };
    let kind: AddonKind = mapping::enum_from_text(&kind)?;
    let journal: EngineConfigJournal = mapping::deserialize_json(&raw)?;
    if journal.is_empty() {
        return Err(storage_error(
            "empty Engine.ini journal must be stored as SQL NULL",
        ));
    }
    journal
        .validate_for_kind(kind)
        .map_err(|error| storage_error(error.to_string()))?;
    if mapping::serialize_json(&journal)? != raw {
        return Err(storage_error(
            "cannot consolidate a non-canonical Engine.ini journal",
        ));
    }
    Ok(journal.is_pending())
}

fn inspect_source_to_source_conflicts(
    connection: &Connection,
    plan: &ConsolidationPlan,
    blocking: &mut BTreeSet<String>,
) -> AppResult<()> {
    for (index, left) in plan.sources.iter().enumerate() {
        for right in &plan.sources[(index + 1)..] {
            let left_id = left.source_game_id.as_str();
            let right_id = right.source_game_id.as_str();

            for (table, columns) in [
                (
                    "installed_addons",
                    &[
                        "kind",
                        "addon_file",
                        "addon_version",
                        "created_files_json",
                        "backed_up_files_json",
                        "managed_files_json",
                        "tracked_sources_json",
                        "host_kind",
                        "reshade_channel",
                        "registered_exe_path",
                        "renodx_config_receipt_json",
                        "engine_config_journal_json",
                    ][..],
                ),
                ("game_covers", &["file_name"][..]),
                (
                    "nvapi_executable_overrides",
                    &["selected_path", "selected_basename"][..],
                ),
            ] {
                if row_exists(connection, table, left_id)?
                    && row_exists(connection, table, right_id)?
                    && !singleton_rows_equal(connection, table, columns, left_id, right_id)?
                {
                    blocking.insert(table.to_owned());
                }
            }

            if keyed_rows_differ(
                connection,
                "profile_addon_capabilities",
                "addon_kind",
                &["source_revision"],
                left_id,
                right_id,
            )? {
                blocking.insert("profile_addon_capabilities".to_owned());
            }

            let left_aggregate = load_optiscaler_aggregate(connection, &left.source_game_id)?;
            let right_aggregate = load_optiscaler_aggregate(connection, &right.source_game_id)?;
            match (&left_aggregate, &right_aggregate) {
                (OptiScalerAggregate::InvalidPartial { .. }, _)
                | (_, OptiScalerAggregate::InvalidPartial { .. }) => {
                    blocking.insert("game_proxy_topologies".to_owned());
                    blocking.insert("optiscaler_install_states".to_owned());
                }
                (OptiScalerAggregate::Complete { .. }, OptiScalerAggregate::Complete { .. })
                    if !equivalent_after_rebase(
                        &left_aggregate,
                        &right_aggregate,
                        &plan.destination_game_id,
                    ) =>
                {
                    blocking.insert("game_proxy_topologies".to_owned());
                    blocking.insert("optiscaler_install_states".to_owned());
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn inspect_optiscaler_pair(
    connection: &Connection,
    destination: &GameId,
    source: &GameId,
    destination_wins: &mut BTreeSet<String>,
    blocking: &mut BTreeSet<String>,
) -> AppResult<()> {
    let destination_aggregate = load_optiscaler_aggregate(connection, destination)?;
    let source_aggregate = load_optiscaler_aggregate(connection, source)?;
    match (&destination_aggregate, &source_aggregate) {
        (OptiScalerAggregate::Complete { .. }, OptiScalerAggregate::Complete { .. })
            if equivalent_after_rebase(&destination_aggregate, &source_aggregate, destination) =>
        {
            destination_wins.insert("game_proxy_topologies".to_owned());
            destination_wins.insert("optiscaler_install_states".to_owned());
        }
        (OptiScalerAggregate::InvalidPartial { .. }, _)
        | (_, OptiScalerAggregate::InvalidPartial { .. })
        | (OptiScalerAggregate::Complete { .. }, OptiScalerAggregate::Complete { .. }) => {
            blocking.insert("game_proxy_topologies".to_owned());
            blocking.insert("optiscaler_install_states".to_owned());
        }
        _ => {}
    }
    Ok(())
}

fn row_exists(connection: &Connection, table: &str, game_id: &str) -> AppResult<bool> {
    connection
        .query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE game_id = :game_id)"),
            named_params! { ":game_id": game_id },
            |row| row.get(0),
        )
        .map_err(storage_error)
}

fn singleton_rows_equal(
    connection: &Connection,
    table: &str,
    columns: &[&str],
    destination: &str,
    source: &str,
) -> AppResult<bool> {
    let equality = columns
        .iter()
        .map(|column| format!("destination.{column} IS source.{column}"))
        .collect::<Vec<_>>()
        .join(" AND ");
    connection
        .query_row(
            &format!(
                "SELECT EXISTS(
                    SELECT 1
                      FROM {table} source
                      JOIN {table} destination
                        ON destination.game_id = :destination
                     WHERE source.game_id = :source
                       AND {equality}
                )"
            ),
            named_params! { ":source": source, ":destination": destination },
            |row| row.get(0),
        )
        .map_err(|error| {
            storage_context(
                &format!("could not compare {table} consolidation rows"),
                error,
            )
        })
}

fn keyed_rows_differ(
    connection: &Connection,
    table: &str,
    key: &str,
    columns: &[&str],
    destination: &str,
    source: &str,
) -> AppResult<bool> {
    let equality = columns
        .iter()
        .map(|column| format!("destination.{column} IS source.{column}"))
        .collect::<Vec<_>>()
        .join(" AND ");
    connection
        .query_row(
            &format!(
                "SELECT EXISTS(
                    SELECT 1 FROM {table} source
                    JOIN {table} destination ON destination.{key} = source.{key}
                    WHERE source.game_id = :source
                      AND destination.game_id = :destination
                      AND NOT ({equality})
                )"
            ),
            named_params! { ":source": source, ":destination": destination },
            |row| row.get(0),
        )
        .map_err(|error| {
            storage_context(
                &format!("could not inspect {table} consolidation conflicts"),
                error,
            )
        })
}

fn shared_mutation_exists(connection: &Connection, game_id: &str) -> AppResult<bool> {
    connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM pending_shared_vulkan_mutations
                 WHERE scope = 'game_shared' AND game_id = :game_id
            )",
            named_params! { ":game_id": game_id },
            |row| row.get(0),
        )
        .map_err(storage_error)
}

fn operation_component_ids(connection: &Connection, game_id: &str) -> AppResult<Vec<String>> {
    let mut statement = connection
        .prepare(
            "SELECT DISTINCT component_id
               FROM operation_items
              WHERE game_id = :game_id",
        )
        .map_err(storage_error)?;
    statement
        .query_map(named_params! { ":game_id": game_id }, |row| row.get(0))
        .map_err(storage_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage_error)
}
