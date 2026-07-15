//! Physical column names for catalog tables (`STRICT` migrations).
//!
//! Mirrors the composed baseline DDL. Schema validation compares these sets to
//! `PRAGMA table_info` after migration so column constants cannot drift from the
//! bundled fragments.

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

pub mod game_covers {
    pub const GAME_ID: &str = "game_id";
    pub const FILE_NAME: &str = "file_name";
    pub const UPDATED_AT: &str = "updated_at";

    pub const ALL: &[&str] = &[GAME_ID, FILE_NAME, UPDATED_AT];
}

pub mod components {
    pub const ID: &str = "id";
    pub const GAME_ID: &str = "game_id";
    pub const KIND: &str = "kind";
    pub const LIBRARY: &str = "library";
    pub const SWAPPABILITY: &str = "swappability";
    pub const FILES_JSON: &str = "files_json";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";

    pub const ALL: &[&str] = &[
        ID,
        GAME_ID,
        KIND,
        LIBRARY,
        SWAPPABILITY,
        FILES_JSON,
        CREATED_AT,
        UPDATED_AT,
    ];
}

pub mod library_artifacts {
    pub const ID: &str = "id";
    pub const LIBRARY: &str = "library";
    pub const FILE_NAME: &str = "file_name";
    pub const FILES_JSON: &str = "files_json";
    pub const SOURCE: &str = "source";
    pub const SOURCE_GAME_ID: &str = "source_game_id";
    pub const TRUST_LEVEL: &str = "trust_level";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";

    pub const ALL: &[&str] = &[
        ID,
        LIBRARY,
        FILE_NAME,
        FILES_JSON,
        SOURCE,
        SOURCE_GAME_ID,
        TRUST_LEVEL,
        CREATED_AT,
        UPDATED_AT,
    ];
}

pub mod component_backups {
    pub const COMPONENT_ID: &str = "component_id";
    pub const GAME_ID: &str = "game_id";
    pub const FILES_JSON: &str = "files_json";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";

    pub const ALL: &[&str] = &[COMPONENT_ID, GAME_ID, FILES_JSON, CREATED_AT, UPDATED_AT];
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

pub mod installed_addons {
    pub const GAME_ID: &str = "game_id";
    pub const KIND: &str = "kind";
    pub const ADDON_FILE: &str = "addon_file";
    pub const ADDON_VERSION: &str = "addon_version";
    pub const CREATED_FILES_JSON: &str = "created_files_json";
    pub const BACKED_UP_FILES_JSON: &str = "backed_up_files_json";
    pub const MANAGED_FILES_JSON: &str = "managed_files_json";
    pub const TRACKED_SOURCES_JSON: &str = "tracked_sources_json";
    pub const HOST_KIND: &str = "host_kind";
    pub const RESHADE_CHANNEL: &str = "reshade_channel";
    pub const REGISTERED_EXE_PATH: &str = "registered_exe_path";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";

    pub const ALL: &[&str] = &[
        GAME_ID,
        KIND,
        ADDON_FILE,
        ADDON_VERSION,
        CREATED_FILES_JSON,
        BACKED_UP_FILES_JSON,
        MANAGED_FILES_JSON,
        TRACKED_SOURCES_JSON,
        HOST_KIND,
        RESHADE_CHANNEL,
        REGISTERED_EXE_PATH,
        CREATED_AT,
        UPDATED_AT,
    ];
}

pub mod pending_file_mutations {
    pub const ID: &str = "id";
    pub const GAME_ID: &str = "game_id";
    pub const FEATURE: &str = "feature";
    pub const SUBJECT_ID: &str = "subject_id";
    pub const STATE: &str = "state";
    pub const MANIFEST_JSON: &str = "manifest_json";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";

    pub const ALL: &[&str] = &[
        ID,
        GAME_ID,
        FEATURE,
        SUBJECT_ID,
        STATE,
        MANIFEST_JSON,
        CREATED_AT,
        UPDATED_AT,
    ];
}

pub mod shared_artifacts {
    pub const KIND: &str = "kind";
    pub const INSTALL_DIR: &str = "install_dir";
    pub const MANIFEST_PATH: &str = "manifest_path";
    pub const DLL_PATH: &str = "dll_path";
    pub const SOURCE_URL: &str = "source_url";
    pub const SOURCE_ETAG: &str = "source_etag";
    pub const SOURCE_DIGEST: &str = "source_digest";
    pub const SOURCE_LAST_MODIFIED: &str = "source_last_modified";
    pub const CHANNEL: &str = "channel";
    pub const ORIGIN: &str = "origin";
    pub const CREATED_FILES_JSON: &str = "created_files_json";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";

