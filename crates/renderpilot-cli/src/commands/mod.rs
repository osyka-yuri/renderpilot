use std::path::PathBuf;

use renderpilot_application::AppInfo;
use renderpilot_domain::{ArtifactId, ComponentId, GameId, GraphicsTechnology, OperationId};

use crate::{
    args::Command,
    catalog,
    error::CliError,
    output::{
        render_apply_operation_output, render_backup_output, render_candidates_output, render_help,
        render_list_artifacts_output, render_list_operations_output, render_plan_swap_output,
        render_rollback_operation_output, render_scan_folder_output, render_summary,
        render_version,
    },
};

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tests;

pub(crate) fn render_command(command: Command, info: AppInfo) -> Result<String, CliError> {
    match command {
        Command::Summary => Ok(render_summary(info)),
        Command::Help => Ok(render_help(info)),
        Command::Version => Ok(render_version(info)),
        Command::ScanFolder { path } => scan_folder(path),
        Command::ListArtifacts { technology } => list_artifacts(technology),
        Command::ListOperations { game_id } => list_operations(game_id),
        Command::Candidates { game_id } => candidates(game_id),
        Command::PlanSwap {
            game_id,
            component_id,
            artifact_id,
        } => plan_swap(game_id, component_id, artifact_id),
        Command::Backup { operation_id } => backup(operation_id, info),
        Command::ApplyOperation { operation_id } => apply_operation(operation_id),
        Command::RollbackOperation { operation_id } => rollback_operation(operation_id),
    }
}

fn scan_folder(path: PathBuf) -> Result<String, CliError> {
    let result = catalog::scan_folder(path)?;

    render_scan_folder_output(result.game, result.libraries).map_err(Into::into)
}

fn list_artifacts(technology: Option<GraphicsTechnology>) -> Result<String, CliError> {
    let artifacts = catalog::list_artifacts(technology)?;

    render_list_artifacts_output(artifacts).map_err(Into::into)
}

fn list_operations(game_id: GameId) -> Result<String, CliError> {
    let result = catalog::list_operations(game_id)?;

    render_list_operations_output(&result).map_err(Into::into)
}

fn candidates(game_id: GameId) -> Result<String, CliError> {
    let result = catalog::find_candidates(game_id)?;

    render_candidates_output(&result.game_id, result.groups).map_err(Into::into)
}

fn plan_swap(
    game_id: GameId,
    component_id: ComponentId,
    artifact_id: ArtifactId,
) -> Result<String, CliError> {
    let result = catalog::build_swap_plan(game_id, component_id, artifact_id)?;

    render_plan_swap_output(&result.plan).map_err(Into::into)
}

fn backup(operation_id: OperationId, info: AppInfo) -> Result<String, CliError> {
    let result = catalog::create_backup(operation_id, info.version())?;

    render_backup_output(&result).map_err(Into::into)
}

fn apply_operation(operation_id: OperationId) -> Result<String, CliError> {
    let result = catalog::apply_operation(operation_id)?;

    render_apply_operation_output(&result).map_err(Into::into)
}

fn rollback_operation(operation_id: OperationId) -> Result<String, CliError> {
    let result = catalog::rollback_operation(operation_id)?;

    render_rollback_operation_output(&result).map_err(Into::into)
}
