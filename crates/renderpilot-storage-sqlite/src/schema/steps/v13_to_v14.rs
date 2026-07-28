use renderpilot_application::{AppError, AppResult};
use rusqlite::{Connection, named_params};

use crate::error::storage_context;

use super::super::version;

/// Adds canonical installation identity and explicit root authority.
pub(super) fn apply(connection: &Connection) -> AppResult<()> {
    let columns = super::super::pragma_column_names(connection, "games")?;
    if !columns.contains("install_key") {
        connection
            .execute_batch(
                "ALTER TABLE games ADD COLUMN install_key
                    TEXT NOT NULL DEFAULT '__renderpilot_pending_install_key__'
                    CHECK (length(trim(install_key)) > 0)
                    CHECK (instr(install_key, char(0)) = 0)
                    CHECK (instr(install_key, '\\') = 0)
                    CHECK (install_key = lower(install_key));",
            )
            .map_err(|error| {
                storage_context("could not add canonical game install identity", error)
            })?;
    }
    if !columns.contains("root_authority") {
        connection
            .execute_batch(
                "ALTER TABLE games ADD COLUMN root_authority TEXT NOT NULL DEFAULT 'legacy'
                 CHECK (root_authority IN ('launcher_manifest', 'user_confirmed', 'legacy'));",
            )
            .map_err(|error| storage_context("could not add game root authority", error))?;
    }
    if !columns.contains("confirmed_executable_path") {
        connection
            .execute_batch(
                "ALTER TABLE games ADD COLUMN confirmed_executable_path TEXT
                 CHECK (
                    confirmed_executable_path IS NULL
                    OR (
                        length(trim(confirmed_executable_path)) > 0
                        AND instr(confirmed_executable_path, char(0)) = 0
                        AND instr(confirmed_executable_path, '\\') = 0
                    )
                 );",
            )
            .map_err(|error| storage_context("could not add confirmed game executable", error))?;
    }

    connection
        .execute_batch(
            "
            WITH normalized_install_paths AS (
                SELECT id,
                       lower(
                           CASE
                               WHEN lower(substr(install_path, 1, 8)) = '//?/unc/'
                               THEN '//' || substr(install_path, 9)
                               WHEN lower(substr(install_path, 1, 4)) = '//?/'
                               THEN substr(install_path, 5)
                               ELSE install_path
                           END
                       ) AS normalized_path
                  FROM games
            )
            UPDATE games
               SET install_key = (
                   SELECT CASE
                              WHEN normalized_path = '/'
                                OR (
                                    length(normalized_path) = 3
                                    AND substr(normalized_path, 2, 2) = ':/'
                                )
                              THEN normalized_path
                              ELSE rtrim(normalized_path, '/')
                          END
                     FROM normalized_install_paths
                    WHERE normalized_install_paths.id = games.id
               )
             WHERE install_key IS NULL
                OR trim(install_key) = ''
                OR install_key = '__renderpilot_pending_install_key__';

            ",
        )
        .map_err(|error| storage_context("could not index canonical game installs", error))?;

    consolidate_exact_install_key_duplicates(connection)?;
    connection
        .execute_batch(
            "
            CREATE UNIQUE INDEX IF NOT EXISTS uq_games_install_key ON games(install_key);
            DROP TABLE IF EXISTS temp.component_rekeys_v14;
            ",
        )
        .map_err(|error| storage_context("could not index canonical game installs", error))?;

    version::write(connection, 14)
}

fn consolidate_exact_install_key_duplicates(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch(
            "
            CREATE TEMP TABLE IF NOT EXISTS component_rekeys_v14 (
                source_component_id TEXT PRIMARY KEY NOT NULL,
                destination_component_id TEXT NOT NULL
            ) STRICT;
            ",
        )
        .map_err(|error| storage_context("could not create v14 component rekey map", error))?;

    let mut statement = connection
        .prepare(
            "
            SELECT id, install_key
              FROM games
             ORDER BY install_key, created_at, id
            ",
        )
        .map_err(|error| storage_context("could not query duplicate install identities", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| storage_context("could not enumerate duplicate installs", error))?;
    let games = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| storage_context("could not read duplicate installs", error))?;
    drop(statement);

    let mut index = 0;
    while index < games.len() {
        let group_start = index;
        let key = games[index].1.as_str();
        while index < games.len() && games[index].1 == key {
            index += 1;
        }
        if index - group_start < 2 {
            continue;
        }

        let keeper = games[group_start].0.as_str();
        for (source, _) in &games[(group_start + 1)..index] {
            consolidate_exact_duplicate(connection, keeper, source)?;
        }
    }
    Ok(())
}

