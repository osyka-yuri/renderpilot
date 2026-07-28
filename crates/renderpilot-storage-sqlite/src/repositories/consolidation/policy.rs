//! Typed per-table conflict policy for runtime catalog consolidation.

use std::collections::{BTreeSet, HashSet};

use renderpilot_application::AppResult;
use rusqlite::{Connection, named_params};

use crate::error::{storage_context, storage_error};

use super::{ConsolidationConflictSummary, ConsolidationPlan, validation::validate_plan};

pub(in crate::repositories) fn inspect_conflicts(
    connection: &Connection,
    plan: &ConsolidationPlan,
) -> AppResult<ConsolidationConflictSummary> {
    validate_plan(plan)?;
    let destination = plan.destination_game_id.as_str();
    let mut destination_wins = BTreeSet::<String>::new();
    let mut blocking = BTreeSet::<String>::new();

    for source in &plan.sources {
        let source_id = source.source_game_id.as_str();
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

        if keyed_rows_differ(
            connection,
            "nvapi_setting_baselines",
            "setting_key",
            &[
                "baseline_dword",
                "baseline_was_predefined",
                "predefined_dword",
                "captured_exe",
            ],
            destination,
            source_id,
        )? {
            blocking.insert("nvapi_setting_baselines".to_owned());
        }
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
                "nvapi_setting_baselines",
                "setting_key",
                &[
                    "baseline_dword",
                    "baseline_was_predefined",
                    "predefined_dword",
                    "captured_exe",
                ],
                left_id,
                right_id,
            )? {
                blocking.insert("nvapi_setting_baselines".to_owned());
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
        }
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
