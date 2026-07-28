//! Transactional execution of an already inspected consolidation plan.

use renderpilot_application::{AppError, AppResult};
use rusqlite::{Connection, OptionalExtension, Transaction, named_params};

use super::{
    ConsolidationConflictSummary, ConsolidationPlan, ConsolidationReport, ConsolidationSource,
    policy::inspect_conflicts, validation::validate_plan,
};
use crate::error::storage_error;

pub(in crate::repositories) fn ensure_conflicts_unchanged(
    connection: &Connection,
    plan: &ConsolidationPlan,
    expected: &ConsolidationConflictSummary,
) -> AppResult<()> {
    let current = inspect_conflicts(connection, plan)?;
    if current != *expected {
        return Err(AppError::storage_failed(
            "consolidation conflict state changed after recovery preview; retry the scan",
        ));
    }
    Ok(())
}

pub(in crate::repositories) fn apply(
    transaction: &Transaction<'_>,
    plan: &ConsolidationPlan,
) -> AppResult<ConsolidationReport> {
    validate_plan(plan)?;
    let preview = inspect_conflicts(transaction, plan)?;
    if preview.has_blocking_conflicts() {
        return Err(AppError::storage_failed(format!(
            "consolidation is blocked by ambiguous managed state in: {}",
            preview.blocking_tables.join(", ")
        )));
    }
    let destination = plan.destination_game_id.as_str();
    let mut report = ConsolidationReport {
        destination_wins_conflicts: preview.destination_wins_tables,
        ..ConsolidationReport::default()
    };

    for source in &plan.sources {
        let source_id = source.source_game_id.as_str();
        ensure_no_pending_mutations(transaction, destination, source_id)?;
        ensure_component_mapping_is_complete(transaction, destination, source)?;

        move_operation_state(transaction, destination, source)?;
        move_component_backups(transaction, destination, source)?;

        transaction
            .execute(
                "UPDATE library_artifacts
                    SET source_game_id = :destination
                  WHERE source_game_id = :source",
                named_params! { ":destination": destination, ":source": source_id },
            )
            .map_err(storage_error)?;

        move_singleton_destination_wins(
            transaction,
            "installed_addons",
            destination,
            source_id,
            r#"
                INSERT INTO installed_addons (
                    game_id, kind, addon_file, addon_version, created_files_json,
                    backed_up_files_json, managed_files_json, tracked_sources_json,
                    host_kind, reshade_channel, registered_exe_path, created_at, updated_at
                )
                SELECT :destination, kind, addon_file, addon_version, created_files_json,
                       backed_up_files_json, managed_files_json, tracked_sources_json,
                       host_kind, reshade_channel, registered_exe_path, created_at, updated_at
                  FROM installed_addons WHERE game_id = :source
                ON CONFLICT(game_id) DO NOTHING
            "#,
        )?;

        move_cover(transaction, destination, source_id, &mut report)?;
        move_singleton_destination_wins(
            transaction,
            "nvapi_executable_overrides",
            destination,
            source_id,
            r#"
                INSERT INTO nvapi_executable_overrides (
                    game_id, selected_path, selected_basename, updated_at
                )
                SELECT :destination, selected_path, selected_basename, updated_at
                  FROM nvapi_executable_overrides WHERE game_id = :source
                ON CONFLICT(game_id) DO NOTHING
            "#,
        )?;
        move_nvapi_baselines(transaction, destination, source_id)?;
        merge_ui_state(transaction, destination, source_id)?;
        move_profile_capabilities(transaction, destination, source_id)?;

        transaction
            .execute(
                "DELETE FROM games WHERE id = :source",
                named_params! { ":source": source_id },
            )
            .map_err(storage_error)?;
        report.removed_game_ids.push(source.source_game_id.clone());
    }

    Ok(report)
}

