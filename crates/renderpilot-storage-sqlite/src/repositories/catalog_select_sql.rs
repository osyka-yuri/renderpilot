//! `SELECT` list fragments and full catalog/journal read queries used by repositories.
//!
//! Projection alias strings must match [`crate::repositories::columns::projection`].
//! `macro_rules!` embeds literals into `concat!(...)` because `concat!` does not accept
//! `const` path fragments on this toolchain (verified with rustc 1.95); workspace MSRV is 1.85.

/// `SELECT` list body; must match [`crate::repositories::columns::projection::game`].
macro_rules! projection_game_sql {
    () => {
        "games.id AS game_id,\n        games.title AS game_title,\n        games.launcher AS game_launcher,\n        games.external_id AS game_external_id,\n        games.platform AS game_platform,\n        games.runtime AS game_runtime,\n        games.install_path AS game_install_path,\n        games.executable_candidates_json AS game_executable_candidates_json"
    };
}

/// `SELECT` list body; must match [`crate::repositories::columns::projection::component`].
macro_rules! projection_component_sql {
    () => {
        "components.id AS component_id,\n        components.game_id AS component_game_id,\n        components.kind AS component_kind,\n        components.library AS component_technology,\n        components.swappability AS component_swappability,\n        components.files_json AS component_files_json"
    };
}

/// `SELECT` list body; must match [`crate::repositories::columns::projection::artifact`].
macro_rules! projection_artifact_sql {
    () => {
        "library_artifacts.id AS artifact_id,\n        library_artifacts.library AS artifact_technology,\n        library_artifacts.file_name AS artifact_file_name,\n        library_artifacts.file_path AS artifact_file_path,\n        library_artifacts.version AS artifact_version,\n        library_artifacts.sha256 AS artifact_sha256,\n        library_artifacts.source AS artifact_source,\n        library_artifacts.source_game_id AS artifact_source_game_id,\n        library_artifacts.trust_level AS artifact_trust_level"
    };
}

/// `SELECT` list body; must match [`crate::repositories::columns::projection::operation`].
macro_rules! projection_operation_sql {
    () => {
        "operations.id AS operation_id,\n        operations.game_id AS operation_game_id,\n        operations.kind AS operation_kind,\n        operations.status AS operation_status,\n        operations.created_at AS operation_created_at,\n        operations.completed_at AS operation_completed_at,\n        operations.metadata_json AS operation_metadata_json"
    };
}

/// `SELECT` list body; must match [`crate::repositories::columns::projection::operation_item`].
macro_rules! projection_operation_item_sql {
    () => {
        "operation_items.operation_id AS item_operation_id,\n        operation_items.component_id AS item_component_id,\n        operation_items.artifact_id AS item_artifact_id,\n        operation_items.source_path AS item_source_path,\n        operation_items.target_path AS item_target_path,\n        operation_items.status AS item_status,\n        operation_items.metadata_json AS item_metadata_json"
    };
}

pub(super) const FIND_GAME_SQL: &str = concat!(
    "
    SELECT
    ",
    projection_game_sql!(),
    "
    FROM games
    WHERE games.id = :id
    "
);

pub(super) const LIST_GAMES_SQL: &str = concat!(
    "
    SELECT
    ",
    projection_game_sql!(),
    "
    FROM games
    ORDER BY games.title, games.id
    "
);

pub(super) const LIST_DISTINCT_GAME_LIBRARIES_SQL: &str = "
    SELECT DISTINCT components.library
    FROM components
    WHERE trim(components.library) <> ''
    ORDER BY components.library
";

pub(super) const LIST_DISTINCT_GAME_LAUNCHERS_SQL: &str = "
    SELECT DISTINCT games.launcher
    FROM games
    WHERE trim(games.launcher) <> ''
    ORDER BY games.launcher
";

pub(super) const LIST_COMPONENTS_FOR_GAME_SQL: &str = concat!(
    "
    SELECT
    ",
    projection_component_sql!(),
    "
    FROM components
    WHERE components.game_id = :game_id
    ORDER BY components.id
    "
);

pub(super) const LIST_ARTIFACTS_SQL: &str = concat!(
    "
    SELECT
    ",
    projection_artifact_sql!(),
    "
    FROM library_artifacts
    ORDER BY library_artifacts.library, library_artifacts.file_name, library_artifacts.file_path
    "
);

pub(super) const SELECT_OPERATION_SQL: &str = concat!(
    "
    SELECT
    ",
    projection_operation_sql!(),
    "
    FROM operations
    WHERE operations.id = ?1
    "
);

pub(super) const SELECT_OPERATIONS_FOR_GAME_SQL: &str = concat!(
    "
    SELECT
    ",
    projection_operation_sql!(),
    "
    FROM operations
    WHERE operations.game_id = ?1
    ORDER BY operations.created_at, operations.id
    "
);

pub(super) const SELECT_OPERATION_ITEMS_SQL: &str = concat!(
    "
    SELECT
    ",
    projection_operation_item_sql!(),
    "
    FROM operation_items
    WHERE operation_items.operation_id = ?1
    ORDER BY operation_items.id
    "
);

#[cfg(test)]
mod tests {
    use super::super::columns::{physical as phys, projection as proj};