fn consolidate_exact_duplicate(
    connection: &Connection,
    destination: &str,
    source: &str,
) -> AppResult<()> {
    let params = named_params! { ":destination": destination, ":source": source };
    let source_params = named_params! { ":source": source };
    connection
        .execute("DELETE FROM temp.component_rekeys_v14", [])
        .map_err(|error| storage_context("could not reset v14 component rekey map", error))?;
    connection
        .execute(
            "
            INSERT INTO temp.component_rekeys_v14 (
                source_component_id, destination_component_id
            )
            SELECT source_component.id, min(destination_component.id)
              FROM components source_component
              JOIN components destination_component
                ON destination_component.game_id = :destination
               AND destination_component.kind = source_component.kind
               AND destination_component.library = source_component.library
               AND destination_component.files_json = source_component.files_json
             WHERE source_component.game_id = :source
             GROUP BY source_component.id
            ",
            params,
        )
        .map_err(|error| storage_context("could not map exact duplicate components", error))?;
    ensure_component_rekeys_are_one_to_one(connection, destination, source)?;
    ensure_duplicate_merge_is_lossless(connection, destination, source)?;

    connection
        .execute(
            "
            UPDATE components
               SET game_id = :destination
             WHERE game_id = :source
               AND id NOT IN (
                   SELECT source_component_id FROM temp.component_rekeys_v14
               )
            ",
            params,
        )
        .map_err(|error| storage_context("could not retain unique duplicate components", error))?;
    connection
        .execute(
            "UPDATE operations SET game_id = :destination WHERE game_id = :source",
            params,
        )
        .map_err(|error| storage_context("could not retain duplicate operations", error))?;
    connection
        .execute(
            "
            UPDATE operation_items
               SET game_id = :destination,
                   component_id = coalesce(
                       (
                           SELECT destination_component_id
                             FROM temp.component_rekeys_v14
                            WHERE source_component_id = operation_items.component_id
                       ),
                       component_id
                   )
             WHERE game_id = :source
            ",
            params,
        )
        .map_err(|error| storage_context("could not rekey duplicate operation items", error))?;

    connection
        .execute(
            "
            INSERT INTO component_backups (
                component_id, game_id, files_json, auxiliary_json, created_at, updated_at
            )
            SELECT rekeys.destination_component_id, :destination, backups.files_json,
                   backups.auxiliary_json, backups.created_at, backups.updated_at
              FROM component_backups backups
              JOIN temp.component_rekeys_v14 rekeys
                ON rekeys.source_component_id = backups.component_id
             WHERE backups.game_id = :source
            ON CONFLICT(component_id) DO NOTHING
            ",
            params,
        )
        .map_err(|error| storage_context("could not rekey duplicate component backups", error))?;
    connection
        .execute(
            "
            DELETE FROM component_backups
             WHERE game_id = :source
               AND component_id IN (
                   SELECT source_component_id FROM temp.component_rekeys_v14
               )
            ",
            source_params,
        )
        .map_err(|error| storage_context("could not remove rekeyed duplicate backups", error))?;
    connection
        .execute(
            "UPDATE component_backups
                SET game_id = :destination
              WHERE game_id = :source",
            params,
        )
        .map_err(|error| storage_context("could not retain unique duplicate backups", error))?;
    connection
        .execute(
            "
            DELETE FROM components
             WHERE game_id = :source
               AND id IN (
                   SELECT source_component_id FROM temp.component_rekeys_v14
               )
            ",
            source_params,
        )
        .map_err(|error| storage_context("could not remove exact duplicate components", error))?;

    connection
        .execute(
            "UPDATE library_artifacts
                SET source_game_id = :destination
              WHERE source_game_id = :source",
            params,
        )
        .map_err(|error| storage_context("could not retain duplicate library artifacts", error))?;
    move_singleton_destination_wins(
        connection,
        "installed_addons",
        destination,
        source,
        "
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
        ",
    )?;
    move_singleton_destination_wins(
        connection,
        "game_covers",
        destination,
        source,
        "
        INSERT INTO game_covers (game_id, file_name, updated_at)
        SELECT :destination, file_name, updated_at
          FROM game_covers WHERE game_id = :source
        ON CONFLICT(game_id) DO NOTHING
        ",
    )?;
    move_singleton_destination_wins(
        connection,
        "nvapi_executable_overrides",
        destination,
        source,
        "
        INSERT INTO nvapi_executable_overrides (
            game_id, selected_path, selected_basename, updated_at
        )
        SELECT :destination, selected_path, selected_basename, updated_at
          FROM nvapi_executable_overrides WHERE game_id = :source
        ON CONFLICT(game_id) DO NOTHING
        ",
    )?;

    connection
        .execute(
            "
            INSERT INTO nvapi_setting_baselines (
                game_id, setting_key, baseline_dword, baseline_was_predefined,
                predefined_dword, captured_exe, captured_at
            )
            SELECT :destination, setting_key, baseline_dword, baseline_was_predefined,
                   predefined_dword, captured_exe, captured_at
              FROM nvapi_setting_baselines WHERE game_id = :source
            ON CONFLICT(game_id, setting_key) DO NOTHING
            ",
            params,
        )
        .map_err(|error| storage_context("could not retain duplicate NVAPI baselines", error))?;
    connection
        .execute(
            "DELETE FROM nvapi_setting_baselines WHERE game_id = :source",
            source_params,
        )
        .map_err(|error| storage_context("could not remove duplicate NVAPI baselines", error))?;

    connection
        .execute(
            "
            INSERT INTO game_ui_state (game_id, is_favorite, is_hidden, updated_at)
            SELECT :destination, is_favorite, is_hidden, updated_at
              FROM game_ui_state WHERE game_id = :source
            ON CONFLICT(game_id) DO UPDATE SET
                is_favorite = max(game_ui_state.is_favorite, excluded.is_favorite),
                is_hidden = max(game_ui_state.is_hidden, excluded.is_hidden),
                updated_at = max(game_ui_state.updated_at, excluded.updated_at)
            ",
            params,
        )
        .map_err(|error| storage_context("could not merge duplicate UI state", error))?;
    connection
        .execute(
            "DELETE FROM game_ui_state WHERE game_id = :source",
            source_params,
        )
        .map_err(|error| storage_context("could not remove duplicate UI state", error))?;

    connection
        .execute(
            "
            INSERT INTO profile_addon_capabilities (
                game_id, addon_kind, source_revision, updated_at
            )
            SELECT :destination, addon_kind, source_revision, updated_at
              FROM profile_addon_capabilities WHERE game_id = :source
            ON CONFLICT(game_id, addon_kind) DO NOTHING
            ",
            params,
        )
        .map_err(|error| storage_context("could not retain duplicate capabilities", error))?;
    connection
        .execute(
            "DELETE FROM profile_addon_capabilities WHERE game_id = :source",
            source_params,
        )
        .map_err(|error| storage_context("could not remove duplicate capabilities", error))?;

    connection
        .execute(
            "UPDATE pending_file_mutations
                SET game_id = :destination
              WHERE game_id = :source",
            params,
        )
        .map_err(|error| storage_context("could not retain pending duplicate mutations", error))?;
    connection
        .execute("DELETE FROM games WHERE id = :source", source_params)
        .map_err(|error| storage_context("could not remove exact duplicate game", error))?;
    Ok(())
}

