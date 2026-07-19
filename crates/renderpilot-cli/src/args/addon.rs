//! RenoDX subcommand parsing kept independent from the generic catalog parser.

use renderpilot_orchestration::domain::GameId;

use super::command::Command;
use super::cursor::{ArgCursor, parse_named_identifier};
use crate::CliError;

pub(super) fn parse_renodx_command(args: &mut ArgCursor) -> Result<Command, CliError> {
    let Some(subcommand) = args.next_keyword()? else {
        return Err(CliError::MissingArgument("<status|uninstall>"));
    };
    let build: fn(GameId) -> Command = match subcommand.as_str() {
        "status" => |game_id| Command::RenodxStatus { game_id },
        "uninstall" => |game_id| Command::RenodxUninstall { game_id },
        _ => return Err(CliError::UnknownArgument(subcommand)),
    };
    let game_id = parse_named_identifier(args, "--game", "<game_id>", CliError::InvalidGameId)?;
    Ok(build(game_id))
}