use renderpilot_orchestration::domain::GraphicsTechnology;

use super::command::{AddGameRootChoiceArg, Command};
use super::cursor::{ArgCursor, parse_identifier_argument, parse_named_identifier};
use crate::CliError;

pub(super) fn parse_add_game_command(args: &mut ArgCursor) -> Result<Command, CliError> {
    let path = args.next_required_path("<install-root>")?;
    let mut executable = None;
    let mut root_choice = AddGameRootChoiceArg::Auto;
    let mut allow_root_correction = false;
    while let Some(argument) = args.next_keyword()? {
        match argument.as_str() {
            "--executable" => {
                executable = Some(args.next_required_path("<executable>")?);
            }
            "--root-choice" => {
                let value = args.next_required_keyword("<auto|selected|recommended>")?;
                root_choice = match value.as_str() {
                    "auto" => AddGameRootChoiceArg::Auto,
                    "selected" => AddGameRootChoiceArg::Selected,
                    "recommended" => AddGameRootChoiceArg::Recommended,
                    _ => return Err(CliError::InvalidAddGameRootChoice(value)),
                };
            }
            "--allow-root-correction" => allow_root_correction = true,
            _ => return Err(CliError::UnexpectedArgument(argument)),
        }
    }

    Ok(Command::AddGame {
        path,
        executable,
        root_choice,
        allow_root_correction,
    })
}

pub(super) fn parse_list_artifacts_command(args: &mut ArgCursor) -> Result<Command, CliError> {
    let mut technology = None;

    while let Some(argument) = args.next_keyword()? {
        match argument.as_str() {
            "--technology" => {
                let value = args.next_required_keyword("<technology>")?;
                let parsed = GraphicsTechnology::from_slug(&value)
                    .ok_or(CliError::InvalidTechnology(value))?;
                technology = Some(parsed);
            }
            _ => return Err(CliError::UnexpectedArgument(argument)),
        }
    }

    Ok(Command::ListArtifacts { technology })
}

pub(super) fn parse_candidates_command(args: &mut ArgCursor) -> Result<Command, CliError> {
    let game_id = parse_named_identifier(args, "--game", "<game_id>", CliError::InvalidGameId)?;
    Ok(Command::Candidates { game_id })
}

pub(super) fn parse_list_operations_command(args: &mut ArgCursor) -> Result<Command, CliError> {
    let game_id = parse_named_identifier(args, "--game", "<game_id>", CliError::InvalidGameId)?;
    Ok(Command::ListOperations { game_id })
}

pub(super) fn parse_plan_swap_command(args: &mut ArgCursor) -> Result<Command, CliError> {
    let mut game_id = None;
    let mut component_id = None;
    let mut artifact_id = None;

    while let Some(argument) = args.next_keyword()? {
        match argument.as_str() {
            "--game" => {
                game_id = Some(parse_identifier_argument(
                    args,
                    "<game_id>",
                    CliError::InvalidGameId,
                )?);
            }
            "--component" => {
                component_id = Some(parse_identifier_argument(
                    args,
                    "<component_id>",
                    CliError::InvalidComponentId,
                )?);
            }
            "--artifact" => {
                artifact_id = Some(parse_identifier_argument(
                    args,
                    "<artifact_id>",
                    CliError::InvalidArtifactId,
                )?);
            }
            _ => return Err(CliError::UnexpectedArgument(argument)),
        }
    }

    Ok(Command::PlanSwap {
        game_id: game_id.ok_or(CliError::MissingArgument("<game_id>"))?,
        component_id: component_id.ok_or(CliError::MissingArgument("<component_id>"))?,
        artifact_id: artifact_id.ok_or(CliError::MissingArgument("<artifact_id>"))?,
    })
}

pub(super) fn parse_apply_command(args: &mut ArgCursor) -> Result<Command, CliError> {
    let mut game_id = None;
    let mut component_id = None;
    let mut artifact_id = None;
    let mut confirmation_token = None;

    while let Some(argument) = args.next_keyword()? {
        match argument.as_str() {
            "--game" => {
                game_id = Some(parse_identifier_argument(
                    args,
                    "<game_id>",
                    CliError::InvalidGameId,
                )?);
            }
            "--component" => {
                component_id = Some(parse_identifier_argument(
                    args,
                    "<component_id>",
                    CliError::InvalidComponentId,
                )?);
            }
            "--artifact" => {
                artifact_id = Some(parse_identifier_argument(
                    args,
                    "<artifact_id>",
                    CliError::InvalidArtifactId,
                )?);
            }
            "--confirmation-token" => {
                confirmation_token = Some(args.next_required_keyword("<confirmation_token>")?);
            }
            _ => return Err(CliError::UnexpectedArgument(argument)),
        }
    }

    Ok(Command::ApplyOperation {
        game_id: game_id.ok_or(CliError::MissingArgument("<game_id>"))?,
        component_id: component_id.ok_or(CliError::MissingArgument("<component_id>"))?,
        artifact_id: artifact_id.ok_or(CliError::MissingArgument("<artifact_id>"))?,
        confirmation_token,
    })
}

pub(super) fn parse_plan_rollback_command(args: &mut ArgCursor) -> Result<Command, CliError> {
    let (game_id, component_id) = parse_rollback_arguments(args)?;
    Ok(Command::PlanRollback {
        game_id,
        component_id,
    })
}

pub(super) fn parse_rollback_command(args: &mut ArgCursor) -> Result<Command, CliError> {
    let (game_id, component_id) = parse_rollback_arguments(args)?;
    Ok(Command::RollbackOperation {
        game_id,
        component_id,
    })
}

fn parse_rollback_arguments(
    args: &mut ArgCursor,
) -> Result<
    (
        renderpilot_orchestration::domain::GameId,
        renderpilot_orchestration::domain::ComponentId,
    ),
    CliError,
> {
    let mut game_id = None;
    let mut component_id = None;

    while let Some(argument) = args.next_keyword()? {
        match argument.as_str() {
            "--game" => {
                game_id = Some(parse_identifier_argument(
                    args,
                    "<game_id>",
                    CliError::InvalidGameId,
                )?);
            }
            "--component" => {
                component_id = Some(parse_identifier_argument(
                    args,
                    "<component_id>",
                    CliError::InvalidComponentId,
                )?);
            }
            _ => return Err(CliError::UnexpectedArgument(argument)),
        }
    }

    Ok((
        game_id.ok_or(CliError::MissingArgument("<game_id>"))?,
        component_id.ok_or(CliError::MissingArgument("<component_id>"))?,
    ))
}