fn ensure_duplicate_merge_is_lossless(
    connection: &Connection,
    destination: &str,
    source: &str,
) -> AppResult<()> {
    let params = named_params! { ":destination": destination, ":source": source };
    if duplicate_singleton_rows_differ(
        connection,
        "games",
        "id",
        &[
            "title",
            "launcher",
            "external_id",
            "platform",
            "runtime",
            "root_authority",
            "confirmed_executable_path",
            "executable_candidates_json",
        ],
        destination,
        source,
    )? {
        return Err(lossy_duplicate_error(destination, source, "game identity"));
    }

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
        if duplicate_singleton_rows_differ(
            connection,
            table,
            "game_id",
            columns,
            destination,
            source,
        )? {
            return Err(lossy_duplicate_error(destination, source, table));
        }
    }

    for (table, key, columns) in [
        (
            "nvapi_setting_baselines",
            "setting_key",
            &[
                "baseline_dword",
                "baseline_was_predefined",
                "predefined_dword",
                "captured_exe",
            ][..],
        ),
        (
            "profile_addon_capabilities",
            "addon_kind",
            &["source_revision"][..],
        ),
    ] {
        if duplicate_keyed_rows_differ(connection, table, key, columns, destination, source)? {
            return Err(lossy_duplicate_error(destination, source, table));
        }
    }

    let backup_conflict: bool = connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1
                  FROM component_backups source_backup
                  JOIN temp.component_rekeys_v14 rekeys
                    ON rekeys.source_component_id = source_backup.component_id
                   JOIN component_backups destination_backup
                     ON destination_backup.component_id = rekeys.destination_component_id
                  WHERE source_backup.game_id = :source
                    AND destination_backup.game_id = :destination
                    AND NOT (
                        source_backup.files_json IS destination_backup.files_json
                        AND source_backup.auxiliary_json IS destination_backup.auxiliary_json
                    )
            )",
            params,
            |row| row.get(0),
        )
        .map_err(|error| {
            storage_context("could not inspect v14 duplicate component backups", error)
        })?;
    if backup_conflict {
        return Err(lossy_duplicate_error(
            destination,
            source,
            "component_backups",
        ));
    }

    let pending_mutations: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pending_file_mutations
              WHERE game_id IN (:destination, :source)",
            params,
            |row| row.get(0),
        )
        .map_err(|error| storage_context("could not inspect v14 pending file mutations", error))?;
    if pending_mutations != 0 {
        return Err(AppError::storage_failed(format!(
            "cannot consolidate duplicate installs {destination} and {source} during schema v14 migration: pending file mutations must be recovered before identity consolidation"
        )));
    }

    Ok(())
}

