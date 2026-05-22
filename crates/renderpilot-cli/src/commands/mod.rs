use std::path::PathBuf;

use renderpilot_application::AppInfo;
use renderpilot_domain::{ArtifactId, ComponentId, GameId, GraphicsTechnology};

use crate::{
    args::Command,
    catalog,
    error::CliError,
    output::{
        render_candidates_output, render_help, render_list_artifacts_output,
        render_list_operations_output, render_plan_swap_output, render_scan_folder_batch_output,
        render_scan_folder_output, render_summary, render_version,
    },
};

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tests;

type CliOutput = Result<String, CliError>;

pub(crate) fn render_command(command: Command, info: AppInfo) -> CliOutput {
    match command {
        Command::Summary => render_summary_command(info),
        Command::Help => render_help_command(info),
        Command::Version => render_version_command(info),

        Command::ScanFolder { path } => scan_folder(path),
        Command::ListArtifacts { technology } => list_artifacts(technology),
        Command::ListOperations { game_id } => list_operations(game_id),
        Command::Candidates { game_id } => candidates(game_id),

        Command::PlanSwap {
            game_id,
            component_id,
            artifact_id,
        } => plan_swap(game_id, component_id, artifact_id),

        Command::ApplyOperation {
            game_id,
            component_id,
            artifact_id,
        } => apply_swap(game_id, component_id, artifact_id),
        Command::RollbackOperation {
            game_id,
            component_id,
        } => rollback_component(game_id, component_id),
    }
}

fn render_summary_command(info: AppInfo) -> CliOutput {
    Ok(render_summary(info))
}

fn render_help_command(info: AppInfo) -> CliOutput {
    Ok(render_help(info))
}

fn render_version_command(info: AppInfo) -> CliOutput {
    Ok(render_version(info))
}

fn scan_folder(path: PathBuf) -> CliOutput {
    let results = catalog::scan_folder(path)?;

    render_scan_folder_results(results)
}

fn render_scan_folder_results(results: Vec<catalog::ScanFolderCatalogResult>) -> CliOutput {
    debug_assert!(
        !results.is_empty(),
        "catalog::scan_folder should return at least one scan result"
    );

    match results.len() {
        1 => {
            let result = into_single_scan_result(results);
            render_single_scan_folder_result(result)
        }
        _ => render_scan_folder_batch_results(results),
    }
}

fn into_single_scan_result(
    mut results: Vec<catalog::ScanFolderCatalogResult>,
) -> catalog::ScanFolderCatalogResult {
    results
        .pop()
        .expect("single scan result should exist after len() == 1 check")
}

fn render_single_scan_folder_result(result: catalog::ScanFolderCatalogResult) -> CliOutput {
    render_output(render_scan_folder_output(result.game, result.libraries))
}

fn render_scan_folder_batch_results(results: Vec<catalog::ScanFolderCatalogResult>) -> CliOutput {
    let scans = results
        .into_iter()
        .map(|result| (result.game, result.libraries))
        .collect();

    render_output(render_scan_folder_batch_output(scans))
}

fn list_artifacts(technology: Option<GraphicsTechnology>) -> CliOutput {
    let artifacts = catalog::list_artifacts(technology)?;

    render_output(render_list_artifacts_output(artifacts))
}

fn list_operations(game_id: GameId) -> CliOutput {
    let result = catalog::list_operations(game_id)?;

    render_output(render_list_operations_output(&result))
}

fn candidates(game_id: GameId) -> CliOutput {
    let result = catalog::find_candidates(game_id)?;

    render_output(render_candidates_output(&result.game_id, result.groups))
}

fn plan_swap(game_id: GameId, component_id: ComponentId, artifact_id: ArtifactId) -> CliOutput {
    let plan = catalog::execute::build_swap_plan(game_id, component_id, artifact_id)?;

    render_output(render_plan_swap_output(&plan))
}

fn apply_swap(game_id: GameId, component_id: ComponentId, artifact_id: ArtifactId) -> CliOutput {
    let result = catalog::execute::apply_swap(game_id, component_id, artifact_id)?;

    render_output(serde_json::to_string_pretty(&result))
}

fn rollback_component(game_id: GameId, component_id: ComponentId) -> CliOutput {
    let result = catalog::execute::rollback_component(game_id, component_id)?;

    render_output(serde_json::to_string_pretty(&result))
}

fn render_output<E>(output: Result<String, E>) -> CliOutput
where
    E: Into<CliError>,
{
    output.map_err(Into::into)
}