    pub const ALL: &[&str] = &[
        KIND,
        INSTALL_DIR,
        MANIFEST_PATH,
        DLL_PATH,
        SOURCE_URL,
        SOURCE_ETAG,
        SOURCE_DIGEST,
        SOURCE_LAST_MODIFIED,
        CHANNEL,
        ORIGIN,
        CREATED_FILES_JSON,
        CREATED_AT,
        UPDATED_AT,
    ];
}

pub mod settings {
    pub const KEY: &str = "key";
    pub const VALUE: &str = "value";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";

    pub const ALL: &[&str] = &[KEY, VALUE, CREATED_AT, UPDATED_AT];
}

pub mod file_hash_cache {
    pub const PATH: &str = "path";
    pub const SIZE: &str = "size";
    pub const MODIFIED_AT: &str = "modified_at";
    pub const SHA256: &str = "sha256";
    pub const VERSION: &str = "version";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";

    pub const ALL: &[&str] = &[
        PATH,
        SIZE,
        MODIFIED_AT,
        SHA256,
        VERSION,
        CREATED_AT,
        UPDATED_AT,
    ];
}

pub mod nvapi_executable_overrides {
    pub const GAME_ID: &str = "game_id";
    pub const SELECTED_PATH: &str = "selected_path";
    pub const SELECTED_BASENAME: &str = "selected_basename";
    pub const UPDATED_AT: &str = "updated_at";

    pub const ALL: &[&str] = &[GAME_ID, SELECTED_PATH, SELECTED_BASENAME, UPDATED_AT];
}

pub mod nvapi_setting_baselines {
    pub const GAME_ID: &str = "game_id";
    pub const SETTING_KEY: &str = "setting_key";
    pub const BASELINE_DWORD: &str = "baseline_dword";
    pub const BASELINE_WAS_PREDEFINED: &str = "baseline_was_predefined";
    pub const PREDEFINED_DWORD: &str = "predefined_dword";
    pub const CAPTURED_EXE: &str = "captured_exe";
    pub const CAPTURED_AT: &str = "captured_at";

    pub const ALL: &[&str] = &[
        GAME_ID,
        SETTING_KEY,
        BASELINE_DWORD,
        BASELINE_WAS_PREDEFINED,
        PREDEFINED_DWORD,
        CAPTURED_EXE,
        CAPTURED_AT,
    ];
}

pub mod game_ui_state {
    pub const GAME_ID: &str = "game_id";
    pub const IS_FAVORITE: &str = "is_favorite";
    pub const IS_HIDDEN: &str = "is_hidden";
    pub const UPDATED_AT: &str = "updated_at";

    pub const ALL: &[&str] = &[GAME_ID, IS_FAVORITE, IS_HIDDEN, UPDATED_AT];
}

/// Tables covered by the physical-column contract (exact set equality vs
/// `PRAGMA table_info` after migration).
///
/// Order must match `schema::contract::REQUIRED_TABLES`.
pub const CONTRACT_TABLES: &[(&str, &[&str])] = &[
    ("games", games::ALL),
    ("game_covers", game_covers::ALL),
    ("components", components::ALL),
    ("library_artifacts", library_artifacts::ALL),
    ("component_backups", component_backups::ALL),
    ("installed_addons", installed_addons::ALL),
    ("pending_file_mutations", pending_file_mutations::ALL),
    ("shared_artifacts", shared_artifacts::ALL),
    ("operations", operations::ALL),
    ("operation_items", operation_items::ALL),
    ("settings", settings::ALL),
    ("file_hash_cache", file_hash_cache::ALL),
    (
        "nvapi_executable_overrides",
        nvapi_executable_overrides::ALL,
    ),
    ("nvapi_setting_baselines", nvapi_setting_baselines::ALL),
    ("game_ui_state", game_ui_state::ALL),
];
