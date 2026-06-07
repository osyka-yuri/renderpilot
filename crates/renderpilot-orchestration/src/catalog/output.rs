//! Serializable output DTOs shared between the CLI and GUI API layers.
//!
//! These types convert catalog result structs into JSON-friendly shapes that
//! are stable across releases. Both `renderpilot-cli` and the future
//! `renderpilot-api` crate use them so the wire format stays consistent.

use renderpilot_application::{ComponentReplacementCandidates, ReplacementCandidate};
use serde::Serialize;
use serde_json::Value;

use super::{OperationListCatalogEntry, OperationListCatalogResult};

// -----------------------------------------------------------------------------
// Candidate output types
// -----------------------------------------------------------------------------

/// Serializable shape for one component's replacement candidates.
#[derive(Debug, Serialize)]
pub struct ComponentCandidateOutput {
    /// Stable identifier of the component.
    pub component_id: String,
    /// Technology slug (`"dlss_super_resolution"`, etc.).
    pub technology: String,
    /// Installed file path of the component.
    pub file_path: String,
    /// Currently installed version, if detectable.
    pub current_version: Option<String>,
    /// Available replacement candidates for this component.
    pub candidates: Vec<CandidateOutput>,
}

/// Serializable shape for a single replacement candidate artifact.
#[derive(Debug, Serialize)]
pub struct CandidateOutput {
    /// Stable artifact id.
    pub artifact_id: String,
    /// Artifact file name.
    pub file_name: String,
    /// Absolute path to the locally cached artifact, if downloaded.
    pub file_path: Option<String>,
    /// Artifact version string, if available.
    pub version: Option<String>,
    /// Game id the artifact was extracted from, if any.
    pub source_game_id: Option<String>,
    /// Comparison result against the currently installed version.
    pub comparison: String,
    /// Library manifest entry id the artifact came from, if any.
    pub manifest_entry_id: Option<String>,
    /// Whether the artifact has been downloaded locally.
    pub is_downloaded: bool,
    /// Whether the artifact is a debug build.
    pub is_debug: bool,
    /// SHA-256 hex digest of the artifact.
    pub sha256: String,
}

impl From<ComponentReplacementCandidates> for ComponentCandidateOutput {
    fn from(group: ComponentReplacementCandidates) -> Self {
        let candidates = group
            .candidates()
            .iter()
            .map(CandidateOutput::from)
            .collect();
        Self {
            component_id: group.component_id().as_str().to_owned(),
            technology: group.technology().as_slug().to_owned(),
            file_path: group.file_path().as_str().to_owned(),
            current_version: group
                .current_version()
                .map(|version| version.as_str().to_owned()),
            candidates,
        }
    }
}

impl From<&ReplacementCandidate> for CandidateOutput {
    fn from(candidate: &ReplacementCandidate) -> Self {
        Self {
            artifact_id: candidate.artifact_id().as_str().to_owned(),
            file_name: candidate.file_name().to_owned(),
            file_path: candidate.file_path().map(|path| path.as_str().to_owned()),
            version: candidate
                .version()
                .map(|version| version.as_str().to_owned()),
            source_game_id: candidate
                .source_game_id()
                .map(|game_id| game_id.as_str().to_owned()),
            comparison: candidate.comparison().as_str().to_owned(),
            manifest_entry_id: candidate.manifest_entry_id().map(String::from),
            is_downloaded: candidate.is_downloaded(),
            is_debug: candidate.is_debug(),
            sha256: candidate.sha256().to_owned(),
        }
    }
}

/// Converts a slice of [`ComponentReplacementCandidates`] into serializable output DTOs.
pub fn component_candidate_outputs(
    groups: Vec<ComponentReplacementCandidates>,
) -> Vec<ComponentCandidateOutput> {
    groups
        .into_iter()
        .map(ComponentCandidateOutput::from)
        .collect()
}

// -----------------------------------------------------------------------------
// Operation summary output
// -----------------------------------------------------------------------------

/// Serializable summary of a single operation record.
#[derive(Debug, Serialize)]
pub struct OperationSummaryOutput {
    /// Stable operation id.
    pub operation_id: String,
    /// Operation kind string (`"swap"`, `"rollback"`, etc.).
    pub kind: String,
    /// Current status string (`"completed"`, `"running"`, etc.).
    pub status: String,
    /// Unix timestamp (milliseconds) when the operation was created.
    pub created_at: i64,
    /// Unix timestamp (milliseconds) when the operation completed, if finished.
    pub completed_at: Option<i64>,
    /// Number of files affected by the operation.
    pub item_count: usize,
    /// Id of the primary component affected.
    pub component_id: String,
    /// Parsed metadata JSON blob, if present.
    pub metadata: Option<Value>,
}

impl From<&OperationListCatalogEntry> for OperationSummaryOutput {
    fn from(entry: &OperationListCatalogEntry) -> Self {
        let metadata = entry
            .operation
            .metadata_json
            .as_ref()
            .and_then(|m| serde_json::from_str(m.as_str()).ok());

        Self {
            operation_id: entry.operation.id.as_str().to_owned(),
            kind: entry.operation.kind.as_str().to_owned(),
            status: entry.operation.status.as_str().to_owned(),
            created_at: entry.operation.created_at.as_i64(),
            completed_at: entry
                .operation
                .completed_at
                .map(|timestamp| timestamp.as_i64()),
            item_count: entry.item_count,
            component_id: entry.component_ids.first().cloned().unwrap_or_default(),
            metadata,
        }
    }
}

/// Converts an [`OperationListCatalogResult`] into a flat list of serializable summaries.
pub fn operation_summary_outputs(
    result: &OperationListCatalogResult,
) -> Vec<OperationSummaryOutput> {
    result
        .operations
        .iter()
        .map(OperationSummaryOutput::from)
        .collect()
}
