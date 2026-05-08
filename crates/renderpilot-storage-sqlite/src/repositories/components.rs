use std::collections::HashSet;

use renderpilot_application::{AppResult, ComponentRepository};
use renderpilot_domain::{GameId, GraphicsComponent};
use rusqlite::{named_params, CachedStatement, Connection, OptionalExtension, Transaction};

use crate::{
    error::{invalid_row, storage_error},
    mapping, sqlite_clock,
};

use super::{
    catalog_select_sql::LIST_COMPONENTS_FOR_GAME_SQL, row_mapping::component_from_row,
    SqliteStorage,
};

const DELETE_COMPONENTS_FOR_GAME_SQL: &str = "
    DELETE FROM components
    WHERE game_id = :game_id
";

const INSERT_COMPONENT_SQL: &str = "
    INSERT INTO components
        (
            id,
            game_id,
            kind,
            technology,
            swappability,
            files_json,
            created_at,
            updated_at
        )
    VALUES
        (
            :id,
            :game_id,
            :kind,
            :technology,
            :swappability,
            :files_json,
            :created_at_ms,
            :updated_at_ms
        )
";

const FIND_COMPONENT_IN_OTHER_GAME_SQL: &str = "
    SELECT game_id
    FROM components
    WHERE id = :id
      AND game_id <> :game_id
    LIMIT 1
";

impl ComponentRepository for SqliteStorage {
    fn replace_components_for_game(
        &self,
        game_id: &GameId,
        components: &[GraphicsComponent],
    ) -> AppResult<()> {
        self.with_transaction(|transaction| {
            replace_components_for_game_within_transaction(transaction, game_id, components)
        })
    }

    fn list_components_for_game(&self, game_id: &GameId) -> AppResult<Vec<GraphicsComponent>> {
        self.query_list(
            LIST_COMPONENTS_FOR_GAME_SQL,
            named_params! {
                ":game_id": game_id.as_str(),
            },
            component_from_row,
        )
    }
}

/// Replaces all components for `game_id` within a transaction.
///
/// This function requires an active `Transaction` object, ensuring that the
/// delete-then-insert sequence is atomic. If any step fails, the caller's
/// transaction will be rolled back.
///
/// Guarantees before the delete step:
/// - every component belongs to `game_id`;
/// - component IDs are unique inside the replacement set;
/// - no replacement component ID is already owned by another game;
/// - component fields are mapped and serialized successfully;
/// - insert statement and timestamp are prepared successfully.
pub(super) fn replace_components_for_game_within_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
    components: &[GraphicsComponent],
) -> AppResult<()> {
    let rows = ComponentSqlRows::from_components(game_id, components)?;

    validate_no_component_ids_owned_by_other_games(transaction, game_id, &rows)?;

    let now_ms = prepare_timestamp_if_needed(transaction, &rows)?;
    let mut insert_statement = prepare_insert_statement_if_needed(transaction, &rows)?;

    delete_components_for_game(transaction, game_id)?;

    if let (Some(statement), Some(timestamp_ms)) = (&mut insert_statement, now_ms) {
        insert_component_rows(statement, &rows, timestamp_ms)?;
    }

    Ok(())
}

fn prepare_timestamp_if_needed(
    connection: &Connection,
    rows: &ComponentSqlRows<'_>,
) -> AppResult<Option<i64>> {
    if rows.is_empty() {
        return Ok(None);
    }

    sqlite_clock::now_ms(connection).map(Some)
}

fn prepare_insert_statement_if_needed<'connection>(
    connection: &'connection Connection,
    rows: &ComponentSqlRows<'_>,
) -> AppResult<Option<CachedStatement<'connection>>> {
    if rows.is_empty() {
        return Ok(None);
    }

    connection
        .prepare_cached(INSERT_COMPONENT_SQL)
        .map(Some)
        .map_err(storage_error)
}

