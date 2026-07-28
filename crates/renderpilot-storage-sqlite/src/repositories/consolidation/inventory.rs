//! Read-only collection of filesystem paths required by recovery bundles.

use std::{collections::BTreeMap, path::PathBuf};

use renderpilot_application::AppResult;
use renderpilot_domain::normalized_path_key;
use rusqlite::{Connection, named_params};

use super::{ConsolidationPlan, validation::validate_plan};
use crate::error::{storage_context, storage_error};

pub(in crate::repositories) fn recovery_file_paths(
    connection: &Connection,
    plan: &ConsolidationPlan,
) -> AppResult<Vec<PathBuf>> {
    validate_plan(plan)?;
    let mut values = Vec::new();
    let game_ids = std::iter::once(&plan.destination_game_id)
        .chain(plan.sources.iter().map(|source| &source.source_game_id));

    for game_id in game_ids {
        collect_text_rows(
            connection,
            "SELECT addon_file FROM installed_addons WHERE game_id = :game_id
             UNION ALL
             SELECT registered_exe_path FROM installed_addons
              WHERE game_id = :game_id AND registered_exe_path IS NOT NULL",
            game_id.as_str(),
            &mut values,
        )?;
        for sql in [
            "SELECT files_json FROM component_backups WHERE game_id = :game_id
             UNION ALL SELECT auxiliary_json FROM component_backups WHERE game_id = :game_id",
            "SELECT files_json FROM library_artifacts WHERE source_game_id = :game_id
             UNION ALL SELECT metadata_json FROM library_artifacts WHERE source_game_id = :game_id",
            "SELECT created_files_json FROM installed_addons WHERE game_id = :game_id
             UNION ALL SELECT backed_up_files_json FROM installed_addons WHERE game_id = :game_id
             UNION ALL SELECT managed_files_json FROM installed_addons WHERE game_id = :game_id
             UNION ALL SELECT tracked_sources_json FROM installed_addons WHERE game_id = :game_id",
            "SELECT manifest_json FROM pending_file_mutations WHERE game_id = :game_id",
            "SELECT metadata_json FROM operations
              WHERE game_id = :game_id AND metadata_json IS NOT NULL",
        ] {
            let mut documents = Vec::new();
            collect_text_rows(connection, sql, game_id.as_str(), &mut documents)?;
            for document in documents {
                let value: serde_json::Value =
                    serde_json::from_str(&document).map_err(|error| {
                        storage_context(
                            "could not parse consolidation recovery path metadata",
                            error,
                        )
                    })?;
                collect_json_strings(&value, &mut values);
            }
        }
    }

    let mut by_key = BTreeMap::new();
    for value in values {
        if !looks_like_absolute_path(&value) {
            continue;
        }
        let path = PathBuf::from(value);
        let key = normalized_path_key(&path.to_string_lossy());
        by_key.entry(key).or_insert(path);
    }
    Ok(by_key.into_values().collect())
}

fn collect_text_rows(
    connection: &Connection,
    sql: &str,
    game_id: &str,
    output: &mut Vec<String>,
) -> AppResult<()> {
    let mut statement = connection.prepare(sql).map_err(storage_error)?;
    let rows = statement
        .query_map(named_params! { ":game_id": game_id }, |row| row.get(0))
        .map_err(storage_error)?;
    output.extend(
        rows.collect::<Result<Vec<String>, _>>()
            .map_err(storage_error)?,
    );
    Ok(())
}

fn collect_json_strings(value: &serde_json::Value, output: &mut Vec<String>) {
    match value {
        serde_json::Value::String(value) => output.push(value.clone()),
        serde_json::Value::Array(values) => {
            for value in values {
                collect_json_strings(value, output);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                collect_json_strings(value, output);
            }
        }
        _ => {}
    }
}

fn looks_like_absolute_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    value.starts_with('/')
        || value.starts_with(r"\\")
        || (bytes.len() >= 3
            && bytes[1] == b':'
            && (bytes[2] == b'/' || bytes[2] == b'\\')
            && bytes[0].is_ascii_alphabetic())
}