fn ensure_component_rekeys_are_one_to_one(
    connection: &Connection,
    destination: &str,
    source: &str,
) -> AppResult<()> {
    let params = named_params! { ":destination": destination, ":source": source };
    let ambiguous_match: bool = connection
        .query_row(
            "SELECT EXISTS(
                SELECT source_component.id
                  FROM components source_component
                  JOIN components destination_component
                    ON destination_component.game_id = :destination
                   AND destination_component.kind = source_component.kind
                   AND destination_component.library = source_component.library
                   AND destination_component.files_json = source_component.files_json
                 WHERE source_component.game_id = :source
                 GROUP BY source_component.id
                HAVING COUNT(*) <> 1
            )",
            params,
            |row| row.get(0),
        )
        .map_err(|error| storage_context("could not prove unique v14 component matches", error))?;
    let destination_reused: bool = connection
        .query_row(
            "SELECT EXISTS(
                SELECT destination_component_id
                  FROM temp.component_rekeys_v14
                 GROUP BY destination_component_id
                HAVING COUNT(*) > 1
            )",
            [],
            |row| row.get(0),
        )
        .map_err(|error| {
            storage_context("could not prove one-to-one v14 component mapping", error)
        })?;
    if ambiguous_match || destination_reused {
        return Err(lossy_duplicate_error(
            destination,
            source,
            "component identity",
        ));
    }
    Ok(())
}