fn validate_no_component_ids_owned_by_other_games(
    connection: &Connection,
    expected_game_id: &GameId,
    rows: &ComponentSqlRows<'_>,
) -> AppResult<()> {
    if rows.is_empty() {
        return Ok(());
    }

    let mut statement = connection
        .prepare_cached(FIND_COMPONENT_IN_OTHER_GAME_SQL)
        .map_err(storage_error)?;

    for row in rows.iter() {
        let conflicting_game_id: Option<String> = statement
            .query_row(
                named_params! {
                    ":id": row.id,
                    ":game_id": expected_game_id.as_str(),
                },
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;

        if let Some(conflicting_game_id) = conflicting_game_id {
            return Err(invalid_row(format!(
                "component {} already belongs to game {}, but replacement target is {}",
                row.id,
                conflicting_game_id,
                expected_game_id.as_str(),
            )));
        }
    }

    Ok(())
}

fn delete_components_for_game(connection: &Connection, game_id: &GameId) -> AppResult<()> {
    connection
        .execute(
            DELETE_COMPONENTS_FOR_GAME_SQL,
            named_params! {
                ":game_id": game_id.as_str(),
            },
        )
        .map_err(storage_error)?;

    Ok(())
}

fn insert_component_rows(
    statement: &mut CachedStatement<'_>,
    rows: &ComponentSqlRows<'_>,
    timestamp_ms: i64,
) -> AppResult<()> {
    for row in rows.iter() {
        insert_component_row(statement, row, timestamp_ms)?;
    }

    Ok(())
}

fn insert_component_row(
    statement: &mut CachedStatement<'_>,
    row: &ComponentSqlRow<'_>,
    timestamp_ms: i64,
) -> AppResult<()> {
    statement
        .execute(named_params! {
            ":id": row.id,
            ":game_id": row.game_id,
            ":kind": row.kind,
            ":technology": row.technology,
            ":swappability": row.swappability,
            ":files_json": row.files_json,
            ":created_at_ms": timestamp_ms,
            ":updated_at_ms": timestamp_ms,
        })
        .map_err(storage_error)?;

    Ok(())
}

#[derive(Debug)]
struct ComponentSqlRows<'a> {
    rows: Vec<ComponentSqlRow<'a>>,
}

impl<'a> ComponentSqlRows<'a> {
    fn from_components(
        expected_game_id: &'a GameId,
        components: &'a [GraphicsComponent],
    ) -> AppResult<Self> {
        let mut rows = Vec::with_capacity(components.len());
        let mut seen_ids = HashSet::with_capacity(components.len());

        for component in components {
            let row = ComponentSqlRow::from_component(expected_game_id, component)?;

            if !seen_ids.insert(row.id) {
                return Err(invalid_row(format!(
                    "duplicate component id {} in replacement set for game {}",
                    row.id,
                    expected_game_id.as_str(),
                )));
            }

            rows.push(row);
        }

        Ok(Self { rows })
    }

    fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    fn iter(&self) -> impl Iterator<Item = &ComponentSqlRow<'a>> {
        self.rows.iter()
    }
}

#[derive(Debug)]
struct ComponentSqlRow<'a> {
    id: &'a str,
    game_id: &'a str,
    kind: String,
    technology: String,
    swappability: String,
    files_json: String,
}

impl<'a> ComponentSqlRow<'a> {
    fn from_component(
        expected_game_id: &'a GameId,
        component: &'a GraphicsComponent,
    ) -> AppResult<Self> {
        validate_component_belongs_to_game(expected_game_id, component)?;

        Ok(Self {
            id: component.id().as_str(),
            game_id: component.game_id().as_str(),
            kind: mapping::enum_to_text(&component.kind())?,
            technology: mapping::enum_to_text(&component.technology())?,
            swappability: mapping::enum_to_text(&component.swappability())?,
            files_json: mapping::serialize_json(component.files())?,
        })
    }
}

fn validate_component_belongs_to_game(
    expected_game_id: &GameId,
    component: &GraphicsComponent,
) -> AppResult<()> {
    if component.game_id() == expected_game_id {
        return Ok(());
    }

    Err(invalid_row(format!(
        "component {} belongs to game {}, but replacement target is {}",
        component.id().as_str(),
        component.game_id().as_str(),
        expected_game_id.as_str(),
    )))
}
