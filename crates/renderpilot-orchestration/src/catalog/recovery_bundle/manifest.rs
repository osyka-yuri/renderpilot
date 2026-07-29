//! Versioned recovery-bundle manifest schemas.

use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecoveryManifest {
    pub(super) format_version: u32,
    pub(super) created_at_unix_ms: u128,
    pub(super) destination_game_id: String,
    pub(super) source_game_ids: Vec<String>,
    pub(super) conflict_tables: Vec<String>,
    pub(super) copied_cover_files: Vec<String>,
    pub(super) missing_cover_files: Vec<String>,
    pub(super) copied_associated_files: Vec<AssociatedFileManifest>,
    pub(super) missing_associated_files: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AssociatedFileManifest {
    pub(super) original_path: String,
    pub(super) bundle_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RootCorrectionRecoveryManifest {
    pub(super) format_version: u32,
    pub(super) created_at_unix_ms: u128,
    pub(super) game_id: String,
    pub(super) previous_root: String,
    pub(super) corrected_root: String,
    pub(super) archived_component_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ManagedCleanupRecoveryManifest {
    pub(super) format_version: u32,
    pub(super) created_at_unix_ms: u128,
    pub(super) game_id: String,
    pub(super) ambiguous_targets: Vec<String>,
    pub(super) copied_associated_files: Vec<AssociatedFileManifest>,
    pub(super) missing_associated_files: Vec<String>,
}
