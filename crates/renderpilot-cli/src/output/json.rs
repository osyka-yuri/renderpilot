use std::collections::BTreeMap;

use renderpilot_orchestration::application::{ComponentReplacementCandidates, OperationPlan};
use renderpilot_orchestration::domain::{GameId, LibraryArtifact};
use serde::Serialize;

use crate::catalog::OperationListCatalogResult;
use renderpilot_orchestration::catalog::output::{
    ComponentCandidateOutput, RollbackPlanOutput, SwapPlanOutput, component_candidate_outputs,
    operation_summary_outputs,
};

type JsonResult<T> = Result<T, serde_json::Error>;

// -----------------------------------------------------------------------------
// Public render functions
// -----------------------------------------------------------------------------

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

pub(crate) fn render_plan_rollback_output(
    plan: &crate::catalog::RollbackPlan,
) -> JsonResult<String> {
    render_pretty_json(RollbackPlanOutput::from(plan))
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

// -----------------------------------------------------------------------------
// Operation list output
// -----------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct OperationListOutput {
    game_id: String,
    operations: Vec<renderpilot_orchestration::catalog::output::OperationSummaryOutput>,
}

impl From<&OperationListCatalogResult> for OperationListOutput {
    fn from(result: &OperationListCatalogResult) -> Self {
        Self {
            game_id: result.game_id.as_str().to_owned(),
            operations: operation_summary_outputs(result),
        }
    }
}

// -----------------------------------------------------------------------------
// Shared helpers
// -----------------------------------------------------------------------------