pub(in crate::repositories) fn verify_foreign_keys(transaction: &Transaction<'_>) -> AppResult<()> {
    let violation: Option<(String, i64, String, i64)> = transaction
        .query_row("PRAGMA foreign_key_check", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .optional()
        .map_err(storage_error)?;

    if let Some((table, row_id, parent, constraint)) = violation {
        return Err(AppError::storage_failed(format!(
            "consolidation violated foreign key {constraint}: {table} row {row_id} -> {parent}"
        )));
    }
    Ok(())
}

fn ensure_no_pending_mutations(
    transaction: &Transaction<'_>,
    destination_game_id: &str,
    source_game_id: &str,
) -> AppResult<()> {
    let pending: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM pending_file_mutations
              WHERE game_id IN (:destination, :source)",
            named_params! {
                ":destination": destination_game_id,
                ":source": source_game_id,
            },
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    if pending != 0 {
        return Err(AppError::storage_failed(format!(
            "cannot consolidate {source_game_id}: {pending} pending file mutation(s) remain after recovery"
        )));
    }
    Ok(())
}

fn ensure_component_mapping_is_complete(
    transaction: &Transaction<'_>,
    destination_game_id: &str,
    source: &ConsolidationSource,
) -> AppResult<()> {
    let source_game_exists: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM games WHERE id = :source)",
            named_params! { ":source": source.source_game_id.as_str() },
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    let destination_game_exists: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM games WHERE id = :destination)",
            named_params! { ":destination": destination_game_id },
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    if !source_game_exists || !destination_game_exists {
        return Err(AppError::storage_failed(format!(
            "consolidation game ownership changed for {}",
            source.source_game_id
        )));
    }

    let source_count: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM components WHERE game_id = :source",
            named_params! { ":source": source.source_game_id.as_str() },
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    if usize::try_from(source_count).ok() != Some(source.component_rekeys.len()) {
        return Err(AppError::storage_failed(format!(
            "component mapping for {} is stale or incomplete",
            source.source_game_id
        )));
    }

    for rekey in &source.component_rekeys {
        let source_owner: Option<String> = transaction
            .query_row(
                "SELECT game_id FROM components WHERE id = :component_id",
                named_params! { ":component_id": rekey.source_component_id },
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        if source_owner.as_deref() != Some(source.source_game_id.as_str()) {
            return Err(AppError::storage_failed(format!(
                "source component {} is missing or no longer belongs to {}",
                rekey.source_component_id, source.source_game_id
            )));
        }

        let destination_owner: Option<String> = transaction
            .query_row(
                "SELECT game_id FROM components WHERE id = :component_id",
                named_params! { ":component_id": rekey.destination_component_id },
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        if destination_owner.as_deref() != Some(destination_game_id) {
            return Err(AppError::storage_failed(format!(
                "destination component {} is missing or does not belong to {}",
                rekey.destination_component_id, destination_game_id
            )));
        }
    }
    Ok(())
}

fn move_operation_state(
    transaction: &Transaction<'_>,
    destination: &str,
    source: &ConsolidationSource,
) -> AppResult<()> {
    let source_id = source.source_game_id.as_str();
    transaction
        .execute(
            "UPDATE operations SET game_id = :destination WHERE game_id = :source",
            named_params! { ":destination": destination, ":source": source_id },
        )
        .map_err(storage_error)?;

    for rekey in &source.component_rekeys {
        transaction
            .execute(
                "UPDATE operation_items
                    SET game_id = :destination,
                        component_id = :destination_component
                  WHERE game_id = :source
                    AND component_id = :source_component",
                named_params! {
                    ":destination": destination,
                    ":destination_component": rekey.destination_component_id,
                    ":source": source_id,
                    ":source_component": rekey.source_component_id,
                },
            )
            .map_err(storage_error)?;
    }

    let remaining: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM operation_items WHERE game_id = :source",
            named_params! { ":source": source_id },
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    if remaining != 0 {
        return Err(AppError::storage_failed(format!(
            "operation item mapping for {source_id} is incomplete"
        )));
    }
    Ok(())
}

fn move_component_backups(
    transaction: &Transaction<'_>,
    destination: &str,
    source: &ConsolidationSource,
) -> AppResult<()> {
    for rekey in &source.component_rekeys {
        transaction
            .execute(
                r#"
                    INSERT INTO component_backups (
                        component_id, game_id, files_json, auxiliary_json,
                        created_at, updated_at
                    )
                    SELECT :destination_component, :destination, files_json,
                           auxiliary_json, created_at, updated_at
                      FROM component_backups
                     WHERE game_id = :source AND component_id = :source_component
                    ON CONFLICT(component_id) DO NOTHING
                "#,
                named_params! {
                    ":destination_component": rekey.destination_component_id,
                    ":destination": destination,
                    ":source": source.source_game_id.as_str(),
                    ":source_component": rekey.source_component_id,
                },
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "DELETE FROM component_backups
                  WHERE game_id = :source AND component_id = :source_component",
                named_params! {
                    ":source": source.source_game_id.as_str(),
                    ":source_component": rekey.source_component_id,
                },
            )
            .map_err(storage_error)?;
    }
    Ok(())
}

fn move_singleton_destination_wins(
    transaction: &Transaction<'_>,
    table: &str,
    destination: &str,
    source: &str,
    insert_sql: &str,
) -> AppResult<()> {
    transaction
        .execute(
            insert_sql,
            named_params! { ":destination": destination, ":source": source },
        )
        .map_err(storage_error)?;
    transaction
        .execute(
            &format!("DELETE FROM {table} WHERE game_id = :source"),
            named_params! { ":source": source },
        )
        .map_err(storage_error)?;
    Ok(())
}

fn move_cover(
    transaction: &Transaction<'_>,
    destination: &str,
    source: &str,
    report: &mut ConsolidationReport,
) -> AppResult<()> {
    let destination_cover: Option<String> = transaction
        .query_row(
            "SELECT file_name FROM game_covers WHERE game_id = :destination",
            named_params! { ":destination": destination },
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    let source_cover: Option<String> = transaction
        .query_row(
            "SELECT file_name FROM game_covers WHERE game_id = :source",
            named_params! { ":source": source },
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;

    if let (Some(destination_cover), Some(source_cover)) = (&destination_cover, &source_cover)
        && destination_cover != source_cover
    {
        report.discarded_cover_file_names.push(source_cover.clone());
    }

    move_singleton_destination_wins(
        transaction,
        "game_covers",
        destination,
        source,
        r#"
            INSERT INTO game_covers (game_id, file_name, updated_at)
            SELECT :destination, file_name, updated_at
              FROM game_covers WHERE game_id = :source
            ON CONFLICT(game_id) DO NOTHING
        "#,
    )
}

fn move_nvapi_baselines(
    transaction: &Transaction<'_>,
    destination: &str,
    source: &str,
) -> AppResult<()> {
    transaction
        .execute(
            r#"
                INSERT INTO nvapi_setting_baselines (
                    game_id, setting_key, baseline_dword,
                    baseline_was_predefined, predefined_dword,
                    captured_exe, captured_at
                )
                SELECT :destination, setting_key, baseline_dword,
                       baseline_was_predefined, predefined_dword,
                       captured_exe, captured_at
                  FROM nvapi_setting_baselines WHERE game_id = :source
                ON CONFLICT(game_id, setting_key) DO NOTHING
            "#,
            named_params! { ":destination": destination, ":source": source },
        )
        .map_err(storage_error)?;
    transaction
        .execute(
            "DELETE FROM nvapi_setting_baselines WHERE game_id = :source",
            named_params! { ":source": source },
        )
        .map_err(storage_error)?;
    Ok(())
}

fn merge_ui_state(transaction: &Transaction<'_>, destination: &str, source: &str) -> AppResult<()> {
    transaction
        .execute(
            r#"
                INSERT INTO game_ui_state (game_id, is_favorite, is_hidden, updated_at)
                SELECT :destination, is_favorite, is_hidden, updated_at
                  FROM game_ui_state WHERE game_id = :source
                ON CONFLICT(game_id) DO UPDATE SET
                    is_favorite = max(game_ui_state.is_favorite, excluded.is_favorite),
                    is_hidden = max(game_ui_state.is_hidden, excluded.is_hidden),
                    updated_at = max(game_ui_state.updated_at, excluded.updated_at)
            "#,
            named_params! { ":destination": destination, ":source": source },
        )
        .map_err(storage_error)?;
    transaction
        .execute(
            "DELETE FROM game_ui_state WHERE game_id = :source",
            named_params! { ":source": source },
        )
        .map_err(storage_error)?;
    Ok(())
}

fn move_profile_capabilities(
    transaction: &Transaction<'_>,
    destination: &str,
    source: &str,
) -> AppResult<()> {
    transaction
        .execute(
            r#"
                INSERT INTO profile_addon_capabilities (
                    game_id, addon_kind, source_revision, updated_at
                )
                SELECT :destination, addon_kind, source_revision, updated_at
                  FROM profile_addon_capabilities WHERE game_id = :source
                ON CONFLICT(game_id, addon_kind) DO NOTHING
            "#,
            named_params! { ":destination": destination, ":source": source },
        )
        .map_err(storage_error)?;
    transaction
        .execute(
            "DELETE FROM profile_addon_capabilities WHERE game_id = :source",
            named_params! { ":source": source },
        )
        .map_err(storage_error)?;
    Ok(())
}