fn duplicate_singleton_rows_differ(
    connection: &Connection,
    table: &str,
    identity_column: &str,
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
                        ON destination.{identity_column} = :destination
                       AND source.{identity_column} = :source
                     WHERE NOT ({equality})
                )"
            ),
            named_params! { ":destination": destination, ":source": source },
            |row| row.get(0),
        )
        .map_err(|error| {
            storage_context(
                &format!("could not compare v14 duplicate {table} state"),
                error,
            )
        })
}

fn duplicate_keyed_rows_differ(
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
                    SELECT 1
                      FROM {table} source
                      JOIN {table} destination
                        ON destination.game_id = :destination
                       AND source.game_id = :source
                       AND destination.{key} = source.{key}
                     WHERE NOT ({equality})
                )"
            ),
            named_params! { ":destination": destination, ":source": source },
            |row| row.get(0),
        )
        .map_err(|error| {
            storage_context(
                &format!("could not compare v14 duplicate {table} keyed state"),
                error,
            )
        })
}

fn lossy_duplicate_error(destination: &str, source: &str, table: &str) -> AppError {
    AppError::storage_failed(format!(
        "cannot consolidate duplicate installs {destination} and {source} during schema v14 migration without discarding conflicting {table} state"
    ))
}

fn move_singleton_destination_wins(
    connection: &Connection,
    table: &str,
    destination: &str,
    source: &str,
    insert_sql: &str,
) -> AppResult<()> {
    connection
        .execute(
            insert_sql,
            named_params! { ":destination": destination, ":source": source },
        )
        .map_err(|error| storage_context(&format!("could not merge duplicate {table}"), error))?;
    connection
        .execute(
            &format!("DELETE FROM {table} WHERE game_id = :source"),
            named_params! { ":source": source },
        )
        .map_err(|error| storage_context(&format!("could not remove duplicate {table}"), error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{InstallKey, PathRef};
    use rusqlite::Connection;

    #[test]
    fn migrated_install_key_is_non_null_and_matches_persisted_v1_contract() {
        let cases = [
            r"C:\Games\Example\\",
            r"\\?\C:\Games\Example",
            r"\\?\UNC\Server\Share\Example\\",
            r"\\Server\Share\Example",
            "D:/",
            "/",
        ];

        for input in cases {
            let path = PathRef::new(input).expect("path");
            let expected = InstallKey::from_path(&path);
            let connection = Connection::open_in_memory().expect("connection");
            connection
                .execute_batch(
                    "
                CREATE TABLE games (
                    id TEXT PRIMARY KEY NOT NULL,
                    install_path TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                ) STRICT;
                PRAGMA user_version = 13;
                ",
                )
                .expect("v13 fixture");
            connection
                .execute(
                    "INSERT INTO games (id, install_path, created_at)
                     VALUES ('manual:legacy', ?1, 1)",
                    [path.as_str()],
                )
                .expect("legacy row");

            let sentinel_checks: (i64, i64, i64, i64) = connection
                .query_row(
                    "
                SELECT length(trim('__renderpilot_pending_install_key__')) > 0,
                       instr('__renderpilot_pending_install_key__', char(0)) = 0,
                       instr('__renderpilot_pending_install_key__', '\\') = 0,
                       '__renderpilot_pending_install_key__'
                           = lower('__renderpilot_pending_install_key__')
                ",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .expect("sentinel validation");
            assert_eq!(sentinel_checks, (1, 1, 1, 1));

            super::apply(&connection).expect("v14 migration");

            let not_null: i64 = connection
                .query_row(
                    "SELECT \"notnull\" FROM pragma_table_info('games')
                  WHERE name = 'install_key'",
                    [],
                    |row| row.get(0),
                )
                .expect("install_key metadata");
            assert_eq!(not_null, 1);
            let row: (String, String, Option<String>) = connection
                .query_row(
                    "SELECT install_key, root_authority, confirmed_executable_path FROM games
                  WHERE id = 'manual:legacy'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .expect("migrated row");
            assert_eq!(
                row,
                (expected.as_str().to_owned(), "legacy".to_owned(), None),
                "{input}"
            );
        }
    }
}
