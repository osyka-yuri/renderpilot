use renderpilot_application::{AppResult, ComponentRepository};
use renderpilot_domain::{GameId, GraphicsComponent};
use rusqlite::{named_params, Connection};

use crate::{
    error::{invalid_row, storage_error},
    mapping,
};

use super::{row_mapping::component_from_row, SqliteStorage};

const LIST_COMPONENTS_FOR_GAME_SQL: &str = "
    SELECT
        id,
        game_id,
        kind,
        technology,
        swappability,
        files_json
    FROM components
    WHERE game_id = :game_id
    ORDER BY id
";

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
            CAST(unixepoch('subsec') * 1000 AS INTEGER)
        )
";

impl ComponentRepository for SqliteStorage {
    fn replace_components_for_game(
        &self,
        game_id: &GameId,
        components: &[GraphicsComponent],
    ) -> AppResult<()> {
        self.with_transaction(|transaction| {
            replace_components_for_game_in_connection(transaction, game_id, components)
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

/// Replaces all components for `game_id` using an existing connection or outer transaction.
///
/// This function intentionally does not start its own transaction.
/// The caller owns transaction boundaries.
///
/// Safety:
/// - All component rows are validated before any delete happens.
/// - Every component must belong to `game_id`.
/// - Inserts use plain `INSERT`, not `UPSERT`, so duplicate component IDs or cross-game
///   inconsistencies fail instead of being silently rewritten.
pub(super) fn replace_components_for_game_in_connection(
    connection: &Connection,
    game_id: &GameId,
    components: &[GraphicsComponent],
) -> AppResult<()> {
    let rows = ComponentSqlRows::from_components(game_id, components)?;

    delete_components_for_game(connection, game_id)?;
    insert_component_rows(connection, &rows)
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

fn insert_component_rows(connection: &Connection, rows: &ComponentSqlRows<'_>) -> AppResult<()> {
    if rows.is_empty() {
        return Ok(());
    }

    let mut statement = connection
        .prepare_cached(INSERT_COMPONENT_SQL)
        .map_err(storage_error)?;

    for row in rows.iter() {
        execute_component_insert(&mut statement, row)?;
    }

    Ok(())
}

fn execute_component_insert(
    statement: &mut rusqlite::CachedStatement<'_>,
    row: &ComponentSqlRow<'_>,
) -> AppResult<()> {
    statement
        .execute(named_params! {
            ":id": row.id,
            ":game_id": row.game_id,
            ":kind": row.kind,
            ":technology": row.technology,
            ":swappability": row.swappability,
            ":files_json": row.files_json,
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

        for component in components {
            rows.push(ComponentSqlRow::from_component(
                expected_game_id,
                component,
            )?);
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
            kind: mapping::enum_to_text(component.kind())?,
            technology: mapping::enum_to_text(component.technology())?,
            swappability: mapping::enum_to_text(component.swappability())?,
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
