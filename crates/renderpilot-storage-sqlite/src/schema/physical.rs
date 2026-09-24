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
    pub const INSTALL_KEY: &str = "install_key";
    pub const ROOT_AUTHORITY: &str = "root_authority";
    pub const CONFIRMED_EXECUTABLE_PATH: &str = "confirmed_executable_path";
    pub const EXECUTABLE_CANDIDATES_JSON: &str = "executable_candidates_json";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";
    pub const PEER_AGGREGATE_REVISION: &str = "peer_aggregate_revision";

    pub const ALL: &[&str] = &[
        ID,
        TITLE,
        LAUNCHER,
        EXTERNAL_ID,
        PLATFORM,
        RUNTIME,
        INSTALL_PATH,
        INSTALL_KEY,
        ROOT_AUTHORITY,
        CONFIRMED_EXECUTABLE_PATH,
        EXECUTABLE_CANDIDATES_JSON,
        CREATED_AT,
        UPDATED_AT,
        PEER_AGGREGATE_REVISION,
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
    pub const TECHNOLOGY: &str = "technology";
    pub const SWAPPABILITY: &str = "swappability";
    pub const FILES_JSON: &str = "files_json";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";

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
    pub const TECHNOLOGY: &str = "technology";
    pub const FILE_NAME: &str = "file_name";
    pub const FILES_JSON: &str = "files_json";
    pub const METADATA_JSON: &str = "metadata_json";
    pub const SOURCE: &str = "source";
    pub const SOURCE_GAME_ID: &str = "source_game_id";
    pub const TRUST_LEVEL: &str = "trust_level";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";

    pub const ALL: &[&str] = &[
        ID,
        TECHNOLOGY,
        FILE_NAME,
        FILES_JSON,
        METADATA_JSON,
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
    pub const AUXILIARY_JSON: &str = "auxiliary_json";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";

    pub const ALL: &[&str] = &[
        COMPONENT_ID,
        GAME_ID,
        FILES_JSON,
        AUXILIARY_JSON,
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
    pub const RENODX_CONFIG_RECEIPT_JSON: &str = "renodx_config_receipt_json";
    pub const ENGINE_CONFIG_JOURNAL_JSON: &str = "engine_config_journal_json";
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
        RENODX_CONFIG_RECEIPT_JSON,
        ENGINE_CONFIG_JOURNAL_JSON,
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
    pub const AGGREGATE_KIND: &str = "aggregate_kind";
    pub const AGGREGATE_REVISION: &str = "aggregate_revision";

    pub const ALL: &[&str] = &[
        ID,
        GAME_ID,
        FEATURE,
        SUBJECT_ID,
        STATE,
        MANIFEST_JSON,
        CREATED_AT,
        UPDATED_AT,
        AGGREGATE_KIND,
        AGGREGATE_REVISION,
    ];
}

pub mod game_proxy_topologies {
    pub const GAME_ID: &str = "game_id";
    pub const ID: &str = "id";
    pub const TOPOLOGY_JSON: &str = "topology_json";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";
    pub const ALL: &[&str] = &[GAME_ID, ID, TOPOLOGY_JSON, CREATED_AT, UPDATED_AT];
}

pub mod optiscaler_install_states {
    pub const GAME_ID: &str = "game_id";
    pub const RELEASE_ID: &str = "release_id";
    pub const MANIFEST_REVISION: &str = "manifest_revision";
    pub const ARCHIVE_SHA256: &str = "archive_sha256";
    pub const SOURCE: &str = "source";
    pub const TARGET_EXE_PATH: &str = "target_exe_path";
    pub const TARGET_DIR: &str = "target_dir";
    pub const MODULES_JSON: &str = "modules_json";
    pub const RELEASE_FILES_JSON: &str = "release_files_json";
    pub const RUNTIME_BINDINGS_JSON: &str = "runtime_bindings_json";
    pub const DIRECTORY_RECEIPTS_JSON: &str = "directory_receipts_json";
    pub const PROXY_TOPOLOGY_ID: &str = "proxy_topology_id";
    pub const CONFIG_SCHEMA: &str = "config_schema";
    pub const CONFIG_BASE_RELEASE: &str = "config_base_release";
    pub const ADOPTION_STATE: &str = "adoption_state";
    pub const PREREQUISITE_BINDING: &str = "prerequisite_binding";
    pub const CONFIGURATION_BASELINE_JSON: &str = "configuration_baseline_json";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";
    pub const ALL: &[&str] = &[
        GAME_ID,
        RELEASE_ID,
        MANIFEST_REVISION,
        ARCHIVE_SHA256,
        SOURCE,
        TARGET_EXE_PATH,
        TARGET_DIR,
        MODULES_JSON,
        RELEASE_FILES_JSON,
        RUNTIME_BINDINGS_JSON,
        DIRECTORY_RECEIPTS_JSON,
        PROXY_TOPOLOGY_ID,
        CONFIG_SCHEMA,
        CONFIG_BASE_RELEASE,
        ADOPTION_STATE,
        PREREQUISITE_BINDING,
        CREATED_AT,
        UPDATED_AT,
        CONFIGURATION_BASELINE_JSON,
    ];
}

pub mod pending_shared_vulkan_mutations {
    pub const RESOURCE_KEY: &str = "resource_key";
    pub const ID: &str = "id";
    pub const SCOPE: &str = "scope";
    pub const GAME_ID: &str = "game_id";
    pub const FEATURE: &str = "feature";
    pub const STATE: &str = "state";
    pub const MANIFEST_JSON: &str = "manifest_json";
    pub const ROOT_CAPABILITIES_JSON: &str = "root_capabilities_json";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";
    pub const AGGREGATE_KIND: &str = "aggregate_kind";
    pub const AGGREGATE_REVISION: &str = "aggregate_revision";
    pub const AGGREGATE_PROGRAM: &str = "aggregate_program";

    pub const ALL: &[&str] = &[
        RESOURCE_KEY,
        ID,
        SCOPE,
        GAME_ID,
        FEATURE,
        STATE,
        MANIFEST_JSON,
        ROOT_CAPABILITIES_JSON,
        CREATED_AT,
        UPDATED_AT,
        AGGREGATE_KIND,
        AGGREGATE_REVISION,
        AGGREGATE_PROGRAM,
    ];
}

pub mod peer_aggregate_reservations {
    pub const GAME_ID: &str = "game_id";
    pub const OPERATION_ID: &str = "operation_id";
    pub const AGGREGATE_KIND: &str = "aggregate_kind";
    pub const PENDING_BINDING: &str = "pending_binding";
    pub const STATE: &str = "state";
    pub const EXPECTED_REVISION: &str = "expected_revision";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";

    pub const ALL: &[&str] = &[
        GAME_ID,
        OPERATION_ID,
        AGGREGATE_KIND,
        PENDING_BINDING,
        STATE,
        EXPECTED_REVISION,
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

pub mod catalog_scan_authority {
    pub const GAME_ID: &str = "game_id";
    pub const READINESS: &str = "readiness";
    pub const AUTHORITY_EPOCH: &str = "authority_epoch";
    pub const INVALIDATION_REASON: &str = "invalidation_reason";
    pub const MUTATION_TOKEN: &str = "mutation_token";
    pub const COMPLETED_AT: &str = "completed_at";
    pub const UPDATED_AT: &str = "updated_at";
    pub const ALL: &[&str] = &[
        GAME_ID,
        READINESS,
        AUTHORITY_EPOCH,
        INVALIDATION_REASON,
        MUTATION_TOKEN,
        COMPLETED_AT,
        UPDATED_AT,
    ];
}

pub mod file_observations {
    pub const OWNER_KIND: &str = "owner_kind";
    pub const OWNER_ID: &str = "owner_id";
    pub const GAME_ID: &str = "game_id";
    pub const ARTIFACT_ID: &str = "artifact_id";
    pub const NORMALIZED_PATH: &str = "normalized_path";
    pub const IDENTITY_KIND: &str = "identity_kind";
    pub const OBJECT_IDENTITY: &str = "object_identity";
    pub const CHANGE_TOKEN: &str = "change_token";
    pub const SIZE: &str = "size";
    pub const ALGORITHM_REVISION: &str = "algorithm_revision";
    pub const SHA256: &str = "sha256";
    pub const VERSION_OBSERVED: &str = "version_observed";
    pub const VERSION: &str = "version";
    pub const RUNTIME_OBSERVED: &str = "runtime_observed";
    pub const RUNTIME_JSON: &str = "runtime_json";
    pub const PE_OBSERVED: &str = "pe_observed";
    pub const PE_JSON: &str = "pe_json";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";
    pub const ALL: &[&str] = &[
        OWNER_KIND,
        OWNER_ID,
        GAME_ID,
        ARTIFACT_ID,
        NORMALIZED_PATH,
        IDENTITY_KIND,
        OBJECT_IDENTITY,
        CHANGE_TOKEN,
        SIZE,
        ALGORITHM_REVISION,
        SHA256,
        VERSION_OBSERVED,
        VERSION,
        RUNTIME_OBSERVED,
        RUNTIME_JSON,
        PE_OBSERVED,
        PE_JSON,
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

pub mod nvapi_owned_profiles {
    pub const GAME_ID: &str = "game_id";
    pub const PROFILE_NAME: &str = "profile_name";
    pub const BINDING_PATH: &str = "binding_path";
    pub const APPLICATION_WITNESS_JSON: &str = "application_witness_json";
    pub const COMPOSITION_JSON: &str = "composition_json";
    pub const STATE: &str = "state";
    pub const UPDATED_AT: &str = "updated_at";
    pub const ALL: &[&str] = &[
        GAME_ID,
        PROFILE_NAME,
        BINDING_PATH,
        APPLICATION_WITNESS_JSON,
        COMPOSITION_JSON,
        STATE,
        UPDATED_AT,
    ];
}

pub mod nvapi_drs_targets {
    pub const TARGET_ID: &str = "target_id";
    pub const PROFILE_NAME: &str = "profile_name";
    pub const PROFILE_IS_PREDEFINED: &str = "profile_is_predefined";
    pub const KIND: &str = "kind";
    pub const IDENTITY_JSON: &str = "identity_json";
    pub const ALL: &[&str] = &[
        TARGET_ID,
        PROFILE_NAME,
        PROFILE_IS_PREDEFINED,
        KIND,
        IDENTITY_JSON,
    ];
}

pub mod nvapi_drs_target_app_witnesses {
    pub const TARGET_ID: &str = "target_id";
    pub const EXECUTABLE_PATH: &str = "executable_path";
    pub const APPLICATION_JSON: &str = "application_json";
    pub const UPDATED_AT: &str = "updated_at";
    pub const ALL: &[&str] = &[TARGET_ID, EXECUTABLE_PATH, APPLICATION_JSON, UPDATED_AT];
}

pub mod nvapi_target_setting_claims {
    pub const TARGET_ID: &str = "target_id";
    pub const SETTING_ID: &str = "setting_id";
    pub const ORIGINAL_PRESENT: &str = "original_present";
    pub const ORIGINAL_VALUE: &str = "original_value";
    pub const EXPECTED_PRESENT: &str = "expected_present";
    pub const EXPECTED_VALUE: &str = "expected_value";
    pub const UPDATED_AT: &str = "updated_at";
    pub const ALL: &[&str] = &[
        TARGET_ID,
        SETTING_ID,
        ORIGINAL_PRESENT,
        ORIGINAL_VALUE,
        EXPECTED_PRESENT,
        EXPECTED_VALUE,
        UPDATED_AT,
    ];
}

pub mod nvapi_game_claim_refs {
    pub const GAME_ID: &str = "game_id";
    pub const TARGET_ID: &str = "target_id";
    pub const SETTING_ID: &str = "setting_id";
    pub const EXECUTABLE_PATH: &str = "executable_path";
    pub const ALL: &[&str] = &[GAME_ID, TARGET_ID, SETTING_ID, EXECUTABLE_PATH];
}

pub mod pending_drs_operations {
    pub const OP_ID: &str = "op_id";
    pub const GAME_ID: &str = "game_id";
    pub const TARGET_ID: &str = "target_id";
    pub const KIND: &str = "kind";
    pub const BEFORE_JSON: &str = "before_json";
    pub const AFTER_JSON: &str = "after_json";
    pub const OBSERVED_JSON: &str = "observed_json";
    pub const PHASE: &str = "phase";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";
    pub const ALL: &[&str] = &[
        OP_ID,
        GAME_ID,
        TARGET_ID,
        KIND,
        BEFORE_JSON,
        AFTER_JSON,
        OBSERVED_JSON,
        PHASE,
        CREATED_AT,
        UPDATED_AT,
    ];
}

pub mod game_ui_state {
    pub const GAME_ID: &str = "game_id";
    pub const IS_FAVORITE: &str = "is_favorite";
    pub const IS_HIDDEN: &str = "is_hidden";
    pub const UPDATED_AT: &str = "updated_at";

    pub const ALL: &[&str] = &[GAME_ID, IS_FAVORITE, IS_HIDDEN, UPDATED_AT];
}

pub mod profile_addon_capabilities {
    pub const GAME_ID: &str = "game_id";
    pub const ADDON_KIND: &str = "addon_kind";
    pub const SOURCE_REVISION: &str = "source_revision";
    pub const UPDATED_AT: &str = "updated_at";
    pub const ALL: &[&str] = &[GAME_ID, ADDON_KIND, SOURCE_REVISION, UPDATED_AT];
}

pub mod portable_path_tags {
    pub const TAG: &str = "tag";
    pub const KIND: &str = "kind";
    pub const VALUE: &str = "value";
    pub const ALL: &[&str] = &[TAG, KIND, VALUE];
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
    (
        "peer_aggregate_reservations",
        peer_aggregate_reservations::ALL,
    ),
    ("game_proxy_topologies", game_proxy_topologies::ALL),
    ("optiscaler_install_states", optiscaler_install_states::ALL),
    (
        "pending_shared_vulkan_mutations",
        pending_shared_vulkan_mutations::ALL,
    ),
    ("shared_artifacts", shared_artifacts::ALL),
    ("operations", operations::ALL),
    ("operation_items", operation_items::ALL),
    ("settings", settings::ALL),
    ("catalog_scan_authority", catalog_scan_authority::ALL),
    ("file_observations", file_observations::ALL),
    (
        "nvapi_executable_overrides",
        nvapi_executable_overrides::ALL,
    ),
    ("nvapi_owned_profiles", nvapi_owned_profiles::ALL),
    ("nvapi_drs_targets", nvapi_drs_targets::ALL),
    (
        "nvapi_drs_target_app_witnesses",
        nvapi_drs_target_app_witnesses::ALL,
    ),
    (
        "nvapi_target_setting_claims",
        nvapi_target_setting_claims::ALL,
    ),
    ("nvapi_game_claim_refs", nvapi_game_claim_refs::ALL),
    ("pending_drs_operations", pending_drs_operations::ALL),
    ("game_ui_state", game_ui_state::ALL),
    (
        "profile_addon_capabilities",
        profile_addon_capabilities::ALL,
    ),
    ("portable_path_tags", portable_path_tags::ALL),
];
