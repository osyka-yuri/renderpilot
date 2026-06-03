use std::collections::BTreeMap;

use renderpilot_application::{
    ComponentReplacementCandidates, OperationPlan, OperationPlanFile, ReplacementCandidate,
};
use renderpilot_detection::DetectedLibraryFile;
use renderpilot_domain::{GameId, GameInstallation, LibraryArtifact};
use serde::Serialize;
use serde_json::Value;

use crate::catalog::{OperationListCatalogEntry, OperationListCatalogResult};

type JsonResult<T> = Result<T, serde_json::Error>;

// -----------------------------------------------------------------------------
// Public render functions
// -----------------------------------------------------------------------------

pub(crate) fn render_scan_folder_output(
    game: GameInstallation,
    components: Vec<DetectedLibraryFile>,
) -> JsonResult<String> {
    render_pretty_json(ScanFolderOutput::new(game, components))
}

pub(crate) fn render_scan_folder_batch_output(
    scans: Vec<(GameInstallation, Vec<DetectedLibraryFile>)>,
) -> JsonResult<String> {
    render_pretty_json(ScanFolderBatchOutput::from_scans(scans))
}

pub(crate) fn render_list_artifacts_output(artifacts: Vec<LibraryArtifact>) -> JsonResult<String> {
    render_pretty_json(ArtifactListOutput::from_artifacts(artifacts))
}

pub(crate) fn render_candidates_output(
    game_id: &GameId,
    groups: Vec<ComponentReplacementCandidates>,
) -> JsonResult<String> {
    render_pretty_json(CandidateListOutput::new(game_id, groups))
}

pub(crate) fn render_list_operations_output(
    result: &OperationListCatalogResult,
) -> JsonResult<String> {
    render_pretty_json(OperationListOutput::from(result))
}

pub(crate) fn render_plan_swap_output(plan: &OperationPlan) -> JsonResult<String> {
    render_pretty_json(SwapPlanOutput::from(plan))
}

// -----------------------------------------------------------------------------
// Public JSON value helpers
// -----------------------------------------------------------------------------

pub(crate) fn candidate_groups_value(
    groups: Vec<ComponentReplacementCandidates>,
) -> JsonResult<Value> {
    render_json_value(component_candidate_outputs(groups))
}

pub(crate) fn operation_summaries_value(result: &OperationListCatalogResult) -> JsonResult<Value> {
    render_json_value(operation_summary_outputs(result))
}

// -----------------------------------------------------------------------------
// JSON serialization helpers
// -----------------------------------------------------------------------------

fn render_json_value<T>(value: T) -> JsonResult<Value>
where
    T: Serialize,
{
    serde_json::to_value(value)
}

fn render_pretty_json<T>(value: T) -> JsonResult<String>
where
    T: Serialize,
{
    let mut json = serde_json::to_string_pretty(&value)?;
    json.push('\n');

    Ok(json)
}

// -----------------------------------------------------------------------------
// Scan folder output
// -----------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ScanFolderOutput {
    game: GameInstallation,
    components: Vec<DetectedLibraryFile>,
}

impl ScanFolderOutput {
    fn new(game: GameInstallation, components: Vec<DetectedLibraryFile>) -> Self {
        Self { game, components }
    }
}

#[derive(Debug, Serialize)]
struct ScanFolderBatchOutput {
    games: Vec<ScanFolderOutput>,
}

impl ScanFolderBatchOutput {
    fn from_scans(scans: Vec<(GameInstallation, Vec<DetectedLibraryFile>)>) -> Self {
        let games = scans
            .into_iter()
            .map(|(game, components)| ScanFolderOutput::new(game, components))
            .collect();

        Self { games }
    }
}

// -----------------------------------------------------------------------------
// Artifact list output
// -----------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ArtifactListOutput {
    groups: Vec<ArtifactTechnologyGroupOutput>,
}

impl ArtifactListOutput {
    fn from_artifacts(artifacts: Vec<LibraryArtifact>) -> Self {
        Self {
            groups: artifact_groups(artifacts),
        }
    }
}

#[derive(Debug, Serialize)]
struct ArtifactTechnologyGroupOutput {
    technology: String,
    artifacts: Vec<ArtifactOutput>,
}

