//! Result-set projection aliases for row mappers.
//!
//! Physical column spellings (migration contract / `PRAGMA table_info`) live
//! under [`crate::schema::physical`]. SQL contract tests import both modules
//! when they need to pair physical names with projection aliases.

/// Globally unique result-column names for every `SELECT` consumed by row mappers.
pub mod projection {
    pub mod game {
        pub const ID: &str = "game_id";
        pub const TITLE: &str = "game_title";
        pub const LAUNCHER: &str = "game_launcher";
        pub const EXTERNAL_ID: &str = "game_external_id";
        pub const PLATFORM: &str = "game_platform";
        pub const RUNTIME: &str = "game_runtime";
        pub const INSTALL_PATH: &str = "game_install_path";
        pub const EXECUTABLE_CANDIDATES_JSON: &str = "game_executable_candidates_json";
    }

    pub mod component {
        pub const ID: &str = "component_id";
        pub const GAME_ID: &str = "component_game_id";
        pub const KIND: &str = "component_kind";
        pub const TECHNOLOGY: &str = "component_technology";
        pub const SWAPPABILITY: &str = "component_swappability";
        pub const FILES_JSON: &str = "component_files_json";
    }

    pub mod artifact {
        pub const ID: &str = "artifact_id";
        pub const TECHNOLOGY: &str = "artifact_technology";
        pub const FILE_NAME: &str = "artifact_file_name";
        pub const FILES_JSON: &str = "artifact_files_json";
        pub const METADATA_JSON: &str = "artifact_metadata_json";
        pub const SOURCE: &str = "artifact_source";
        pub const SOURCE_GAME_ID: &str = "artifact_source_game_id";
        pub const TRUST_LEVEL: &str = "artifact_trust_level";
    }

    pub mod operation {
        pub const ID: &str = "operation_id";
        pub const GAME_ID: &str = "operation_game_id";
        pub const KIND: &str = "operation_kind";
        pub const STATUS: &str = "operation_status";
        pub const CREATED_AT: &str = "operation_created_at";
        pub const COMPLETED_AT: &str = "operation_completed_at";
        pub const METADATA_JSON: &str = "operation_metadata_json";
    }

    pub mod operation_item {
        pub const OPERATION_ID: &str = "item_operation_id";
        pub const COMPONENT_ID: &str = "item_component_id";
        pub const ARTIFACT_ID: &str = "item_artifact_id";
        pub const SOURCE_PATH: &str = "item_source_path";
        pub const TARGET_PATH: &str = "item_target_path";
        pub const STATUS: &str = "item_status";
        pub const METADATA_JSON: &str = "item_metadata_json";
    }
}
