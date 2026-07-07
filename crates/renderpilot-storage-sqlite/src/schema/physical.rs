//! Physical column names for contracted catalog tables (`STRICT` migrations).
//!
//! Mirrors `migrations/0001_initial.sql`. Schema validation compares these
//! sets to `PRAGMA table_info` after migration so column constants cannot drift
//! from the bundled DDL. Other tables use separate mappers and are not listed
//! in [`CONTRACT_TABLES`].

pub mod games {
    pub const ID: &str = "id";
    pub const TITLE: &str = "title";
    pub const LAUNCHER: &str = "launcher";
    pub const EXTERNAL_ID: &str = "external_id";
    pub const PLATFORM: &str = "platform";
    pub const RUNTIME: &str = "runtime";
    pub const INSTALL_PATH: &str = "install_path";
    pub const EXECUTABLE_CANDIDATES_JSON: &str = "executable_candidates_json";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";

    /// All physical columns of `games`, in migration order.
    pub const ALL: &[&str] = &[
        ID,
        TITLE,
        LAUNCHER,
        EXTERNAL_ID,
        PLATFORM,
        RUNTIME,
        INSTALL_PATH,
        EXECUTABLE_CANDIDATES_JSON,
        CREATED_AT,
        UPDATED_AT,
    ];
}

pub mod components {
    pub const ID: &str = "id";
    pub const GAME_ID: &str = "game_id";
    pub const KIND: &str = "kind";
    pub const TECHNOLOGY: &str = "library";
    pub const SWAPPABILITY: &str = "swappability";
    pub const FILES_JSON: &str = "files_json";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";

    /// All physical columns of `components`, in migration order.
    pub const ALL: &[&str] = &[
        ID,
        GAME_ID,
        KIND,
        TECHNOLOGY,
        SWAPPABILITY,
        FILES_JSON,
        CREATED_AT,
        UPDATED_AT,
    ];
}

pub mod library_artifacts {
    pub const ID: &str = "id";
    pub const TECHNOLOGY: &str = "library";
    pub const FILE_NAME: &str = "file_name";
    pub const FILES_JSON: &str = "files_json";
    pub const SOURCE: &str = "source";
    pub const SOURCE_GAME_ID: &str = "source_game_id";
    pub const TRUST_LEVEL: &str = "trust_level";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";

    /// All physical columns of `library_artifacts`, in migration order.
    pub const ALL: &[&str] = &[
        ID,
        TECHNOLOGY,
        FILE_NAME,
        FILES_JSON,
        SOURCE,
        SOURCE_GAME_ID,
        TRUST_LEVEL,
        CREATED_AT,
        UPDATED_AT,
    ];
}

pub mod operations {
    pub const ID: &str = "id";
    pub const GAME_ID: &str = "game_id";
    pub const KIND: &str = "kind";
    pub const STATUS: &str = "status";
    pub const CREATED_AT: &str = "created_at";
    pub const COMPLETED_AT: &str = "completed_at";
    pub const UPDATED_AT: &str = "updated_at";
    pub const METADATA_JSON: &str = "metadata_json";

    /// All physical columns of `operations`, in migration order.
    pub const ALL: &[&str] = &[
        ID,
        GAME_ID,
        KIND,
        STATUS,
        CREATED_AT,
        COMPLETED_AT,
        UPDATED_AT,
        METADATA_JSON,
    ];
}

pub mod operation_items {
    pub const ID: &str = "id";
    pub const OPERATION_ID: &str = "operation_id";
    pub const GAME_ID: &str = "game_id";
    pub const COMPONENT_ID: &str = "component_id";
    pub const ARTIFACT_ID: &str = "artifact_id";
    pub const SOURCE_PATH: &str = "source_path";
    pub const TARGET_PATH: &str = "target_path";
    pub const STATUS: &str = "status";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";
    pub const METADATA_JSON: &str = "metadata_json";

    /// All physical columns of `operation_items`, in migration order.
    pub const ALL: &[&str] = &[
        ID,
        OPERATION_ID,
        GAME_ID,
        COMPONENT_ID,
        ARTIFACT_ID,
        SOURCE_PATH,
        TARGET_PATH,
        STATUS,
        CREATED_AT,
        UPDATED_AT,
        METADATA_JSON,
    ];
}

/// Tables covered by the physical-column contract (exact set equality vs
/// `PRAGMA table_info` after migration). Other tables use separate mappers.
pub const CONTRACT_TABLES: &[(&str, &[&str])] = &[
    ("games", games::ALL),
    ("components", components::ALL),
    ("library_artifacts", library_artifacts::ALL),
    ("operations", operations::ALL),
    ("operation_items", operation_items::ALL),
];