impl ArtifactTechnologyGroupOutput {
    fn new(technology: String, mut artifacts: Vec<ArtifactOutput>) -> Self {
        sort_artifacts_for_output(&mut artifacts);

        Self {
            technology,
            artifacts,
        }
    }
}

#[derive(Debug, Serialize)]
struct ArtifactOutput {
    file_name: String,
    file_path: String,
    version: Option<String>,
    sha256: String,
    source: Option<String>,
    source_game_id: Option<String>,
    trust_level: String,
}

impl From<LibraryArtifact> for ArtifactOutput {
    fn from(artifact: LibraryArtifact) -> Self {
        Self {
            file_name: artifact.file_name().to_owned(),
            file_path: artifact.path().as_str().to_owned(),
            version: artifact
                .version()
                .map(|version| version.as_str().to_owned()),
            sha256: artifact.sha256().as_str().to_owned(),
            source: artifact.source().map(str::to_owned),
            source_game_id: artifact
                .source_game_id()
                .map(|game_id| game_id.as_str().to_owned()),
            trust_level: artifact.trust_level().as_str().to_owned(),
        }
    }
}

fn artifact_groups(artifacts: Vec<LibraryArtifact>) -> Vec<ArtifactTechnologyGroupOutput> {
    group_artifacts_by_technology(artifacts)
        .into_iter()
        .map(|(technology, artifacts)| ArtifactTechnologyGroupOutput::new(technology, artifacts))
        .collect()
}

fn group_artifacts_by_technology(
    artifacts: Vec<LibraryArtifact>,
) -> BTreeMap<String, Vec<ArtifactOutput>> {
    let mut groups = BTreeMap::<String, Vec<ArtifactOutput>>::new();

    for artifact in artifacts {
        let technology = artifact.technology().as_slug().to_owned();

        groups
            .entry(technology)
            .or_default()
            .push(ArtifactOutput::from(artifact));
    }

    groups
}

fn sort_artifacts_for_output(artifacts: &mut [ArtifactOutput]) {
    artifacts.sort_by(|left, right| {
        left.file_name
            .cmp(&right.file_name)
            .then_with(|| left.file_path.cmp(&right.file_path))
    });
}

// -----------------------------------------------------------------------------
// Candidates output
// -----------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct CandidateListOutput {
    game_id: String,
    groups: Vec<ComponentCandidateOutput>,
}

impl CandidateListOutput {
    fn new(game_id: &GameId, groups: Vec<ComponentReplacementCandidates>) -> Self {
        Self {
            game_id: game_id.as_str().to_owned(),
            groups: component_candidate_outputs(groups),
        }
    }
}

#[derive(Debug, Serialize)]
struct ComponentCandidateOutput {
    component_id: String,
    technology: String,
    file_path: String,
    current_version: Option<String>,
    candidates: Vec<CandidateOutput>,
}

impl From<ComponentReplacementCandidates> for ComponentCandidateOutput {
    fn from(group: ComponentReplacementCandidates) -> Self {
        Self {
            component_id: group.component_id().as_str().to_owned(),
            technology: group.technology().as_slug().to_owned(),
            file_path: group.file_path().as_str().to_owned(),
            current_version: group
                .current_version()
                .map(|version| version.as_str().to_owned()),
            candidates: candidate_outputs(&group),
        }
    }
}

#[derive(Debug, Serialize)]
struct CandidateOutput {
    artifact_id: String,
    file_name: String,
    file_path: Option<String>,
    version: Option<String>,
    source_game_id: Option<String>,
    comparison: String,
    manifest_entry_id: Option<String>,
    is_downloaded: bool,
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
        }
    }
}

fn component_candidate_outputs(
    groups: Vec<ComponentReplacementCandidates>,
) -> Vec<ComponentCandidateOutput> {
    groups
        .into_iter()
        .map(ComponentCandidateOutput::from)
        .collect()
}

fn candidate_outputs(group: &ComponentReplacementCandidates) -> Vec<CandidateOutput> {
    group
        .candidates()
        .iter()
        .map(CandidateOutput::from)
        .collect()
}

// -----------------------------------------------------------------------------
// Operation list output
// -----------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct OperationListOutput {
    game_id: String,
    operations: Vec<OperationSummaryOutput>,
}

