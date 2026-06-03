//! SQLite column names: physical (`STRICT` tables) vs result-set aliases for row mappers.
//!
//! Bundled-schema contract: column spellings must match `migrations/0001_initial.sql`. There is no
//! incremental migration from older on-disk catalogs; see [`crate::schema`] (`apply`).

/// Physical column names as defined in SQLite migrations (`STRICT` tables).
///
/// Mirrors `migrations/0001_initial.sql`. `SELECT` fragments in [`crate::repositories::catalog_select_sql`]
/// duplicate these spellings because `concat!` only accepts string literals on this toolchain.
#[allow(dead_code)]
pub(super) mod physical {
    pub mod games {
        pub const ID: &str = "id";
        pub const TITLE: &str = "title";
        pub const LAUNCHER: &str = "launcher";
        pub const EXTERNAL_ID: &str = "external_id";
        pub const PLATFORM: &str = "platform";
        pub const RUNTIME: &str = "runtime";
        pub const INSTALL_PATH: &str = "install_path";
        pub const EXECUTABLE_CANDIDATES_JSON: &str = "executable_candidates_json";
    }

    pub mod components {
        pub const ID: &str = "id";
        pub const GAME_ID: &str = "game_id";
        pub const KIND: &str = "kind";
        pub const TECHNOLOGY: &str = "library";
        pub const SWAPPABILITY: &str = "swappability";
        pub const FILES_JSON: &str = "files_json";
    }

    pub mod library_artifacts {
        pub const ID: &str = "id";
        pub const TECHNOLOGY: &str = "library";
        pub const FILE_NAME: &str = "file_name";
        pub const FILES_JSON: &str = "files_json";
        pub const SOURCE: &str = "source";
        pub const SOURCE_GAME_ID: &str = "source_game_id";
        pub const TRUST_LEVEL: &str = "trust_level";
    }

    pub mod operations {
        pub const ID: &str = "id";
        pub const GAME_ID: &str = "game_id";
        pub const KIND: &str = "kind";
        pub const STATUS: &str = "status";
        pub const CREATED_AT: &str = "created_at";
        pub const COMPLETED_AT: &str = "completed_at";
        pub const METADATA_JSON: &str = "metadata_json";
    }

    pub mod operation_items {
        pub const OPERATION_ID: &str = "operation_id";
        pub const GAME_ID: &str = "game_id";
        pub const COMPONENT_ID: &str = "component_id";
        pub const ARTIFACT_ID: &str = "artifact_id";
        pub const SOURCE_PATH: &str = "source_path";
        pub const TARGET_PATH: &str = "target_path";
        pub const STATUS: &str = "status";
        pub const METADATA_JSON: &str = "metadata_json";
    }
}

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