    fn assert_fragment_has_as_aliases(fragment: &str, aliases: &[&'static str]) {
        for alias in aliases {
            let needle = format!(" AS {alias}");
            assert!(
                fragment.contains(&needle),
                "projection fragment must contain `{needle}` (alias `{alias}`)"
            );
        }
    }

    fn assert_fragment_uses_physical_columns(
        fragment: &str,
        pairs: &[(&'static str, &'static str)],
    ) {
        for (table, phys_col) in pairs {
            let needle = format!("{table}.{phys_col}");
            assert!(
                fragment.contains(&needle),
                "projection fragment must qualify physical column `{needle}`"
            );
        }
    }

    #[test]
    fn game_projection_sql_matches_projection_aliases() {
        let f = projection_game_sql!();
        assert_fragment_has_as_aliases(
            f,
            &[
                proj::game::ID,
                proj::game::TITLE,
                proj::game::LAUNCHER,
                proj::game::EXTERNAL_ID,
                proj::game::PLATFORM,
                proj::game::RUNTIME,
                proj::game::INSTALL_PATH,
                proj::game::EXECUTABLE_CANDIDATES_JSON,
            ],
        );
        assert_fragment_uses_physical_columns(
            f,
            &[
                ("games", phys::games::ID),
                ("games", phys::games::TITLE),
                ("games", phys::games::LAUNCHER),
                ("games", phys::games::EXTERNAL_ID),
                ("games", phys::games::PLATFORM),
                ("games", phys::games::RUNTIME),
                ("games", phys::games::INSTALL_PATH),
                ("games", phys::games::EXECUTABLE_CANDIDATES_JSON),
            ],
        );
    }

    #[test]
    fn component_projection_sql_matches_projection_aliases() {
        let f = projection_component_sql!();
        assert_fragment_has_as_aliases(
            f,
            &[
                proj::component::ID,
                proj::component::GAME_ID,
                proj::component::KIND,
                proj::component::TECHNOLOGY,
                proj::component::SWAPPABILITY,
                proj::component::FILES_JSON,
            ],
        );
        assert_fragment_uses_physical_columns(
            f,
            &[
                ("components", phys::components::ID),
                ("components", phys::components::GAME_ID),
                ("components", phys::components::KIND),
                ("components", phys::components::TECHNOLOGY),
                ("components", phys::components::SWAPPABILITY),
                ("components", phys::components::FILES_JSON),
            ],
        );
    }

    #[test]
    fn artifact_projection_sql_matches_projection_aliases() {
        let f = projection_artifact_sql!();
        assert_fragment_has_as_aliases(
            f,
            &[
                proj::artifact::ID,
                proj::artifact::TECHNOLOGY,
                proj::artifact::FILE_NAME,
                proj::artifact::FILE_PATH,
                proj::artifact::VERSION,
                proj::artifact::SHA256,
                proj::artifact::SOURCE,
                proj::artifact::SOURCE_GAME_ID,
                proj::artifact::TRUST_LEVEL,
            ],
        );
        assert_fragment_uses_physical_columns(
            f,
            &[
                ("library_artifacts", phys::library_artifacts::ID),
                ("library_artifacts", phys::library_artifacts::TECHNOLOGY),
                ("library_artifacts", phys::library_artifacts::FILE_NAME),
                ("library_artifacts", phys::library_artifacts::FILE_PATH),
                ("library_artifacts", phys::library_artifacts::VERSION),
                ("library_artifacts", phys::library_artifacts::SHA256),
                ("library_artifacts", phys::library_artifacts::SOURCE),
                ("library_artifacts", phys::library_artifacts::SOURCE_GAME_ID),
                ("library_artifacts", phys::library_artifacts::TRUST_LEVEL),
            ],
        );
    }

    #[test]
    fn operation_projection_sql_matches_projection_aliases() {
        let f = projection_operation_sql!();
        assert_fragment_has_as_aliases(
            f,
            &[
                proj::operation::ID,
                proj::operation::GAME_ID,
                proj::operation::KIND,
                proj::operation::STATUS,
                proj::operation::CREATED_AT,
                proj::operation::COMPLETED_AT,
                proj::operation::METADATA_JSON,
            ],
        );
        assert_fragment_uses_physical_columns(
            f,
            &[
                ("operations", phys::operations::ID),
                ("operations", phys::operations::GAME_ID),
                ("operations", phys::operations::KIND),
                ("operations", phys::operations::STATUS),
                ("operations", phys::operations::CREATED_AT),
                ("operations", phys::operations::COMPLETED_AT),
                ("operations", phys::operations::METADATA_JSON),
            ],
        );
    }

    #[test]
    fn operation_item_projection_sql_matches_projection_aliases() {
        let f = projection_operation_item_sql!();
        assert_fragment_has_as_aliases(
            f,
            &[
                proj::operation_item::OPERATION_ID,
                proj::operation_item::COMPONENT_ID,
                proj::operation_item::ARTIFACT_ID,
                proj::operation_item::SOURCE_PATH,
                proj::operation_item::TARGET_PATH,
                proj::operation_item::STATUS,
                proj::operation_item::METADATA_JSON,
            ],
        );
        assert_fragment_uses_physical_columns(
            f,
            &[
                ("operation_items", phys::operation_items::OPERATION_ID),
                ("operation_items", phys::operation_items::COMPONENT_ID),
                ("operation_items", phys::operation_items::ARTIFACT_ID),
                ("operation_items", phys::operation_items::SOURCE_PATH),
                ("operation_items", phys::operation_items::TARGET_PATH),
                ("operation_items", phys::operation_items::STATUS),
                ("operation_items", phys::operation_items::METADATA_JSON),
            ],
        );
    }
}
