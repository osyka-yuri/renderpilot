use renderpilot_orchestration::application::AppInfo;
use renderpilot_orchestration::domain::{ArtifactId, ComponentId, GameId, LibraryTechnology};
use renderpilot_orchestration::{Context, ServiceError};

use crate::{
    args::command::Command,
    catalog,
    error::CliError,
    luma,
    output::{
        render_candidates_output, render_help, render_list_artifacts_output,
        render_list_operations_output, render_plan_rollback_output, render_plan_swap_output,
        render_summary, render_version,
    },
    renodx,
};

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tests;

mod add_game;

use add_game::add_game;

type CliOutput = Result<String, CliError>;

pub(crate) fn render_command(command: Command, info: AppInfo) -> CliOutput {
    render_command_with_context(command, info, Context::open)
}

pub(crate) fn render_command_with_context<F>(
    command: Command,
    info: AppInfo,
    open_context: F,
) -> CliOutput
where
    F: FnOnce() -> Result<Context, ServiceError>,
{
    match command {
        Command::Summary => render_summary_command(info),
        Command::Help => render_help_command(info),
        Command::Version => render_version_command(info),
        other => {
            let context = open_context()?;
            render_stateful_command(other, &context)
        }
    }
}

fn render_stateful_command(command: Command, context: &Context) -> CliOutput {
    match command {
        Command::AddGame {
            path,
            executable,
            root_choice,
            allow_root_correction,
        } => add_game(
            context,
            path,
            executable,
            root_choice,
            allow_root_correction,
        ),
        Command::ListArtifacts { technology } => list_artifacts(context, technology),
        Command::ListOperations { game_id } => list_operations(context, &game_id),
        Command::Candidates { game_id } => candidates(context, &game_id),

        Command::PlanSwap {
            game_id,
            component_id,
            artifact_id,
        } => plan_swap(context, &game_id, &component_id, &artifact_id),

        Command::ApplyOperation {
            game_id,
            component_id,
            artifact_id,
            confirmation_token,
        } => apply_swap(
            context,
            &game_id,
            &component_id,
            &artifact_id,
            confirmation_token.as_deref(),
        ),
        Command::PlanRollback {
            game_id,
            component_id,
        } => plan_rollback(context, &game_id, &component_id),
        Command::RollbackOperation {
            game_id,
            component_id,
        } => rollback_component(context, &game_id, &component_id),

        Command::RenodxStatus { game_id } => renodx_status(context, &game_id),
        Command::RenodxUninstall { game_id } => renodx_uninstall(context, &game_id),
        Command::RenodxCheckUpdate { game_id } => renodx_check_update(context, &game_id),
        Command::RenodxCheckUpdates => renodx_check_updates(context),
        Command::LumaStatus { game_id } => luma_status(context, &game_id),
        Command::LumaUninstall { game_id } => luma_uninstall(context, &game_id),
        Command::LumaCheckUpdate { game_id, deep } => luma_check_update(context, &game_id, deep),
        Command::LumaCheckUpdates => luma_check_updates(context),

        Command::Summary | Command::Help | Command::Version => Err(ServiceError::invalid_input(
            "stateless command reached the stateful command dispatcher",
        )
        .into()),
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

fn list_artifacts(
    context: &renderpilot_orchestration::Context,
    technology: Option<LibraryTechnology>,
) -> CliOutput {
    let artifacts = catalog::list_artifacts(context, technology)?;

    render_output(render_list_artifacts_output(artifacts))
}

fn list_operations(context: &renderpilot_orchestration::Context, game_id: &GameId) -> CliOutput {
    let result = catalog::list_operations(context, game_id)?;

    render_output(render_list_operations_output(&result))
}

fn candidates(context: &renderpilot_orchestration::Context, game_id: &GameId) -> CliOutput {
    let result = catalog::find_candidates(context, game_id)?;

    render_output(render_candidates_output(&result.game_id, result.groups))
}

fn plan_swap(
    context: &renderpilot_orchestration::Context,
    game_id: &GameId,
    component_id: &ComponentId,
    artifact_id: &ArtifactId,
) -> CliOutput {
    let plan = catalog::build_swap_plan(context, game_id, component_id, artifact_id)?;

    render_output(render_plan_swap_output(&plan.plan))
}

fn apply_swap(
    context: &renderpilot_orchestration::Context,
    game_id: &GameId,
    component_id: &ComponentId,
    artifact_id: &ArtifactId,
    confirmation_token: Option<&str>,
) -> CliOutput {
    let result = catalog::apply_swap_confirmed(
        context,
        game_id,
        component_id,
        artifact_id,
        confirmation_token,
    )?;

    render_json(&result)
}

fn plan_rollback(
    context: &renderpilot_orchestration::Context,
    game_id: &GameId,
    component_id: &ComponentId,
) -> CliOutput {
    let plan = catalog::build_rollback_plan(context, game_id, component_id)?;
    render_output(render_plan_rollback_output(&plan))
}

fn rollback_component(
    context: &renderpilot_orchestration::Context,
    game_id: &GameId,
    component_id: &ComponentId,
) -> CliOutput {
    let result = catalog::rollback_component(context, game_id, component_id)?;

    render_json(&result)
}

fn renodx_status(context: &renderpilot_orchestration::Context, game_id: &GameId) -> CliOutput {
    let state = renodx::status(context, game_id)?;

    render_json(&state)
}

fn renodx_uninstall(context: &renderpilot_orchestration::Context, game_id: &GameId) -> CliOutput {
    renodx::uninstall(context, game_id)?;
    // Report the resulting (not-installed) state, mirroring the desktop facade.
    let state = renodx::status(context, game_id)?;

    render_json(&state)
}

fn renodx_check_update(
    context: &renderpilot_orchestration::Context,
    game_id: &GameId,
) -> CliOutput {
    let report = block_on(async {
        let bundle = renodx::manifest_store::get_or_fetch_bundle().await?;
        renodx::check_update(context, &bundle.tool, &bundle.reshade.sources, game_id).await
    })?;
    render_json(&report)
}

fn renodx_check_updates(context: &renderpilot_orchestration::Context) -> CliOutput {
    let statuses = block_on(async {
        match renodx::manifest_store::get_or_fetch_bundle().await {
            Ok(bundle) => {
                renodx::check_updates(context, &bundle.tool, &bundle.reshade.sources).await
            }
            Err(_) => renodx::unknown_updates_for_installed(context),
        }
    })?;
    render_json(&statuses_to_map(statuses))
}

fn luma_status(context: &renderpilot_orchestration::Context, game_id: &GameId) -> CliOutput {
    let manifest = block_on(luma::manifest_store::get_or_fetch_manifest()).ok();
    let state = luma::status(context, manifest.as_ref(), game_id)?;
    render_json(&state)
}

fn luma_uninstall(context: &renderpilot_orchestration::Context, game_id: &GameId) -> CliOutput {
    luma::uninstall(context, game_id)?;
    let state = luma::status(context, None, game_id)?;
    render_json(&state)
}

fn luma_check_update(
    context: &renderpilot_orchestration::Context,
    game_id: &GameId,
    deep: bool,
) -> CliOutput {
    let report = block_on(async {
        let bundle = luma::manifest_store::get_or_fetch_bundle().await?;
        luma::check_update(
            context,
            &bundle.tool,
            &bundle.reshade.sources,
            game_id,
            deep,
        )
        .await
    })?;
    render_json(&report)
}

fn luma_check_updates(context: &renderpilot_orchestration::Context) -> CliOutput {
    let statuses = block_on(async {
        match luma::manifest_store::get_or_fetch_bundle().await {
            Ok(bundle) => luma::check_updates(context, &bundle.tool, &bundle.reshade.sources).await,
            Err(_) => luma::unknown_updates_for_installed(context),
        }
    })?;
    render_json(&statuses_to_map(statuses))
}

fn statuses_to_map(
    statuses: Vec<(
        GameId,
        renderpilot_orchestration::addons::update::UpdateStatus,
    )>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::with_capacity(statuses.len());
    for (game_id, status) in statuses {
        if let Ok(value) = serde_json::to_value(status) {
            map.insert(game_id.as_str().to_owned(), value);
        }
    }
    map
}

fn block_on<F, T>(future: F) -> Result<T, ServiceError>
where
    F: std::future::Future<Output = Result<T, ServiceError>>,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            ServiceError::command_failed(format!("failed to start async runtime for CLI: {error}"))
        })?
        .block_on(future)
}

fn render_output<E>(output: Result<String, E>) -> CliOutput
where
    E: Into<CliError>,
{
    output.map_err(Into::into)
}

/// Renders a serializable value as pretty JSON, mapping a serialization failure
/// into a [`CliError`].
fn render_json<T: serde::Serialize>(value: &T) -> CliOutput {
    render_output(serde_json::to_string_pretty(value))
}