impl From<&OperationListCatalogResult> for OperationListOutput {
    fn from(result: &OperationListCatalogResult) -> Self {
        Self {
            game_id: result.game_id.as_str().to_owned(),
            operations: operation_summary_outputs(result),
        }
    }
}

#[derive(Debug, Serialize)]
struct OperationSummaryOutput {
    operation_id: String,
    kind: String,
    status: String,
    created_at: i64,
    completed_at: Option<i64>,
    item_count: usize,
    component_id: String,
}

impl From<&OperationListCatalogEntry> for OperationSummaryOutput {
    fn from(entry: &OperationListCatalogEntry) -> Self {
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
        }
    }
}

fn operation_summary_outputs(result: &OperationListCatalogResult) -> Vec<OperationSummaryOutput> {
    result
        .operations
        .iter()
        .map(OperationSummaryOutput::from)
        .collect()
}

// -----------------------------------------------------------------------------
// Swap plan output
// -----------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct SwapPlanOutput {
    operation_id: String,
    game_id: String,
    operation_type: String,
    target_path: String,
    replacement_path: String,
    original_version: Option<String>,
    replacement_version: Option<String>,
    original_sha256: Option<String>,
    replacement_sha256: Option<String>,
    risk_level: String,
    requires_elevation: bool,
    artifact_id: String,
    blockers: Vec<String>,
    warnings: Vec<String>,
    files: Vec<SwapPlanFileOutput>,
}

/// One file in a bundle swap plan. `target_path`/`replacement_path` at the plan
/// level remain the primary file for backward compatibility; this enumerates the
/// whole bundle so a client can show "1 replaced, 2 added".
#[derive(Debug, Serialize)]
struct SwapPlanFileOutput {
    action: String,
    target_path: String,
    replacement_path: Option<String>,
    original_version: Option<String>,
    replacement_version: Option<String>,
    original_sha256: Option<String>,
    replacement_sha256: Option<String>,
}

impl From<&OperationPlanFile> for SwapPlanFileOutput {
    fn from(file: &OperationPlanFile) -> Self {
        Self {
            action: file.action().as_str().to_owned(),
            target_path: file.target_path().as_str().to_owned(),
            replacement_path: file.replacement_path().map(|path| path.as_str().to_owned()),
            original_version: file
                .original_version()
                .map(|version| version.as_str().to_owned()),
            replacement_version: file
                .replacement_version()
                .map(|version| version.as_str().to_owned()),
            original_sha256: file.original_sha256().map(|hash| hash.as_str().to_owned()),
            replacement_sha256: file
                .replacement_sha256()
                .map(|hash| hash.as_str().to_owned()),
        }
    }
}

impl From<&OperationPlan> for SwapPlanOutput {
    fn from(plan: &OperationPlan) -> Self {
        Self {
            operation_id: plan.operation_id().as_str().to_owned(),
            game_id: plan.game_id().as_str().to_owned(),
            operation_type: plan.operation_type().as_str().to_owned(),
            target_path: plan.target_path().as_str().to_owned(),
            replacement_path: plan.replacement_path().as_str().to_owned(),
            original_version: plan
                .original_version()
                .map(|version| version.as_str().to_owned()),
            replacement_version: plan
                .replacement_version()
                .map(|version| version.as_str().to_owned()),
            original_sha256: plan.original_sha256().map(|hash| hash.as_str().to_owned()),
            replacement_sha256: plan
                .replacement_sha256()
                .map(|hash| hash.as_str().to_owned()),
            risk_level: plan.risk_level().as_str().to_owned(),
            requires_elevation: plan.requires_elevation(),
            artifact_id: plan.artifact_id().as_str().to_owned(),
            blockers: plan
                .blockers()
                .iter()
                .map(|blocker| blocker.as_str().to_owned())
                .collect(),
            warnings: plan
                .warnings()
                .iter()
                .map(|warning| warning.as_str().to_owned())
                .collect(),
            files: plan.files().iter().map(SwapPlanFileOutput::from).collect(),
        }
    }
}

// -----------------------------------------------------------------------------
// Shared helpers
// -----------------------------------------------------------------------------
