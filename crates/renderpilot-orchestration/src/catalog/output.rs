//! Serializable output DTOs shared between the CLI and GUI API layers.
//!
//! These types convert catalog result structs into JSON-friendly shapes that
//! are stable across releases. Both `renderpilot-cli` and the future
//! `renderpilot-api` crate use them so the wire format stays consistent.

use renderpilot_application::{ComponentReplacementCandidates, ReplacementCandidate};
use renderpilot_domain::ComponentVersionReport;
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
    /// Honest installed-version state for this component.
    pub version_report: ComponentVersionReportOutput,
    /// Available replacement candidates for this component.
    pub candidates: Vec<CandidateOutput>,
}

/// JSON-safe representation of the installed component version state.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ComponentVersionReportOutput {
    /// All relevant installed files resolve to one version.
    Known {
        /// Display version.
        version: String,
    },
    /// Known installed files prove the component is on multiple releases.
    Mixed {
        /// Lowest known version.
        min_version: String,
        /// Highest known version.
        max_version: String,
    },
    /// Metadata cannot establish a trustworthy installed version.
    Unknown,
}

impl From<&ComponentVersionReport> for ComponentVersionReportOutput {
    fn from(report: &ComponentVersionReport) -> Self {
        match report {
            ComponentVersionReport::Known(version) => Self::Known {
                version: version.as_str().to_owned(),
            },
            ComponentVersionReport::Mixed { min, max } => Self::Mixed {
                min_version: min.as_str().to_owned(),
                max_version: max.as_str().to_owned(),
            },
            ComponentVersionReport::Unknown => Self::Unknown,
        }
    }
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
    /// Curated catalog package id the artifact came from, if any.
    pub catalog_package_id: Option<String>,
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
            version_report: ComponentVersionReportOutput::from(group.version_report()),
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
            catalog_package_id: candidate.catalog_package_id().map(String::from),
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

#[cfg(test)]
mod tests {
    use super::ComponentVersionReportOutput;
    use renderpilot_domain::{ComponentVersionReport, Version};
    use serde_json::json;

    #[test]
    fn version_report_domain_variants_serialize_to_stable_wire_shapes() {
        // Domain → output → JSON in one path: both the From mapping and the
        // wire contract must stay locked together.
        let cases = [
            (
                ComponentVersionReport::Known(Version::parse("2.9.0").expect("version")),
                json!({ "kind": "known", "version": "2.9.0" }),
            ),
            (
                ComponentVersionReport::Mixed {
                    min: Version::parse("2.4.0").expect("version"),
                    max: Version::parse("2.9.0").expect("version"),
                },
                json!({
                    "kind": "mixed",
                    "min_version": "2.4.0",
                    "max_version": "2.9.0",
                }),
            ),
            (
                ComponentVersionReport::Unknown,
                json!({ "kind": "unknown" }),
            ),
        ];

        for (domain, expected_wire) in cases {
            let wire =
                serde_json::to_value(ComponentVersionReportOutput::from(&domain)).expect("json");
            assert_eq!(wire, expected_wire);
        }
    }
}
