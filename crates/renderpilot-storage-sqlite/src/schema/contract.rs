//! Full catalog schema contract: tables, indexes, triggers, and physical columns.
//!
//! Object names and column sets are the single registry consumed by validation.
//! DDL fragments under [`super::ddl`] must stay aligned with these lists.

use super::physical;

/// Every user table in the CURRENT catalog schema.
///
/// Names and order match [`physical::CONTRACT_TABLES`] (enforced by schema tests).
pub(super) const REQUIRED_TABLES: &[&str] = &[
    "games",
    "game_covers",
    "components",
    "library_artifacts",
    "component_backups",
    "installed_addons",
    "pending_file_mutations",
    "shared_artifacts",
    "operations",
    "operation_items",
    "settings",
    "file_hash_cache",
    "nvapi_executable_overrides",
    "nvapi_setting_baselines",
    "game_ui_state",
    "profile_addon_capabilities",
    "scan_source_checkpoints",
];

/// Every named index created by the baseline (excluding auto-indexes).
pub(super) const REQUIRED_INDEXES: &[&str] = &[
    "uq_games_launcher_external_id",
    "uq_games_install_key",
    "idx_games_launcher_install_path",
    "idx_games_updated_at",
    "idx_game_covers_updated_at",
    "idx_components_game_id",
    "idx_components_game_id_library",
    "idx_components_library",
    "idx_library_artifacts_library",
    "idx_library_artifacts_source_game_id",
    "idx_library_artifacts_updated_at",
    "idx_component_backups_game_id",
    "idx_pending_file_mutations_game_id",
    "idx_operations_game_id",
    "idx_operations_game_id_created_at",
    "idx_operations_status",
    "idx_operation_items_operation_id",
    "idx_operation_items_game_id",
    "idx_operation_items_component_id",
    "idx_operation_items_artifact_id",
    "idx_operation_items_status",
    "idx_settings_updated_at",
    "idx_file_hash_cache_updated_at",
    "idx_profile_addon_capabilities_kind",
];

/// Every named trigger created by the baseline.
pub(super) const REQUIRED_TRIGGERS: &[&str] = &[
    "trg_operation_items_artifact_library_insert",
    "trg_operation_items_artifact_library_update",
    "trg_games_touch_updated_at",
    "trg_game_covers_touch_updated_at",
    "trg_components_touch_updated_at",
    "trg_library_artifacts_touch_updated_at",
    "trg_operations_touch_updated_at",
    "trg_operation_items_touch_updated_at",
    "trg_settings_touch_updated_at",
    "trg_file_hash_cache_touch_updated_at",
    "trg_nvapi_executable_overrides_touch_updated_at",
    "trg_game_ui_state_touch_updated_at",
    "trg_installed_addons_touch_updated_at",
    "trg_shared_artifacts_touch_updated_at",
    "trg_profile_addon_capabilities_touch_updated_at",
    "trg_scan_source_checkpoints_touch_updated_at",
];

/// Exact physical-column contract for every catalog table.
pub(super) const CONTRACT_TABLES: &[(&str, &[&str])] = physical::CONTRACT_TABLES;

/// Required handling for every table whose rows are scoped to a game or
/// component. A contract test discovers scoped columns from
/// [`CONTRACT_TABLES`] and fails when a new table has no policy.
#[allow(
    dead_code,
    reason = "consumed by the schema contract test; kept beside production schema metadata"
)]
pub(super) const CONSOLIDATION_POLICIES: &[(&str, &str)] = &[
    ("game_covers", "destination_wins_then_file_gc"),
    ("components", "explicit_component_rekey"),
    ("library_artifacts", "reassign_source_game"),
    ("component_backups", "component_rekey_destination_wins"),
    ("installed_addons", "destination_wins"),
    ("pending_file_mutations", "recover_before_consolidation"),
    ("operations", "reassign_game"),
    ("operation_items", "reassign_game_and_component"),
    ("nvapi_executable_overrides", "destination_wins"),
    ("nvapi_setting_baselines", "destination_wins_per_setting"),
    ("game_ui_state", "merge_boolean_flags"),
    ("profile_addon_capabilities", "destination_wins_per_kind"),
];
